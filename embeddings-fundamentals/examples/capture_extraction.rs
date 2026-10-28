//! Captures real extraction output for the synthetic fixtures, so BDD scenarios can
//! replay it deterministically without poppler or Tesseract (PLAN §0.1).
//!
//! For every PDF in `tests/fixtures/corpus/` and every page, it writes
//! `tests/fixtures/extraction/<stem>/p<N>.native.txt` (`pdftotext -layout`) and
//! `p<N>.ocr.tsv` (`pdftoppm -png -singlefile -r 300` then `tesseract -l por+eng
//! --psm 6 tsv`), plus `CAPTURE.toml` with the tool versions. For image runs it also
//! writes `images.list` (`pdfimages -list`) and `img-p<N>-<position>.png` for every
//! image an image run keeps: listed as `image`, both sides ≥ 64 px, on a page whose
//! text layer passes the native-text rule (on other pages the raster is the OCR
//! source). Run with `cargo run --example capture_extraction`, or with `-- --images`
//! to capture only the image files. The output depends on the installed tool
//! versions, so recapturing is a deliberate fixture change.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use embeddings_fundamentals::extraction::ExtractionConfig;
use embeddings_fundamentals::extraction::images::{ImageFilter, parse_list};
use embeddings_fundamentals::extraction::poppler::native_lines;

const CORPUS: &str = "tests/fixtures/corpus";
const OUT: &str = "tests/fixtures/extraction";

fn run(tool: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("run {tool}"))?;
    if !output.status.success() {
        bail!("{tool} failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(output.stdout)
}

fn version(tool: &str, flag: &str) -> Result<String> {
    let output = Command::new(tool)
        .arg(flag)
        .output()
        .with_context(|| format!("run {tool}"))?;
    let text = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    Ok(String::from_utf8_lossy(&text)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned())
}

/// Writes `images.list` and the PNGs of the images an image run keeps.
fn capture_images(pdf_str: &str, dir: &Path, scratch: &Path) -> Result<usize> {
    let list = run("pdfimages", &["-list", pdf_str])?;
    fs::write(dir.join("images.list"), &list)?;
    let (filter, config) = (ImageFilter::default(), ExtractionConfig::default());
    let mut kept = 0;
    for image in parse_list(&String::from_utf8(list)?)?
        .iter()
        .filter(|i| filter.keeps(i))
    {
        let n = image.page.to_string();
        let native = run("pdftotext", &["-layout", "-f", &n, "-l", &n, pdf_str, "-"])?;
        let text = native_lines(&String::from_utf8_lossy(&native))
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !config.native_text_is_usable(&text) {
            continue;
        }
        let root = scratch.join(format!("img-{n}"));
        let root_str = root.to_str().context("utf-8 path")?;
        run(
            "pdfimages",
            &["-png", "-p", "-f", &n, "-l", &n, pdf_str, root_str],
        )?;
        let file = format!(
            "{root_str}-{:03}-{:03}.png",
            image.page, image.position_on_page
        );
        fs::copy(
            &file,
            dir.join(format!(
                "img-p{}-{}.png",
                image.page, image.position_on_page
            )),
        )
        .with_context(|| format!("copy {file}"))?;
        kept += 1;
    }
    Ok(kept)
}

fn main() -> Result<()> {
    let images_only = std::env::args().any(|a| a == "--images");
    let scratch = tempfile::tempdir().context("temp dir")?;
    let mut pdfs: Vec<_> = fs::read_dir(CORPUS)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    pdfs.retain(|p| p.extension().is_some_and(|e| e == "pdf"));
    pdfs.sort();
    for pdf in &pdfs {
        let pdf_str = pdf.to_str().context("utf-8 path")?;
        let stem = pdf
            .file_stem()
            .context("stem")?
            .to_string_lossy()
            .into_owned();
        let info = String::from_utf8(run("pdfinfo", &[pdf_str])?)?;
        let pages: u32 = info
            .lines()
            .find_map(|l| l.strip_prefix("Pages:"))
            .context("Pages: line")?
            .trim()
            .parse()?;
        let dir = Path::new(OUT).join(&stem);
        fs::create_dir_all(&dir)?;
        let kept = capture_images(pdf_str, &dir, scratch.path())?;
        println!("captured {stem}: {kept} kept images");
        if images_only {
            continue;
        }
        for page in 1..=pages {
            let n = page.to_string();
            let native = run("pdftotext", &["-layout", "-f", &n, "-l", &n, pdf_str, "-"])?;
            fs::write(dir.join(format!("p{page}.native.txt")), native)?;
            let prefix = scratch.path().join(format!("{stem}-{page}"));
            let prefix_str = prefix.to_str().context("utf-8 path")?;
            run(
                "pdftoppm",
                &[
                    "-q",
                    "-png",
                    "-singlefile",
                    "-r",
                    "300",
                    "-f",
                    &n,
                    "-l",
                    &n,
                    pdf_str,
                    prefix_str,
                ],
            )?;
            let png = format!("{prefix_str}.png");
            let tsv = run(
                "tesseract",
                &[&png, "-", "-l", "por+eng", "--psm", "6", "tsv"],
            )?;
            fs::write(dir.join(format!("p{page}.ocr.tsv")), tsv)?;
        }
        println!("captured {stem}: {pages} pages");
    }
    if images_only {
        return Ok(());
    }
    let mut capture =
        String::from("# Tool versions used by `cargo run --example capture_extraction`.\n");
    writeln!(capture, "pdftotext = {:?}", version("pdftotext", "-v")?)?;
    writeln!(capture, "pdftoppm = {:?}", version("pdftoppm", "-v")?)?;
    writeln!(
        capture,
        "tesseract = {:?}",
        version("tesseract", "--version")?
    )?;
    writeln!(
        capture,
        "ocr = \"pdftoppm -png -singlefile -r 300; tesseract -l por+eng --psm 6 tsv\""
    )?;
    writeln!(capture, "pdfimages = {:?}", version("pdfimages", "-v")?)?;
    writeln!(
        capture,
        "images = \"pdfimages -list; pdfimages -png -p -f N -l N\""
    )?;
    fs::write(Path::new(OUT).join("CAPTURE.toml"), capture)?;
    Ok(())
}
