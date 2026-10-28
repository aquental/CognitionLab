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

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::ids::{Checksum, EmbeddingSpaceId, ExperimentId, Fingerprint, RevisionId, SourceId};
use crate::record::Record;

const RECORDS_FILE: &str = "records.jsonl";
/// Seal written with every revision: checksum and count of its records file.
const SEAL_FILE: &str = "revision.json";

/// Integrity seal of a revision, written in staging before the atomic rename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionSeal {
    records_checksum: Checksum,
    records: usize,
}

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
    /// Another process holds the writer lock of this store.
    #[error("store {0} is locked by another writer")]
    WriterBusy(PathBuf),
    /// A write was attempted through a read-only handle.
    #[error("store opened read-only")]
    ReadOnly,
    /// A revision does not match its integrity seal.
    #[error("revision {path} failed its integrity seal: {reason}")]
    Integrity {
        /// Revision directory.
        path: PathBuf,
        /// What does not match.
        reason: String,
    },
    /// An asset URI is not of the form `assets/<sha256 hex>.png`.
    #[error("invalid asset uri {0:?}")]
    InvalidAssetUri(String),
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

/// Directory of content-addressed image assets, relative to the store root.
const ASSETS_DIR: &str = "assets";

/// `assets/<sha256 hex>.png` for an asset checksum.
fn asset_uri(checksum: &Checksum) -> String {
    let hex = checksum
        .as_str()
        .strip_prefix("sha256:")
        .unwrap_or(checksum.as_str());
    format!("{ASSETS_DIR}/{hex}.png")
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
    records: usize,
}

/// What publishing did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// The staged records were installed as the revision.
    Installed {
        /// Records written.
        records: usize,
    },
    /// An intact revision with the same id already existed and was re-activated; the
    /// staged records were discarded.
    Reused {
        /// Records in the existing revision.
        records: usize,
    },
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
    /// Frozen evaluation inputs of an experiment run (PLAN §4.2); `None` for a plain
    /// ingestion that no evaluation belongs to.
    #[serde(default)]
    pub evaluation: Option<EvaluationProvenance>,
}

/// Exact query set and labels an experiment evaluated, by version and file checksum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationProvenance {
    /// Query-set version label.
    pub query_set_version: String,
    /// Checksum of the query-set file bytes.
    pub query_set_checksum: Checksum,
    /// Label-set version label.
    pub labels_version: String,
    /// Checksum of the label file bytes.
    pub labels_checksum: Checksum,
}

/// Filesystem record store.
#[derive(Debug)]
pub struct FsRecordStore {
    root: PathBuf,
    fault: Mutex<Option<FaultPoint>>,
    /// Held exclusive lock on `<root>/.writer.lock`; `None` for read-only handles.
    /// The OS releases it when the process exits, so a crashed writer never blocks.
    writer_lock: Option<WriterLock>,
}

/// Name of the lock file that serializes writers of one store root.
const WRITER_LOCK: &str = ".writer.lock";

/// Store roots whose writer lock this process holds.
///
/// POSIX record locks exclude other processes only, so writers inside this process
/// are excluded here. The root is claimed *before* the lock file is opened: closing
/// any descriptor of a file drops the process's record locks on it, so a second
/// in-process opener must never open (and then close) the lock file.
static HELD_ROOTS: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Exclusive writer lock on one store root.
///
/// Uses a POSIX record lock (`fcntl F_SETLK`) rather than `flock`. A `flock` belongs
/// to the open file description, so a child process spawned concurrently by any
/// thread holds a copy of the descriptor until its `exec` and keeps the lock alive
/// after this writer drops it; reopening then fails spuriously with `WriterBusy`.
/// Record locks belong to the process and are never inherited by children.
#[derive(Debug)]
struct WriterLock {
    root: PathBuf,
    file: Option<File>,
}

