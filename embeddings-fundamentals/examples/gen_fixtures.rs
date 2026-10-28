//! Generates the synthetic, anonymized fixture corpus under `tests/fixtures/corpus/`.
//!
//! All content is invented. Nothing here comes from real patients. The generated files
//! are checked in, so tests never need this program. It runs only when the fixtures
//! change: `cargo run --example gen_fixtures`. Scanned pages are produced by rendering
//! a generated text page with `pdftoppm` (poppler), which must be installed for
//! regeneration only.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use image::{GrayImage, ImageEncoder, Luma, Rgb, RgbImage};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use sha2::{Digest, Sha256};

const OUT: &str = "tests/fixtures/corpus";

/// Lab report, page 1 (blood count). Rows keep name | value | unit | range together.
const LAB_PAGE_1: &[&str] = &[
    "LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO",
    "Paciente: [ANONIMIZADO]   Registro: [ANONIMIZADO]",
    "",
    "Hemograma",
    "Exame | Resultado | Unidade | Intervalo de referência",
    "Hemoglobina | 13,5 | g/dL | 12,0–16,0",
    "Hematócrito | 40,1 | % | 36,0–46,0",
    "Leucócitos | 6.800 | /mm³ | 4.000–10.000",
    "Plaquetas | 250.000 | /mm³ | 150.000–450.000",
];

/// Lab report, page 2 (biochemistry).
const LAB_PAGE_2: &[&str] = &[
    "Bioquímica",
    "Exame | Resultado | Unidade | Intervalo de referência",
    "Glicemia de jejum | 92 | mg/dL | 70–99",
    "Creatinina | 0,9 | mg/dL | 0,6–1,1",
    "Colesterol total | 182 | mg/dL | < 190",
    "Triglicerídeos | 120 | mg/dL | < 150",
    "",
    "Conclusão: valores dentro dos intervalos de referência.",
];

/// Ultrasound report text (searchable page of the mixed PDF).
const US_REPORT: &[&str] = &[
    "LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO",
    "Paciente: [ANONIMIZADO]",
    "",
    "Técnica",
    "Exame realizado com transdutor convexo de 3,5 MHz, em jejum de 8 horas.",
    "",
    "Achados",
    "Fígado de dimensões normais, contornos regulares e ecotextura homogênea.",
    "Vesícula biliar normodistendida, paredes finas, sem cálculos.",
    "",
    "Conclusão",
    "Ultrassonografia abdominal sem alterações significativas.",
];

/// Encodes text for a standard Type1 font with `/WinAnsiEncoding` (Windows-1252).
fn win_ansi(text: &str) -> Result<Vec<u8>> {
    text.chars()
        .map(|c| match c {
            '\u{20}'..='\u{7e}' => u8::try_from(u32::from(c)).context("ascii"),
            '\u{a0}'..='\u{ff}' => u8::try_from(u32::from(c)).context("latin-1"),
            '\u{2013}' => Ok(0x96),
            '\u{2014}' => Ok(0x97),
            other => bail!("character {other:?} is not in WinAnsiEncoding"),
        })
        .collect()
}

fn text_content(lines: &[&str]) -> Result<Content> {
    let mut ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 11.into()]),
        Operation::new("TL", vec![16.into()]),
        Operation::new("Td", vec![56.into(), 780.into()]),
    ];
    for line in lines {
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(win_ansi(line)?, StringFormat::Literal)],
        ));
        ops.push(Operation::new("T*", vec![]));
    }
    ops.push(Operation::new("ET", vec![]));
    Ok(Content { operations: ops })
}

/// A page under construction: either text lines or a full-page raster image, plus
/// optional embedded raster images.
enum PageBody<'a> {
    Text(&'a [&'a str]),
    Scanned(&'a Path),
}

struct EmbeddedImage {
    rgb: RgbImage,
    /// Grayscale soft mask; the extractor must not report it as a separate image.
    soft_mask: Option<GrayImage>,
    /// Placement: x, y, width, height in points.
    rect: (i64, i64, i64, i64),
}

fn image_stream(doc: &mut Document, rgb: &RgbImage, soft_mask: Option<ObjectId>) -> ObjectId {
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(rgb.width()),
        "Height" => i64::from(rgb.height()),
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
    };
    if let Some(mask) = soft_mask {
        dict.set("SMask", mask);
    }
    let mut stream = Stream::new(dict, rgb.as_raw().clone());
    // Compression failures only cost size; the uncompressed stream stays valid.
    let _ = stream.compress();
    doc.add_object(stream)
}

