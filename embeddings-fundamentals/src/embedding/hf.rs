//! Hugging Face model files shared by the local engines: pinned downloads, the hub
//! cache layout, the model's position limit, and a token counter backed by the pinned
//! `tokenizer.json`.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use sha2::{Digest, Sha256};
use tokenizers::Tokenizer;

use super::TokenCounter;
use super::local::ModelTokenizer;

/// Errors raised while reading model files.
#[derive(Debug, thiserror::Error)]
pub enum HfError {
    /// A model file is missing or malformed.
    #[error("model {model}: {reason}")]
    Model {
        /// Model identifier.
        model: String,
        /// Reason.
        reason: String,
    },
    /// A file could not be downloaded at the pinned revision.
    #[error("cannot fetch {file} of {model}: {reason}")]
    Fetch {
        /// Model identifier.
        model: String,
        /// Repository file.
        file: String,
        /// Reason.
        reason: String,
    },
    /// The tokenizer could not be loaded.
    #[error("cannot load tokenizer {path}: {reason}")]
    Tokenizer {
        /// Tokenizer path.
        path: PathBuf,
        /// Reason.
        reason: String,
    },
    /// The configured input limit exceeds what the model accepts, so the engine would
    /// truncate silently.
    #[error("model {model} accepts {model_limit} tokens, but max_input_tokens is {configured}")]
    LimitAboveModel {
        /// Model identifier.
        model: String,
        /// Position limit of the model.
        model_limit: usize,
        /// Configured limit.
        configured: usize,
    },
    /// The hub cache lock that serializes model downloads could not be taken.
    #[error("cannot lock {path}: {reason}")]
    Lock {
        /// Lock file.
        path: PathBuf,
        /// Reason.
        reason: String,
    },
}

/// Serializes downloads within the process. The file lock in [`exclusive`] alone is not
/// enough: where it is a `fcntl` lock it belongs to the process, not to one thread.
static FETCH: Mutex<()> = Mutex::new(());

/// Lock file, under the hub cache's own `.locks` directory, held while models download.
const LOCK_FILE: &str = "embeddings-fundamentals.lock";

/// Attempts per file while a process outside this crate holds the blob lock (about 5 s
/// each).
const LOCK_ATTEMPTS: u32 = 120;

/// Runs `fetch` while no other model download of this process, or of another process of
/// this crate sharing the hub cache, runs. hf-hub gives up on its per-blob lock after a
/// few seconds (0.5: about 5 s; 1.0, inside EmbedAnything: 10 s) instead of waiting for a
/// concurrent download of the same blob, so every download goes through here, including
/// the ones EmbedAnything makes itself. Blocking: call it on the blocking pool, and never
/// from inside `fetch`, which would wait for itself.
///
/// # Errors
///
/// Returns [`HfError::Lock`] when the lock file cannot be opened or locked.
pub fn exclusive<T>(fetch: impl FnOnce() -> T) -> Result<T, HfError> {
    exclusive_at(&hub_cache().join(".locks").join(LOCK_FILE), fetch)
}

fn exclusive_at<T>(lock: &Path, fetch: impl FnOnce() -> T) -> Result<T, HfError> {
    let fail = |e: std::io::Error| HfError::Lock {
        path: lock.to_owned(),
        reason: e.to_string(),
    };
    // A poisoned guard only means another download panicked; the cache is still valid.
    let _guard = FETCH.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(dir) = lock.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    // Closing the file (it drops before the guard) releases the lock.
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock)
        .map_err(fail)?;
    file.lock().map_err(fail)?;
    Ok(fetch())
}

/// Downloads `files` of `model_id` at `revision` (or reuses them from the Hugging Face
/// cache), in order, under [`exclusive`]. Blocking: call it on the blocking pool.
///
/// # Errors
///
/// Returns [`HfError::Fetch`] naming the first file that cannot be fetched, or
/// [`HfError::Lock`].
pub fn download(model_id: &str, revision: &str, files: &[&str]) -> Result<Vec<PathBuf>, HfError> {
    let fail = |file: &str, reason: String| HfError::Fetch {
        model: model_id.to_owned(),
        file: file.to_owned(),
        reason,
    };
    exclusive(|| {
        let api = hf_hub::api::sync::ApiBuilder::from_env()
            .with_progress(false)
            .build()
            .map_err(|e| fail("*", e.to_string()))?;
        let repo = api.repo(hf_hub::Repo::with_revision(
            model_id.to_owned(),
            hf_hub::RepoType::Model,
            revision.to_owned(),
        ));
        files
            .iter()
            .map(|file| {
                let mut attempt = 1;
                loop {
                    match repo.get(file) {
                        Err(hf_hub::api::sync::ApiError::LockAcquisition(_))
                            if attempt < LOCK_ATTEMPTS =>
                        {
                            attempt += 1;
                        }
                        other => return other.map_err(|e| fail(file, e.to_string())),
                    }
                }
            })
            .collect()
    })?
}

