//! OpenAI embeddings adapter (remote), behind feature `engine-openai`.
//!
//! The adapter sends raw text (OpenAI models use no prefix) to `POST /v1/embeddings`
//! and reports the provider's token usage for cost estimates. HTTP 408, 429, and 5xx
//! responses, timeouts, and connection failures are transient; every other failure is
//! permanent. Error messages carry the status and the provider's error type or code,
//! never request or response text. The API key is read only by [`load`], which the
//! factory calls after the corpus remote-permission check, and it is never logged or
//! printed (`Debug` omits it).

use std::num::NonZeroUsize;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{
    AssetRef, Capabilities, Capability, DistanceMetric, Embedding, EmbeddingBatch, EmbeddingError,
    EmbeddingStrategy, EngineId, InputKind, ModelInfo, SpaceDescriptor, TextInput, TokenCounter,
    Usage, l2_normalize,
};
use crate::domain::Execution;

/// Default API base URL.
pub const DEFAULT_API_BASE: &str = "https://api.openai.com/v1";

/// Environment variable holding the API key.
pub const API_KEY_ENV: &str = "OPENAI_API_KEY";

/// Engine version recorded in provenance: the API generation this adapter speaks.
pub const ENGINE_VERSION: &str = "v1";

/// Input limit of the `text-embedding-3-*` models in `cl100k_base` tokens.
pub const MODEL_MAX_TOKENS: usize = 8191;

/// Inputs accepted per request.
pub const MAX_BATCH: usize = 2048;

/// Errors raised while building the adapter.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The model is not pinned (PRD §6.4).
    #[error("model {model} has no pinned model_revision")]
    MissingRevision {
        /// Model identifier.
        model: String,
    },
    /// The pinned revision must name the model snapshot that is requested.
    #[error("model_revision {revision} does not name model {model}")]
    RevisionMismatch {
        /// Model identifier.
        model: String,
        /// Configured revision.
        revision: String,
    },
    /// No credential is available.
    #[error("{API_KEY_ENV} is not set")]
    MissingKey,
    /// The configured limit is above the model's.
    #[error("max_input_tokens {configured} is above the model limit of {MODEL_MAX_TOKENS}")]
    LimitAboveModel {
        /// Configured limit.
        configured: usize,
    },
    /// The HTTP client could not be built.
    #[error("cannot build the HTTP client: {0}")]
    Client(String),
}

/// `cl100k_base` token counter (the `text-embedding-3-*` tokenizer).
#[derive(Debug, Clone, Copy, Default)]
pub struct Cl100kTokenCounter;

impl TokenCounter for Cl100kTokenCounter {
    fn count(&self, text: &str) -> usize {
        tiktoken_rs::cl100k_base_singleton()
            .encode_ordinary(text)
            .len()
    }

    fn id(&self) -> &str {
        "tiktoken:cl100k_base"
    }
}

/// Input budget in `cl100k_base` tokens: the model limit minus prefix tokens.
fn budget(
    limit: Option<std::num::NonZeroUsize>,
    policy: super::PrefixPolicy,
    kind: InputKind,
) -> Option<usize> {
    let prefix = Cl100kTokenCounter.count(policy.prefix(kind));
    Some(limit?.get().saturating_sub(prefix))
}

/// The OpenAI adapter.
pub struct OpenAiAdapter {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    info: ModelInfo,
    capabilities: Capabilities,
}

impl std::fmt::Debug for OpenAiAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiAdapter")
            .field("endpoint", &self.endpoint)
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

/// Request body of `POST /embeddings`.
#[derive(Debug, Serialize)]
pub struct EmbeddingsRequest<'a> {
    /// Model name.
    pub model: &'a str,
    /// Texts exactly as submitted.
    pub input: &'a [String],
    /// Always `float`.
    pub encoding_format: &'static str,
}

