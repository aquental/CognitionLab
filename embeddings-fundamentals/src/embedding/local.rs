//! Contract logic shared by the local text engines (EmbedAnything, FastEmbed).
//!
//! An engine supplies only a [`TextBackend`] that turns submitted strings into vectors.
//! [`LocalTextAdapter`] does everything else the [`EmbeddingStrategy`] contract
//! requires: it checks capabilities and the batch limit, rejects inputs above the token
//! budget before the backend sees them (so the engine's own silent truncation never
//! applies), applies the prefix policy exactly once, L2-normalizes, and pairs each
//! vector with its item id. Tokenization, prefixing, inference, and normalization all
//! run on the blocking pool (PLAN §2.3). The tokenizer is a type parameter (static
//! dispatch, PLAN §2.2); it is seen as `&dyn TokenCounter` only through the
//! `EmbeddingStrategy` interface. Keeping this
//! engine-independent lets it be unit-tested without compiling an engine.

use std::sync::Arc;

use super::{
    AssetRef, Capabilities, Capability, Embedding, EmbeddingBatch, EmbeddingError,
    EmbeddingStrategy, InputKind, ModelInfo, TextInput, TokenCounter, l2_normalize,
};

/// The engine-specific part of a local text adapter.
pub trait TextBackend: Send + Sync + 'static {
    /// Embeds `texts` (already prefixed) in order, one vector per text. Called on the
    /// blocking pool, so it may run CPU-bound inference directly.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingError`] when inference fails.
    fn embed_blocking(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;
}

/// A token counter that also knows how many special tokens the model adds.
pub trait ModelTokenizer: TokenCounter {
    /// Special tokens the model adds to every input (for example `[CLS]` and `[SEP]`).
    fn special_tokens(&self) -> usize;
}

/// A local text adapter: one [`TextBackend`] and its tokenizer plus the shared
/// contract logic.
pub struct LocalTextAdapter<B, T> {
    backend: Arc<B>,
    tokens: Arc<T>,
    info: ModelInfo,
    capabilities: Capabilities,
}

impl<B, T> std::fmt::Debug for LocalTextAdapter<B, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalTextAdapter")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

/// Input budget in model tokens: the model limit minus special and prefix tokens.
fn budget<T: ModelTokenizer>(
    tokens: &T,
    limit: Option<std::num::NonZeroUsize>,
    policy: super::PrefixPolicy,
    kind: InputKind,
) -> Option<usize> {
    let prefix = tokens.count(policy.prefix(kind));
    Some(
        limit?
            .get()
            .saturating_sub(tokens.special_tokens() + prefix),
    )
}

/// Checks every input against `budget` (model tokens) and applies the prefix policy.
fn prepare<T: ModelTokenizer>(
    tokens: &T,
    budget: Option<usize>,
    policy: super::PrefixPolicy,
    kind: InputKind,
    raw: &[TextInput],
) -> Result<Vec<String>, EmbeddingError> {
    if let Some(budget) = budget {
        for input in raw {
            let count = tokens.count(&input.text);
            if count > budget {
                return Err(EmbeddingError::InputTooLong {
                    item: input.id.clone(),
                    tokens: count,
                    limit: budget,
                });
            }
        }
    }
    Ok(raw
        .iter()
        .map(|i| policy.apply(kind, &i.text).into_owned())
        .collect())
}

/// Embeds `texts` with `backend`, checks the vector count, and L2-normalizes.
fn infer<B: TextBackend>(backend: &B, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    let vectors = backend.embed_blocking(texts)?;
    if vectors.len() != texts.len() {
        return Err(EmbeddingError::Permanent {
            reason: format!(
                "engine returned {} vectors for {} inputs",
                vectors.len(),
                texts.len()
            ),
        });
    }
    Ok(vectors
        .into_iter()
        .map(|mut v| {
            l2_normalize(&mut v);
            v
        })
        .collect())
}

fn join_error(e: tokio::task::JoinError) -> EmbeddingError {
    EmbeddingError::Permanent {
        reason: format!("inference task failed: {e}"),
    }
}

