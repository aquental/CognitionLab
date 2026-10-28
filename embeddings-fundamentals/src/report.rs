//! Comparison reports (PRD §9.3–§9.4, §12 deliverables 6–7): one result per evaluated
//! configuration, rendered as JSON and Markdown.
//!
//! Configurations that differ in engine, weight format, or model are reported as an
//! **end-to-end configuration comparison**: differences are not attributed to one
//! variable alone (PRD §10).

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::cost::CostEstimate;
use crate::domain::{Execution, RunStatus};
use crate::embedding::{EngineId, PrefixPolicy};
use crate::evaluation::{DatasetRef, EvaluationReport};
use crate::fidelity::FidelityReport;
use crate::ids::{Checksum, EmbeddingSpaceId, ExperimentId, Fingerprint};
use crate::measurement::{EmbeddingSummary, Hardware, LatencyStats, Observation};
use crate::pipeline::RunReport;

/// Wording used for every multi-configuration comparison.
pub const COMPARISON_KIND: &str = "end-to-end configuration comparison";

/// Fingerprints that must match for two configurations to be comparable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprints {
    /// Extraction configuration and tools.
    pub extraction: Fingerprint,
    /// Chunking configuration, tokenizer, and budget.
    pub chunking: Fingerprint,
    /// Query set, labels, `k`, and metrics.
    pub evaluation: Fingerprint,
}

/// Ingestion measurements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestionMeasurement {
    /// Wall time of the whole ingestion, in milliseconds.
    pub duration_ms: f64,
    /// Pages extracted.
    pub pages: usize,
    /// Chunks embedded.
    pub chunks: usize,
    /// `duration_ms / pages`; `None` when no page was extracted.
    pub ms_per_page: Option<f64>,
    /// `chunks / duration`; `None` for a zero duration.
    pub chunks_per_second: Option<f64>,
}

/// Cold start (PRD §9.4): the first operation in a fresh process. The experiment process
/// starts fresh, so this is its first model load (local) or client construction (remote)
/// plus its first embedding call, which also opens the first connection of a remote engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColdStart {
    /// First model load or client construction of the process, in milliseconds.
    pub model_load_ms: f64,
    /// First embedding call of the process (the first ingestion batch); `None` when
    /// ingestion made no call, for example because every source was unchanged.
    pub first_operation_ms: Option<f64>,
    /// Inputs in that first call.
    pub first_operation_items: Option<usize>,
    /// `model_load_ms + first_operation_ms`; `None` without a first call.
    pub total_ms: Option<f64>,
}

/// The query side's adapter load and first query. It runs after ingestion inside the
/// same, already warm process, so it is **not** a cold start; it is reported to show the
/// query-side load cost separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuerySideStart {
    /// Second model load or client construction, in milliseconds.
    pub model_load_ms: f64,
    /// First query on that adapter (the discarded warm-up query).
    pub first_query_ms: f64,
    /// Sum of both.
    pub total_ms: f64,
}

/// Measurements of one configuration (PRD §9.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurements {
    /// Ingestion duration and rates.
    pub ingestion: IngestionMeasurement,
    /// Model load before ingestion, in milliseconds.
    pub ingestion_model_load_ms: f64,
    /// Document embedding calls during ingestion.
    pub embedding: EmbeddingSummary,
    /// Cold start of the process (PRD §9.4).
    pub cold_start: ColdStart,
    /// Query-side load and first query within the same process (not a cold start).
    pub query_side_start: QuerySideStart,
    /// Warm query latency over `latency_repeats` repetitions of every frozen query.
    pub warm_query_latency: LatencyStats,
    /// Repetitions per query.
    pub latency_repeats: usize,
    /// Process high-water mark (`ru_maxrss`), labeled as local process memory.
    pub peak_local_ram: Observation,
    /// GPU memory, or why it is unavailable.
    pub gpu_memory: Observation,
    /// Inference memory at the provider (remote engines only).
    pub provider_inference_memory: Option<Observation>,
    /// Bytes of the active index on disk.
    pub index_size: Observation,
}

/// Provider cost of ingestion and of one pass over the frozen queries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Costs {
    /// Ingestion run.
    pub ingestion: CostEstimate,
    /// One pass over the frozen query set (warm repetitions excluded).
    pub query_set: CostEstimate,
}

/// Reliability counters (PRD §9.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reliability {
    /// Failed sources.
    pub failed_sources: usize,
    /// Unreadable pages.
    pub failed_pages: usize,
    /// Failed adapter calls (batches), including retried ones.
    pub failed_batches: usize,
    /// Retries.
    pub retries: u32,
    /// Truncated chunks.
    pub truncations: usize,
}

