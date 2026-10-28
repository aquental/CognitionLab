//! Steps for `rel01.feature` (PRD §5.6): integrity across failure and recovery.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use cucumber::{given, then, when};
use embeddings_fundamentals::domain::{Execution, FailureStage, RunStatus, SourceStatus};
use embeddings_fundamentals::embedding::deterministic::{
    DeterministicEmbedder, InjectedDefect, WhitespaceTokenCounter,
};
use embeddings_fundamentals::embedding::{EmbeddingStrategy, ErrorClass, TokenCounter};
use embeddings_fundamentals::extraction::fixture::FixtureExtractor;
use embeddings_fundamentals::ids::{ExperimentId, SourceId};
use embeddings_fundamentals::storage::FaultPoint;

use super::common::expected;
use crate::{LabWorld, SpaceOf};

fn source(id: &str) -> Result<SourceId> {
    Ok(id.parse()?)
}

fn report_for<'a>(
    world: &'a LabWorld,
    id: &str,
) -> Result<&'a embeddings_fundamentals::pipeline::SourceReport> {
    world
        .last_run()?
        .source(&source(id)?)
        .with_context(|| format!("{id} missing from the run report"))
}

/// Parses a count written as digits or as an English word ("one", "two", ...).
fn count(word: &str) -> Result<u32> {
    if let Ok(n) = word.parse() {
        return Ok(n);
    }
    let words = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    let n = words
        .iter()
        .position(|w| *w == word)
        .with_context(|| format!("not a count: {word:?}"))?;
    Ok(u32::try_from(n)?)
}

fn parse_class(class: &str) -> Result<ErrorClass> {
    Ok(match class {
        "transient" => ErrorClass::Transient,
        "permanent" => ErrorClass::Permanent,
        other => anyhow::bail!("unknown error class {other:?}"),
    })
}

// --- A failed replacement preserves the committed revision -------------------------

#[given(expr = "source {string} has a committed revision with checksum {string}")]
async fn committed_revision(world: &mut LabWorld, id: String, label: String) -> Result<()> {
    world.use_captured_extraction()?;
    let entry = world.add_fixture_source("lab-searchable-001", Some(&id))?;
    world.ingest().await?;
    let report = report_for(world, &id)?;
    ensure!(
        report.status == SourceStatus::Succeeded,
        "initial commit failed: {:?}",
        report.reason
    );
    let revision = report.revision_id.clone().context("no revision")?;
    world
        .notes
        .insert(format!("checksum:{label}"), entry.checksum.to_string());
    world
        .notes
        .insert(format!("revision:{label}"), revision.to_string());
    world.notes.insert(
        "experiment:old".into(),
        world.last_run()?.experiment_id.to_string(),
    );
    Ok(())
}

#[given(expr = "a changed version of {string} has checksum {string}")]
fn changed_version(world: &mut LabWorld, id: String, label: String) -> Result<()> {
    let entry = world.add_fixture_source("us-mixed-003", Some(&id))?;
    world
        .notes
        .insert(format!("checksum:{label}"), entry.checksum.to_string());
    ensure!(
        world.notes.get("checksum:old") != world.notes.get(&format!("checksum:{label}")),
        "the changed version has the same checksum"
    );
    Ok(())
}

#[given("a storage fault is injected before the new revision commit")]
fn storage_fault(world: &mut LabWorld) {
    world.store.inject_fault(FaultPoint::BeforePointerRename);
}

#[when("I ingest the changed source")]
async fn ingest_changed(world: &mut LabWorld) -> Result<()> {
    world.ingest().await
}

#[then(expr = "the source attempt is marked {string} with stage and reason")]
fn attempt_marked(world: &mut LabWorld, status: String) -> Result<()> {
    let id = world
        .sources
        .last()
        .context("no source")?
        .source_id
        .to_string();
    let report = report_for(world, &id)?;
    ensure!(
        report.status.as_str() == status,
        "status {}",
        report.status.as_str()
    );
    ensure!(
        report.stage == Some(FailureStage::Storage),
        "stage {:?}",
        report.stage
    );
    ensure!(
        report.reason.as_deref().is_some_and(|r| !r.is_empty()),
        "no reason"
    );
    Ok(())
}