impl<B: TextBackend, T: ModelTokenizer + 'static> LocalTextAdapter<B, T> {
    /// Assembles an adapter. `info` must describe the backend's model and the prefix
    /// policy this adapter applies.
    #[must_use]
    pub fn new(backend: B, tokens: T, info: ModelInfo, capabilities: Capabilities) -> Self {
        Self {
            backend: Arc::new(backend),
            tokens: Arc::new(tokens),
            info,
            capabilities,
        }
    }

    /// Runs the backend on the blocking pool and normalizes the vectors.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingError::Permanent`] when inference fails or returns another
    /// number of vectors than texts.
    pub async fn run(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let backend = Arc::clone(&self.backend);
        tokio::task::spawn_blocking(move || infer(backend.as_ref(), &texts))
            .await
            .map_err(join_error)?
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
        // Same budget the pipeline chunks against: limit minus special and prefix tokens.
        // Tokenizing the batch is CPU work, so it runs on the blocking pool together with
        // inference; the backend never sees an input above the budget.
        let (limit, policy) = (self.capabilities.max_input_tokens, self.info.prefix_policy);
        let (backend, tokens, inputs) = (
            Arc::clone(&self.backend),
            Arc::clone(&self.tokens),
            raw.to_vec(),
        );
        let vectors = tokio::task::spawn_blocking(move || {
            let budget = budget(tokens.as_ref(), limit, policy, kind);
            let texts = prepare(tokens.as_ref(), budget, policy, kind, &inputs)?;
            infer(backend.as_ref(), &texts)
        })
        .await
        .map_err(join_error)??;
        let embeddings = raw
            .iter()
            .zip(vectors)
            .map(|(i, vector)| Embedding {
                id: i.id.clone(),
                vector,
            })
            .collect();
        Ok(EmbeddingBatch {
            embeddings,
            usage: None,
        })
    }
}