/// Everything reported for one configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationResult {
    /// Machine the measurements were taken on (captured by the experiment run).
    pub hardware: Hardware,
    /// Experiment id of the ingestion run.
    pub experiment_id: ExperimentId,
    /// Ingestion status.
    pub run_status: RunStatus,
    /// Engine.
    pub engine: EngineId,
    /// Engine version.
    pub engine_version: String,
    /// Model.
    pub model_id: String,
    /// Pinned model revision.
    pub model_revision: Option<String>,
    /// Weight format.
    pub weight_format: Option<String>,
    /// Vector dimensions.
    pub dimensions: usize,
    /// Prefix policy.
    pub prefix_policy: PrefixPolicy,
    /// Local or remote.
    pub execution: Execution,
    /// Embedding space.
    pub embedding_space_id: EmbeddingSpaceId,
    /// Inputs per adapter call.
    pub batch_size: usize,
    /// Corpus manifest checksum.
    pub corpus_checksum: Checksum,
    /// Configuration fingerprints.
    pub fingerprints: Fingerprints,
    /// Retrieval evaluation.
    pub evaluation: EvaluationReport,
    /// Efficiency measurements.
    pub measurements: Measurements,
    /// Provider costs.
    pub cost: Costs,
    /// Reliability counters.
    pub reliability: Reliability,
    /// Extraction fidelity of this run's extraction configuration.
    pub fidelity: Option<FidelityReport>,
}

/// Why an evaluation did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationNotRunStatus {
    /// The ingestion was incomplete and a frozen label points into what is missing.
    NotRun,
}

/// The evaluation part of an [`IncompleteExperiment`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationNotRun {
    /// Always [`EvaluationNotRunStatus::NotRun`].
    pub status: EvaluationNotRunStatus,
    /// Why (label and chunk ids only, no document content).
    pub reason: String,
}

/// An experiment whose ingestion was incomplete and whose evaluation could not run
/// because a frozen label points into a failed source (PLAN §4.3: the run still
/// reports its status and reliability counters, and exits with code 2). It is not a
/// [`ConfigurationResult`] and cannot enter a comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncompleteExperiment {
    /// Machine the run was taken on.
    pub hardware: Hardware,
    /// Experiment id of the ingestion run.
    pub experiment_id: ExperimentId,
    /// Ingestion status (always incomplete).
    pub run_status: RunStatus,
    /// The evaluation that did not run, and why.
    pub evaluation: EvaluationNotRun,
    /// Reliability counters of the ingestion.
    pub reliability: Reliability,
    /// The full ingestion report (per-source status, stage, and reason).
    pub ingestion: RunReport,
}

/// A comparison of configurations evaluated on the same corpus and queries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// Always [`COMPARISON_KIND`].
    pub comparison_kind: String,
    /// Machine the measurements were taken on.
    pub hardware: Hardware,
    /// Corpus manifest checksum shared by every configuration.
    pub corpus_checksum: Checksum,
    /// Query set shared by every configuration.
    pub query_set: DatasetRef,
    /// Labels shared by every configuration.
    pub labels: DatasetRef,
    /// Configurations, in the order given.
    pub configurations: Vec<ConfigurationResult>,
    /// Limitations and outstanding items.
    pub notes: Vec<String>,
}

/// Reasons configurations cannot be put in one comparison.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    /// No configuration was given.
    #[error("a comparison needs at least one configuration")]
    Empty,
    /// The configurations used different inputs.
    #[error("configuration {experiment} differs in {what}; results are not comparable")]
    NotComparable {
        /// Offending experiment.
        experiment: ExperimentId,
        /// What differs.
        what: &'static str,
    },
}

impl ComparisonReport {
    /// Builds a comparison, refusing configurations evaluated on other inputs: the
    /// corpus, query set, labels, extraction, and evaluation must match. Chunking may
    /// differ only through the model tokenizer and budget, which the report shows.
    ///
    /// # Errors
    ///
    /// Returns [`ReportError`] for an empty or non-comparable set.
    pub fn new(
        configurations: Vec<ConfigurationResult>,
        notes: Vec<String>,
    ) -> Result<Self, ReportError> {
        let first = configurations.first().ok_or(ReportError::Empty)?;
        let mut hardware = first.hardware.clone();
        for c in &configurations {
            let differs = |what| ReportError::NotComparable {
                experiment: c.experiment_id.clone(),
                what,
            };
            if c.corpus_checksum != first.corpus_checksum {
                return Err(differs("corpus checksum"));
            }
            if c.evaluation.query_set != first.evaluation.query_set {
                return Err(differs("query set"));
            }
            if c.evaluation.labels != first.evaluation.labels {
                return Err(differs("relevance labels"));
            }
            if c.fingerprints.extraction != first.fingerprints.extraction {
                return Err(differs("extraction fingerprint"));
            }
            if c.fingerprints.evaluation != first.fingerprints.evaluation {
                return Err(differs("evaluation fingerprint"));
            }
            // Timings and memory from different machines are not comparable; a probe
            // field one run could not read is unknown, not different.
            hardware = hardware
                .merge(&c.hardware)
                .ok_or_else(|| differs("measurement hardware"))?;
        }
        Ok(Self {
            comparison_kind: COMPARISON_KIND.to_owned(),
            hardware,
            corpus_checksum: first.corpus_checksum.clone(),
            query_set: first.evaluation.query_set.clone(),
            labels: first.evaluation.labels.clone(),
            configurations,
            notes,
        })
    }