#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingItem>,
    model: String,
    usage: Option<ResponseUsage>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct ResponseUsage {
    total_tokens: Option<u64>,
    prompt_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: Option<ErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct ErrorDetail {
    #[serde(rename = "type")]
    kind: Option<String>,
    code: Option<String>,
}

/// Retry class of an HTTP status (PLAN §0.1 error classes).
#[must_use]
pub fn status_is_transient(status: u16) -> bool {
    matches!(status, 408 | 429) || (500..600).contains(&status)
}

fn transport_error(e: &reqwest::Error) -> EmbeddingError {
    if e.is_timeout() || e.is_connect() || e.is_request() {
        EmbeddingError::Transient {
            reason: format!("openai request failed: {}", without_url(e)),
        }
    } else {
        EmbeddingError::Permanent {
            reason: format!("openai request failed: {}", without_url(e)),
        }
    }
}

/// The reqwest error message without the URL (it is fixed and uninteresting).
fn without_url(e: &reqwest::Error) -> String {
    let mut kind = Vec::new();
    if e.is_timeout() {
        kind.push("timeout");
    }
    if e.is_connect() {
        kind.push("connect");
    }
    if e.is_decode() {
        kind.push("decode");
    }
    if e.is_body() {
        kind.push("body");
    }
    if kind.is_empty() {
        kind.push("request");
    }
    kind.join("+")
}

impl OpenAiAdapter {
    /// Builds the adapter with an explicit key and endpoint (tests use a local server).
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] for an unpinned model, a limit above the model's, or an
    /// HTTP client that cannot be built.
    pub fn with_key(
        model_id: &str,
        model_revision: Option<&str>,
        dimensions: NonZeroUsize,
        max_input_tokens: Option<NonZeroUsize>,
        api_base: &str,
        api_key: String,
    ) -> Result<Self, LoadError> {
        let revision = model_revision.ok_or_else(|| LoadError::MissingRevision {
            model: model_id.to_owned(),
        })?;
        // The API has no commit hashes: the pin is the model snapshot name, and the
        // response's `model` field is checked against it on every call.
        if revision != model_id {
            return Err(LoadError::RevisionMismatch {
                model: model_id.to_owned(),
                revision: revision.to_owned(),
            });
        }
        let limit = match max_input_tokens {
            Some(limit) if limit.get() > MODEL_MAX_TOKENS => {
                return Err(LoadError::LimitAboveModel {
                    configured: limit.get(),
                });
            }
            Some(limit) => limit,
            None => NonZeroUsize::new(MODEL_MAX_TOKENS).unwrap_or(NonZeroUsize::MIN),
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| LoadError::Client(without_url(&e)))?;
        let info = ModelInfo::from_descriptor(SpaceDescriptor {
            engine: EngineId::OpenAi,
            engine_version: ENGINE_VERSION.to_owned(),
            model_id: model_id.to_owned(),
            model_revision: Some(revision.to_owned()),
            weight_format: None,
            dimensions,
            normalized: true,
            metric: DistanceMetric::Cosine,
            prefix_policy: super::PrefixPolicy::None,
            preprocessing: format!("encoding_format=float;max_input_tokens={limit}"),
        })
        .map_err(|e| LoadError::Client(e.to_string()))?;
        Ok(Self {
            client,
            endpoint: format!("{}/embeddings", api_base.trim_end_matches('/')),
            api_key,
            info,
            capabilities: Capabilities {
                text_documents: true,
                text_queries: true,
                images: false,
                text_to_image_compatible: false,
                batch: true,
                max_input_tokens: Some(limit),
                max_batch: NonZeroUsize::new(MAX_BATCH),
                execution: Execution::Remote,
            },
        })
    }

    /// Reads [`API_KEY_ENV`] and builds the adapter. Call only after the corpus
    /// remote-permission check has passed.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::MissingKey`] when the key is unset, and the errors of
    /// [`OpenAiAdapter::with_key`].
    pub fn load(
        model_id: &str,
        model_revision: Option<&str>,
        dimensions: NonZeroUsize,
        max_input_tokens: Option<NonZeroUsize>,
        api_base: Option<&str>,
    ) -> Result<Self, LoadError> {
        let key = std::env::var(API_KEY_ENV)
            .ok()
            .filter(|k| !k.trim().is_empty())
            .ok_or(LoadError::MissingKey)?;
        Self::with_key(
            model_id,
            model_revision,
            dimensions,
            max_input_tokens,
            api_base.unwrap_or(DEFAULT_API_BASE),
            key,
        )
    }

    async fn call(
        &self,
        input: &[String],
    ) -> Result<(Vec<Vec<f32>>, Option<Usage>), EmbeddingError> {
        let body = EmbeddingsRequest {
            model: &self.info.model_id,
            input,
            encoding_format: "float",
        };
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_error(&e))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let detail = response
                .json::<ErrorBody>()
                .await
                .ok()
                .and_then(|b| b.error)
                .map(|d| {
                    [d.kind, d.code]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("/")
                })
                .unwrap_or_default();
            let reason = format!("openai returned HTTP {status} {detail}")
                .trim_end()
                .to_owned();
            return Err(if status_is_transient(status) {
                EmbeddingError::Transient { reason }
            } else {
                EmbeddingError::Permanent { reason }
            });
        }
        let parsed: EmbeddingsResponse = response.json().await.map_err(|e| transport_error(&e))?;
        if parsed.model != self.info.model_id {
            return Err(EmbeddingError::Permanent {
                reason: format!(
                    "openai served model {}, expected {}",
                    parsed.model, self.info.model_id
                ),
            });
        }
        order_by_index(parsed.data, input.len()).map(|vectors| {
            (
                vectors,
                parsed
                    .usage
                    .and_then(|u| u.total_tokens.or(u.prompt_tokens))
                    .map(|tokens| Usage { tokens }),
            )
        })
    }

    async fn embed_text(
        &self,
        kind: InputKind,
        raw: &[TextInput],
    ) -> Result<EmbeddingBatch, EmbeddingError> {
        let capability = match kind {
            InputKind::Document => Capability::TextDocuments,
            InputKind::Query => Capability::TextQueries,
        };
        self.capabilities.require(capability, &self.info)?;
        self.capabilities.check_batch(raw.len())?;
        if raw.is_empty() {
            return Ok(EmbeddingBatch::default());
        }
        // BPE tokenization (and the first load of the cl100k tables) is CPU work, so the
        // budget check and prefixing run on the blocking pool (PLAN §2.3).
        let (limit, policy, inputs) = (
            self.capabilities.max_input_tokens,
            self.info.prefix_policy,
            raw.to_vec(),
        );
        let texts = tokio::task::spawn_blocking(move || {
            if let Some(budget) = budget(limit, policy, kind) {
                for input in &inputs {
                    let tokens = Cl100kTokenCounter.count(&input.text);
                    if tokens > budget {
                        return Err(EmbeddingError::InputTooLong {
                            item: input.id.clone(),
                            tokens,
                            limit: budget,
                        });
                    }
                }
            }
            Ok(inputs
                .iter()
                .map(|i| policy.apply(kind, &i.text).into_owned())
                .collect::<Vec<String>>())
        })
        .await
        .map_err(|e| EmbeddingError::Permanent {
            reason: format!("tokenization task failed: {e}"),
        })??;
        let (vectors, usage) = self.call(&texts).await?;
        let embeddings = raw
            .iter()
            .zip(vectors)
            .map(|(i, mut vector)| {
                l2_normalize(&mut vector);
                Embedding {
                    id: i.id.clone(),
                    vector,
                }
            })
            .collect();
        Ok(EmbeddingBatch { embeddings, usage })
    }
}

