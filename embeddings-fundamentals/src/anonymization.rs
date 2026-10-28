//! Helper for the anonymization review (PRD §11, PLAN §4.4).
//!
//! It reports what a reviewer must inspect: identifier-like filenames, PDF information
//! dictionary and XMP metadata, and image EXIF tags. It never decides that a source is
//! anonymized. Only the reviewer's checks in the manifest make a source eligible.

use std::path::Path;

use regex::Regex;
use serde::Serialize;

/// Something a reviewer has to look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Where the finding is: `filename`, `pdf-info`, `pdf-xmp`, or `exif`.
    pub area: &'static str,
    /// Human-readable description (keys or patterns, never the values).
    pub detail: String,
}

/// PDF information-dictionary keys that commonly carry identifying text.
const RISKY_PDF_KEYS: &[&str] = &["Author", "Title", "Subject", "Keywords", "Creator"];

/// Flags filename patterns that look like identifiers (CPF, dates, two capitalized
/// words). The match itself is not reported.
#[must_use]
pub fn filename_findings(file: &Path) -> Vec<Finding> {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let patterns: [(&str, &str); 3] = [
        (r"\d{3}\.?\d{3}\.?\d{3}-?\d{2}", "CPF-like number"),
        (
            r"\d{2}[/._-]\d{2}[/._-]\d{4}|\d{4}-\d{2}-\d{2}",
            "date-like sequence",
        ),
        (
            r"\b[A-ZÁÉÍÓÚÂÊÔÃÕÇ][a-záéíóúâêôãõç]+[ _-][A-ZÁÉÍÓÚÂÊÔÃÕÇ][a-záéíóúâêôãõç]+\b",
            "capitalized name pair",
        ),
    ];
    patterns
        .iter()
        .filter(|(pattern, _)| Regex::new(pattern).is_ok_and(|re| re.is_match(&name)))
        .map(|(_, label)| Finding {
            area: "filename",
            detail: format!("filename contains a {label}"),
        })
        .collect()
}

/// Lists risky PDF information keys and the presence of XMP metadata.
#[must_use]
pub fn pdf_findings(path: &Path) -> Vec<Finding> {
    let Ok(doc) = lopdf::Document::load(path) else {
        return vec![Finding {
            area: "pdf-info",
            detail: "PDF cannot be parsed; review manually".to_owned(),
        }];
    };
    let mut findings = Vec::new();
    if let Ok(info) = doc
        .trailer
        .get(b"Info")
        .and_then(|o| doc.dereference(o))
        .and_then(|(_, o)| o.as_dict())
    {
        for key in RISKY_PDF_KEYS {
            if info.has(key.as_bytes()) {
                findings.push(Finding {
                    area: "pdf-info",
                    detail: format!("information dictionary has /{key}"),
                });
            }
        }
    }
    let has_xmp = doc.catalog().is_ok_and(|catalog| catalog.has(b"Metadata"));
    if has_xmp {
        findings.push(Finding {
            area: "pdf-xmp",
            detail: "catalog has an XMP /Metadata stream".to_owned(),
        });
    }
    findings
}

/// Lists EXIF tags present in an image (tag names only).
#[must_use]
pub fn image_findings(path: &Path) -> Vec<Finding> {
    let Ok(file) = std::fs::File::open(path) else {
        return vec![Finding {
            area: "exif",
            detail: "image cannot be opened; review manually".to_owned(),
        }];
    };
    let mut reader = std::io::BufReader::new(file);
    match exif::Reader::new().read_from_container(&mut reader) {
        Ok(exif) => {
            let tags: Vec<String> = exif.fields().map(|f| f.tag.to_string()).collect();
            if tags.is_empty() {
                Vec::new()
            } else {
                vec![Finding {
                    area: "exif",
                    detail: format!("EXIF tags present: {}", tags.join(", ")),
                }]
            }
        }
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_like_filenames_are_flagged() {
        assert_eq!(filename_findings(Path::new("123.456.789-09.pdf")).len(), 1);
        assert_eq!(
            filename_findings(Path::new("laudo-12-03-2024.pdf")).len(),
            1
        );
        assert_eq!(filename_findings(Path::new("Maria Silva.pdf")).len(), 1);
        assert!(filename_findings(Path::new("lab-searchable-001.pdf")).is_empty());
    }

    #[test]
    fn synthetic_fixtures_have_no_risky_metadata() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus");
        assert!(pdf_findings(&corpus.join("lab-searchable-001.pdf")).is_empty());
        assert!(image_findings(&corpus.join("chest-001.png")).is_empty());
        assert!(image_findings(&corpus.join("abd-us-001.jpg")).is_empty());
    }
}
