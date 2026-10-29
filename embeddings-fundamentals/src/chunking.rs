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

/// Revision of the `structure-aware-v1` rules, part of the chunking fingerprint: a rule
/// change that alters chunk output must change revision ids, or an unchanged source
/// would keep chunks made by the old rules. `r2` keeps a table header that no row used
/// as a paragraph instead of dropping it (found on real sample exams).
pub const RULES_ID: &str = "structure-aware-v1/r2";

/// Kind of structural unit a chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkKind {
    /// Running text.
    Paragraph,
    /// One laboratory table row with its section and header.
    TableRow,
    /// One image (image records; never produced by the text chunker).
    Image,
}

/// The kinds of unit the text chunker produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextKind {
    Paragraph,
    TableRow,
}

impl From<TextKind> for ChunkKind {
    fn from(kind: TextKind) -> Self {
        match kind {
            TextKind::Paragraph => Self::Paragraph,
            TextKind::TableRow => Self::TableRow,
        }
    }
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
    /// This strategy does not implement overlap, so recording one would be false.
    #[error("{STRATEGY_ID} does not support overlap_tokens = {0}; use 0")]
    UnsupportedOverlap(usize),
}

/// A chunk before page-level numbering.
struct Piece {
    kind: TextKind,
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

/// Whole-cell column labels of lab tables (lowercase).
const HEADER_LABELS: &[&str] = &[
    "exame",
    "exames",
    "analito",
    "parâmetro",
    "parametro",
    "resultado",
    "resultados",
    "valor",
    "valores",
    "unidade",
    "unidades",
    "referência",
    "referencia",
    "intervalo de referência",
    "intervalo de referencia",
    "valor de referência",
    "valor de referencia",
    "valores de referência",
    "valores de referencia",
    "método",
    "metodo",
    "material",
];

/// A digit-free table line opens a new header inside a table only when more than half of
/// its cells are, as whole cells, known column labels. Data rows that merely contain a
/// label word ("Exame parasitológico", "Imunidade") stay rows.
fn looks_like_header(line: &str) -> bool {
    let cells = cells(line);
    let labels = cells
        .iter()
        .filter(|c| HEADER_LABELS.contains(&c.to_lowercase().as_str()))
        .count();
    cells.len() >= 2 && labels * 2 > cells.len()
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
    let ends_sentence = trimmed.ends_with(['.', '!', '?', ';']);
    if words <= 4
        && trimmed.chars().count() <= 40
        && starts_upper
        && !ends_sentence
        && !trimmed
            .chars()
            .any(|c| c.is_ascii_digit() || c == '|' || c == ':')
    {
        return LineKind::Heading;
    }
    LineKind::Text
}

struct Unit {
    kind: TextKind,
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
        if config.overlap_tokens != 0 {
            return Err(ChunkError::UnsupportedOverlap(config.overlap_tokens));
        }
        Ok(Self { config })
    }

    /// The configuration in effect.
    #[must_use]
    pub fn config(&self) -> &ChunkingConfig {
        &self.config
    }