/// Places each returned vector at its `index`; every index must appear exactly once.
fn order_by_index(
    data: Vec<EmbeddingItem>,
    expected: usize,
) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    if data.len() != expected {
        return Err(EmbeddingError::Permanent {
            reason: format!(
                "openai returned {} vectors for {expected} inputs",
                data.len()
            ),
        });
    }
    let mut slots: Vec<Option<Vec<f32>>> = vec![None; expected];
    for item in data {
        match slots.get_mut(item.index) {
            Some(slot @ None) => *slot = Some(item.embedding),
            _ => {
                return Err(EmbeddingError::Permanent {
                    reason: format!(
                        "openai returned an invalid or repeated index {}",
                        item.index
                    ),
                });
            }
        }
    }
    slots
        .into_iter()
        .map(|s| {
            s.ok_or_else(|| EmbeddingError::Permanent {
                reason: "openai response is missing an index".to_owned(),
            })
        })
        .collect()
}

#[async_trait::async_trait]
impl EmbeddingStrategy for OpenAiAdapter {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn model_info(&self) -> &ModelInfo {
        &self.info
    }

    fn token_counter(&self) -> &dyn TokenCounter {
        &Cl100kTokenCounter
    }

    fn input_budget(&self, kind: InputKind) -> Option<usize> {
        budget(
            self.capabilities.max_input_tokens,
            self.info.prefix_policy,
            kind,
        )
    }

    async fn embed_documents(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.embed_text(InputKind::Document, raw).await
    }

    async fn embed_queries(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.embed_text(InputKind::Query, raw).await
    }

