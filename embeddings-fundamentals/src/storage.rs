//! Filesystem record store with atomic revision publication (PLAN §4.1, §4.2).
//!
//! ```text
//! <root>/<space>/revisions/<source>/<revision>/records.jsonl   immutable once published
//! <root>/<space>/tmp/stage-*/                                   per-attempt staging
//! <root>/<space>/active/<source>.json                           the commit point
//! <root>/experiments/<experiment>.json                          append-only snapshots
//! ```
//!
//! Search reads only revisions named by an `active/` pointer or by an experiment
//! snapshot, so a staged or renamed revision is invisible until its pointer is
//! renamed into place. Replaced revisions are kept. All operations are synchronous
//! (directory `fsync` has no async equivalent); async callers use `spawn_blocking`.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::ids::{Checksum, EmbeddingSpaceId, ExperimentId, Fingerprint, RevisionId, SourceId};
use crate::record::Record;

const RECORDS_FILE: &str = "records.jsonl";

/// Where an injected storage fault fires (failure-injection tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    /// Before anything is written.
    BeforeStage,
    /// After staging, before the revision directory is renamed into place.
    BeforeRevisionRename,
    /// After the revision is in place, before the active pointer is renamed.
    BeforePointerRename,
}

/// Storage errors.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A filesystem operation failed.
    #[error("store i/o error at {path}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// Stored JSON could not be read.
    #[error("corrupt store file {path}")]
    Corrupt {
        /// Path involved.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: serde_json::Error,
    },
    /// A fault injected by a test fired.
    #[error("injected storage fault at {0:?}")]
    InjectedFault(FaultPoint),
    /// An experiment snapshot with this id already exists.
    #[error("experiment {0} already exists; snapshots are append-only")]
    ExperimentExists(ExperimentId),
    /// A record does not belong to the revision being staged.
    #[error("record {record} does not belong to {source_id}/{revision}")]
    ForeignRecord {
        /// Offending record.
        record: String,
        /// Source being staged.
        source_id: SourceId,
        /// Revision being staged.
        revision: RevisionId,
    },
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.to_owned(),
        source,
    }
}

fn fsync_dir(dir: &Path) -> Result<(), StoreError> {
    File::open(dir).and_then(|f| f.sync_all()).map_err(io(dir))
}

/// Pointer naming the active revision of a source in a space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivePointer {
    /// Active revision.
    pub revision_id: RevisionId,
    /// Checksum of the revision's `records.jsonl`.
    pub records_checksum: Checksum,
}

/// A revision of one source in one space.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RevisionRef {
    /// Embedding space.
    pub space: EmbeddingSpaceId,
    /// Source.
    pub source_id: SourceId,
    /// Revision.
    pub revision_id: RevisionId,
}

/// Records written to a staging directory, not yet visible.
#[derive(Debug)]
#[must_use = "a staged revision is invisible until published"]
pub struct StagedRevision {
    space: EmbeddingSpaceId,
    source_id: SourceId,
    revision_id: RevisionId,
    dir: PathBuf,
    records_checksum: Checksum,
}

/// Append-only record of what an experiment run used (PRD §8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentSnapshot {
    /// Experiment run identifier.
    pub experiment_id: ExperimentId,
    /// Revisions searched by this experiment.
    pub revisions: Vec<RevisionRef>,
    /// Configuration fingerprints (extraction, chunking, embedding space, ...).
    pub fingerprints: BTreeMap<String, Fingerprint>,
    /// Checksum of the corpus manifest.
    pub corpus_checksum: Checksum,
}

/// Filesystem record store.
#[derive(Debug)]
pub struct FsRecordStore {
    root: PathBuf,
    fault: Mutex<Option<FaultPoint>>,
}