#[async_trait::async_trait]
impl<B: TextBackend, T: ModelTokenizer + 'static> EmbeddingStrategy for LocalTextAdapter<B, T> {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn model_info(&self) -> &ModelInfo {
        &self.info
    }

    fn token_counter(&self) -> &dyn TokenCounter {
        &*self.tokens
    }

    fn input_budget(&self, kind: InputKind) -> Option<usize> {
        budget(
            self.tokens.as_ref(),
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
    use std::num::NonZeroUsize;
    use std::sync::Mutex;

    use super::*;
    use crate::domain::Execution;
    use crate::embedding::{DistanceMetric, EngineId, PrefixPolicy, SpaceDescriptor};
    use crate::ids::ItemId;

    /// Records the exact strings it receives and returns unnormalized vectors.
    #[derive(Debug, Default)]
    struct Recording {
        received: Mutex<Vec<Vec<String>>>,
        drop_last: bool,
    }

    impl TextBackend for Arc<Recording> {
        fn embed_blocking(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
            self.received
                .lock()
                .map_err(|e| EmbeddingError::Permanent {
                    reason: e.to_string(),
                })?
                .push(texts.to_vec());
            let n = if self.drop_last {
                texts.len().saturating_sub(1)
            } else {
                texts.len()
            };
            Ok(vec![vec![3.0, 4.0]; n])
        }
    }

    struct Words;

    impl TokenCounter for Words {
        fn count(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }
        fn id(&self) -> &str {
            "words"
        }
    }

    impl ModelTokenizer for Words {
        fn special_tokens(&self) -> usize {
            2
        }
    }

    /// A tokenizer that takes 300 ms per input (stands in for a large tokenizer).
    struct SlowWords;

    impl TokenCounter for SlowWords {
        fn count(&self, text: &str) -> usize {
            std::thread::sleep(std::time::Duration::from_millis(300));
            Words.count(text)
        }
        fn id(&self) -> &str {
            "slow-words"
        }
    }

    impl ModelTokenizer for SlowWords {
        fn special_tokens(&self) -> usize {
            2
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tokenization_runs_on_the_blocking_pool() {
        // Regression (C2-20): budget checks tokenize every input; a slow tokenizer must
        // not stall the single runtime worker.
        // Two inputs: the prefix count and both inputs take 900 ms of tokenizer time.
        let slow = adapter_with(PrefixPolicy::E5, Arc::new(Recording::default()), SlowWords);
        let started = std::time::Instant::now();
        let ticks = async {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(10));
            for _ in 0..5 {
                interval.tick().await;
            }
            started.elapsed()
        };
        let inputs = [input("a", "um"), input("b", "dois")];
        let embed = slow.embed_documents(&inputs);
        let (ticked, out) = tokio::join!(ticks, embed);
        out.expect("embedded");
        assert!(
            ticked < std::time::Duration::from_millis(250),
            "the timer must keep ticking while the tokenizer runs, took {ticked:?}"
        );
    }

    fn adapter(
        policy: PrefixPolicy,
        backend: Arc<Recording>,
    ) -> LocalTextAdapter<Arc<Recording>, Words> {
        adapter_with(policy, backend, Words)
    }

    fn adapter_with<T: ModelTokenizer + 'static>(
        policy: PrefixPolicy,
        backend: Arc<Recording>,
        tokens: T,
    ) -> LocalTextAdapter<Arc<Recording>, T> {
        let info = ModelInfo::from_descriptor(SpaceDescriptor {
            engine: EngineId::FastEmbed,
            engine_version: "test".into(),
            model_id: "m".into(),
            model_revision: Some("r".into()),
            weight_format: Some("onnx".into()),
            dimensions: NonZeroUsize::new(2).expect("dims"),
            normalized: true,
            metric: DistanceMetric::Cosine,
            prefix_policy: policy,
            preprocessing: "mean-pooling".into(),
        })
        .expect("info");
        LocalTextAdapter::new(
            backend,
            tokens,
            info,
            Capabilities {
                text_documents: true,
                text_queries: true,
                images: false,
                text_to_image_compatible: false,
                batch: true,
                max_input_tokens: NonZeroUsize::new(8),
                max_batch: NonZeroUsize::new(2),
                execution: Execution::Local,
            },
        )
    }

    fn input(id: &str, text: &str) -> TextInput {
        TextInput {
            id: id.parse::<ItemId>().expect("id"),
            text: text.into(),
        }
    }

    #[tokio::test]
    async fn the_backend_receives_raw_text_with_the_e5_prefix_applied_once() {
        let backend = Arc::new(Recording::default());
        let e5 = adapter(PrefixPolicy::E5, Arc::clone(&backend));
        e5.embed_documents(&[input("a", "Hemoglobina 13,5 g/dL")])
            .await
            .expect("documents");
        e5.embed_queries(&[input("q", "Onde está a hemoglobina?")])
            .await
            .expect("queries");
        let plain = adapter(PrefixPolicy::None, Arc::clone(&backend));
        plain
            .embed_queries(&[input("q", "Onde está a hemoglobina?")])
            .await
            .expect("plain");
        let received = backend.received.lock().expect("lock").clone();
        assert_eq!(
            received,
            vec![
                vec!["passage: Hemoglobina 13,5 g/dL".to_owned()],
                vec!["query: Onde está a hemoglobina?".to_owned()],
                vec!["Onde está a hemoglobina?".to_owned()],
            ]
        );
    }

    #[tokio::test]
    async fn limits_are_enforced_before_the_backend_and_vectors_are_normalized() {
        let backend = Arc::new(Recording::default());
        let e5 = adapter(PrefixPolicy::E5, Arc::clone(&backend));
        // Budget = 8 − 2 special − 1 prefix token ("passage:") = 5 words.
        assert_eq!(e5.input_budget(InputKind::Document), Some(5));
        let long = e5
            .embed_documents(&[input("a", "um dois tres quatro cinco seis")])
            .await;
        assert!(matches!(
            long,
            Err(EmbeddingError::InputTooLong {
                tokens: 6,
                limit: 5,
                ..
            })
        ));
        let batch = e5
            .embed_documents(&[input("a", "x"), input("b", "y"), input("c", "z")])
            .await;
        assert!(matches!(
            batch,
            Err(EmbeddingError::BatchTooLarge { size: 3, max: 2 })
        ));
        assert!(
            backend.received.lock().expect("lock").is_empty(),
            "nothing reached the backend"
        );
        let ok = e5
            .embed_documents(&[input("a", "x"), input("b", "y")])
            .await
            .expect("embed");
        let ids: Vec<&str> = ok.embeddings.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        for e in &ok.embeddings {
            approx::assert_relative_eq!(e.vector[0], 0.6, epsilon = 1e-6);
            approx::assert_relative_eq!(e.vector[1], 0.8, epsilon = 1e-6);
        }
        let images = e5.embed_images(&[]).await;
        assert!(matches!(images, Err(EmbeddingError::Unsupported { .. })));
    }

    #[tokio::test]
    async fn a_short_response_is_a_permanent_error() {
        let backend = Arc::new(Recording {
            drop_last: true,
            ..Recording::default()
        });
        let plain = adapter(PrefixPolicy::None, backend);
        let outcome = plain
            .embed_documents(&[input("a", "x"), input("b", "y")])
            .await;
        assert!(matches!(outcome, Err(EmbeddingError::Permanent { .. })));
    }
}
