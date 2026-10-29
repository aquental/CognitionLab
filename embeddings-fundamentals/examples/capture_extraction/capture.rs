//! Image capture of `capture_extraction`, shared with the integration test that proves a
//! recaptured masked image replays (C3-7).

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use embeddings_fundamentals::extraction::ExtractionConfig;
use embeddings_fundamentals::extraction::images::{ImageFilter, ImageInfo, mask_of, parse_list};
use embeddings_fundamentals::extraction::poppler::native_lines;

pub fn run(tool: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("run {tool}"))?;
    if !output.status.success() {
        bail!("{tool} failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(output.stdout)
}

/// Writes `images.list` and, for every image an image run keeps, its PNG and the PNG of
/// its mask (the replay needs both: the pipeline prepares a kept image with its mask).
/// Returns the number of kept images.
pub fn capture_images(pdf_str: &str, dir: &Path, scratch: &Path) -> Result<usize> {
    let list = run("pdfimages", &["-list", pdf_str])?;
    fs::write(dir.join("images.list"), &list)?;
    let (filter, config) = (ImageFilter::default(), ExtractionConfig::default());
    let listed = parse_list(&String::from_utf8(list)?)?;
    let mut kept = 0;
    for image in listed.iter().filter(|i| filter.keeps(i)) {
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
        let copy = |info: &ImageInfo| -> Result<()> {
            let file = format!(
                "{root_str}-{:03}-{:03}.png",
                info.page, info.position_on_page
            );
            fs::copy(
                &file,
                dir.join(format!("img-p{}-{}.png", info.page, info.position_on_page)),
            )
            .with_context(|| format!("copy {file}"))?;
            Ok(())
        };
        copy(image)?;
        if let Some(mask) = mask_of(&listed, image) {
            copy(mask)?;
        }
        kept += 1;
    }
    Ok(kept)
}