    async fn embed_images(&self, _assets: &[AssetRef]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.capabilities.require(Capability::Images, &self.info)?;
        Ok(EmbeddingBatch::default())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::embedding::ErrorClass;
    use crate::ids::ItemId;

    /// One canned HTTP response per request; records each request's headers and body.
    async fn server(responses: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<(String, String)>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        tokio::spawn(async move {
            for (status, body) in responses {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0_u8; 4096];
                let (head, payload) = loop {
                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break (String::new(), String::new());
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let head = text[..end].to_owned();
                        let length = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + length {
                            let body = String::from_utf8_lossy(&buf[end + 4..end + 4 + length]);
                            break (head, body.to_string());
                        }
                    }
                };
                log.lock().expect("log").push((head, payload));
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        (format!("http://{addr}/v1"), seen)
    }

    fn adapter(base: &str) -> OpenAiAdapter {
        OpenAiAdapter::with_key(
            "text-embedding-3-small",
            Some("text-embedding-3-small"),
            NonZeroUsize::new(2).expect("dims"),
            None,
            base,
            "sk-test".to_owned(),
        )
        .expect("adapter")
    }

    fn input(id: &str, text: &str) -> TextInput {
        TextInput {
            id: id.parse::<ItemId>().expect("id"),
            text: text.into(),
        }
    }

    const OK: &str = r#"{"object":"list","model":"text-embedding-3-small",
        "data":[{"object":"embedding","index":1,"embedding":[0.0,2.0]},
                {"object":"embedding","index":0,"embedding":[3.0,4.0]}],
        "usage":{"prompt_tokens":7,"total_tokens":7}}"#;

    #[tokio::test]
    async fn sends_raw_text_and_reorders_vectors_by_index() {
        let (base, seen) = server(vec![(200, OK.to_owned())]).await;
        let batch = adapter(&base)
            .embed_queries(&[
                input("a", "Onde está a hemoglobina?"),
                input("b", "glicemia"),
            ])
            .await
            .expect("embed");
        let (head, body) = seen.lock().expect("seen")[0].clone();
        assert!(head.starts_with("POST /v1/embeddings "), "{head}");
        assert!(
            head.to_ascii_lowercase()
                .contains("authorization: bearer sk-test")
        );
        let body: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert_eq!(
            body,
            serde_json::json!({
                "model": "text-embedding-3-small",
                "input": ["Onde está a hemoglobina?", "glicemia"],
                "encoding_format": "float"
            })
        );
        assert_eq!(batch.usage, Some(Usage { tokens: 7 }));
        assert_eq!(batch.embeddings[0].id.as_str(), "a");
        approx::assert_relative_eq!(batch.embeddings[0].vector[1], 0.8, epsilon = 1e-6);
        approx::assert_relative_eq!(batch.embeddings[1].vector[1], 1.0, epsilon = 1e-6);
    }

    #[tokio::test]
    async fn http_status_decides_the_retry_class() {
        let error = r#"{"error":{"message":"irrelevant","type":"rate_limit","code":"x"}}"#;
        let (base, _) = server(vec![
            (429, error.to_owned()),
            (503, "{}".to_owned()),
            (400, error.to_owned()),
            (401, "{}".to_owned()),
        ])
        .await;
        let openai = adapter(&base);
        let mut classes = Vec::new();
        for _ in 0..4 {
            let err = openai
                .embed_documents(&[input("a", "x")])
                .await
                .expect_err("error");
            assert!(!err.to_string().contains("irrelevant"), "{err}");
            classes.push(err.class());
        }
        assert_eq!(
            classes,
            [
                ErrorClass::Transient,
                ErrorClass::Transient,
                ErrorClass::Permanent,
                ErrorClass::Permanent
            ]
        );
    }

    #[tokio::test]
    async fn a_different_served_model_or_bad_indices_are_permanent() {
        let other = OK.replace(
            r#""model":"text-embedding-3-small""#,
            r#""model":"text-embedding-3-large""#,
        );
        let repeated = OK.replace(r#""index":1"#, r#""index":0"#);
        let (base, _) = server(vec![(200, other), (200, repeated)]).await;
        let openai = adapter(&base);
        for _ in 0..2 {
            let err = openai
                .embed_documents(&[input("a", "x"), input("b", "y")])
                .await
                .expect_err("rejected");
            assert_eq!(err.class(), ErrorClass::Permanent, "{err}");
        }
    }

    #[tokio::test]
    async fn connection_failures_are_transient_and_limits_apply_before_sending() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}/v1", listener.local_addr().expect("addr"));
        drop(listener);
        let openai = adapter(&base);
        let err = openai
            .embed_documents(&[input("a", "x")])
            .await
            .expect_err("refused");
        assert_eq!(err.class(), ErrorClass::Transient, "{err}");
        let long = "palavra ".repeat(MODEL_MAX_TOKENS + 1);
        let err = openai
            .embed_documents(&[input("a", &long)])
            .await
            .expect_err("too long");
        assert!(matches!(err, EmbeddingError::InputTooLong { .. }), "{err}");
    }

    #[test]
    fn the_pin_must_name_the_model_and_the_limit_must_fit() {
        let mismatch = OpenAiAdapter::with_key(
            "text-embedding-3-small",
            Some("text-embedding-3-large"),
            NonZeroUsize::new(2).expect("dims"),
            None,
            DEFAULT_API_BASE,
            "k".into(),
        );
        assert!(matches!(mismatch, Err(LoadError::RevisionMismatch { .. })));
        let oversized = OpenAiAdapter::with_key(
            "text-embedding-3-small",
            Some("text-embedding-3-small"),
            NonZeroUsize::new(2).expect("dims"),
            NonZeroUsize::new(9000),
            DEFAULT_API_BASE,
            "k".into(),
        );
        assert!(matches!(oversized, Err(LoadError::LimitAboveModel { .. })));
        let debug = format!("{:?}", adapter(DEFAULT_API_BASE));
        assert!(!debug.contains("sk-test"), "{debug}");
    }
}
