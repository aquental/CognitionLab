//! Structure-aware chunking (PRD §6.3, PLAN Phase 1 step 3).
//!
//! `structure-aware-v1` walks the lines of a page:
//!
//! * a short title line (≤ 4 words, no digits, `|` or `:`) starts a **section**;
//! * a table line with ≥ 3 cells (`|`-separated or split by 2+ spaces) and no digits
//!   is the **table header**, and one with a digit is a **row**. Each row becomes its
//!   own chunk carrying the section and header, so the test name, value, unit, and
//!   reference range stay together with their column names;
//! * other lines accumulate into **paragraphs**, flushed at blank lines, headings, and
//!   tables, and prefixed with the current section.
//!
//! Token budgets are measured with the model's tokenizer. Oversized paragraphs are
//! split at line and then word boundaries. A table row is indivisible: it is truncated
//! (and the truncation recorded) only when truncation is enabled, otherwise chunking
//! fails for the source.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::domain::ExtractionMethod;
use crate::embedding::TokenCounter;
use crate::extraction::{BoundingBox, PageText};

/// Identifier of this strategy, recorded in every record.
pub const STRATEGY_ID: &str = "structure-aware-v1";

/// Kind of structural unit a chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkKind {
    /// Running text.
    Paragraph,
    /// One laboratory table row with its section and header.
    TableRow,
}

/// A recorded truncation, in model tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Truncation {
    /// Tokens before truncation.
    pub original_tokens: usize,
    /// Tokens actually submitted.
    pub submitted_tokens: usize,
}

/// One chunk of a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    /// Page the chunk comes from.
    pub page: NonZeroU32,
    /// 0-based order of the chunk within its page.
    pub index: u32,
    /// Structural kind.
    pub kind: ChunkKind,
    /// Section heading in effect, if any.
    pub section: Option<String>,
    /// Text submitted for embedding (after any truncation).
    pub text: String,
    /// Union of the line boxes, when the extractor provides positions.
    pub bounding_box: Option<BoundingBox>,
    /// How the page text was obtained.
    pub extraction_method: ExtractionMethod,
    /// Extraction tool and version.
    pub extraction_engine: String,
    /// Page-level extraction confidence, when reported.
    pub extraction_confidence: Option<f32>,
    /// Present when the chunk was truncated.
    pub truncation: Option<Truncation>,
}

/// Chunking settings; their canonical JSON is part of the chunking fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkingConfig {
    /// Strategy identifier.
    pub strategy: String,
    /// Tokens shared between consecutive chunks (always 0 for this strategy).
    pub overlap_tokens: usize,
    /// Whether an indivisible unit above the budget may be truncated.
    pub truncation_enabled: bool,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            strategy: STRATEGY_ID.to_owned(),
            overlap_tokens: 0,
            truncation_enabled: false,
        }
    }
}

/// Chunking failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChunkError {
    /// An indivisible unit exceeds the token budget and truncation is disabled.
    #[error("page {page}: indivisible unit has {tokens} tokens, above the input limit of {limit}")]
    InputTooLong {
        /// Page of the unit.
        page: NonZeroU32,
        /// Token count of the unit.
        tokens: usize,
        /// Token budget.
        limit: usize,
    },
    /// The configured strategy is not implemented.
    #[error("unknown chunking strategy {0:?}")]
    UnknownStrategy(String),
}

/// A chunk before page-level numbering.
struct Piece {
    kind: ChunkKind,
    section: Option<String>,
    text: String,
    bbox: Option<BoundingBox>,
    truncation: Option<Truncation>,
}

enum LineKind {
    Blank,
    Heading,
    TableHeader,
    TableRow,
    Text,
}

fn cells(line: &str) -> Vec<&str> {
    let trimmed = line.trim();
    let parts: Vec<&str> = if trimmed.contains('|') {
        trimmed.split('|').map(str::trim).collect()
    } else {
        trimmed
            .split("  ")
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect()
    };
    parts.into_iter().filter(|c| !c.is_empty()).collect()
}

fn classify(line: &str) -> LineKind {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LineKind::Blank;
    }
    let cells = cells(trimmed);
    if cells.len() >= 3 {
        return if trimmed.chars().any(|c| c.is_ascii_digit()) {
            LineKind::TableRow
        } else {
            LineKind::TableHeader
        };
    }
    let words = trimmed.split_whitespace().count();
    let starts_upper = trimmed.chars().next().is_some_and(char::is_uppercase);
    if words <= 4
        && trimmed.chars().count() <= 40
        && starts_upper
        && !trimmed
            .chars()
            .any(|c| c.is_ascii_digit() || c == '|' || c == ':')
    {
        return LineKind::Heading;
    }
    LineKind::Text
}