impl WriterLock {
    fn acquire(root: &Path) -> Result<Self, StoreError> {
        let key = fs::canonicalize(root).map_err(io(root))?;
        {
            let mut held = HELD_ROOTS.lock().unwrap_or_else(PoisonError::into_inner);
            if !held.insert(key.clone()) {
                return Err(StoreError::WriterBusy(root.to_owned()));
            }
        }
        // From here on the claim is released by `Drop` on every path.
        let mut lock = Self {
            root: key,
            file: None,
        };
        let lock_path = root.join(WRITER_LOCK);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(io(&lock_path))?;
        // The constants are small (1 and 0) but their C types differ by platform.
        fn short<T: TryInto<nix::libc::c_short>>(
            value: T,
            path: &Path,
        ) -> Result<nix::libc::c_short, StoreError> {
            value
                .try_into()
                .map_err(|_| io(path)(std::io::Error::other("lock constant does not fit c_short")))
        }
        let whole_file = nix::libc::flock {
            l_start: 0,
            l_len: 0, // 0 = to end of file, i.e. the whole file
            l_pid: 0,
            l_type: short(nix::libc::F_WRLCK, &lock_path)?,
            l_whence: short(nix::libc::SEEK_SET, &lock_path)?,
        };
        match nix::fcntl::fcntl(&file, nix::fcntl::FcntlArg::F_SETLK(&whole_file)) {
            Ok(_) => {}
            Err(nix::errno::Errno::EAGAIN | nix::errno::Errno::EACCES) => {
                return Err(StoreError::WriterBusy(root.to_owned()));
            }
            Err(errno) => return Err(io(&lock_path)(std::io::Error::from(errno))),
        }
        lock.file = Some(file);
        Ok(lock)
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        // Close the descriptor (releasing the record lock) before another in-process
        // writer may claim the root and open the lock file.
        drop(self.file.take());
        HELD_ROOTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.root);
    }
}