/// Hugging Face hub cache directory: `$HF_HUB_CACHE`, `$HF_HOME/hub`, or
/// `~/.cache/huggingface/hub`.
#[must_use]
pub fn hub_cache() -> PathBuf {
    if let Some(dir) = std::env::var_os("HF_HUB_CACHE") {
        return PathBuf::from(dir);
    }
    if let Some(home) = std::env::var_os("HF_HOME") {
        return PathBuf::from(home).join("hub");
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    home.join(".cache").join("huggingface").join("hub")
}

/// Path of `file` in the cached snapshot of `model_id` at `revision`.
#[must_use]
pub fn cached_file(model_id: &str, revision: &str, file: &str) -> PathBuf {
    hub_cache()
        .join(format!("models--{}", model_id.replace('/', "--")))
        .join("snapshots")
        .join(revision)
        .join(file)
}

/// Model families whose position table reserves `pad_token_id + 1` leading slots, so
/// the usable sequence length is `max_position_embeddings − pad_token_id − 1`.
const OFFSET_POSITION_MODELS: [&str; 3] = ["roberta", "xlm-roberta", "camembert"];

/// Number of tokens (special tokens included) the model accepts, from its
/// `config.json`.
///
/// # Errors
///
/// Returns [`HfError::Model`] when the file is unreadable or has no position limit.
pub fn model_position_limit(model_id: &str, config_json: &Path) -> Result<NonZeroUsize, HfError> {
    let bad = |reason: String| HfError::Model {
        model: model_id.to_owned(),
        reason,
    };
    let config: serde_json::Value = std::fs::read(config_json)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| bad(format!("cannot read {}", config_json.display())))?;
    let positions = config
        .get("max_position_embeddings")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| bad("config.json has no max_position_embeddings".to_owned()))?;
    let offset = match config.get("model_type").and_then(serde_json::Value::as_str) {
        Some(kind) if OFFSET_POSITION_MODELS.contains(&kind) => config
            .get("pad_token_id")
            .and_then(serde_json::Value::as_u64)
            .map_or(0, |pad| pad + 1),
        _ => 0,
    };
    positions
        .checked_sub(offset)
        .and_then(|n| usize::try_from(n).ok())
        .and_then(NonZeroUsize::new)
        .ok_or_else(|| bad("config.json position limit is not positive".to_owned()))
}

/// The finite input limit the pipeline enforces: the configured limit, or the model's
/// position limit when none is configured. A configured limit above the model's is
/// refused, because the engine would then truncate without provenance.
///
/// # Errors
///
/// Returns [`HfError`] when `config.json` is unusable or the configured limit is too
/// large.
pub fn effective_limit(
    model_id: &str,
    config_json: &Path,
    configured: Option<NonZeroUsize>,
) -> Result<NonZeroUsize, HfError> {
    let model_limit = model_position_limit(model_id, config_json)?;
    match configured {
        None => Ok(model_limit),
        Some(limit) if limit > model_limit => Err(HfError::LimitAboveModel {
            model: model_id.to_owned(),
            model_limit: model_limit.get(),
            configured: limit.get(),
        }),
        Some(limit) => Ok(limit),
    }
}

/// Token counter backed by the model's `tokenizer.json`.
#[derive(Debug)]
pub struct HfTokenCounter {
    tokenizer: Tokenizer,
    special_tokens: usize,
    id: String,
}