struct Unit {
    kind: ChunkKind,
    section: Option<String>,
    lines: Vec<String>,
    bbox: Option<BoundingBox>,
}

fn merge_bbox(acc: Option<BoundingBox>, next: Option<&BoundingBox>) -> Option<BoundingBox> {
    match (acc, next) {
        (Some(a), Some(b)) => Some(a.union(b)),
        (None, Some(b)) => Some(b.clone()),
        (a, None) => a,
    }
}

fn with_section(section: Option<&str>, body: &str) -> String {
    match section {
        Some(s) => format!("{s}\n{body}"),
        None => body.to_owned(),
    }
}

/// Longest prefix of `text`, cut at a whitespace boundary (any whitespace, keeping the
/// original separators), that fits `budget` tokens.
fn truncate_to_budget(text: &str, budget: usize, counter: &dyn TokenCounter) -> String {
    let pieces: Vec<&str> = text.split_inclusive(char::is_whitespace).collect();
    let prefix = |n: usize| pieces[..n].concat().trim_end().to_owned();
    let (mut lo, mut hi) = (0usize, pieces.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if counter.count(&prefix(mid)) <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    prefix(lo)
}

/// Splits paragraph lines into pieces that each fit `budget` with the section prefix.
fn split_paragraph(
    section: Option<&str>,
    lines: &[String],
    budget: usize,
    counter: &dyn TokenCounter,
) -> Vec<String> {
    let fits = |body: &str| counter.count(&with_section(section, body)) <= budget;
    let mut pieces = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in lines {
        let candidate = current
            .iter()
            .chain(std::iter::once(line))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if fits(&candidate) {
            current.push(line.clone());
            continue;
        }
        if !current.is_empty() {
            pieces.push(current.join("\n"));
            current.clear();
        }
        if fits(line) {
            current.push(line.clone());
            continue;
        }
        // The line alone is too long: split it at word boundaries.
        let mut words: Vec<&str> = Vec::new();
        for word in line.split(' ') {
            words.push(word);
            if !fits(&words.join(" ")) && words.len() > 1 {
                words.pop();
                pieces.push(words.join(" "));
                words = vec![word];
            }
        }
        if !words.is_empty() {
            current.push(words.join(" "));
        }
    }
    if !current.is_empty() {
        pieces.push(current.join("\n"));
    }
    pieces
        .into_iter()
        .map(|body| with_section(section, &body))
        .collect()
}

/// The `structure-aware-v1` strategy.
#[derive(Debug, Clone, Default)]
pub struct StructureAwareV1 {
    config: ChunkingConfig,
}

impl StructureAwareV1 {
    /// Creates the strategy.
    ///
    /// # Errors
    ///
    /// Returns [`ChunkError::UnknownStrategy`] when `config.strategy` names another strategy.
    pub fn new(config: ChunkingConfig) -> Result<Self, ChunkError> {
        if config.strategy != STRATEGY_ID {
            return Err(ChunkError::UnknownStrategy(config.strategy));
        }
        Ok(Self { config })
    }

    /// The configuration in effect.
    #[must_use]
    pub fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn units(page: &PageText) -> Vec<Unit> {
        let mut units = Vec::new();
        let mut section: Option<String> = None;
        let mut header: Option<String> = None;
        let mut paragraph: Option<Unit> = None;
        let flush = |paragraph: &mut Option<Unit>, units: &mut Vec<Unit>| {
            if let Some(unit) = paragraph.take() {
                units.push(unit);
            }
        };
        for line in &page.lines {
            let text = line.text.trim();
            match classify(text) {
                LineKind::Blank => {
                    flush(&mut paragraph, &mut units);
                    header = None;
                }
                LineKind::Heading => {
                    flush(&mut paragraph, &mut units);
                    section = Some(text.to_owned());
                    header = None;
                }
                LineKind::TableHeader => {
                    flush(&mut paragraph, &mut units);
                    header = Some(text.to_owned());
                }
                LineKind::TableRow => {
                    flush(&mut paragraph, &mut units);
                    let lines = header
                        .iter()
                        .cloned()
                        .chain(std::iter::once(text.to_owned()))
                        .collect();
                    units.push(Unit {
                        kind: ChunkKind::TableRow,
                        section: section.clone(),
                        lines,
                        bbox: line.bounding_box.clone(),
                    });
                }
                LineKind::Text => {
                    let unit = paragraph.get_or_insert_with(|| Unit {
                        kind: ChunkKind::Paragraph,
                        section: section.clone(),
                        lines: Vec::new(),
                        bbox: None,
                    });
                    unit.lines.push(text.to_owned());
                    unit.bbox = merge_bbox(unit.bbox.take(), line.bounding_box.as_ref());
                }
            }
        }
        flush(&mut paragraph, &mut units);
        units
    }

    /// Chunks one page. `budget` is the model's token budget for raw document text
    /// (already net of prefix and special tokens); `None` means unlimited.
    ///
    /// # Errors
    ///
    /// Returns [`ChunkError::InputTooLong`] when a table row exceeds the budget and
    /// truncation is disabled.
    pub fn chunk(
        &self,
        page: &PageText,
        counter: &dyn TokenCounter,
        budget: Option<usize>,
    ) -> Result<Vec<Chunk>, ChunkError> {
        let mut texts: Vec<Piece> = Vec::new();
        for unit in Self::units(page) {
            let body = unit.lines.join("\n");
            let full = with_section(unit.section.as_deref(), &body);
            let tokens = counter.count(&full);
            match budget {
                Some(limit) if tokens > limit => match unit.kind {
                    ChunkKind::Paragraph => {
                        for piece in
                            split_paragraph(unit.section.as_deref(), &unit.lines, limit, counter)
                        {
                            // A piece can still exceed the budget when a single word plus
                            // its section prefix is too long: it is indivisible.
                            let piece_tokens = counter.count(&piece);
                            let (text, truncation) = if piece_tokens <= limit {
                                (piece, None)
                            } else if self.config.truncation_enabled {
                                let cut = truncate_to_budget(&piece, limit, counter);
                                let submitted = counter.count(&cut);
                                (
                                    cut,
                                    Some(Truncation {
                                        original_tokens: piece_tokens,
                                        submitted_tokens: submitted,
                                    }),
                                )
                            } else {
                                return Err(ChunkError::InputTooLong {
                                    page: page.page,
                                    tokens: piece_tokens,
                                    limit,
                                });
                            };
                            texts.push(Piece {
                                kind: unit.kind,
                                section: unit.section.clone(),
                                text,
                                bbox: unit.bbox.clone(),
                                truncation,
                            });
                        }
                    }
                    ChunkKind::TableRow if self.config.truncation_enabled => {
                        let cut = truncate_to_budget(&full, limit, counter);
                        let truncation = Truncation {
                            original_tokens: tokens,
                            submitted_tokens: counter.count(&cut),
                        };
                        texts.push(Piece {
                            kind: unit.kind,
                            section: unit.section,
                            text: cut,
                            bbox: unit.bbox,
                            truncation: Some(truncation),
                        });
                    }
                    ChunkKind::TableRow => {
                        return Err(ChunkError::InputTooLong {
                            page: page.page,
                            tokens,
                            limit,
                        });
                    }
                },
                _ => texts.push(Piece {
                    kind: unit.kind,
                    section: unit.section,
                    text: full,
                    bbox: unit.bbox,
                    truncation: None,
                }),
            }
        }
        let mut chunks = Vec::with_capacity(texts.len());
        for (index, piece) in texts.into_iter().enumerate() {
            chunks.push(Chunk {
                page: page.page,
                index: u32::try_from(index).unwrap_or(u32::MAX),
                kind: piece.kind,
                section: piece.section,
                text: piece.text,
                bounding_box: piece.bbox,
                extraction_method: page.method,
                extraction_engine: page.engine.clone(),
                extraction_confidence: page.confidence,
                truncation: piece.truncation,
            });
        }
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::deterministic::WhitespaceTokenCounter;
    use crate::extraction::TextLine;

    fn page(lines: &[&str]) -> PageText {
        PageText {
            page: NonZeroU32::MIN,
            method: ExtractionMethod::Native,
            engine: "test".into(),
            lines: lines
                .iter()
                .map(|l| TextLine {
                    text: (*l).to_owned(),
                    bounding_box: None,
                })
                .collect(),
            confidence: None,
        }
    }

    fn chunker(truncate: bool) -> StructureAwareV1 {
        StructureAwareV1::new(ChunkingConfig {
            truncation_enabled: truncate,
            ..ChunkingConfig::default()
        })
        .expect("strategy")
    }

    const LAB: &[&str] = &[
        "Hemograma",
        "Exame | Resultado | Unidade | Intervalo de referência",
        "Hemoglobina | 13,5 | g/dL | 12,0–16,0",
        "Hematócrito | 40,1 | % | 36,0–46,0",
    ];

    #[test]
    fn lab_rows_keep_name_value_unit_and_range_with_header_and_section() {
        let chunks = chunker(false)
            .chunk(&page(LAB), &WhitespaceTokenCounter, None)
            .expect("chunk");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].kind, ChunkKind::TableRow);
        assert_eq!(chunks[0].section.as_deref(), Some("Hemograma"));
        assert_eq!(
            chunks[0].text,
            "Hemograma\nExame | Resultado | Unidade | Intervalo de referência\nHemoglobina | 13,5 | g/dL | 12,0–16,0"
        );
        assert_eq!((chunks[0].index, chunks[1].index), (0, 1));
    }

    #[test]
    fn ocr_style_rows_with_hyphen_ranges_and_aligned_columns_are_rows() {
        for row in [
            "Hemoglobina | 13,5 | g/dL | 12,0-16,0",
            "Hemoglobina   13,5   g/dL   12,0 a 16,0",
            "Colesterol total | 182 | mg/dL | < 190",
        ] {
            let chunks = chunker(false)
                .chunk(&page(&["Hemograma", row]), &WhitespaceTokenCounter, None)
                .expect("chunk");
            assert_eq!(chunks.len(), 1, "{row}");
            assert_eq!(chunks[0].kind, ChunkKind::TableRow, "{row}");
            assert!(chunks[0].text.ends_with(row));
        }
    }

    #[test]
    fn paragraphs_carry_their_section_and_split_at_blank_lines() {
        let lines = [
            "Técnica",
            "Exame realizado com transdutor convexo de 3,5 MHz.",
            "",
            "Conclusão",
            "Ultrassonografia abdominal sem alterações significativas.",
        ];
        let chunks = chunker(false)
            .chunk(&page(&lines), &WhitespaceTokenCounter, None)
            .expect("chunk");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1].section.as_deref(), Some("Conclusão"));
        assert_eq!(
            chunks[1].text,
            "Conclusão\nUltrassonografia abdominal sem alterações significativas."
        );
    }

    #[test]
    fn oversized_paragraphs_are_split_to_fit_without_truncation() {
        let long = "palavra ".repeat(25);
        let lines = ["Achados", long.trim()];
        let chunks = chunker(false)
            .chunk(&page(&lines), &WhitespaceTokenCounter, Some(10))
            .expect("chunk");
        assert!(chunks.len() >= 3);
        for chunk in &chunks {
            assert!(
                WhitespaceTokenCounter.count(&chunk.text) <= 10,
                "{:?}",
                chunk.text
            );
            assert!(chunk.truncation.is_none());
            assert!(chunk.text.starts_with("Achados\n"));
        }
        let words: usize = chunks
            .iter()
            .map(|c| WhitespaceTokenCounter.count(&c.text) - 1)
            .sum();
        assert_eq!(words, 25, "no word is lost when splitting");
    }

    #[test]
    fn oversized_rows_fail_when_truncation_is_disabled() {
        let err = chunker(false)
            .chunk(&page(LAB), &WhitespaceTokenCounter, Some(8))
            .expect_err("too long");
        assert!(
            matches!(err, ChunkError::InputTooLong { limit: 8, .. }),
            "{err}"
        );
    }

    #[test]
    fn oversized_rows_are_truncated_and_recorded_when_enabled() {
        let chunks = chunker(true)
            .chunk(&page(LAB), &WhitespaceTokenCounter, Some(12))
            .expect("chunk");
        let truncated: Vec<_> = chunks.iter().filter_map(|c| c.truncation).collect();
        assert!(!truncated.is_empty());
        for chunk in &chunks {
            assert!(WhitespaceTokenCounter.count(&chunk.text) <= 12);
        }
        let first = chunks[0].truncation.expect("truncated");
        assert_eq!(
            first.original_tokens,
            WhitespaceTokenCounter.count(&LAB[..3].join("\n"))
        );
        assert_eq!(first.submitted_tokens, 12);
    }

    #[test]
    fn an_indivisible_paragraph_fails_or_is_truncated_like_a_row() {
        let lines = ["Achados", "texto"];
        let err = chunker(false)
            .chunk(&page(&lines), &WhitespaceTokenCounter, Some(1))
            .expect_err("too long");
        assert!(
            matches!(
                err,
                ChunkError::InputTooLong {
                    tokens: 2,
                    limit: 1,
                    ..
                }
            ),
            "{err}"
        );
        let chunks = chunker(true)
            .chunk(&page(&lines), &WhitespaceTokenCounter, Some(1))
            .expect("chunk");
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0].truncation,
            Some(Truncation {
                original_tokens: 2,
                submitted_tokens: 1
            })
        );
        assert!(WhitespaceTokenCounter.count(&chunks[0].text) <= 1);
    }

    #[test]
    fn unknown_strategy_is_rejected() {
        let config = ChunkingConfig {
            strategy: "fixed-512".into(),
            ..ChunkingConfig::default()
        };
        assert!(matches!(
            StructureAwareV1::new(config),
            Err(ChunkError::UnknownStrategy(_))
        ));
    }
}
