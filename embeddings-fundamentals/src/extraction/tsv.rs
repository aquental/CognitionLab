//! Parser for Tesseract's TSV output.
//!
//! Only word rows (`level = 5`) carry text. Words are grouped into lines by
//! `(block, paragraph, line)` in output order. Line boxes are converted from rendered
//! pixels to PDF points (`× 72 / dpi`) with a top-left origin. Page confidence is the
//! mean `conf` of words with `conf ≥ 0`, scaled to `[0, 1]`; `-1` marks non-word rows.

use std::num::NonZeroU32;

use super::{BoundingBox, ExtractionError, PageText, TextLine};
use crate::domain::ExtractionMethod;

const COLUMNS: usize = 12;

#[derive(Debug)]
struct Word {
    key: (u32, u32, u32),
    text: String,
    bbox: BoundingBox,
    conf: f64,
}

fn parse_u32(field: &str) -> Result<u32, ExtractionError> {
    field.trim().parse().map_err(|_| ExtractionError::Parse {
        what: "tesseract tsv",
        reason: format!("expected an integer, found {field:?}"),
    })
}

fn px_to_pt(px: u32, dpi: u32) -> f32 {
    // Pixel coordinates of a rendered page fit comfortably in f32.
    #[expect(
        clippy::cast_precision_loss,
        reason = "page pixel coordinates are small"
    )]
    let (px, dpi) = (px as f32, dpi as f32);
    px * 72.0 / dpi
}