impl HfTokenCounter {
    /// Loads `tokenizer.json` with truncation and padding disabled.
    ///
    /// # Errors
    ///
    /// Returns [`HfError::Tokenizer`] when the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, HfError> {
        let fail = |reason: String| HfError::Tokenizer {
            path: path.to_owned(),
            reason,
        };
        let bytes = std::fs::read(path).map_err(|e| fail(e.to_string()))?;
        let mut tokenizer = Tokenizer::from_bytes(&bytes).map_err(|e| fail(e.to_string()))?;
        // tokenizer.json may carry truncation (MiniLM: 128) and padding settings; counts
        // must report the full, unpadded length or truncation would be invisible.
        tokenizer
            .with_truncation(None)
            .map_err(|e| fail(e.to_string()))?;
        tokenizer.with_padding(None);
        let special_tokens = tokenizer
            .encode("", true)
            .map_err(|e| fail(e.to_string()))?
            .len();
        let id = format!(
            "hf-tokenizer:sha256:{}",
            hex::encode(Sha256::digest(&bytes))
        );
        Ok(Self {
            tokenizer,
            special_tokens,
            id,
        })
    }
}

impl HfTokenCounter {
    /// Token ids of `text` with the model's special tokens, never truncated or padded.
    ///
    /// # Errors
    ///
    /// Returns the tokenizer's reason when `text` cannot be encoded.
    pub fn encode_ids(&self, text: &str) -> Result<Vec<u32>, String> {
        self.tokenizer
            .encode(text, true)
            .map(|e| e.get_ids().to_vec())
            .map_err(|e| e.to_string())
    }
}

impl TokenCounter for HfTokenCounter {
    fn count(&self, text: &str) -> usize {
        // Encoding cannot fail for valid UTF-8 with a loaded tokenizer; count words as a
        // conservative fallback rather than panicking.
        self.tokenizer
            .encode(text, false)
            .map_or_else(|_| text.split_whitespace().count(), |e| e.len())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

impl ModelTokenizer for HfTokenCounter {
    fn special_tokens(&self) -> usize {
        self.special_tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(dir: &Path, json: &str) -> PathBuf {
        let path = dir.join("config.json");
        std::fs::write(&path, json).expect("write");
        path
    }

    #[test]
    fn exclusive_fetches_run_one_at_a_time() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().expect("tmp");
        let lock = dir.path().join("locks").join(LOCK_FILE);
        let (running, most) = (AtomicUsize::new(0), AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    exclusive_at(&lock, || {
                        let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                        most.fetch_max(now, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        running.fetch_sub(1, Ordering::SeqCst);
                    })
                    .expect("lock");
                });
            }
        });
        assert_eq!(most.into_inner(), 1, "two fetches overlapped");
    }

    #[test]
    fn the_cache_lock_file_is_held_while_fetching_and_released_after() {
        // Another process opens the lock file on its own, as `other` does here.
        let dir = tempfile::tempdir().expect("tmp");
        let lock = dir.path().join("locks").join(LOCK_FILE);
        let other = || std::fs::File::open(&lock).expect("open").try_lock();
        let during = exclusive_at(&lock, other).expect("lock");
        assert!(
            matches!(during, Err(std::fs::TryLockError::WouldBlock)),
            "another process could take the lock mid-fetch: {during:?}"
        );
        assert!(other().is_ok(), "the lock outlives the fetch");
    }

    #[test]
    fn roberta_family_limits_exclude_the_padding_offset() {
        let dir = tempfile::tempdir().expect("tmp");
        let bert = config(
            dir.path(),
            r#"{"model_type":"bert","max_position_embeddings":512,"pad_token_id":0}"#,
        );
        assert_eq!(model_position_limit("m", &bert).expect("bert").get(), 512);
        let xlmr = config(
            dir.path(),
            r#"{"model_type":"xlm-roberta","max_position_embeddings":514,"pad_token_id":1}"#,
        );
        assert_eq!(model_position_limit("m", &xlmr).expect("xlmr").get(), 512);
        assert!(matches!(
            effective_limit("m", &xlmr, NonZeroUsize::new(513)),
            Err(HfError::LimitAboveModel {
                model_limit: 512,
                configured: 513,
                ..
            })
        ));
        assert_eq!(
            effective_limit("m", &xlmr, NonZeroUsize::new(256))
                .expect("configured")
                .get(),
            256
        );
        let missing = config(dir.path(), r#"{"model_type":"bert"}"#);
        assert!(matches!(
            model_position_limit("m", &missing),
            Err(HfError::Model { .. })
        ));
    }
}