fn active_checksums(world: &LabWorld) -> Result<BTreeSet<String>> {
    Ok(world
        .active_records()?
        .iter()
        .map(|r| r.metadata.source_checksum.to_string())
        .collect())
}

#[then(expr = "only the {string} revision remains searchable for that configuration")]
fn only_revision_searchable(world: &mut LabWorld, label: String) -> Result<()> {
    let expected = world
        .notes
        .get(&format!("checksum:{label}"))
        .context("unknown label")?
        .clone();
    let checksums = active_checksums(world)?;
    ensure!(
        checksums == BTreeSet::from([expected]),
        "searchable checksums {checksums:?}"
    );
    let count = world.active_records()?.len();
    let expected_count: usize = super::common::expected("lab-searchable-001")?
        .text_records
        .iter()
        .sum();
    ensure!(
        count == expected_count,
        "{count} records searchable, the old revision has {expected_count}"
    );
    Ok(())
}

#[then(expr = "no partial records from the {string} revision are searchable")]
fn no_partial_records(world: &mut LabWorld, label: String) -> Result<()> {
    let forbidden = world
        .notes
        .get(&format!("checksum:{label}"))
        .context("unknown label")?
        .clone();
    ensure!(
        !active_checksums(world)?.contains(&forbidden),
        "records of the {label} revision are searchable"
    );
    Ok(())
}

#[when("I remove the fault and retry the changed source successfully")]
async fn retry_without_fault(world: &mut LabWorld) -> Result<()> {
    world.store.clear_fault();
    world.ingest().await?;
    let id = world
        .sources
        .last()
        .context("no source")?
        .source_id
        .to_string();
    let report = report_for(world, &id)?;
    ensure!(
        report.status == SourceStatus::Succeeded,
        "retry failed: {:?}",
        report.reason
    );
    world.notes.insert(
        "revision:new".into(),
        report.revision_id.clone().context("revision")?.to_string(),
    );
    world.notes.insert(
        "experiment:new".into(),
        world.last_run()?.experiment_id.to_string(),
    );
    Ok(())
}

#[then(expr = "only the {string} revision is active for that configuration")]
fn only_revision_active(world: &mut LabWorld, label: String) -> Result<()> {
    let expected = world
        .notes
        .get(&format!("checksum:{label}"))
        .context("unknown label")?
        .clone();
    let checksums = active_checksums(world)?;
    ensure!(
        checksums == BTreeSet::from([expected]),
        "active checksums {checksums:?}"
    );
    let pointer = world
        .store
        .active_pointer(&world.embedder.model_info_space(), &source("S1")?)?
        .context("no active pointer")?;
    let revision = world
        .notes
        .get(&format!("revision:{label}"))
        .context("revision")?;
    ensure!(
        pointer.revision_id.as_str() == revision,
        "pointer {} != {revision}",
        pointer.revision_id
    );
    Ok(())
}

#[then("no duplicate active records exist")]
fn no_duplicates(world: &mut LabWorld) -> Result<()> {
    let records = world.active_records()?;
    let ids: BTreeSet<_> = records.iter().map(|r| r.record_id.clone()).collect();
    let chunks: BTreeSet<_> = records.iter().map(|r| r.chunk_id.clone()).collect();
    ensure!(
        ids.len() == records.len() && chunks.len() == records.len(),
        "duplicate records"
    );
    let expected_count: usize = expected("us-mixed-003")?.text_records.iter().sum();
    ensure!(
        records.len() == expected_count,
        "{} active records, expected {expected_count}",
        records.len()
    );
    Ok(())
}

