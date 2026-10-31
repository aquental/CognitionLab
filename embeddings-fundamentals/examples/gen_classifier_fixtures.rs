//! Generates the synthetic inputs of the `classify` command's tests under
//! `tests/fixtures/classifier/cli/` (spec/CLASSIFIER.md §20, Phase 5).
//!
//! All content is invented, names and numbers included; nothing comes from a real exam.
//! The files are checked in, so tests never need this program. It runs only when the
//! fixtures change: `cargo run --example gen_classifier_fixtures`. It needs `pdftoppm`
//! (poppler) to render the scanned page, as `gen_fixtures` does.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, StringFormat, dictionary};

const OUT: &str = "tests/fixtures/classifier/cli";

/// A Holter report with a labelled, invented patient name.
const HOLTER: &[&str] = &[
    "CLINICA FICTICIA - HOLTER DE 24 HORAS (LAUDO SINTETICO)",
    "Paciente: Nilo Teixeira   Idade: 47 anos",
    "",
    "Ritmo sinusal predominante durante todo o registro.",
    "Frequencia cardiaca minima 52 bpm, media 71 bpm, maxima 128 bpm.",
    "Extrassistoles ventriculares isoladas e raras, sem pausas significativas.",
    "",
    "Conclusao: Holter de 24 horas sem arritmias complexas.",
];

/// An echocardiogram report, for the A9 case (a known name that is a file-name term).
const ECO: &[&str] = &[
    "CLINICA FICTICIA - ECOCARDIOGRAMA TRANSTORACICO (LAUDO SINTETICO)",
    "",
    "Ventriculo esquerdo de dimensoes normais, FEVE 62 %.",
    "Strain longitudinal global -19,5 %.",
    "Valvas sem alteracoes estruturais significativas.",
    "",
    "Conclusao: ecocardiograma dentro da normalidade.",
];

/// A scanned echocardiogram with strain analysis.
const ECO_STRAIN: &[&str] = &[
    "ECOCARDIOGRAMA COM STRAIN - LAUDO SINTETICO",
    "",
    "Analise da deformacao miocardica por speckle tracking.",
    "Strain longitudinal global -20 %, valores dos segmentos preservados.",
    "Fracao de ejecao estimada em 60 %.",
    "",
    "Conclusao: funcao sistolica preservada.",
];

fn text_content(lines: &[&str]) -> Result<Content> {
    let mut ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 12.into()]),
        Operation::new("TL", vec![18.into()]),
        Operation::new("Td", vec![56.into(), 780.into()]),
    ];
    for line in lines {
        if !line.is_ascii() {
            bail!("fixture text must be ASCII: {line:?}");
        }
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                line.as_bytes().to_vec(),
                StringFormat::Literal,
            )],
        ));
        ops.push(Operation::new("T*", vec![]));
    }
    ops.push(Operation::new("ET", vec![]));
    Ok(Content { operations: ops })
}

/// One A4 page: text lines, or a full-page grayscale raster.
enum Pagina<'a> {
    Texto(&'a [&'a str]),
    Digitalizada(&'a image::GrayImage),
}

fn build_pdf(pagina: &Pagina<'_>, out: &Path) -> Result<()> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let (content, xobjects) = match pagina {
        Pagina::Texto(lines) => (text_content(lines)?, dictionary! {}),
        Pagina::Digitalizada(gray) => {
            let mut stream = Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image",
                    "Width" => i64::from(gray.width()), "Height" => i64::from(gray.height()),
                    "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
                },
                gray.as_raw().clone(),
            );
            let _ = stream.compress();
            let id = doc.add_object(stream);
            let ops = vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        595.into(),
                        0.into(),
                        0.into(),
                        842.into(),
                        0.into(),
                        0.into(),
                    ],
                ),
                Operation::new("Do", vec!["Scan".into()]),
                Operation::new("Q", vec![]),
            ];
            (Content { operations: ops }, dictionary! { "Scan" => id })
        }
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id }, "XObject" => xobjects },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 },
        ),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.compress();
    doc.save(out)
        .with_context(|| format!("save {}", out.display()))?;
    Ok(())
}

/// Renders the first page of `pdf` to a grayscale image at 150 dpi, like a scanner.
fn render(pdf: &Path, prefix: &Path) -> Result<image::GrayImage> {
    let status = Command::new("pdftoppm")
        .args(["-png", "-gray", "-singlefile", "-r", "150"])
        .arg(pdf)
        .arg(prefix)
        .status()
        .context("run pdftoppm (install poppler to regenerate fixtures)")?;
    if !status.success() {
        bail!("pdftoppm failed");
    }
    Ok(image::open(prefix.with_extension("png"))?.to_luma8())
}

fn main() -> Result<()> {
    let out = Path::new(OUT);
    fs::create_dir_all(out)?;
    build_pdf(&Pagina::Texto(HOLTER), &out.join("holter_24h.pdf"))?;
    build_pdf(&Pagina::Texto(ECO), &out.join("eco_mapa.pdf"))?;
    let tmp = tempfile::tempdir()?;
    let fonte = tmp.path().join("eco_strain_texto.pdf");
    build_pdf(&Pagina::Texto(ECO_STRAIN), &fonte)?;
    let scan = render(&fonte, &tmp.path().join("scan"))?;
    build_pdf(
        &Pagina::Digitalizada(&scan),
        &out.join("eco_strain_digitalizado.pdf"),
    )?;
    scan.save(out.join("eco_strain.png"))?;
    println!("wrote 4 fixtures to {OUT}");
    Ok(())
}