fn gray_stream(doc: &mut Document, gray: &GrayImage) -> ObjectId {
    let dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(gray.width()),
        "Height" => i64::from(gray.height()),
        "ColorSpace" => "DeviceGray",
        "BitsPerComponent" => 8,
    };
    let mut stream = Stream::new(dict, gray.as_raw().clone());
    let _ = stream.compress();
    doc.add_object(stream)
}

fn build_pdf(pages: &[(PageBody<'_>, Vec<EmbeddedImage>)], out: &Path) -> Result<()> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let mut kids = Vec::new();
    for (body, images) in pages {
        let mut xobjects = Dictionary::new();
        let mut content = match body {
            PageBody::Text(lines) => text_content(lines)?,
            PageBody::Scanned(png) => {
                let scan = image::open(png)
                    .with_context(|| format!("open {}", png.display()))?
                    .to_rgb8();
                let id = image_stream(&mut doc, &scan, None);
                xobjects.set("Scan", id);
                Content {
                    operations: vec![
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
                    ],
                }
            }
        };
        for (i, image) in images.iter().enumerate() {
            let mask = image.soft_mask.as_ref().map(|m| gray_stream(&mut doc, m));
            let id = image_stream(&mut doc, &image.rgb, mask);
            let name = format!("Im{i}");
            xobjects.set(name.as_bytes().to_vec(), id);
            let (x, y, w, h) = image.rect;
            content.operations.extend([
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![w.into(), 0.into(), 0.into(), h.into(), x.into(), y.into()],
                ),
                Operation::new("Do", vec![Object::Name(name.into_bytes())]),
                Operation::new("Q", vec![]),
            ]);
        }
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
                "XObject" => xobjects,
            },
        });
        kids.push(Object::from(page_id));
    }
    let count = i64::try_from(kids.len()).context("page count")?;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let info_id = doc.add_object(dictionary! {
        "Producer" => Object::string_literal("embeddings-fundamentals gen_fixtures"),
    });
    doc.trailer.set("Info", info_id);
    doc.compress();
    doc.save(out)
        .with_context(|| format!("save {}", out.display()))?;
    Ok(())
}

/// Renders page `page` of `pdf` to a grayscale PNG, simulating a scanner.
fn render_scan(pdf: &Path, page: u32, out_prefix: &Path) -> Result<PathBuf> {
    let status = Command::new("pdftoppm")
        .args([
            "-png",
            "-gray",
            "-singlefile",
            "-r",
            "150",
            "-f",
            &page.to_string(),
            "-l",
        ])
        .arg(page.to_string())
        .arg(pdf)
        .arg(out_prefix)
        .status()
        .context("run pdftoppm (install poppler to regenerate fixtures)")?;
    if !status.success() {
        bail!("pdftoppm failed for {}", pdf.display());
    }
    Ok(out_prefix.with_extension("png"))
}

/// Synthetic chest radiograph: dark background, two bright lung fields, a spine.
fn chest_radiograph() -> GrayImage {
    let (w, h) = (256u32, 256u32);
    GrayImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (f64::from(x), f64::from(y));
        let lung = |cx: f64| ((fx - cx) / 52.0).powi(2) + ((fy - 130.0) / 85.0).powi(2) <= 1.0;
        let spine = (fx - 128.0).abs() < 9.0 && fy > 30.0;
        let ribs = y % 22 < 3 && (40..216).contains(&x);
        let value: u8 = if spine {
            235
        } else if lung(78.0) || lung(178.0) {
            if ribs { 170 } else { 60 }
        } else {
            25
        };
        Luma([value])
    })
}

/// Synthetic abdominal ultrasound: a fan-shaped sector with speckle.
fn abdominal_ultrasound() -> RgbImage {
    let (w, h) = (256u32, 192u32);
    RgbImage::from_fn(w, h, |x, y| {
        let (dx, dy) = (f64::from(x) - 128.0, f64::from(y) + 10.0);
        let r = (dx * dx + dy * dy).sqrt();
        let angle = dx.atan2(dy).abs();
        let inside = angle < 0.75 && (25.0..190.0).contains(&r);
        let speckle = u8::try_from((x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) % 90)
            .unwrap_or(0);
        let organ = ((dx + 20.0) / 45.0).powi(2) + ((dy - 110.0) / 30.0).powi(2) <= 1.0;
        let v = if !inside {
            0
        } else if organ {
            20u8.saturating_add(speckle / 6)
        } else {
            70u8.saturating_add(speckle)
        };
        Rgb([v, v, v])
    })
}