#[then("saved experiment provenance still identifies the revision used by each earlier run")]
fn provenance_kept(world: &mut LabWorld) -> Result<()> {
    let s1 = source("S1")?;
    for (experiment, revision) in [
        ("experiment:old", "revision:old"),
        ("experiment:new", "revision:new"),
    ] {
        let id: ExperimentId = world.notes.get(experiment).context("experiment")?.parse()?;
        let expected = world.notes.get(revision).context("revision")?.clone();
        let snapshot = world.store.load_experiment(&id)?;
        let pinned: Vec<_> = snapshot
            .revisions
            .iter()
            .filter(|r| r.source_id == s1)
            .collect();
        ensure!(
            pinned.len() == 1 && pinned[0].revision_id.as_str() == expected,
            "{id} pins {pinned:?}, expected {expected}"
        );
        let records = world.store.records(&snapshot.revisions)?;
        ensure!(!records.is_empty(), "{id}: pinned revision is not readable");
        ensure!(
            records.iter().all(|r| r.revision_id.as_str() == expected),
            "{id}: wrong revision records"
        );
    }
    Ok(())
}

// --- Retries --------------------------------------------------------------------------

#[given("an embedding batch fails transiently on attempt 1 and succeeds on attempt 2")]
fn transient_once(world: &mut LabWorld) -> Result<()> {
    world.use_captured_extraction()?;
    world.add_fixture_source("lab-searchable-001", None)?;
    world.embedder.fail_next(1, ErrorClass::Transient);
    Ok(())
}

#[given(expr = "the configured maximum attempt count is {int}")]
fn max_attempts(world: &mut LabWorld, attempts: u32) -> Result<()> {
    world.settings.max_attempts =
        std::num::NonZeroU32::new(attempts).context("attempts must be > 0")?;
    Ok(())
}

#[then(regex = r"^exactly (\w+) embedding attempts are made$")]
fn exact_attempts(world: &mut LabWorld, attempts: String) -> Result<()> {
    let attempts = count(&attempts)?;
    let calls = world.embedder.calls().len();
    ensure!(
        calls == usize::try_from(attempts)?,
        "adapter received {calls} calls"
    );
    let reported = world
        .last_run()?
        .sources
        .iter()
        .map(|s| s.attempts)
        .sum::<u32>();
    ensure!(reported == attempts, "report counts {reported} attempts");
    Ok(())
}

#[then(regex = r"^the run records (\w+) retry$")]
fn run_retries(world: &mut LabWorld, retries: String) -> Result<()> {
    let retries = count(&retries)?;
    let observed = world.last_run()?.retries;
    ensure!(observed == retries, "run records {observed} retries");
    Ok(())
}

#[then("each expected record is committed exactly once")]
fn committed_once(world: &mut LabWorld) -> Result<()> {
    let records = world.active_records()?;
    let expected_count: usize = expected("lab-searchable-001")?.text_records.iter().sum();
    let ids: BTreeSet<_> = records.iter().map(|r| r.record_id.clone()).collect();
    ensure!(
        records.len() == expected_count && ids.len() == expected_count,
        "{} records committed",
        records.len()
    );
    Ok(())
}

#[given(expr = "source {string} always fails with a {string} embedding error")]
fn always_fails(world: &mut LabWorld, id: String, class: String) -> Result<()> {
    world.use_captured_extraction()?;
    world.add_fixture_source("lab-searchable-001", Some(&id))?;
    world.embedder.fail_source_always(&id, parse_class(&class)?);
    Ok(())
}

#[given(expr = "independent source {string} can be processed successfully")]
fn independent_source(world: &mut LabWorld, id: String) -> Result<()> {
    world.add_fixture_source("us-mixed-003", Some(&id))?;
    Ok(())
}

#[when("I ingest both sources")]
async fn ingest_both(world: &mut LabWorld) -> Result<()> {
    world.ingest().await
}