    /// Renders the report as Markdown.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        let _ = writeln!(md, "# Retrieval comparison ({})\n", self.comparison_kind);
        let _ = writeln!(
            md,
            "Configurations differ in engine, weight format, or model, so differences are \
             not attributed to a single variable.\n"
        );
        let h = &self.hardware;
        let _ = writeln!(md, "## Setup\n");
        let _ = writeln!(
            md,
            "- Hardware: {} {} · CPU {} · {} logical CPUs · {} memory · accelerator {}",
            h.os,
            h.arch,
            h.cpu.as_deref().unwrap_or("unknown"),
            h.logical_cpus
                .map_or_else(|| "?".to_owned(), |n| n.to_string()),
            h.memory_bytes
                .map_or_else(|| "unknown".to_owned(), |b| format!("{b} bytes")),
            h.accelerator
        );
        let _ = writeln!(md, "- Corpus checksum: `{}`", self.corpus_checksum);
        let _ = writeln!(
            md,
            "- Query set: {} (`{}`)",
            self.query_set.version, self.query_set.checksum
        );
        let _ = writeln!(
            md,
            "- Labels: {} (`{}`)\n",
            self.labels.version, self.labels.checksum
        );

        let _ = writeln!(md, "## Configurations\n");
        let _ = writeln!(
            md,
            "| Experiment | Engine | Model | Revision | Weights | Dims | Prefix | Batch | Space | Extraction fp | Chunking fp | Evaluation fp |"
        );
        let _ = writeln!(md, "|---|---|---|---|---|---:|---|---:|---|---|---|---|");
        for c in &self.configurations {
            let _ = writeln!(
                md,
                "| {} | {} {} | {} | {} | {} | {} | {:?} | {} | `{}` | `{}` | `{}` | `{}` |",
                c.experiment_id,
                c.engine,
                c.engine_version,
                c.model_id,
                c.model_revision.as_deref().unwrap_or("-"),
                c.weight_format.as_deref().unwrap_or("-"),
                c.dimensions,
                c.prefix_policy,
                c.batch_size,
                c.embedding_space_id,
                short(c.fingerprints.extraction.as_str()),
                short(c.fingerprints.chunking.as_str()),
                short(c.fingerprints.evaluation.as_str()),
            );
        }

        let _ = writeln!(md, "\n## Retrieval quality (k = 3, macro averages)\n");
        let _ = writeln!(
            md,
            "| Experiment | Ingestion run | Failed sources | Evaluation | Precision@3 | Hit@3 | nDCG@3 |"
        );
        let _ = writeln!(md, "|---|---|---:|---|---:|---:|---:|");
        for c in &self.configurations {
            let m = &c.evaluation.metrics;
            let _ = writeln!(
                md,
                "| {} | {} | {} | {:?} | {:.4} | {:.4} | {:.4} |",
                c.experiment_id,
                match c.run_status {
                    RunStatus::Succeeded => "succeeded",
                    RunStatus::Incomplete => "**INCOMPLETE**",
                },
                c.reliability.failed_sources,
                c.evaluation.status,
                m.precision_at_3,
                m.hit_at_3,
                m.ndcg_at_3
            );
        }

        let _ = writeln!(md, "\n## Efficiency\n");
        let _ = writeln!(
            md,
            "| Experiment | Ingestion ms | ms/page | chunks/s | Batch latency ms | Embeds/s | Model load ms | Cold start ms | Warm median ms | Warm p95 ms | Peak local RAM | Index size | GPU memory |"
        );
        let _ = writeln!(
            md,
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|"
        );
        for c in &self.configurations {
            let m = &c.measurements;
            let _ = writeln!(
                md,
                "| {} | {:.1} | {} | {} | {:.2} | {:.1} | {:.1} | {} | {:.2} | {:.2} | {} | {} | {} |",
                c.experiment_id,
                m.ingestion.duration_ms,
                opt(m.ingestion.ms_per_page, 1),
                opt(m.ingestion.chunks_per_second, 1),
                m.embedding.mean_batch_latency_ms,
                m.embedding.items_per_second,
                m.ingestion_model_load_ms,
                opt(m.cold_start.total_ms, 1),
                m.warm_query_latency.median_ms,
                m.warm_query_latency.p95_ms,
                observation(&m.peak_local_ram),
                observation(&m.index_size),
                observation(&m.gpu_memory),
            );
        }