impl FsRecordStore {
    /// Opens the store for writing: takes the exclusive writer lock and only then
    /// removes staging directories abandoned by interrupted writers. Readers and other
    /// writers can never lose a staged revision to this cleanup.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::WriterBusy`] when another writer holds the lock, or
    /// [`StoreError::Io`] when the root cannot be created or cleaned.
    pub fn open_writer(root: &Path) -> Result<Self, StoreError> {
        fs::create_dir_all(root).map_err(io(root))?;
        let lock = WriterLock::acquire(root)?;
        let store = Self {
            root: root.to_owned(),
            fault: Mutex::new(None),
            writer_lock: Some(lock),
        };
        for space in store.spaces()? {
            let tmp = store.space_dir(&space).join("tmp");
            if tmp.exists() {
                fs::remove_dir_all(&tmp).map_err(io(&tmp))?;
            }
        }
        // Asset writes that never reached their rename leave `.tmp*` files behind.
        let assets = store.root.join(ASSETS_DIR);
        if assets.is_dir() {
            for entry in fs::read_dir(&assets).map_err(io(&assets))? {
                let path = entry.map_err(io(&assets))?.path();
                if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(".tmp"))
                {
                    fs::remove_file(&path).map_err(io(&path))?;
                }
            }
        }
        Ok(store)
    }

    /// Opens the store read-only: no lock, no cleanup, and every write is refused.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] when the root does not exist.
    pub fn open_reader(root: &Path) -> Result<Self, StoreError> {
        if !root.is_dir() {
            return Err(io(root)(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "store root not found",
            )));
        }
        Ok(Self {
            root: root.to_owned(),
            fault: Mutex::new(None),
            writer_lock: None,
        })
    }

    fn require_writer(&self) -> Result<(), StoreError> {
        if self.writer_lock.is_some() {
            Ok(())
        } else {
            Err(StoreError::ReadOnly)
        }
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

    /// Bytes on disk of the revisions active in `space` (records and seals): the index
    /// that queries read. Retained older revisions, staging, and quarantine are excluded.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a pointer or revision directory cannot be read.
    pub fn index_size_bytes(&self, space: &EmbeddingSpaceId) -> Result<u64, StoreError> {
        let mut total = 0_u64;
        for revision in self.active(space)? {
            let dir = self.revision_dir(space, &revision.source_id, &revision.revision_id);
            for entry in std::fs::read_dir(&dir).map_err(io(&dir))? {
                let path = entry.map_err(io(&dir))?.path();
                total += std::fs::metadata(&path).map_err(io(&path))?.len();
            }
        }
        Ok(total)
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
        self.require_writer()?;
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
        let seal = RevisionSeal {
            records_checksum: Checksum::of_bytes(&bytes),
            records: records.len(),
        };
        let seal_path = dir.join(SEAL_FILE);
        let seal_json = serde_json::to_vec(&seal).map_err(|source| StoreError::Corrupt {
            path: seal_path.clone(),
            source,
        })?;
        {
            let mut file = File::create(&seal_path).map_err(io(&seal_path))?;
            file.write_all(&seal_json).map_err(io(&seal_path))?;
            file.sync_all().map_err(io(&seal_path))?;
        }
        fsync_dir(&dir)?;
        Ok(StagedRevision {
            space: space.clone(),
            source_id: source_id.clone(),
            revision_id: revision_id.clone(),
            dir,
            records_checksum: Checksum::of_bytes(&bytes),
            records: records.len(),
        })
    }

    /// Publishes a staged revision. Renaming the active pointer is the commit point:
    /// a failure before it leaves the previously active revision searchable.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] on I/O failure or an injected fault.
    pub fn publish(&self, staged: StagedRevision) -> Result<PublishOutcome, StoreError> {
        self.require_writer()?;
        let target = self.revision_dir(&staged.space, &staged.source_id, &staged.revision_id);
        let parent = target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.clone());
        fs::create_dir_all(&parent).map_err(io(&parent))?;
        let (records_checksum, outcome) = match self.verified_records(&target) {
            Some(Ok((checksum, records))) => {
                // An intact revision directory only appears through an atomic rename of a
                // fully written, fsync'd staging directory, and it may be pinned by an
                // earlier experiment snapshot (a source reverted to an earlier revision).
                // It is reused as-is: its records keep the experiment id of the run that
                // produced them, and the new staging is discarded.
                fs::remove_dir_all(&staged.dir).map_err(io(&staged.dir))?;
                (checksum, PublishOutcome::Reused { records })
            }
            Some(Err(reason)) => {
                // A damaged directory (missing or unreadable records) is quarantined under
                // tmp/ and replaced, so a retry can heal it.
                // The quarantine lives outside tmp/ (which `open` deletes), so the damaged
                // bytes survive a crash or fault between this move and the install.
                warn!(path = %target.display(), reason = %reason, "quarantining damaged revision");
                let quarantine = self.space_dir(&staged.space).join("quarantine");
                fs::create_dir_all(&quarantine).map_err(io(&quarantine))?;
                let stale = quarantine.join(format!(
                    "{}-{}-{}",
                    staged.source_id,
                    staged.revision_id,
                    staged
                        .dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ));
                fs::rename(&target, &stale).map_err(io(&target))?;
                fsync_dir(&quarantine)?;
                self.install(&staged, &target, &parent)?;
                (
                    staged.records_checksum.clone(),
                    PublishOutcome::Installed {
                        records: staged.records,
                    },
                )
            }
            None => {
                self.install(&staged, &target, &parent)?;
                (
                    staged.records_checksum.clone(),
                    PublishOutcome::Installed {
                        records: staged.records,
                    },
                )
            }
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
        fsync_dir(&active)?;
        Ok(outcome)
    }

    fn install(
        &self,
        staged: &StagedRevision,
        target: &Path,
        parent: &Path,
    ) -> Result<(), StoreError> {
        self.check_fault(FaultPoint::BeforeRevisionRename)?;
        fs::rename(&staged.dir, target).map_err(io(target))?;
        fsync_dir(parent)
    }

    /// Checksum and record count of an existing revision directory: `None` when it does
    /// not exist, `Some(Err)` when it exists but its records are missing or unreadable.
    fn verified_records(&self, target: &Path) -> Option<Result<(Checksum, usize), String>> {
        if !target.exists() {
            return None;
        }
        Some(Self::check_seal(target))
    }

    /// Verifies a revision directory against its seal: the records file must hash to
    /// the sealed checksum, hold the sealed number of records, and every line must parse.
    /// Truncated (even empty) or altered files fail, not only unparseable ones.
    fn check_seal(dir: &Path) -> Result<(Checksum, usize), String> {
        Self::load_sealed(dir).map(|(checksum, records)| (checksum, records.len()))
    }

    /// Reads a revision once and returns its records only if they match the seal, so the
    /// bytes that are verified are exactly the bytes that are returned.
    fn load_sealed(dir: &Path) -> Result<(Checksum, Vec<Record>), String> {
        let seal_path = dir.join(SEAL_FILE);
        let seal: RevisionSeal = fs::read(&seal_path)
            .map_err(|e| format!("cannot read seal: {e}"))
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| format!("corrupt seal: {e}")))?;
        let bytes =
            fs::read(dir.join(RECORDS_FILE)).map_err(|e| format!("cannot read records: {e}"))?;
        let checksum = Checksum::of_bytes(&bytes);
        if checksum != seal.records_checksum {
            return Err(format!(
                "records checksum {checksum} does not match the seal {}",
                seal.records_checksum
            ));
        }
        let mut records = Vec::with_capacity(seal.records);
        for line in String::from_utf8_lossy(&bytes)
            .lines()
            .filter(|l| !l.trim().is_empty())
        {
            records.push(
                serde_json::from_str::<Record>(line)
                    .map_err(|e| format!("corrupt record line: {e}"))?,
            );
        }
        if records.len() != seal.records {
            return Err(format!(
                "{} records on disk, seal says {}",
                records.len(),
                seal.records
            ));
        }
        Ok((checksum, records))
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

    /// Removes the active pointer of `source` in `space`, so search no longer sees any
    /// revision of it; returns whether a pointer existed. Revisions are kept (PRD §8) and
    /// stay readable through the experiment snapshots that pin them. Used when a
    /// reprocessed source no longer yields any record for the run's modality.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ReadOnly`] on a reader, and [`StoreError`] when the pointer
    /// cannot be removed or its directory cannot be synced.
    pub fn deactivate(
        &self,
        space: &EmbeddingSpaceId,
        source: &SourceId,
    ) -> Result<bool, StoreError> {
        self.require_writer()?;
        let path = self.pointer_path(space, source);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(io(&path)(e)),
        }
        if let Some(dir) = path.parent() {
            fsync_dir(dir)?;
        }
        Ok(true)
    }

    /// Whether the active pointer of `source` names an intact revision: the records file
    /// exists and its checksum equals the one recorded in the pointer.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the pointer exists but cannot be read.
    pub fn active_is_intact(
        &self,
        space: &EmbeddingSpaceId,
        source: &SourceId,
    ) -> Result<bool, StoreError> {
        let Some(pointer) = self.active_pointer(space, source)? else {
            return Ok(false);
        };
        let dir = self.revision_dir(space, source, &pointer.revision_id);
        Ok(Self::check_seal(&dir).is_ok_and(|(checksum, _)| checksum == pointer.records_checksum))
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
            let dir = self.revision_dir(&rev.space, &rev.source_id, &rev.revision_id);
            let (_, mut sealed) =
                Self::load_sealed(&dir).map_err(|reason| StoreError::Integrity {
                    path: dir.clone(),
                    reason,
                })?;
            records.append(&mut sealed);
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

    /// Stores a prepared (metadata-free) PNG under `assets/<sha256>.png` and returns its
    /// URI, relative to the store root. Assets are content-addressed and shared by every
    /// space: an identical asset is stored once, and an existing file whose bytes no
    /// longer match its name is replaced. The write is atomic (temporary file, fsync,
    /// rename, directory fsync); an asset of a run that later fails is harmless.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ReadOnly`] for a reader and [`StoreError::Io`] on failure.
    pub fn put_asset(&self, png: &[u8]) -> Result<String, StoreError> {
        self.require_writer()?;
        let checksum = Checksum::of_bytes(png);
        let uri = asset_uri(&checksum);
        let path = self.asset_path(&uri)?;
        if fs::read(&path).is_ok_and(|bytes| Checksum::of_bytes(&bytes) == checksum) {
            return Ok(uri);
        }
        let dir = self.root.join(ASSETS_DIR);
        fs::create_dir_all(&dir).map_err(io(&dir))?;
        let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(io(&dir))?;
        tmp.write_all(png).map_err(io(&path))?;
        tmp.as_file().sync_all().map_err(io(&path))?;
        tmp.persist(&path).map_err(|e| io(&path)(e.error))?;
        fsync_dir(&dir)?;
        Ok(uri)
    }

    /// Path of the asset named by `uri` (`assets/<sha256 hex>.png`).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidAssetUri`] for any other form, so a URI can never
    /// name a file outside the asset directory.
    pub fn asset_path(&self, uri: &str) -> Result<PathBuf, StoreError> {
        let name = uri
            .strip_prefix("assets/")
            .and_then(|n| n.strip_suffix(".png"))
            .filter(|hex| {
                hex.len() == 64
                    && hex
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .ok_or_else(|| StoreError::InvalidAssetUri(uri.to_owned()))?;
        Ok(self.root.join(ASSETS_DIR).join(format!("{name}.png")))
    }

    /// Reads the asset named by `uri` and checks its bytes against its name.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Integrity`] when the bytes do not match, and other
    /// [`StoreError`]s for an invalid URI or a missing file.
    pub fn read_asset(&self, uri: &str) -> Result<Vec<u8>, StoreError> {
        let path = self.asset_path(uri)?;
        let bytes = fs::read(&path).map_err(io(&path))?;
        if asset_uri(&Checksum::of_bytes(&bytes)) == uri {
            Ok(bytes)
        } else {
            Err(StoreError::Integrity {
                path,
                reason: "asset bytes do not match their checksum".to_owned(),
            })
        }
    }

    /// Saves an experiment snapshot. Snapshots are create-only.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ExperimentExists`] when the id is taken, or an I/O error.
    pub fn save_experiment(&self, snapshot: &ExperimentSnapshot) -> Result<(), StoreError> {
        self.require_writer()?;
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

    /// Whether a snapshot file exists for `id` (readable or not).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] when existence cannot be determined.
    pub fn experiment_exists(&self, id: &ExperimentId) -> Result<bool, StoreError> {
        let path = self.root.join("experiments").join(format!("{id}.json"));
        path.try_exists().map_err(io(&path))
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
                original_asset: None,
                generated_caption: None,
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
        store.publish(staged).map(|_| ())
    }

    #[test]
    fn published_revision_becomes_active_and_readable() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open_writer(dir.path()).expect("open");
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
            let store = FsRecordStore::open_writer(dir.path()).expect("open");
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
        let store = FsRecordStore::open_writer(dir.path()).expect("open");
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
    fn reuse_reports_the_existing_records_and_damaged_revisions_heal() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open_writer(dir.path()).expect("open");
        let s1: SourceId = "S1".parse().expect("id");
        let stage = |experiment: &str| {
            let records = vec![
                record(&s1, &rev(1), 0, experiment),
                record(&s1, &rev(1), 1, experiment),
            ];
            store
                .stage(&space(), &s1, &rev(1), &records)
                .expect("stage")
        };
        assert_eq!(
            store.publish(stage("exp-1")).expect("first"),
            PublishOutcome::Installed { records: 2 }
        );
        assert_eq!(
            store.publish(stage("exp-2")).expect("again"),
            PublishOutcome::Reused { records: 2 }
        );
        // Damage the revision: the next publish quarantines it and installs the new records.
        let target = dir
            .path()
            .join(space().as_str())
            .join("revisions/S1")
            .join(rev(1).as_str());
        fs::remove_file(target.join(RECORDS_FILE)).expect("damage");
        assert_eq!(
            store.publish(stage("exp-3")).expect("heal"),
            PublishOutcome::Installed { records: 2 }
        );
        let records = store.active_records(&space()).expect("records");
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.experiment_id.as_str() == "exp-3"));
        fs::write(target.join(RECORDS_FILE), b"not json\n").expect("corrupt");
        assert_eq!(
            store.publish(stage("exp-4")).expect("heal corrupt"),
            PublishOutcome::Installed { records: 2 }
        );
        // Truncation to zero bytes or to a valid prefix still parses, but breaks the seal.
        let full = fs::read(target.join(RECORDS_FILE)).expect("read");
        let first_line = full.split(|b| *b == b'\n').next().expect("line").to_vec();
        for truncated in [Vec::new(), [first_line, b"\n".to_vec()].concat()] {
            fs::write(target.join(RECORDS_FILE), &truncated).expect("truncate");
            assert!(
                !store.active_is_intact(&space(), &s1).expect("intact"),
                "truncation detected"
            );
            assert_eq!(
                store.publish(stage("exp-5")).expect("heal"),
                PublishOutcome::Installed { records: 2 }
            );
            assert!(store.active_is_intact(&space(), &s1).expect("intact"));
            assert_eq!(store.active_records(&space()).expect("records").len(), 2);
        }
    }

    #[test]
    fn a_fault_while_replacing_a_damaged_revision_keeps_its_bytes_outside_tmp() {
        let dir = tempfile::tempdir().expect("tmp");
        let s1: SourceId = "S1".parse().expect("id");
        {
            let store = FsRecordStore::open_writer(dir.path()).expect("open");
            publish(&store, &s1, &rev(1), "exp-1").expect("publish");
            let target = dir
                .path()
                .join(space().as_str())
                .join("revisions/S1")
                .join(rev(1).as_str());
            fs::write(target.join(RECORDS_FILE), b"not json\n").expect("damage");
            assert!(!store.active_is_intact(&space(), &s1).expect("intact"));
            store.inject_fault(FaultPoint::BeforeRevisionRename);
            assert!(publish(&store, &s1, &rev(1), "exp-2").is_err());
        }
        // Reopening deletes tmp/, but the quarantined directory survives.
        let store = FsRecordStore::open_writer(dir.path()).expect("reopen");
        let quarantine = dir.path().join(space().as_str()).join("quarantine");
        assert_eq!(fs::read_dir(&quarantine).expect("quarantine").count(), 1);
        // A retry installs a fresh revision and the pointer resolves to parseable records.
        publish(&store, &s1, &rev(1), "exp-3").expect("retry");
        assert!(store.active_is_intact(&space(), &s1).expect("intact"));
        assert_eq!(store.active_records(&space()).expect("records").len(), 2);
    }

    #[test]
    fn readers_never_clean_staging_and_writers_are_exclusive() {
        let dir = tempfile::tempdir().expect("tmp");
        let writer = FsRecordStore::open_writer(dir.path()).expect("writer");
        assert!(matches!(
            FsRecordStore::open_writer(dir.path()),
            Err(StoreError::WriterBusy(_))
        ));
        let s1: SourceId = "S1".parse().expect("id");
        let staged = writer
            .stage(&space(), &s1, &rev(1), &[record(&s1, &rev(1), 0, "exp-1")])
            .expect("stage");
        let reader = FsRecordStore::open_reader(dir.path()).expect("reader");
        assert!(matches!(
            reader.stage(&space(), &s1, &rev(1), &[]),
            Err(StoreError::ReadOnly)
        ));
        assert_eq!(
            writer
                .publish(staged)
                .expect("publish after a reader opened"),
            PublishOutcome::Installed { records: 1 }
        );
        assert_eq!(reader.active_records(&space()).expect("read").len(), 1);
        drop(writer);
        FsRecordStore::open_writer(dir.path()).expect("lock released on drop");
    }

    #[test]
    fn reads_refuse_revisions_that_fail_their_seal() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open_writer(dir.path()).expect("open");
        let s1: SourceId = "S1".parse().expect("id");
        publish(&store, &s1, &rev(1), "exp-1").expect("publish");
        let target = dir
            .path()
            .join(space().as_str())
            .join("revisions/S1")
            .join(rev(1).as_str());
        let full = fs::read(target.join(RECORDS_FILE)).expect("read");
        let first_line = [full.split(|b| *b == b'\n').next().expect("line"), b"\n"].concat();
        for damaged in [Vec::new(), first_line] {
            fs::write(target.join(RECORDS_FILE), &damaged).expect("damage");
            assert!(matches!(
                store.active_records(&space()),
                Err(StoreError::Integrity { .. })
            ));
        }
    }

    #[test]
    fn experiment_snapshots_are_create_only() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open_writer(dir.path()).expect("open");
        let snapshot = ExperimentSnapshot {
            experiment_id: "exp-1".parse().expect("id"),
            revisions: vec![],
            fingerprints: BTreeMap::new(),
            corpus_checksum: Checksum::of_bytes(b"c"),
            evaluation: None,
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
            let store = FsRecordStore::open_writer(dir.path()).expect("open");
            let records = vec![record(&s1, &rev(1), 0, "exp-1")];
            let staged = store
                .stage(&space(), &s1, &rev(1), &records)
                .expect("stage");
            assert!(store.active_records(&space()).expect("records").is_empty());
            drop(staged); // simulates a crash before publish
        }
        let store = FsRecordStore::open_writer(dir.path()).expect("reopen");
        assert!(!dir.path().join(space().as_str()).join("tmp").exists());
        assert!(store.active_records(&space()).expect("records").is_empty());
    }

    #[test]
    fn writer_lock_is_released_on_drop_while_children_are_spawned() {
        // Regression: with `flock`, a child spawned by another thread between fork
        // and exec held the lock past drop, so reopening failed with `WriterBusy`.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let spawners: Vec<_> = (0..4)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = std::process::Command::new("/bin/sleep")
                            .arg("0.05")
                            .status();
                    }
                })
            })
            .collect();
        let dir = tempfile::tempdir().expect("tmp");
        let (mut busy, mut opens) = (0_u32, 0_u32);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        while std::time::Instant::now() < deadline {
            opens += 1;
            match FsRecordStore::open_writer(dir.path()) {
                Ok(store) => drop(store),
                Err(StoreError::WriterBusy(_)) => busy += 1,
                Err(e) => panic!("unexpected error: {e}"),
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for s in spawners {
            s.join().expect("spawner thread");
        }
        assert_eq!(
            busy, 0,
            "reopen after drop reported WriterBusy {busy}/{opens} times"
        );
    }

    /// Child half of `writer_lock_excludes_another_process`: a no-op unless the
    /// parent sets `EF_LOCK_HOLDER_ROOT`, then holds the writer lock until stdin closes.
    #[test]
    fn lock_holder_child() {
        let Some(root) = std::env::var_os("EF_LOCK_HOLDER_ROOT") else {
            return;
        };
        let _store = FsRecordStore::open_writer(Path::new(&root)).expect("child acquires lock");
        println!("LOCKED");
        std::io::Write::flush(&mut std::io::stdout()).expect("flush");
        let mut sink = String::new();
        while std::io::stdin().read_line(&mut sink).expect("stdin") > 0 {}
    }

    #[test]
    fn writer_lock_excludes_another_process() {
        use std::io::BufRead as _;
        let dir = tempfile::tempdir().expect("tmp");
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "storage::tests::lock_holder_child",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("EF_LOCK_HOLDER_ROOT", dir.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn lock holder");
        let stdout = child.stdout.take().expect("child stdout");
        let mut lines = std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok);
        let locked = lines.any(|line| line.contains("LOCKED"));
        assert!(locked, "child never reported holding the lock");
        assert!(
            matches!(
                FsRecordStore::open_writer(dir.path()),
                Err(StoreError::WriterBusy(_))
            ),
            "a second process must not open the store for writing"
        );
        drop(child.stdin.take()); // EOF releases the child
        lines.for_each(drop); // keep the pipe open until the child's harness finishes
        assert!(child.wait().expect("wait child").success());
        FsRecordStore::open_writer(dir.path()).expect("lock is free after the holder exits");
    }

    #[test]
    fn assets_are_content_addressed_atomic_and_verified() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open_writer(dir.path()).expect("store");
        let uri = store.put_asset(b"png bytes").expect("put");
        let hex = Checksum::of_bytes(b"png bytes")
            .as_str()
            .trim_start_matches("sha256:")
            .to_owned();
        assert_eq!(uri, format!("assets/{hex}.png"));
        assert_eq!(store.read_asset(&uri).expect("read"), b"png bytes");
        assert_eq!(
            store.put_asset(b"png bytes").expect("again"),
            uri,
            "idempotent"
        );
        // A damaged asset is detected on read and healed by the next put.
        let path = store.asset_path(&uri).expect("path");
        std::fs::write(&path, b"tampered").expect("tamper");
        assert!(matches!(
            store.read_asset(&uri),
            Err(StoreError::Integrity { .. })
        ));
        store.put_asset(b"png bytes").expect("heal");
        assert_eq!(store.read_asset(&uri).expect("read"), b"png bytes");
        // URIs cannot leave the asset directory.
        for bad in [
            "../records.jsonl",
            "assets/../../etc/passwd.png",
            &format!("assets/{}.png", "A".repeat(64)),
            "assets/abc.png",
            &format!("/tmp/{hex}.png"),
        ] {
            assert!(
                matches!(store.asset_path(bad), Err(StoreError::InvalidAssetUri(_))),
                "{bad}"
            );
        }
        // Abandoned temporary files are removed when a writer opens the store.
        let stray = dir.path().join("assets").join(".tmpXYZ");
        std::fs::write(&stray, b"partial").expect("stray");
        drop(store);
        let reader = FsRecordStore::open_reader(dir.path()).expect("reader");
        assert!(matches!(reader.put_asset(b"x"), Err(StoreError::ReadOnly)));
        assert!(stray.exists(), "readers never clean");
        drop(reader);
        let _writer = FsRecordStore::open_writer(dir.path()).expect("writer");
        assert!(!stray.exists());
    }

    #[test]
    fn foreign_records_cannot_be_staged() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = FsRecordStore::open_writer(dir.path()).expect("open");
        let s1: SourceId = "S1".parse().expect("id");
        let s2: SourceId = "S2".parse().expect("id");
        let records = vec![record(&s2, &rev(1), 0, "exp-1")];
        assert!(matches!(
            store.stage(&space(), &s1, &rev(1), &records),
            Err(StoreError::ForeignRecord { .. })
        ));
    }
}