/// A small (48 px) logo whose alpha channel becomes a soft mask in the PDF. Both the
/// logo (below the 64 px minimum) and its mask must be dropped by image extraction.
fn logo_with_mask() -> (RgbImage, GrayImage) {
    let rgb = RgbImage::from_fn(48, 48, |x, _| {
        if x < 24 {
            Rgb([20, 90, 160])
        } else {
            Rgb([200, 200, 200])
        }
    });
    let mask = GrayImage::from_fn(48, 48, |x, y| {
        let d = (f64::from(x) - 24.0).hypot(f64::from(y) - 24.0);
        Luma([if d < 20.0 { 255 } else { 0 }])
    });
    (rgb, mask)
}

fn checksum(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

fn main() -> Result<()> {
    let out = Path::new(OUT);
    fs::create_dir_all(out).context("create fixture dir")?;
    let scratch = tempfile::tempdir().context("temp dir")?;

    // 1. Searchable two-page lab report.
    let lab = out.join("lab-searchable-001.pdf");
    build_pdf(
        &[
            (PageBody::Text(LAB_PAGE_1), vec![]),
            (PageBody::Text(LAB_PAGE_2), vec![]),
        ],
        &lab,
    )?;

    // 2. Scanned lab report: the same pages rendered to images, no text layer.
    let scan1 = render_scan(&lab, 1, &scratch.path().join("scan1"))?;
    let scan2 = render_scan(&lab, 2, &scratch.path().join("scan2"))?;
    build_pdf(
        &[
            (PageBody::Scanned(&scan1), vec![]),
            (PageBody::Scanned(&scan2), vec![]),
        ],
        &out.join("lab-scanned-002.pdf"),
    )?;

    // 3. Mixed report: page 1 native text with an embedded ultrasound image and a
    //    masked logo; page 2 scanned.
    let us_text = scratch.path().join("us-text.pdf");
    build_pdf(&[(PageBody::Text(US_REPORT), vec![])], &us_text)?;
    let us_scan = render_scan(&us_text, 1, &scratch.path().join("us-scan"))?;
    let (logo, mask) = logo_with_mask();
    build_pdf(
        &[
            (
                PageBody::Text(US_REPORT),
                vec![
                    EmbeddedImage {
                        rgb: abdominal_ultrasound(),
                        soft_mask: None,
                        rect: (300, 80, 240, 180),
                    },
                    EmbeddedImage {
                        rgb: logo,
                        soft_mask: Some(mask),
                        rect: (480, 770, 48, 48),
                    },
                ],
            ),
            (PageBody::Scanned(&us_scan), vec![]),
        ],
        &out.join("us-mixed-003.pdf"),
    )?;

    // 4. Standalone images. Filenames carry no Portuguese query terms.
    let mut png = Vec::new();
    let chest = chest_radiograph();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        chest.as_raw(),
        chest.width(),
        chest.height(),
        image::ExtendedColorType::L8,
    )?;
    fs::write(out.join("chest-001.png"), png).context("write chest-001.png")?;
    let mut jpeg = Vec::new();
    let us = abdominal_ultrasound();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).write_image(
        us.as_raw(),
        us.width(),
        us.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    fs::write(out.join("abd-us-001.jpg"), jpeg).context("write abd-us-001.jpg")?;

    // 5. Manifest with checksums (PRD §4, PLAN §4.4).
    let entries = [
        (
            "lab-searchable-001",
            "lab-searchable-001.pdf",
            "laboratory_report",
            "searchable_pdf",
        ),
        (
            "lab-scanned-002",
            "lab-scanned-002.pdf",
            "laboratory_report",
            "scanned_pdf",
        ),
        (
            "us-mixed-003",
            "us-mixed-003.pdf",
            "imaging_report",
            "mixed_pdf",
        ),
        ("chest-001", "chest-001.png", "radiograph", "png"),
        ("abd-us-001", "abd-us-001.jpg", "ultrasound_image", "jpeg"),
    ];
    let mut manifest = String::from(
        "# Synthetic fixture corpus generated by `cargo run --example gen_fixtures`.\n\
         # All content is invented; no real patient data.\n\n\
         corpus_id = \"synthetic-fixtures-v1\"\nremote_allowed = false\n",
    );
    for (id, file, doc_type, input_type) in entries {
        let sum = checksum(&out.join(file))?;
        write!(
            manifest,
            "\n[[sources]]\nsource_id = \"{id}\"\nfile = \"{file}\"\nchecksum = \"{sum}\"\n\
             document_type = \"{doc_type}\"\ninput_type = \"{input_type}\"\neligible = true\nrequired = true\n\
             anonymization = {{ filename = \"ok\", metadata = \"ok\", text = \"ok\", pixels = \"ok\", \
             reviewer = \"synthetic-generator\", date = \"2026-10-28\" }}\n"
        )?;
    }
    fs::write(out.join("manifest.toml"), manifest).context("write manifest")?;
    println!("fixtures written to {}", out.display());
    Ok(())
}
