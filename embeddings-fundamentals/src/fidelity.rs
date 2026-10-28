//! Extraction fidelity (PRD §9.4): field-level accuracy of extracted laboratory rows
//! against a frozen, checked-in sample.
//!
//! For each sample row, the extracted row is the table line on the same source page
//! whose first cell equals the expected test name; when no line has that name (an OCR
//! error in the name), the line whose second cell equals the expected value is used, so
//! the other fields are still judged. A field is correct when its cell equals the
//! expected text after Unicode NFC normalization, whitespace collapsing, and
//! case-folding of the test name only. Accuracy is reported per field, overall, and per
//! extraction method (native text vs OCR).
//!
//! The input is the **extraction output** of the sampled pages (native text or OCR, as
//! the pipeline selects per page), never the stored chunks: chunking budgets and
//! truncation must not change an extraction-fidelity figure.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::domain::ExtractionMethod;
use crate::extraction::PageText;
use crate::ids::{Checksum, SourceId};

/// One extracted page of a sampled source.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedPage {
    /// Source the page belongs to.
    pub source_id: SourceId,
    /// Extraction output of the page.
    pub page: PageText,
}

/// One expected laboratory row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleRow {
    /// Source document.
    pub source_id: SourceId,
    /// 1-based page.
    pub page: u32,
    /// Test name.
    pub test_name: String,
    /// Result value.
    pub value: String,
    /// Unit.
    pub unit: String,
    /// Reference range.
    pub reference_range: String,
}

/// The frozen fidelity sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    /// Version label.
    pub version: String,
    /// Corpus the sample was taken from.
    pub corpus: String,
    /// Rows.
    #[serde(rename = "row")]
    pub rows: Vec<SampleRow>,
}

/// Errors loading a sample.
#[derive(Debug, thiserror::Error)]
pub enum FidelityError {
    /// The file cannot be read.
    #[error("cannot read fidelity sample {path}: {source}")]
    Read {
        /// Path.
        path: String,
        /// I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file is invalid.
    #[error("invalid fidelity sample {path}: {source}")]
    Parse {
        /// Path.
        path: String,
        /// Parse error.
        #[source]
        source: toml::de::Error,
    },
}

/// A sample and the checksum of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSample {
    /// Parsed sample.
    pub sample: Sample,
    /// Checksum of the file bytes.
    pub checksum: Checksum,
}

impl LoadedSample {
    /// Loads a sample file.
    ///
    /// # Errors
    ///
    /// Returns [`FidelityError`] for unreadable or invalid files.
    pub fn load(path: &Path) -> Result<Self, FidelityError> {
        let shown = path.display().to_string();
        let bytes = std::fs::read(path).map_err(|source| FidelityError::Read {
            path: shown.clone(),
            source,
        })?;
        let sample = toml::from_str(&String::from_utf8_lossy(&bytes)).map_err(|source| {
            FidelityError::Parse {
                path: shown,
                source,
            }
        })?;
        Ok(Self {
            sample,
            checksum: Checksum::of_bytes(&bytes),
        })
    }
}

/// Field of a laboratory row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// Test name.
    TestName,
    /// Result value.
    Value,
    /// Unit.
    Unit,
    /// Reference range.
    ReferenceRange,
}

const FIELDS: [Field; 4] = [
    Field::TestName,
    Field::Value,
    Field::Unit,
    Field::ReferenceRange,
];

/// Correct over total for one group of fields.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Accuracy {
    /// Correct fields.
    pub correct: usize,
    /// Fields judged.
    pub total: usize,
    /// `correct / total` (0 when nothing was judged).
    pub accuracy: f64,
}

impl Accuracy {
    fn add(&mut self, ok: bool) {
        self.total += 1;
        if ok {
            self.correct += 1;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "sample sizes are tiny (tens of rows)"
        )]
        let ratio = self.correct as f64 / self.total as f64;
        self.accuracy = ratio;
    }
}

/// One field that did not match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mismatch {
    /// Source document.
    pub source_id: SourceId,
    /// Page.
    pub page: u32,
    /// Expected test name (identifies the row).
    pub row: String,
    /// Field.
    pub field: Field,
    /// Expected text.
    pub expected: String,
    /// Extracted text, or `None` when no row was found.
    pub extracted: Option<String>,
}