/// Parses Tesseract TSV into a page of OCR lines.
///
/// # Errors
///
/// Returns [`ExtractionError::Parse`] when the header or a row is malformed.
pub fn parse(
    tsv: &str,
    page: NonZeroU32,
    dpi: u32,
    engine: &str,
) -> Result<PageText, ExtractionError> {
    let mut rows = tsv.lines();
    let header = rows.next().unwrap_or_default();
    if !header.starts_with("level\t") {
        return Err(ExtractionError::Parse {
            what: "tesseract tsv",
            reason: "missing header row".to_owned(),
        });
    }
    let mut words = Vec::new();
    for row in rows.filter(|r| !r.trim().is_empty()) {
        let fields: Vec<&str> = row.splitn(COLUMNS, '\t').collect();
        if fields.len() < COLUMNS - 1 {
            return Err(ExtractionError::Parse {
                what: "tesseract tsv",
                reason: format!("row has {} columns", fields.len()),
            });
        }
        if parse_u32(fields[0])? != 5 {
            continue;
        }
        let text = fields.get(11).map_or("", |t| t.trim());
        if text.is_empty() {
            continue;
        }
        let conf: f64 = fields[10]
            .trim()
            .parse()
            .map_err(|_| ExtractionError::Parse {
                what: "tesseract tsv",
                reason: format!("bad confidence {:?}", fields[10]),
            })?;
        let (left, top, width, height) = (
            parse_u32(fields[6])?,
            parse_u32(fields[7])?,
            parse_u32(fields[8])?,
            parse_u32(fields[9])?,
        );
        words.push(Word {
            key: (
                parse_u32(fields[2])?,
                parse_u32(fields[3])?,
                parse_u32(fields[4])?,
            ),
            text: text.to_owned(),
            bbox: BoundingBox::points(
                px_to_pt(left, dpi),
                px_to_pt(top, dpi),
                px_to_pt(width, dpi),
                px_to_pt(height, dpi),
            ),
            conf,
        });
    }

    struct OpenLine {
        key: (u32, u32, u32),
        words: Vec<String>,
        bbox: BoundingBox,
        last_right: f32,
    }
    let close = |line: OpenLine| TextLine {
        text: line.words.join(" "),
        bounding_box: Some(line.bbox),
    };
    let mut lines: Vec<TextLine> = Vec::new();
    let mut current: Option<OpenLine> = None;
    for word in &words {
        match current.as_mut() {
            Some(line) if line.key == word.key => {
                // A horizontal gap wider than the word height marks a column boundary;
                // two spaces keep it visible to the whitespace-aligned table parser.
                let gap = word.bbox.x - (line.last_right);
                if gap > word.bbox.h {
                    line.words.push(String::new());
                }
                line.words.push(word.text.clone());
                line.last_right = word.bbox.x + word.bbox.w;
                line.bbox = line.bbox.union(&word.bbox);
            }
            _ => {
                if let Some(line) = current.take() {
                    lines.push(close(line));
                }
                current = Some(OpenLine {
                    key: word.key,
                    words: vec![word.text.clone()],
                    bbox: word.bbox.clone(),
                    last_right: word.bbox.x + word.bbox.w,
                });
            }
        }
    }
    if let Some(line) = current {
        lines.push(close(line));
    }

    let confidences: Vec<f64> = words.iter().map(|w| w.conf).filter(|c| *c >= 0.0).collect();
    let confidence = if confidences.is_empty() {
        None
    } else {
        #[expect(clippy::cast_precision_loss, reason = "word counts are small")]
        let mean = confidences.iter().sum::<f64>() / confidences.len() as f64;
        #[expect(clippy::cast_possible_truncation, reason = "value is in [0, 1]")]
        let scaled = (mean / 100.0).clamp(0.0, 1.0) as f32;
        Some(scaled)
    };
    Ok(PageText {
        page,
        method: ExtractionMethod::Ocr,
        engine: engine.to_owned(),
        lines,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";

    fn tsv(rows: &[&str]) -> String {
        std::iter::once(HEADER)
            .chain(rows.iter().copied())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn words_group_into_lines_with_boxes_in_points() {
        let input = tsv(&[
            "1\t1\t0\t0\t0\t0\t0\t0\t2480\t3508\t-1\t",
            "5\t1\t1\t1\t1\t1\t300\t600\t200\t50\t96.0\tHemoglobina",
            "5\t1\t1\t1\t1\t2\t520\t600\t20\t50\t90.0\t|",
            "5\t1\t1\t1\t1\t3\t560\t600\t100\t50\t94.0\t13,5",
            "5\t1\t1\t1\t2\t1\t300\t700\t200\t50\t-1\tHematócrito",
        ]);
        let page = parse(&input, NonZeroU32::MIN, 300, "tesseract test").expect("parse");
        assert_eq!(page.method, ExtractionMethod::Ocr);
        assert_eq!(page.lines.len(), 2);
        assert_eq!(page.lines[0].text, "Hemoglobina | 13,5");
        let bbox = page.lines[0].bounding_box.clone().expect("bbox");
        approx::assert_abs_diff_eq!(bbox.x, 72.0, epsilon = 1e-4); // 300 px at 300 dpi
        approx::assert_abs_diff_eq!(bbox.y, 144.0, epsilon = 1e-4);
        approx::assert_abs_diff_eq!(bbox.w, 360.0 * 72.0 / 300.0, epsilon = 1e-4);
        // Mean of 96, 90, 94 (the -1 word is excluded) = 93.33 → 0.9333.
        approx::assert_abs_diff_eq!(page.confidence.expect("conf"), 0.933_333, epsilon = 1e-5);
    }

    #[test]
    fn wide_column_gaps_become_double_spaces_for_aligned_tables() {
        // Columns at x = 300, 900, 1200, 1500 px with 40 px tall words: gaps far wider
        // than the word height. A normal inter-word gap (Glicemia de jejum) stays single.
        let input = tsv(&[
            "5\t1\t1\t1\t1\t1\t300\t600\t280\t40\t95\tHemoglobina",
            "5\t1\t1\t1\t1\t2\t900\t600\t90\t40\t95\t13,5",
            "5\t1\t1\t1\t1\t3\t1200\t600\t90\t40\t95\tg/dL",
            "5\t1\t1\t1\t1\t4\t1500\t600\t200\t40\t95\t12,0–16,0",
            "5\t1\t1\t1\t2\t1\t300\t700\t200\t40\t95\tGlicemia",
            "5\t1\t1\t1\t2\t2\t515\t700\t40\t40\t95\tde",
            "5\t1\t1\t1\t2\t3\t570\t700\t120\t40\t95\tjejum",
        ]);
        let page = parse(&input, NonZeroU32::MIN, 300, "t").expect("parse");
        assert_eq!(page.lines[0].text, "Hemoglobina  13,5  g/dL  12,0–16,0");
        assert_eq!(page.lines[1].text, "Glicemia de jejum");
        let chunker = crate::chunking::StructureAwareV1::default();
        let chunks = chunker
            .chunk(
                &page,
                &crate::embedding::deterministic::WhitespaceTokenCounter,
                None,
            )
            .expect("chunk");
        assert_eq!(
            chunks[0].kind,
            crate::chunking::ChunkKind::TableRow,
            "aligned OCR row is a table row"
        );
    }

    #[test]
    fn header_only_output_is_a_blank_page_without_confidence() {
        let page = parse(HEADER, NonZeroU32::MIN, 300, "t").expect("parse");
        assert!(page.is_blank());
        assert_eq!(page.confidence, None);
    }

    #[test]
    fn malformed_input_is_rejected() {
        assert!(parse("not a tsv", NonZeroU32::MIN, 300, "t").is_err());
        assert!(
            parse(
                &tsv(&["5\t1\tx\t1\t1\t1\t0\t0\t1\t1\t90\tw"]),
                NonZeroU32::MIN,
                300,
                "t"
            )
            .is_err()
        );
    }
}