#[then(expr = "{string} has exactly {int} embedding attempts and status {string}")]
fn attempts_and_status(
    world: &mut LabWorld,
    id: String,
    attempts: u32,
    status: String,
) -> Result<()> {
    let calls = world.embedder.calls_for_source(&id).len();
    ensure!(
        calls == usize::try_from(attempts)?,
        "{id}: adapter received {calls} calls"
    );
    let report = report_for(world, &id)?;
    ensure!(
        report.attempts == attempts,
        "{id}: report counts {} attempts",
        report.attempts
    );
    ensure!(
        report.status.as_str() == status,
        "{id}: status {}",
        report.status.as_str()
    );
    Ok(())
}

#[then(expr = "{string} has no newly committed records")]
fn no_new_records(world: &mut LabWorld, id: String) -> Result<()> {
    let s = source(&id)?;
    ensure!(
        world.active_records()?.iter().all(|r| r.source_id != s),
        "{id} has active records"
    );
    ensure!(
        report_for(world, &id)?.records_committed == 0,
        "{id} reports committed records"
    );
    Ok(())
}

#[then(expr = "{string} has status {string} and its expected committed records")]
fn status_and_records(world: &mut LabWorld, id: String, status: String) -> Result<()> {
    let report = report_for(world, &id)?;
    ensure!(
        report.status.as_str() == status,
        "{id}: status {}",
        report.status.as_str()
    );
    let s = source(&id)?;
    let count = world
        .active_records()?
        .iter()
        .filter(|r| r.source_id == s)
        .count();
    let expected_count: usize = expected("us-mixed-003")?.text_records.iter().sum();
    ensure!(
        count == expected_count,
        "{id}: {count} active records, expected {expected_count}"
    );
    Ok(())
}

#[then(expr = "the overall run status is {string}")]
fn overall_status(world: &mut LabWorld, status: String) -> Result<()> {
    let observed = world.last_run()?.run_status.as_str();
    ensure!(observed == status, "run status {observed}");
    Ok(())
}

#[then("the CLI exits with a nonzero status")]
fn cli_nonzero(world: &mut LabWorld) -> Result<()> {
    let code = world.last_run()?.exit_code();
    ensure!(code != 0, "exit code {code}");
    Ok(())
}

#[then(expr = "the report includes the failure stage, reason, and attempt count for {string}")]
fn failure_details(world: &mut LabWorld, id: String) -> Result<()> {
    let report = report_for(world, &id)?;
    ensure!(
        report.stage == Some(FailureStage::Embedding),
        "{id}: stage {:?}",
        report.stage
    );
    let reason = report.reason.as_deref().context("no reason")?;
    ensure!(reason.contains("injected"), "{id}: reason {reason:?}");
    ensure!(report.attempts >= 1, "{id}: attempts {}", report.attempts);
    Ok(())
}

// --- Invalid vectors -----------------------------------------------------------------

#[given(expr = "source {string} has a committed revision in a live isolated store")]
async fn committed_in_live_store(world: &mut LabWorld, id: String) -> Result<()> {
    committed_revision(world, id, "old".into()).await
}

#[given(
    expr = "a changed version of {string} whose adapter response contains a vector with {string}"
)]
fn changed_with_defect(world: &mut LabWorld, id: String, defect: String) -> Result<()> {
    changed_version(world, id.clone(), "new".into())?;
    let injected = match defect.as_str() {
        "incorrect dimensions" => InjectedDefect::WrongDimensions,
        "NaN component" => InjectedDefect::NaN,
        "infinite component" => InjectedDefect::Infinite,
        other => anyhow::bail!("unknown defect {other:?}"),
    };
    world.embedder.inject_defect_for_source(&id, injected);
    world.notes.insert("defect".into(), defect);
    Ok(())
}