        let _ = writeln!(md, "\n## Cost and reliability\n");
        let _ = writeln!(
            md,
            "| Experiment | Ingestion cost | Query-set cost | Provider memory | Failed pages | Failed batches | Retries | Truncations |"
        );
        let _ = writeln!(md, "|---|---|---|---|---:|---:|---:|---:|");
        for c in &self.configurations {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                c.experiment_id,
                cost(&c.cost.ingestion),
                cost(&c.cost.query_set),
                c.measurements
                    .provider_inference_memory
                    .as_ref()
                    .map_or_else(|| "local (see peak RAM)".to_owned(), observation),
                c.reliability.failed_pages,
                c.reliability.failed_batches,
                c.reliability.retries,
                c.reliability.truncations
            );
        }

        let fidelity: Vec<&ConfigurationResult> = self
            .configurations
            .iter()
            .filter(|c| c.fidelity.is_some())
            .collect();
        if !fidelity.is_empty() {
            let _ = writeln!(md, "\n## Extraction fidelity\n");
            let _ = writeln!(
                md,
                "| Experiment | Sample | Overall | Test name | Value | Unit | Reference range | Native | OCR |"
            );
            let _ = writeln!(md, "|---|---|---:|---:|---:|---:|---:|---:|---:|");
            for c in fidelity {
                if let Some(f) = &c.fidelity {
                    let field = |k| f.by_field.get(&k).map_or(0.0, |a| a.accuracy);
                    let method = |k: &str| {
                        f.by_method
                            .get(k)
                            .map_or_else(|| "-".to_owned(), |a| format!("{:.3}", a.accuracy))
                    };
                    let _ = writeln!(
                        md,
                        "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} |",
                        c.experiment_id,
                        f.sample_version,
                        f.overall.accuracy,
                        field(crate::fidelity::Field::TestName),
                        field(crate::fidelity::Field::Value),
                        field(crate::fidelity::Field::Unit),
                        field(crate::fidelity::Field::ReferenceRange),
                        method("native"),
                        method("ocr"),
                    );
                }
            }
        }

        let _ = writeln!(md, "\n## Top-3 results per query\n");
        for c in &self.configurations {
            let _ = writeln!(
                md,
                "### {} ({} / {})\n",
                c.experiment_id, c.engine, c.model_id
            );
            let _ = writeln!(
                md,
                "| Query | Rank | Retrieved item | Source/page | Score | Relevance | Note |"
            );
            let _ = writeln!(md, "|---|---:|---|---|---:|---:|---|");
            for q in &c.evaluation.queries {
                if q.results.is_empty() {
                    let _ = writeln!(md, "| {} | - | no candidates | - | - | - | - |", q.text);
                }
                for r in &q.results {
                    let item = r
                        .chunk_text
                        .as_deref()
                        .map(|t| t.replace('\n', " / ").replace('|', "\\|"))
                        .or_else(|| r.asset_uri.clone())
                        .unwrap_or_default();
                    let _ = writeln!(
                        md,
                        "| {} | {} | {} | {} / {} | {:.4} | {} | {} |",
                        q.text, r.rank, item, r.source_id, r.page_number, r.score, r.label, r.note
                    );
                }
            }
            let _ = writeln!(md);
        }

        if !self.notes.is_empty() {
            let _ = writeln!(md, "## Limitations and outstanding items\n");
            for n in &self.notes {
                let _ = writeln!(md, "- {n}");
            }
        }
        md
    }
}

fn short(fingerprint: &str) -> &str {
    fingerprint.get(..12).unwrap_or(fingerprint)
}

fn opt(value: Option<f64>, digits: usize) -> String {
    value.map_or_else(|| "-".to_owned(), |v| format!("{v:.digits$}"))
}

fn observation(o: &Observation) -> String {
    match o {
        Observation::Measured { value, unit, .. } => format!("{value:.0} {unit}"),
        Observation::Unavailable { reason } => format!("unavailable ({reason})"),
    }
}

fn cost(c: &CostEstimate) -> String {
    match (c.estimated_cost, &c.currency) {
        (Some(v), Some(cur)) => format!("{v:.8} {cur}"),
        _ => format!(
            "{:?}: {}",
            c.estimate_status,
            c.reason.as_deref().unwrap_or("-")
        ),
    }
}