    fn units(page: &PageText) -> Vec<Unit> {
        /// Ends the current table header. A header no row used (a multi-column line
        /// without digits followed by text, a heading, or the end of the page) is kept
        /// as a paragraph, so its text is never dropped.
        fn release_header(
            header: &mut Option<String>,
            used: &mut bool,
            section: Option<&String>,
            heading_used: &mut bool,
            units: &mut Vec<Unit>,
        ) {
            if let (false, Some(text)) = (*used, header.take()) {
                *heading_used = true;
                units.push(Unit {
                    kind: TextKind::Paragraph,
                    section: section.cloned(),
                    lines: vec![text],
                    bbox: None,
                });
            }
            *used = true;
        }
        let mut units = Vec::new();
        let mut section: Option<String> = None;
        let mut header: Option<String> = None;
        let mut header_used = true;
        let mut paragraph: Option<Unit> = None;
        // A heading with no body under it (followed by another heading, or the last
        // content of the page) is kept as a paragraph so its text is never dropped.
        let mut heading_used = true;
        let mut previous_section: Option<String> = None;
        let flush = |paragraph: &mut Option<Unit>, units: &mut Vec<Unit>| {
            if let Some(unit) = paragraph.take() {
                units.push(unit);
            }
        };
        let keep_unused = |section: &Option<String>,
                           previous: &Option<String>,
                           used: bool,
                           units: &mut Vec<Unit>| {
            if let (false, Some(heading)) = (used, section) {
                units.push(Unit {
                    kind: TextKind::Paragraph,
                    section: previous.clone(),
                    lines: vec![heading.clone()],
                    bbox: None,
                });
            }
        };
        // Whether the previous non-blank line belonged to a table. Inside a table that
        // already has a header, a line without digits is a qualitative result row
        // ("Proteína | Negativo | Negativo"), not a new header.
        let mut in_table = false;
        for line in &page.lines {
            let text = line.text.trim();
            let kind = match classify(text) {
                LineKind::TableHeader
                    if in_table && header.is_some() && !looks_like_header(text) =>
                {
                    LineKind::TableRow
                }
                other => other,
            };
            in_table = match kind {
                LineKind::TableHeader | LineKind::TableRow => true,
                // A blank line inside a table does not end it.
                LineKind::Blank => in_table,
                LineKind::Heading | LineKind::Text => false,
            };
            match kind {
                LineKind::Blank => {
                    flush(&mut paragraph, &mut units);
                    if !in_table {
                        release_header(
                            &mut header,
                            &mut header_used,
                            section.as_ref(),
                            &mut heading_used,
                            &mut units,
                        );
                    }
                }
                LineKind::Heading => {
                    flush(&mut paragraph, &mut units);
                    release_header(
                        &mut header,
                        &mut header_used,
                        section.as_ref(),
                        &mut heading_used,
                        &mut units,
                    );
                    keep_unused(&section, &previous_section, heading_used, &mut units);
                    previous_section = section.take();
                    section = Some(text.to_owned());
                    heading_used = false;
                    header = None;
                }
                LineKind::TableHeader => {
                    flush(&mut paragraph, &mut units);
                    release_header(
                        &mut header,
                        &mut header_used,
                        section.as_ref(),
                        &mut heading_used,
                        &mut units,
                    );
                    header = Some(text.to_owned());
                    header_used = false;
                }
                LineKind::TableRow => {
                    flush(&mut paragraph, &mut units);
                    heading_used = true;
                    header_used = true;
                    let lines = header
                        .iter()
                        .cloned()
                        .chain(std::iter::once(text.to_owned()))
                        .collect();
                    units.push(Unit {
                        kind: TextKind::TableRow,
                        section: section.clone(),
                        lines,
                        bbox: line.bounding_box.clone(),
                    });
                }
                LineKind::Text => {
                    release_header(
                        &mut header,
                        &mut header_used,
                        section.as_ref(),
                        &mut heading_used,
                        &mut units,
                    );
                    heading_used = true;
                    let unit = paragraph.get_or_insert_with(|| Unit {
                        kind: TextKind::Paragraph,
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
        release_header(
            &mut header,
            &mut header_used,
            section.as_ref(),
            &mut heading_used,
            &mut units,
        );
        keep_unused(&section, &previous_section, heading_used, &mut units);
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
                    TextKind::Paragraph => {
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
                    TextKind::TableRow if self.config.truncation_enabled => {
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
                    TextKind::TableRow => {
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
                kind: piece.kind.into(),
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
    fn a_table_header_no_row_uses_is_kept_as_text() {
        // Regression (real sample exams): a multi-column line without digits is a table
        // header, and it was dropped when text, a heading, a blank line, another header,
        // or the end of the page followed it instead of a row.
        let cases: [&[&str]; 5] = [
            &[
                "Paciente  Convênio  Médico",
                "Texto livre do laudo segue aqui.",
            ],
            &["Exame | Método | Material", "Conclusão"],
            &[
                "Exame | Método | Material",
                "",
                "Texto depois da linha em branco.",
            ],
            &[
                "Campo | Valor | Nota",
                "Exame | Resultado | Unidade | Referência",
                "Hemoglobina | 13,5 | g/dL | 12,0–16,0",
            ],
            &["Achados", "Exame | Método | Material"],
        ];
        for lines in cases {
            let chunks = chunker(false)
                .chunk(&page(lines), &WhitespaceTokenCounter, None)
                .expect("chunk");
            let covered: std::collections::BTreeSet<&str> = chunks
                .iter()
                .flat_map(|c| c.text.split_whitespace())
                .collect();
            for line in lines {
                for token in line.split_whitespace() {
                    assert!(
                        covered.contains(token),
                        "{token:?} of {lines:?} was dropped"
                    );
                }
            }
        }
        // A header that rows use is still attached to them, not repeated as a paragraph.
        let chunks = chunker(false)
            .chunk(&page(LAB), &WhitespaceTokenCounter, None)
            .expect("chunk");
        assert!(
            chunks
                .iter()
                .all(|c| c.kind == ChunkKind::TableRow || !c.text.contains("Exame | Resultado")),
            "{chunks:?}"
        );
    }

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
    fn short_sentences_and_bodiless_headings_are_never_dropped() {
        let chunks = chunker(false)
            .chunk(
                &page(&["Conclusão", "Exame normal."]),
                &WhitespaceTokenCounter,
                Some(10),
            )
            .expect("chunk");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Conclusão\nExame normal.");
        let chunks = chunker(false)
            .chunk(
                &page(&["Achados", "Conclusão"]),
                &WhitespaceTokenCounter,
                None,
            )
            .expect("chunk");
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Achados", "Achados\nConclusão"],
            "both lines are indexed"
        );
    }

    #[test]
    fn qualitative_rows_are_rows_not_headers() {
        let lines = [
            "Urina tipo I",
            "Exame | Resultado | Referência",
            "Proteína | Negativo | Negativo",
            "pH | 6,0 | 5,0–7,0",
            "Glicose | Ausente | Ausente",
        ];
        let chunks = chunker(false)
            .chunk(&page(&lines), &WhitespaceTokenCounter, None)
            .expect("chunk");
        let rows: Vec<&str> = chunks
            .iter()
            .map(|c| c.text.lines().last().unwrap_or_default())
            .collect();
        assert_eq!(
            rows,
            [
                "Proteína | Negativo | Negativo",
                "pH | 6,0 | 5,0–7,0",
                "Glicose | Ausente | Ausente"
            ]
        );
        for chunk in &chunks {
            assert_eq!(chunk.kind, ChunkKind::TableRow);
            assert!(
                chunk.text.contains("Exame | Resultado | Referência"),
                "header kept: {:?}",
                chunk.text
            );
        }
    }

    #[test]
    fn blank_lines_inside_a_table_keep_its_header() {
        let lines = [
            "Exame | Resultado | Referência",
            "",
            "Proteína | Negativo | Negativo",
            "pH | 6,0 | 5,0–7,0",
            "",
            "Glicose | Ausente | Ausente",
        ];
        let chunks = chunker(false)
            .chunk(&page(&lines), &WhitespaceTokenCounter, None)
            .expect("chunk");
        assert_eq!(chunks.len(), 3, "{chunks:?}");
        for chunk in &chunks {
            assert_eq!(chunk.kind, ChunkKind::TableRow);
            assert!(
                chunk.text.starts_with("Exame | Resultado | Referência\n"),
                "{:?}",
                chunk.text
            );
        }
        // A new table after a blank line keeps its own header.
        let lines = [
            "Exame | Resultado | Unidade",
            "Hb | 13,5 | g/dL",
            "",
            "Exame | Valor | Unidade",
            "Ureia | 30 | mg/dL",
        ];
        let chunks = chunker(false)
            .chunk(&page(&lines), &WhitespaceTokenCounter, None)
            .expect("chunk");
        assert_eq!(chunks.len(), 2);
        assert!(
            chunks[1].text.starts_with("Exame | Valor | Unidade\n"),
            "{:?}",
            chunks[1].text
        );
    }

    #[test]
    fn data_rows_containing_label_words_are_not_headers() {
        for row in [
            "Exame parasitológico | Negativo | Negativo",
            "Método | Colorimétrico | —",
            "Imunidade | Reagente | Não reagente",
        ] {
            for lines in [
                vec!["Exame | Resultado | Referência", row],
                vec!["Exame | Resultado | Referência", "", row],
            ] {
                let chunks = chunker(false)
                    .chunk(&page(&lines), &WhitespaceTokenCounter, None)
                    .expect("chunk");
                assert_eq!(chunks.len(), 1, "{row}: {chunks:?}");
                assert_eq!(chunks[0].kind, ChunkKind::TableRow);
                assert_eq!(
                    chunks[0].text,
                    format!("Exame | Resultado | Referência\n{row}")
                );
            }
        }
        assert!(looks_like_header(
            "Exame | Resultado | Unidade | Intervalo de referência"
        ));
        assert!(looks_like_header("Exame | Valor | Unidade"));
    }

    #[test]
    fn overlap_is_rejected_because_it_is_not_implemented() {
        let config = ChunkingConfig {
            overlap_tokens: 5,
            ..ChunkingConfig::default()
        };
        assert!(matches!(
            StructureAwareV1::new(config),
            Err(ChunkError::UnsupportedOverlap(5))
        ));
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