#[then("validation fails with the affected item ID and defect")]
fn validation_fails(world: &mut LabWorld) -> Result<()> {
    let id = world
        .sources
        .last()
        .context("no source")?
        .source_id
        .to_string();
    let report = report_for(world, &id)?;
    ensure!(
        report.stage == Some(FailureStage::Validation),
        "stage {:?}",
        report.stage
    );
    let reason = report.reason.as_deref().context("no reason")?;
    let defect = world.notes.get("defect").context("no defect")?;
    let first_item = format!("{id}-p1-text-0");
    ensure!(
        reason.contains(&first_item),
        "reason {reason:?} does not name item {first_item}"
    );
    ensure!(
        reason.contains(defect.as_str()),
        "reason {reason:?} does not name the defect {defect}"
    );
    Ok(())
}

#[then("no records from the new source revision are staged or committed")]
fn nothing_staged(world: &mut LabWorld) -> Result<()> {
    no_partial_records(world, "new".into())?;
    let space = world.embedder.model_info_space();
    let space_dir = world.store_dir.path().join(space.as_str());
    let revisions = std::fs::read_dir(space_dir.join("revisions/S1"))?.count();
    ensure!(
        revisions == 1,
        "{revisions} revision directories for S1, expected only the committed one"
    );
    let tmp = space_dir.join("tmp");
    let staged = if tmp.exists() {
        std::fs::read_dir(&tmp)?.count()
    } else {
        0
    };
    ensure!(staged == 0, "{staged} staging directories left behind");
    Ok(())
}

#[then(expr = "the active revision pointer for {string} is unchanged")]
fn pointer_unchanged(world: &mut LabWorld, id: String) -> Result<()> {
    let pointer = world
        .store
        .active_pointer(&world.embedder.model_info_space(), &source(&id)?)?
        .context("no active pointer")?;
    let old = world.notes.get("revision:old").context("old revision")?;
    ensure!(
        pointer.revision_id.as_str() == old,
        "pointer moved to {}",
        pointer.revision_id
    );
    Ok(())
}

// --- Unreadable text ------------------------------------------------------------------

#[given("a required scanned text page whose OCR output is empty")]
fn empty_ocr(world: &mut LabWorld) -> Result<()> {
    let entry = world.add_fixture_source("lab-scanned-002", None)?;
    ensure!(entry.required, "fixture is not required");
    let extractor = FixtureExtractor::default();
    extractor.set_native("lab-scanned-002", 1, "");
    extractor.set_ocr_tsv(
        "lab-scanned-002",
        1,
        "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
    );
    world.extractor = Arc::new(extractor);
    Ok(())
}

#[then(expr = "the page is flagged {string}")]
fn page_flagged(world: &mut LabWorld, flag: String) -> Result<()> {
    ensure!(flag == "unreadable", "unsupported flag {flag}");
    let id = world
        .sources
        .last()
        .context("no source")?
        .source_id
        .to_string();
    let pages = &report_for(world, &id)?.pages;
    ensure!(pages.len() == 1 && pages[0].unreadable, "pages {pages:?}");
    Ok(())
}

#[then("no text embedding is requested for that page")]
fn no_embedding_for_page(world: &mut LabWorld) -> Result<()> {
    ensure!(world.embedder.calls().is_empty(), "the adapter was called");
    Ok(())
}

#[then(expr = "the source is not marked {string}")]
fn not_marked(world: &mut LabWorld, status: String) -> Result<()> {
    let id = world
        .sources
        .last()
        .context("no source")?
        .source_id
        .to_string();
    let observed = report_for(world, &id)?.status.as_str();
    ensure!(observed != status, "source is {observed}");
    Ok(())
}

#[then(expr = "the run is marked {string} with the page-level reason")]
fn run_marked_with_reason(world: &mut LabWorld, status: String) -> Result<()> {
    let run = world.last_run()?;
    ensure!(
        run.run_status.as_str() == status,
        "run {}",
        run.run_status.as_str()
    );
    ensure!(
        run.unreadable_pages == 1,
        "run counts {} unreadable pages",
        run.unreadable_pages
    );
    let id = world
        .sources
        .last()
        .context("no source")?
        .source_id
        .to_string();
    let reason = report_for(world, &id)?
        .reason
        .as_deref()
        .context("no reason")?
        .to_owned();
    ensure!(reason.contains("unreadable pages [1]"), "reason {reason:?}");
    Ok(())
}