/// Fidelity of one extraction configuration over the sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FidelityReport {
    /// Sample version.
    pub sample_version: String,
    /// Sample checksum.
    pub sample_checksum: Checksum,
    /// Corpus of the sample.
    pub corpus: String,
    /// Rows in the sample.
    pub rows: usize,
    /// Rows for which no extracted line was found.
    pub rows_not_found: usize,
    /// Accuracy over every field.
    pub overall: Accuracy,
    /// Accuracy per field.
    pub by_field: BTreeMap<Field, Accuracy>,
    /// Accuracy per extraction method of the page.
    pub by_method: BTreeMap<String, Accuracy>,
    /// Fields that did not match.
    pub mismatches: Vec<Mismatch>,
}

fn norm(s: &str) -> String {
    s.nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn cells(line: &str) -> Vec<String> {
    line.split('|').map(norm).collect()
}

/// Evaluates `sample` against the extracted `pages` of one extraction configuration.
#[must_use]
pub fn evaluate(sample: &LoadedSample, pages: &[ExtractedPage]) -> FidelityReport {
    let mut by_field: BTreeMap<Field, Accuracy> = BTreeMap::new();
    let mut by_method: BTreeMap<String, Accuracy> = BTreeMap::new();
    let mut overall = Accuracy::default();
    let mut mismatches = Vec::new();
    let mut not_found = 0;
    // Lines already scored, per page, so no extracted line counts for two sample rows.
    let mut used: std::collections::BTreeMap<_, std::collections::BTreeSet<usize>> =
        std::collections::BTreeMap::new();
    for row in &sample.sample.rows {
        let page = pages
            .iter()
            .find(|p| p.source_id == row.source_id && p.page.page.get() == row.page)
            .map(|p| &p.page);
        let method = page
            .map_or("missing", |p| match p.method {
                ExtractionMethod::Native => "native",
                ExtractionMethod::Ocr => "ocr",
            })
            .to_owned();
        let lines: Vec<Vec<String>> = page
            .map(|p| p.lines.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|l| l.text.as_str())
            .filter(|l| l.contains('|'))
            .map(cells)
            .collect();
        let name = norm(&row.test_name).to_lowercase();
        let value = norm(&row.value);
        // Match by test name first. The value is only a fallback when exactly one line on
        // the page carries it and no other sample row already claimed that line, so one
        // extracted line is never scored as two rows.
        let by_name = lines
            .iter()
            .position(|c| c.first().is_some_and(|n| n.to_lowercase() == name));
        let by_value = || {
            let mut matches = lines
                .iter()
                .enumerate()
                .filter(|(_, c)| c.get(1) == Some(&value))
                .map(|(i, _)| i);
            match (matches.next(), matches.next()) {
                (Some(i), None) => Some(i),
                _ => None,
            }
        };
        let page_key = (row.source_id.clone(), row.page);
        let index = by_name
            .or_else(by_value)
            .filter(|i| used.entry(page_key.clone()).or_default().insert(*i));
        let found = index.and_then(|i| lines.get(i));
        if found.is_none() {
            not_found += 1;
        }
        for field in FIELDS {
            let (expected, index) = match field {
                Field::TestName => (&row.test_name, 0),
                Field::Value => (&row.value, 1),
                Field::Unit => (&row.unit, 2),
                Field::ReferenceRange => (&row.reference_range, 3),
            };
            let extracted = found.and_then(|c| c.get(index)).cloned();
            let ok = extracted.as_ref().is_some_and(|e| {
                if field == Field::TestName {
                    e.to_lowercase() == norm(expected).to_lowercase()
                } else {
                    *e == norm(expected)
                }
            });
            by_field.entry(field).or_default().add(ok);
            by_method.entry(method.clone()).or_default().add(ok);
            overall.add(ok);
            if !ok {
                mismatches.push(Mismatch {
                    source_id: row.source_id.clone(),
                    page: row.page,
                    row: row.test_name.clone(),
                    field,
                    expected: expected.clone(),
                    extracted,
                });
            }
        }
    }
    FidelityReport {
        sample_version: sample.sample.version.clone(),
        sample_checksum: sample.checksum.clone(),
        corpus: sample.sample.corpus.clone(),
        rows: sample.sample.rows.len(),
        rows_not_found: not_found,
        overall,
        by_field,
        by_method,
        mismatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_normalized_and_rows_found_by_value_when_the_name_is_wrong() {
        assert_eq!(
            cells("Hemoglobina  | 13,5 |g/dL| 12,0–16,0"),
            ["Hemoglobina", "13,5", "g/dL", "12,0–16,0"]
        );
        // Decomposed "ó" (o + combining acute) equals the precomposed form after NFC.
        assert_eq!(norm("Hemato\u{301}crito"), norm("Hematócrito"));
    }

    fn page(text: &str) -> ExtractedPage {
        ExtractedPage {
            source_id: "lab-searchable-001".parse().expect("source"),
            page: PageText {
                page: std::num::NonZeroU32::MIN,
                method: ExtractionMethod::Native,
                engine: "test".into(),
                lines: text
                    .lines()
                    .map(|l| crate::extraction::TextLine {
                        text: l.to_owned(),
                        bounding_box: None,
                    })
                    .collect(),
                confidence: None,
            },
        }
    }

    fn sample(rows: &[(&str, &str, &str, &str)]) -> LoadedSample {
        let mut text = String::from("version = \"t\"\ncorpus = \"t\"\n");
        for (name, value, unit, range) in rows {
            text.push_str(&format!(
                "[[row]]\nsource_id = \"lab-searchable-001\"\npage = 1\ntest_name = \"{name}\"\nvalue = \"{value}\"\nunit = \"{unit}\"\nreference_range = \"{range}\"\n"
            ));
        }
        LoadedSample {
            sample: toml::from_str(&text).expect("sample"),
            checksum: Checksum::of_bytes(text.as_bytes()),
        }
    }

    #[test]
    fn one_extracted_line_is_never_scored_for_two_rows_with_the_same_value() {
        // Regression (G2-5): both rows' names are unreadable in the extraction and both
        // values are 92, so the value fallback is ambiguous and neither row binds.
        let records = [page("Xxx | 92 | mg/dL | 70–99\nYyy | 92 | g/dL | 1–2")];
        let report = evaluate(
            &sample(&[
                ("Glicemia", "92", "mg/dL", "70–99"),
                ("Outro", "92", "g/dL", "1–2"),
            ]),
            &records,
        );
        assert_eq!(report.rows_not_found, 2, "{report:?}");
        assert_eq!(report.overall.correct, 0);
    }

    #[test]
    fn a_unique_value_still_binds_and_a_claimed_line_is_not_reused() {
        let records = [page(
            "Xxx | 92 | mg/dL | 70–99\nCreatinina | 0,9 | mg/dL | 0,6–1,2",
        )];
        // Row 1 binds by its unique value; row 2 by name; row 3 names row 2's line again.
        let report = evaluate(
            &sample(&[
                ("Glicemia", "92", "mg/dL", "70–99"),
                ("Creatinina", "0,9", "mg/dL", "0,6–1,2"),
                ("Creatinina", "0,9", "mg/dL", "0,6–1,2"),
            ]),
            &records,
        );
        assert_eq!(report.rows_not_found, 1, "{report:?}");
        // Row 1: name wrong, value/unit/range right (3); row 2: all 4 right.
        assert_eq!(report.overall.correct, 7);
    }

    #[test]
    fn the_checked_in_sample_loads() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("config/fidelity/sample-synthetic-v1.toml");
        let sample = LoadedSample::load(&path).expect("sample");
        assert_eq!(sample.sample.rows.len(), 16);
        let report = evaluate(&sample, &[]);
        assert_eq!(report.rows_not_found, 16);
        assert_eq!(report.overall.total, 64);
        assert_eq!(report.overall.correct, 0);
        assert_eq!(report.by_method.get("missing").map(|a| a.total), Some(64));
    }
}