impl FsRecordStore {
    /// Opens (creating if needed) a store rooted at `root`, and removes abandoned
    /// staging directories left by interrupted runs.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] when the root cannot be created or cleaned.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        fs::create_dir_all(root).map_err(io(root))?;
        let store = Self {
            root: root.to_owned(),
            fault: Mutex::new(None),
        };
        for space in store.spaces()? {
            let tmp = store.space_dir(&space).join("tmp");
            if tmp.exists() {
                fs::remove_dir_all(&tmp).map_err(io(&tmp))?;
            }
        }
        Ok(store)
    }

    /// Store root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Makes every subsequent write fail at `point` until [`Self::clear_fault`].
    pub fn inject_fault(&self, point: FaultPoint) {
        *self.fault.lock().unwrap_or_else(PoisonError::into_inner) = Some(point);
    }

    /// Removes an injected fault.
    pub fn clear_fault(&self) {
        *self.fault.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn check_fault(&self, point: FaultPoint) -> Result<(), StoreError> {
        if *self.fault.lock().unwrap_or_else(PoisonError::into_inner) == Some(point) {
            return Err(StoreError::InjectedFault(point));
        }
        Ok(())
    }

    fn space_dir(&self, space: &EmbeddingSpaceId) -> PathBuf {
        self.root.join(space.as_str())
    }

    fn revision_dir(
        &self,
        space: &EmbeddingSpaceId,
        source: &SourceId,
        revision: &RevisionId,
    ) -> PathBuf {
        self.space_dir(space)
            .join("revisions")
            .join(source.as_str())
            .join(revision.as_str())
    }

    fn pointer_path(&self, space: &EmbeddingSpaceId, source: &SourceId) -> PathBuf {
        self.space_dir(space)
            .join("active")
            .join(format!("{source}.json"))
    }

    /// Embedding spaces present in the store.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] when the root cannot be listed.
    pub fn spaces(&self) -> Result<Vec<EmbeddingSpaceId>, StoreError> {
        let mut spaces = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(io(&self.root))? {
            let entry = entry.map_err(io(&self.root))?;
            if let Ok(space) = entry
                .file_name()
                .to_string_lossy()
                .parse::<EmbeddingSpaceId>()
            {
                spaces.push(space);
            }
        }
        spaces.sort();
        Ok(spaces)
    }

    /// Writes `records` to a new staging directory (fsync'd), invisible to search.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] on I/O failure, an injected fault, or a record that
    /// belongs to another source or revision.
    pub fn stage(
        &self,
        space: &EmbeddingSpaceId,
        source_id: &SourceId,
        revision_id: &RevisionId,
        records: &[Record],
    ) -> Result<StagedRevision, StoreError> {
        self.check_fault(FaultPoint::BeforeStage)?;
        if let Some(r) = records
            .iter()
            .find(|r| &r.source_id != source_id || &r.revision_id != revision_id)
        {
            return Err(StoreError::ForeignRecord {
                record: r.record_id.to_string(),
                source_id: source_id.clone(),
                revision: revision_id.clone(),
            });
        }
        let tmp = self.space_dir(space).join("tmp");
        fs::create_dir_all(&tmp).map_err(io(&tmp))?;
        let dir = tempfile::Builder::new()
            .prefix("stage-")
            .tempdir_in(&tmp)
            .map_err(io(&tmp))?
            .keep();
        let path = dir.join(RECORDS_FILE);
        let mut bytes = Vec::new();
        for record in records {
            serde_json::to_writer(&mut bytes, record).map_err(|source| StoreError::Corrupt {
                path: path.clone(),
                source,
            })?;
            bytes.push(b'\n');
        }
        {
            let file = File::create(&path).map_err(io(&path))?;
            let mut writer = BufWriter::new(file);
            writer.write_all(&bytes).map_err(io(&path))?;
            let file = writer.into_inner().map_err(|e| io(&path)(e.into_error()))?;
            file.sync_all().map_err(io(&path))?;
        }
        fsync_dir(&dir)?;
        Ok(StagedRevision {
            space: space.clone(),
            source_id: source_id.clone(),
            revision_id: revision_id.clone(),
            dir,
            records_checksum: Checksum::of_bytes(&bytes),
        })
    }

    /// Publishes a staged revision. Renaming the active pointer is the commit point:
    /// a failure before it leaves the previously active revision searchable.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] on I/O failure or an injected fault.
    pub fn publish(&self, staged: StagedRevision) -> Result<(), StoreError> {
        let target = self.revision_dir(&staged.space, &staged.source_id, &staged.revision_id);
        let parent = target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.clone());
        fs::create_dir_all(&parent).map_err(io(&parent))?;
        let records_checksum = if target.exists() {
            // A revision directory only ever appears through an atomic rename of a fully
            // written, fsync'd staging directory, so an existing one is complete. It may
            // be referenced by an earlier experiment snapshot (a source reverted to an
            // earlier revision), so it is reused as-is and never replaced: its records
            // keep the experiment id of the run that produced them.
            fs::remove_dir_all(&staged.dir).map_err(io(&staged.dir))?;
            let path = target.join(RECORDS_FILE);
            Checksum::of_bytes(&fs::read(&path).map_err(io(&path))?)
        } else {
            self.check_fault(FaultPoint::BeforeRevisionRename)?;
            fs::rename(&staged.dir, &target).map_err(io(&target))?;
            fsync_dir(&parent)?;
            staged.records_checksum.clone()
        };

        self.check_fault(FaultPoint::BeforePointerRename)?;
        let active = self.space_dir(&staged.space).join("active");
        fs::create_dir_all(&active).map_err(io(&active))?;
        let pointer = ActivePointer {
            revision_id: staged.revision_id.clone(),
            records_checksum,
        };
        let final_path = self.pointer_path(&staged.space, &staged.source_id);
        let tmp_path = active.join(format!(
            ".{}.json.tmp-{}",
            staged.source_id,
            staged
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let json = serde_json::to_vec(&pointer).map_err(|source| StoreError::Corrupt {
            path: tmp_path.clone(),
            source,
        })?;
        {
            let mut file = File::create(&tmp_path).map_err(io(&tmp_path))?;
            file.write_all(&json).map_err(io(&tmp_path))?;
            file.sync_all().map_err(io(&tmp_path))?;
        }
        fs::rename(&tmp_path, &final_path).map_err(io(&final_path))?;
        fsync_dir(&active)
    }

    /// Active pointer of `source` in `space`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the pointer exists but cannot be read.
    pub fn active_pointer(
        &self,
        space: &EmbeddingSpaceId,
        source: &SourceId,
    ) -> Result<Option<ActivePointer>, StoreError> {
        let path = self.pointer_path(space, source);
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|source| StoreError::Corrupt { path, source }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(&path)(e)),
        }
    }

    /// Active revisions of every source in `space`, sorted by source.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a pointer cannot be read.
    pub fn active(&self, space: &EmbeddingSpaceId) -> Result<Vec<RevisionRef>, StoreError> {
        let dir = self.space_dir(space).join("active");
        let mut refs = Vec::new();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(refs),
            Err(e) => return Err(io(&dir)(e)),
        };
        for entry in entries {
            let name = entry
                .map_err(io(&dir))?
                .file_name()
                .to_string_lossy()
                .into_owned();
            let Some(source) = name.strip_suffix(".json").filter(|s| !s.starts_with('.')) else {
                continue;
            };
            let Ok(source_id) = source.parse::<SourceId>() else {
                continue;
            };
            if let Some(pointer) = self.active_pointer(space, &source_id)? {
                refs.push(RevisionRef {
                    space: space.clone(),
                    source_id,
                    revision_id: pointer.revision_id,
                });
            }
        }
        refs.sort();
        Ok(refs)
    }

    /// Records of the given revisions (revision-pinned reads).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a revision file is missing or corrupt.
    pub fn records(&self, revisions: &[RevisionRef]) -> Result<Vec<Record>, StoreError> {
        let mut records = Vec::new();
        for rev in revisions {
            let path = self
                .revision_dir(&rev.space, &rev.source_id, &rev.revision_id)
                .join(RECORDS_FILE);
            let file = File::open(&path).map_err(io(&path))?;
            for line in BufReader::new(file).lines() {
                let line = line.map_err(io(&path))?;
                if line.trim().is_empty() {
                    continue;
                }
                records.push(serde_json::from_str(&line).map_err(|source| {
                    StoreError::Corrupt {
                        path: path.clone(),
                        source,
                    }
                })?);
            }
        }
        Ok(records)
    }

    /// Records of every active revision in `space`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a pointer or revision cannot be read.
    pub fn active_records(&self, space: &EmbeddingSpaceId) -> Result<Vec<Record>, StoreError> {
        self.records(&self.active(space)?)
    }

    /// Saves an experiment snapshot. Snapshots are create-only.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ExperimentExists`] when the id is taken, or an I/O error.
    pub fn save_experiment(&self, snapshot: &ExperimentSnapshot) -> Result<(), StoreError> {
        let dir = self.root.join("experiments");
        fs::create_dir_all(&dir).map_err(io(&dir))?;
        let path = dir.join(format!("{}.json", snapshot.experiment_id));
        let json = serde_json::to_vec_pretty(snapshot).map_err(|source| StoreError::Corrupt {
            path: path.clone(),
            source,
        })?;
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(StoreError::ExperimentExists(snapshot.experiment_id.clone()));
            }
            Err(e) => return Err(io(&path)(e)),
        };
        file.write_all(&json).map_err(io(&path))?;
        file.sync_all().map_err(io(&path))?;
        fsync_dir(&dir)
    }

    /// Loads an experiment snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the snapshot is missing or corrupt.
    pub fn load_experiment(&self, id: &ExperimentId) -> Result<ExperimentSnapshot, StoreError> {
        let path = self.root.join("experiments").join(format!("{id}.json"));
        let bytes = fs::read(&path).map_err(io(&path))?;
        serde_json::from_slice(&bytes).map_err(|source| StoreError::Corrupt { path, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunking::ChunkKind;
    use crate::domain::{DocumentType, ExtractionMethod, Modality};
    use crate::embedding::{DistanceMetric, EngineId};
    use crate::ids::{ChunkId, RecordId};
    use crate::record::{RecordEmbedding, RecordMetadata};
    use std::num::NonZeroU32;

    fn space() -> EmbeddingSpaceId {
        "space-00000000000000000000000000000001"
            .parse()
            .expect("space")
    }

    fn rev(n: u8) -> RevisionId {
        format!("rev-{n:016x}").parse().expect("rev")
    }

    fn record(source: &SourceId, revision: &RevisionId, index: u32, experiment: &str) -> Record {
        let chunk = ChunkId::new(source, NonZeroU32::MIN, Modality::Text, index).expect("chunk");
        Record {
            record_id: RecordId::derive(revision, &chunk, &space()).expect("id"),
            chunk_id: chunk,
            source_id: source.clone(),
            revision_id: revision.clone(),
            chunk_text: Some(format!("texto {index}")),
            modality: Modality::Text,
            asset_uri: None,
            metadata: RecordMetadata {
                source_file: "s.pdf".into(),
                source_checksum: Checksum::of_bytes(b"s"),
                document_type: DocumentType::LaboratoryReport,
                language: "pt-BR".into(),
                page_number: 1,
                section: None,
                chunk_index: index,
                chunk_kind: ChunkKind::Paragraph,
                bounding_box: None,
                extraction_method: ExtractionMethod::Native,
                extraction_engine: "test".into(),
                extraction_confidence: None,
                chunking_strategy: "structure-aware-v1".into(),
                overlap_tokens: 0,
                truncation: None,
                anonymization_status: "verified".into(),
            },
            embedding: RecordEmbedding {
                engine: EngineId::Deterministic,
                engine_version: "0".into(),
                model_id: "m".into(),
                model_revision: None,
                dimensions: 2,
                embedding_space_id: space(),
                normalized: true,
                distance_metric: DistanceMetric::Cosine,
                vector: vec![1.0, 0.0],
            },
            experiment_id: experiment.parse().expect("experiment"),
        }
    }

    fn publish(
        store: &FsRecordStore,
        source: &SourceId,
        revision: &RevisionId,
        experiment: &str,
    ) -> Result<(), StoreError> {
        let records = vec![
            record(source, revision, 0, experiment),
            record(source, revision, 1, experiment),
        ];
        let staged = store.stage(&space(), source, revision, &records)?;
        store.publish(staged)
    }

    #[test]
    fn published_revision_becomes_active_and_readable() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open(dir.path()).expect("open");
        let s1: SourceId = "S1".parse().expect("id");
        publish(&store, &s1, &rev(1), "exp-1").expect("publish");
        let active = store.active(&space()).expect("active");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].revision_id, rev(1));
        assert_eq!(store.active_records(&space()).expect("records").len(), 2);
    }

    #[test]
    fn a_fault_at_any_point_keeps_the_previous_revision_active() {
        for point in [
            FaultPoint::BeforeStage,
            FaultPoint::BeforeRevisionRename,
            FaultPoint::BeforePointerRename,
        ] {
            let dir = tempfile::tempdir().expect("tmp");
            let store = FsRecordStore::open(dir.path()).expect("open");
            let s1: SourceId = "S1".parse().expect("id");
            publish(&store, &s1, &rev(1), "exp-1").expect("first publish");
            store.inject_fault(point);
            let err = publish(&store, &s1, &rev(2), "exp-2").expect_err("fault");
            assert!(
                matches!(err, StoreError::InjectedFault(p) if p == point),
                "{err}"
            );
            let records = store.active_records(&space()).expect("records");
            assert!(
                records.iter().all(|r| r.revision_id == rev(1)),
                "{point:?}: old revision only"
            );
            assert_eq!(records.len(), 2);
            store.clear_fault();
            publish(&store, &s1, &rev(2), "exp-3").expect("retry after fault");
            let records = store.active_records(&space()).expect("records");
            assert!(
                records.iter().all(|r| r.revision_id == rev(2)),
                "{point:?}: new revision only"
            );
            assert_eq!(records.len(), 2, "{point:?}: no duplicates");
            // The replaced revision stays readable for earlier experiments.
            let old = RevisionRef {
                space: space(),
                source_id: s1.clone(),
                revision_id: rev(1),
            };
            assert_eq!(store.records(&[old]).expect("old").len(), 2);
        }
    }

    #[test]
    fn republishing_an_existing_revision_reuses_it_unchanged() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open(dir.path()).expect("open");
        let s1: SourceId = "S1".parse().expect("id");
        publish(&store, &s1, &rev(1), "exp-1").expect("publish 1");
        publish(&store, &s1, &rev(2), "exp-2").expect("publish 2");
        // The source reverts to revision 1 in a later experiment.
        publish(&store, &s1, &rev(1), "exp-3").expect("publish 1 again");
        let records = store.active_records(&space()).expect("records");
        assert!(
            records.iter().all(|r| r.experiment_id.as_str() == "exp-1"),
            "original records are kept"
        );
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn experiment_snapshots_are_create_only() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open(dir.path()).expect("open");
        let snapshot = ExperimentSnapshot {
            experiment_id: "exp-1".parse().expect("id"),
            revisions: vec![],
            fingerprints: BTreeMap::new(),
            corpus_checksum: Checksum::of_bytes(b"c"),
        };
        store.save_experiment(&snapshot).expect("save");
        assert!(matches!(
            store.save_experiment(&snapshot),
            Err(StoreError::ExperimentExists(_))
        ));
        assert_eq!(
            store
                .load_experiment(&snapshot.experiment_id)
                .expect("load"),
            snapshot
        );
    }

    #[test]
    fn abandoned_staging_is_removed_on_open_and_never_visible() {
        let dir = tempfile::tempdir().expect("tmp");
        let s1: SourceId = "S1".parse().expect("id");
        {
            let store = FsRecordStore::open(dir.path()).expect("open");
            let records = vec![record(&s1, &rev(1), 0, "exp-1")];
            let staged = store
                .stage(&space(), &s1, &rev(1), &records)
                .expect("stage");
            assert!(store.active_records(&space()).expect("records").is_empty());
            drop(staged); // simulates a crash before publish
        }
        let store = FsRecordStore::open(dir.path()).expect("reopen");
        assert!(!dir.path().join(space().as_str()).join("tmp").exists());
        assert!(store.active_records(&space()).expect("records").is_empty());
    }

    #[test]
    fn foreign_records_cannot_be_staged() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open(dir.path()).expect("open");
        let s1: SourceId = "S1".parse().expect("id");
        let s2: SourceId = "S2".parse().expect("id");
        let records = vec![record(&s2, &rev(1), 0, "exp-1")];
        assert!(matches!(
            store.stage(&space(), &s1, &rev(1), &records),
            Err(StoreError::ForeignRecord { .. })
        ));
    }
}