// --- Truncation -----------------------------------------------------------------------

/// Token limit of the adapter in the truncation scenarios (whitespace tokens).
const LIMIT: usize = 12;
const LONG_ROW: &str = "Hemoglobina | 13,5 | g/dL | 12,0–16,0 | método automatizado por citometria de fluxo com contagem celular";

fn truncation_source(world: &mut LabWorld) -> Result<()> {
    world.add_fixture_source("lab-searchable-001", None)?;
    let extractor = FixtureExtractor::default();
    // A usable text layer: one oversized table row, plus a paragraph long enough to pass
    // the native-text rule that is split (never truncated) to fit the limit.
    let paragraph = "Observação: amostra coletada em jejum, processada no mesmo dia e analisada \
                     em equipamento calibrado conforme o procedimento interno do laboratório, \
                     sem intercorrências durante a coleta ou o transporte do material.";
    extractor.set_native(
        "lab-searchable-001",
        1,
        &format!("Hemograma\n{LONG_ROW}\n\n{paragraph}\n"),
    );
    world.extractor = Arc::new(extractor);
    let mut caps = DeterministicEmbedder::builder()
        .build()?
        .capabilities()
        .clone();
    caps.max_input_tokens = NonZeroUsize::new(LIMIT);
    world.embedder = Arc::new(
        DeterministicEmbedder::builder()
            .capabilities(caps)
            .build()?,
    );
    let row_tokens = WhitespaceTokenCounter.count(&format!("Hemograma\n{LONG_ROW}"));
    ensure!(
        row_tokens > LIMIT,
        "fixture row has only {row_tokens} tokens"
    );
    world
        .notes
        .insert("row_tokens".into(), row_tokens.to_string());
    Ok(())
}

#[given("a chunk exceeds the configured model input limit")]
fn chunk_exceeds(world: &mut LabWorld) -> Result<()> {
    truncation_source(world)
}

#[given("a single table row whose token count exceeds the configured model input limit")]
fn row_exceeds(world: &mut LabWorld) -> Result<()> {
    truncation_source(world)
}

#[given("truncation is explicitly enabled for the experiment")]
fn truncation_enabled(world: &mut LabWorld) {
    world.settings.chunking.truncation_enabled = true;
}

#[given("truncation is disabled for the experiment")]
fn truncation_disabled(world: &mut LabWorld) {
    world.settings.chunking.truncation_enabled = false;
}

#[when("I prepare that chunk for embedding")]
async fn prepare_chunk(world: &mut LabWorld) -> Result<()> {
    world.ingest().await
}

#[then("the submitted input does not exceed the model limit")]
fn submitted_within_limit(world: &mut LabWorld) -> Result<()> {
    let calls = world.embedder.calls();
    ensure!(!calls.is_empty(), "nothing was submitted");
    for text in calls.iter().flat_map(|c| c.submitted.iter()) {
        let tokens = WhitespaceTokenCounter.count(text);
        ensure!(tokens <= LIMIT, "submitted {tokens} tokens: {text:?}");
    }
    Ok(())
}

#[then("the record identifies the truncation and original and submitted input lengths")]
fn record_identifies_truncation(world: &mut LabWorld) -> Result<()> {
    let row_tokens: usize = world
        .notes
        .get("row_tokens")
        .context("row tokens")?
        .parse()?;
    let truncated: Vec<_> = world
        .active_records()?
        .into_iter()
        .filter(|r| r.metadata.truncation.is_some())
        .collect();
    ensure!(
        truncated.len() == 1,
        "{} truncated records",
        truncated.len()
    );
    let record = &truncated[0];
    let t = record.metadata.truncation.context("truncation")?;
    let text = record.chunk_text.as_deref().context("text")?;
    ensure!(
        t.original_tokens == row_tokens,
        "original {} != {row_tokens}",
        t.original_tokens
    );
    ensure!(
        t.submitted_tokens == WhitespaceTokenCounter.count(text),
        "submitted length does not match the text"
    );
    ensure!(
        t.submitted_tokens <= LIMIT && t.submitted_tokens < t.original_tokens,
        "{t:?}"
    );
    ensure!(
        text.starts_with("Hemograma\nHemoglobina"),
        "truncation must keep the start of the row"
    );
    Ok(())
}

#[then(regex = r"^the report counts (\w+) truncation$")]
fn report_truncations(world: &mut LabWorld, expected: String) -> Result<()> {
    let expected = usize::try_from(count(&expected)?)?;
    let observed = world.last_run()?.truncations;
    ensure!(observed == expected, "report counts {observed} truncations");
    Ok(())
}

#[then(expr = "the source has status {string} with stage {string} and an input-limit reason")]
fn status_stage_limit(world: &mut LabWorld, status: String, stage: String) -> Result<()> {
    let id = world
        .sources
        .last()
        .context("no source")?
        .source_id
        .to_string();
    let report = report_for(world, &id)?;
    ensure!(
        report.status.as_str() == status,
        "status {}",
        report.status.as_str()
    );
    ensure!(
        report.stage.map(FailureStage::as_str) == Some(stage.as_str()),
        "stage {:?}",
        report.stage
    );
    let reason = report.reason.as_deref().context("no reason")?;
    ensure!(reason.contains("input limit"), "reason {reason:?}");
    Ok(())
}

#[then("no embedding is requested for that source revision")]
fn no_embedding_requested(world: &mut LabWorld) -> Result<()> {
    ensure!(world.embedder.calls().is_empty(), "the adapter was called");
    Ok(())
}

#[then(expr = "the run is marked {string}")]
fn run_marked(world: &mut LabWorld, status: String) -> Result<()> {
    let observed = world.last_run()?.run_status;
    ensure!(
        observed.as_str() == status,
        "run status {}",
        observed.as_str()
    );
    ensure!(
        observed == RunStatus::Incomplete || status != "incomplete",
        "inconsistent status"
    );
    Ok(())
}

// --- Remote permission ------------------------------------------------------------------

#[given("remote-provider processing is disabled for the corpus")]
fn remote_disabled(world: &mut LabWorld) -> Result<()> {
    world.use_captured_extraction()?;
    world.add_fixture_source("lab-searchable-001", None)?;
    world.remote_allowed = false;
    Ok(())
}

#[when("I request ingestion with a remote embedding adapter")]
async fn ingest_remote(world: &mut LabWorld) -> Result<()> {
    let mut caps = DeterministicEmbedder::builder()
        .build()?
        .capabilities()
        .clone();
    caps.execution = Execution::Remote;
    world.embedder = Arc::new(
        DeterministicEmbedder::builder()
            .capabilities(caps)
            .build()?,
    );
    world.ingest().await
}

#[then("validation fails with a corpus-permission error")]
fn permission_error(world: &mut LabWorld) -> Result<()> {
    let error = world
        .validation_error
        .as_deref()
        .context("the run was not refused")?;
    ensure!(
        error.contains("does not allow remote processing"),
        "error {error:?}"
    );
    ensure!(world.runs.is_empty(), "a run report was produced");
    Ok(())
}

#[then("no remote-provider request is sent")]
fn no_remote_request(world: &mut LabWorld) -> Result<()> {
    ensure!(
        world.embedder.calls().is_empty(),
        "the remote adapter received {} calls",
        world.embedder.calls().len()
    );
    ensure!(
        world.extractor.ocr_calls().is_empty(),
        "extraction ran before validation"
    );
    ensure!(world.active_records()?.is_empty(), "records were written");
    Ok(())
}
