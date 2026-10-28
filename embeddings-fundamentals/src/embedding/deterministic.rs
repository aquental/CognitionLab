//! Hash-based embedding adapter for deterministic tests (PLAN Phase 0).
//!
//! The adapter runs the real [`EmbeddingStrategy`] contract: it applies its prefix
//! policy, honours its capabilities, and returns L2-normalized vectors derived from
//! SHA-256. It also records every call and supports scripted fault injection, so BDD
//! scenarios can assert on retries, invalid vectors, and the exact inputs received.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, MutexGuard, PoisonError};

use sha2::{Digest, Sha256};

use super::{
    AssetRef, Capabilities, Capability, DistanceMetric, Embedding, EmbeddingBatch, EmbeddingError,
    EmbeddingStrategy, EngineId, ErrorClass, InputKind, ModelInfo, PrefixPolicy, SpaceDescriptor,
    TextInput, TokenCounter, Usage, l2_normalize,
};
use crate::domain::Execution;
use crate::ids::{IdError, ItemId};

/// Which adapter method was called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// [`EmbeddingStrategy::embed_documents`].
    Documents,
    /// [`EmbeddingStrategy::embed_queries`].
    Queries,
    /// [`EmbeddingStrategy::embed_images`].
    Images,
}

/// One recorded call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// Method that was called.
    pub method: Method,
    /// Item ids of the inputs.
    pub ids: Vec<ItemId>,
    /// Inputs exactly as received (raw text, or asset paths for images).
    pub raw_inputs: Vec<String>,
    /// Text after the prefix policy was applied (empty for images).
    pub submitted: Vec<String>,
}

/// A defect to inject into the next successful response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectedDefect {
    /// Return one extra component.
    WrongDimensions,
    /// Set the first component to NaN.
    NaN,
    /// Set the first component to +infinity.
    Infinite,
}

/// Which response item a defect corrupts.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DefectTarget {
    /// This exact item.
    Item(ItemId),
    /// The first item whose id starts with `<source>-` in each response.
    FirstOfSource(String),
}

#[derive(Debug, Default)]
struct FaultPlan {
    /// Fail this many upcoming calls with the given class, then succeed.
    failures: Option<(u32, ErrorClass)>,
    /// Fail every call with the given class.
    always: Option<ErrorClass>,
    /// Fail every call containing an item of these sources (`<source>-` id prefix).
    source_failures: Vec<(String, ErrorClass)>,
    /// Corrupt one vector in every response that contains the target.
    defect: Option<(DefectTarget, InjectedDefect)>,
}

fn belongs_to(id: &ItemId, source: &str) -> bool {
    id.as_str()
        .strip_prefix(source)
        .is_some_and(|rest| rest.starts_with('-'))
}

/// Whitespace token counter used by the deterministic adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct WhitespaceTokenCounter;

impl TokenCounter for WhitespaceTokenCounter {
    fn count(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
}

/// Builder for [`DeterministicEmbedder`].
#[derive(Debug, Clone)]
#[must_use]
pub struct DeterministicBuilder {
    model_id: String,
    dimensions: NonZeroUsize,
    prefix_policy: PrefixPolicy,
    capabilities: Capabilities,
    engine_label: EngineId,
    overrides: HashMap<String, Vec<f32>>,
    keywords: Vec<(String, Vec<(usize, f32)>)>,
}

impl Default for DeterministicBuilder {
    fn default() -> Self {
        Self {
            model_id: "deterministic-v1".to_owned(),
            dimensions: NonZeroUsize::new(8).unwrap_or(NonZeroUsize::MIN),
            prefix_policy: PrefixPolicy::None,
            capabilities: Capabilities {
                text_documents: true,
                text_queries: true,
                images: false,
                text_to_image_compatible: false,
                batch: true,
                max_input_tokens: NonZeroUsize::new(512),
                max_batch: NonZeroUsize::new(64),
                execution: Execution::Local,
            },
            engine_label: EngineId::Deterministic,
            overrides: HashMap::new(),
            keywords: Vec::new(),
        }
    }
}

impl DeterministicBuilder {
    /// Sets the model identifier (part of the embedding space identity).
    pub fn model_id(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }

    /// Sets the vector dimensions.
    pub fn dimensions(mut self, dimensions: NonZeroUsize) -> Self {
        self.dimensions = dimensions;
        self
    }

    /// Sets the prefix policy applied before hashing.
    pub fn prefix_policy(mut self, policy: PrefixPolicy) -> Self {
        self.prefix_policy = policy;
        self
    }

    /// Replaces the advertised capabilities.
    pub fn capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Reports `engine` in [`ModelInfo`], so contract tests can run the real
    /// orchestration under another engine's identity.
    pub fn engine_label(mut self, engine: EngineId) -> Self {
        self.engine_label = engine;
        self
    }

    /// Returns `vector` (normalized) for inputs whose submitted text, or asset path,
    /// equals `key`. Used to freeze rankings in fixtures.
    pub fn with_vector(mut self, key: impl Into<String>, vector: Vec<f32>) -> Self {
        self.overrides.insert(key.into(), vector);
        self
    }

    /// Keyword axes: any text containing `keyword` (case-insensitive) gets the given
    /// `(axis, weight)` components, summed over every matching keyword, and no hash
    /// noise. Texts without a keyword get hash vectors that are zero on every reserved
    /// axis, so they score exactly 0 against a keyword-only query. This makes frozen
    /// rankings derivable by hand.
    pub fn with_keyword_vector(mut self, keyword: &str, components: &[(usize, f32)]) -> Self {
        self.keywords
            .push((keyword.to_lowercase(), components.to_vec()));
        self
    }

    /// Builds the adapter.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] if the space identity cannot be derived.
    pub fn build(self) -> Result<DeterministicEmbedder, IdError> {
        let info = ModelInfo::from_descriptor(SpaceDescriptor {
            engine: self.engine_label,
            engine_version: env!("CARGO_PKG_VERSION").to_owned(),
            model_id: self.model_id,
            model_revision: Some("deterministic".to_owned()),
            weight_format: None,
            dimensions: self.dimensions,
            normalized: true,
            metric: DistanceMetric::Cosine,
            prefix_policy: self.prefix_policy,
            preprocessing: "sha256-seeded".to_owned(),
        })?;
        Ok(DeterministicEmbedder {
            info,
            capabilities: self.capabilities,
            overrides: self.overrides,
            keywords: self.keywords,
            faults: Mutex::new(FaultPlan::default()),
            calls: Mutex::new(Vec::new()),
        })
    }
}

/// Deterministic, fault-injectable embedding adapter.
#[derive(Debug)]
pub struct DeterministicEmbedder {
    info: ModelInfo,
    capabilities: Capabilities,
    overrides: HashMap<String, Vec<f32>>,
    keywords: Vec<(String, Vec<(usize, f32)>)>,
    faults: Mutex<FaultPlan>,
    calls: Mutex<Vec<Call>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock only means another test thread panicked; the data is still usable.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl DeterministicEmbedder {
    /// Starts a builder with 8 dimensions, no prefix, and local text capabilities.
    pub fn builder() -> DeterministicBuilder {
        DeterministicBuilder::default()
    }

    /// Fails the next `count` calls with `class`, then succeeds.
    pub fn fail_next(&self, count: u32, class: ErrorClass) {
        lock(&self.faults).failures = Some((count, class));
    }

    /// Fails every call with `class`.
    pub fn fail_always(&self, class: ErrorClass) {
        lock(&self.faults).always = Some(class);
    }

    /// Corrupts the vector returned for `item`.
    pub fn inject_defect(&self, item: ItemId, defect: InjectedDefect) {
        lock(&self.faults).defect = Some((DefectTarget::Item(item), defect));
    }

    /// Corrupts the first vector of `source` in every response.
    pub fn inject_defect_for_source(&self, source: &str, defect: InjectedDefect) {
        lock(&self.faults).defect = Some((DefectTarget::FirstOfSource(source.to_owned()), defect));
    }

    /// Fails every call that contains an item of `source` with `class`.
    pub fn fail_source_always(&self, source: &str, class: ErrorClass) {
        lock(&self.faults)
            .source_failures
            .push((source.to_owned(), class));
    }

    /// Calls that contained at least one item of `source`.
    #[must_use]
    pub fn calls_for_source(&self, source: &str) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|c| c.ids.iter().any(|id| belongs_to(id, source)))
            .collect()
    }

    /// Removes all injected faults.
    pub fn clear_faults(&self) {
        *lock(&self.faults) = FaultPlan::default();
    }

    /// Every call received so far, including failed attempts.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        lock(&self.calls).clone()
    }

    /// Deterministic vector for `key`: SHA-256-seeded components, L2-normalized.
    #[must_use]
    pub fn vector_for(&self, key: &str) -> Vec<f32> {
        if let Some(vector) = self.overrides.get(key) {
            let mut v = vector.clone();
            l2_normalize(&mut v);
            return v;
        }
        let dims = self.info.dimensions.get();
        if !self.keywords.is_empty() {
            let lower = key.to_lowercase();
            let reserved = self
                .keywords
                .iter()
                .flat_map(|(_, c)| c.iter().map(|(axis, _)| axis + 1))
                .max()
                .unwrap_or(0)
                .min(dims);
            let mut v = vec![0.0_f32; dims];
            let mut matched = false;
            for (keyword, components) in &self.keywords {
                if lower.contains(keyword.as_str()) {
                    matched = true;
                    for (axis, weight) in components {
                        if let Some(slot) = v.get_mut(*axis) {
                            *slot += weight;
                        }
                    }
                }
            }
            if !matched {
                let noise = self.hash_vector(key, dims);
                for (slot, value) in v.iter_mut().zip(noise).skip(reserved) {
                    *slot = value;
                }
            }
            l2_normalize(&mut v);
            return v;
        }
        let mut vector = self.hash_vector(key, dims);
        l2_normalize(&mut vector);
        vector
    }

    fn hash_vector(&self, key: &str, dims: usize) -> Vec<f32> {
        let mut vector = Vec::with_capacity(dims);
        let mut counter: u32 = 0;
        while vector.len() < dims {
            let digest = Sha256::new()
                .chain_update(self.info.embedding_space_id.as_str().as_bytes())
                .chain_update([0u8])
                .chain_update(key.as_bytes())
                .chain_update(counter.to_le_bytes())
                .finalize();
            for pair in digest.as_chunks::<2>().0 {
                if vector.len() == dims {
                    break;
                }
                let raw = u16::from_le_bytes(*pair);
                vector.push(f32::from(raw) / f32::from(u16::MAX) * 2.0 - 1.0);
            }
            counter = counter.wrapping_add(1);
        }
        vector
    }

    fn record(
        &self,
        method: Method,
        ids: Vec<ItemId>,
        raw_inputs: Vec<String>,
        submitted: Vec<String>,
    ) {
        lock(&self.calls).push(Call {
            method,
            ids,
            raw_inputs,
            submitted,
        });
    }

    fn scripted_failure(&self, ids: &[ItemId]) -> Option<EmbeddingError> {
        let mut faults = lock(&self.faults);
        let source_class = faults
            .source_failures
            .iter()
            .find(|(source, _)| ids.iter().any(|id| belongs_to(id, source)))
            .map(|(_, class)| *class);
        let class = if let Some(class) = faults.always.or(source_class) {
            Some(class)
        } else if let Some((remaining, class)) = faults.failures {
            faults.failures = remaining
                .checked_sub(1)
                .filter(|n| *n > 0)
                .map(|n| (n, class));
            (remaining > 0).then_some(class)
        } else {
            None
        };
        class.map(|class| match class {
            ErrorClass::Transient => EmbeddingError::Transient {
                reason: "injected transient failure".to_owned(),
            },
            ErrorClass::Permanent => EmbeddingError::Permanent {
                reason: "injected permanent failure".to_owned(),
            },
        })
    }

    fn apply_defect(&self, embeddings: &mut [Embedding]) {
        let defect = lock(&self.faults).defect.clone();
        let Some((target, defect)) = defect else {
            return;
        };
        let position = match &target {
            DefectTarget::Item(item) => embeddings.iter().position(|e| &e.id == item),
            DefectTarget::FirstOfSource(source) => {
                embeddings.iter().position(|e| belongs_to(&e.id, source))
            }
        };
        if let Some(embedding) = position.and_then(|i| embeddings.get_mut(i)) {
            match defect {
                InjectedDefect::WrongDimensions => embedding.vector.push(0.0),
                InjectedDefect::NaN => {
                    if let Some(first) = embedding.vector.first_mut() {
                        *first = f32::NAN;
                    }
                }
                InjectedDefect::Infinite => {
                    if let Some(first) = embedding.vector.first_mut() {
                        *first = f32::INFINITY;
                    }
                }
            }
        }
    }

    fn embed_text(
        &self,
        method: Method,
        kind: InputKind,
        raw: &[TextInput],
    ) -> Result<EmbeddingBatch, EmbeddingError> {
        let submitted: Vec<String> = raw
            .iter()
            .map(|input| {
                self.info
                    .prefix_policy
                    .apply(kind, &input.text)
                    .into_owned()
            })
            .collect();
        let ids: Vec<ItemId> = raw.iter().map(|i| i.id.clone()).collect();
        self.record(
            method,
            ids.clone(),
            raw.iter().map(|i| i.text.clone()).collect(),
            submitted.clone(),
        );
        let capability = match kind {
            InputKind::Document => Capability::TextDocuments,
            InputKind::Query => Capability::TextQueries,
        };
        self.capabilities.require(capability, &self.info)?;
        self.capabilities.check_batch(raw.len())?;
        if let Some(error) = self.scripted_failure(&ids) {
            return Err(error);
        }
        if let Some(limit) = self.capabilities.max_input_tokens {
            for (input, text) in raw.iter().zip(&submitted) {
                let tokens = WhitespaceTokenCounter.count(text);
                if tokens > limit.get() {
                    return Err(EmbeddingError::InputTooLong {
                        item: input.id.clone(),
                        tokens,
                        limit: limit.get(),
                    });
                }
            }
        }
        let mut embeddings: Vec<Embedding> = raw
            .iter()
            .zip(&submitted)
            .map(|(input, text)| Embedding {
                id: input.id.clone(),
                vector: self.vector_for(text),
            })
            .collect();
        self.apply_defect(&mut embeddings);
        let tokens = submitted
            .iter()
            .map(|t| WhitespaceTokenCounter.count(t))
            .sum::<usize>();
        Ok(EmbeddingBatch {
            embeddings,
            usage: Some(Usage {
                tokens: u64::try_from(tokens).unwrap_or(u64::MAX),
            }),
        })
    }
}

#[async_trait::async_trait]
impl EmbeddingStrategy for DeterministicEmbedder {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn model_info(&self) -> &ModelInfo {
        &self.info
    }

    fn token_counter(&self) -> &dyn TokenCounter {
        &WhitespaceTokenCounter
    }

    fn input_budget(&self, kind: InputKind) -> Option<usize> {
        let limit = self.capabilities.max_input_tokens?.get();
        let prefix = WhitespaceTokenCounter.count(self.info.prefix_policy.prefix(kind));
        Some(limit.saturating_sub(prefix))
    }

    async fn embed_documents(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.embed_text(Method::Documents, InputKind::Document, raw)
    }

    async fn embed_queries(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.embed_text(Method::Queries, InputKind::Query, raw)
    }

    async fn embed_images(&self, assets: &[AssetRef]) -> Result<EmbeddingBatch, EmbeddingError> {
        let keys: Vec<String> = assets
            .iter()
            .map(|a| a.path.display().to_string())
            .collect();
        let ids: Vec<ItemId> = assets.iter().map(|a| a.id.clone()).collect();
        self.record(Method::Images, ids.clone(), keys.clone(), Vec::new());
        self.capabilities.require(Capability::Images, &self.info)?;
        self.capabilities.check_batch(assets.len())?;
        if let Some(error) = self.scripted_failure(&ids) {
            return Err(error);
        }
        let mut embeddings: Vec<Embedding> = assets
            .iter()
            .zip(&keys)
            .map(|(asset, key)| Embedding {
                id: asset.id.clone(),
                vector: self.vector_for(key),
            })
            .collect();
        self.apply_defect(&mut embeddings);
        Ok(EmbeddingBatch {
            embeddings,
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::validate_batch;

    fn input(id: &str, text: &str) -> TextInput {
        TextInput {
            id: id.parse().expect("item id"),
            text: text.to_owned(),
        }
    }

    #[tokio::test]
    async fn vectors_are_deterministic_and_normalized() {
        let embedder = DeterministicEmbedder::builder().build().expect("build");
        let a = embedder
            .embed_documents(&[input("a", "Hemoglobina 13,5 g/dL")])
            .await;
        let b = embedder
            .embed_documents(&[input("a", "Hemoglobina 13,5 g/dL")])
            .await;
        let a = a.expect("embed a");
        assert_eq!(a, b.expect("embed b"));
        let norm: f32 = a.embeddings[0]
            .vector
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
            .sqrt();
        approx::assert_relative_eq!(norm, 1.0, epsilon = 1e-5);
        let ids = vec!["a".parse().expect("id")];
        assert_eq!(validate_batch(&a, &ids, embedder.model_info()), Ok(()));
    }

    #[test]
    fn vector_components_are_frozen_and_text_dependent() {
        let embedder = DeterministicEmbedder::builder().build().expect("build");
        assert_eq!(
            embedder.model_info().embedding_space_id.as_str(),
            "space-c2b47d9244e2a152c3f4053a1bc018ec"
        );
        let v = embedder.vector_for("Hemoglobina 13,5 g/dL");
        // Golden components computed independently in Python: sha256(space_id || 0x00 ||
        // text || counter_le_u32), little-endian u16 pairs mapped to [-1, 1], L2-normalized.
        for (actual, expected) in v.iter().zip([0.179_264_f32, 0.571_513, 0.351_240]) {
            approx::assert_abs_diff_eq!(*actual, expected, epsilon = 1e-5);
        }
        assert_ne!(
            v,
            embedder.vector_for("Glicemia 92 mg/dL"),
            "different texts must differ"
        );
    }

    #[tokio::test]
    async fn prefix_policy_is_applied_once_and_raw_text_is_recorded() {
        let embedder = DeterministicEmbedder::builder()
            .prefix_policy(PrefixPolicy::E5)
            .build()
            .expect("build");
        embedder
            .embed_queries(&[input("q", "hemoglobina")])
            .await
            .expect("query");
        embedder
            .embed_documents(&[input("d", "hemoglobina")])
            .await
            .expect("doc");
        let calls = embedder.calls();
        assert_eq!(calls[0].method, Method::Queries);
        assert_eq!(calls[0].raw_inputs, vec!["hemoglobina".to_owned()]);
        assert_eq!(calls[0].submitted, vec!["query: hemoglobina".to_owned()]);
        assert_eq!(calls[1].submitted, vec!["passage: hemoglobina".to_owned()]);
    }

    #[tokio::test]
    async fn scripted_transient_failures_then_success() {
        let embedder = DeterministicEmbedder::builder().build().expect("build");
        embedder.fail_next(1, ErrorClass::Transient);
        let first = embedder.embed_documents(&[input("a", "x")]).await;
        assert_eq!(first.err().map(|e| e.class()), Some(ErrorClass::Transient));
        assert!(embedder.embed_documents(&[input("a", "x")]).await.is_ok());
        assert_eq!(embedder.calls().len(), 2);
    }

    #[tokio::test]
    async fn always_failing_permanent_errors_never_recover() {
        let embedder = DeterministicEmbedder::builder().build().expect("build");
        embedder.fail_always(ErrorClass::Permanent);
        for _ in 0..3 {
            let result = embedder.embed_documents(&[input("a", "x")]).await;
            assert_eq!(result.err().map(|e| e.class()), Some(ErrorClass::Permanent));
        }
    }

    #[tokio::test]
    async fn injected_defects_are_caught_by_validation() {
        for (defect, expected) in [
            (InjectedDefect::WrongDimensions, "incorrect dimensions"),
            (InjectedDefect::NaN, "NaN component"),
            (InjectedDefect::Infinite, "infinite component"),
        ] {
            let embedder = DeterministicEmbedder::builder().build().expect("build");
            embedder.inject_defect("b".parse().expect("id"), defect);
            let batch = embedder
                .embed_documents(&[input("a", "x"), input("b", "y")])
                .await
                .expect("embed");
            let ids: Vec<ItemId> = vec!["a".parse().expect("id"), "b".parse().expect("id")];
            let err = validate_batch(&batch, &ids, embedder.model_info()).expect_err("defect");
            assert!(
                err.to_string().contains(expected),
                "{err} should mention {expected}"
            );
            assert!(
                err.to_string().contains("item b"),
                "{err} should name item b"
            );
        }
    }

    #[tokio::test]
    async fn missing_image_capability_is_an_explicit_error() {
        let embedder = DeterministicEmbedder::builder().build().expect("build");
        let asset = AssetRef {
            id: "img".parse().expect("id"),
            path: "chest-001.png".into(),
        };
        let err = embedder
            .embed_images(&[asset])
            .await
            .expect_err("unsupported");
        assert!(matches!(
            err,
            EmbeddingError::Unsupported {
                capability: Capability::Images,
                ..
            }
        ));
        assert_eq!(err.class(), ErrorClass::Permanent);
    }

    #[tokio::test]
    async fn overrides_freeze_vectors_for_fixtures() {
        let embedder = DeterministicEmbedder::builder()
            .dimensions(NonZeroUsize::new(2).expect("dims"))
            .with_vector("hemoglobina", vec![3.0, 4.0])
            .build()
            .expect("build");
        let batch = embedder
            .embed_queries(&[input("q", "hemoglobina")])
            .await
            .expect("embed");
        approx::assert_relative_eq!(batch.embeddings[0].vector[0], 0.6, epsilon = 1e-6);
    }

    #[tokio::test]
    async fn batch_limits_are_enforced_for_text_and_images() {
        let inputs = |n: usize| -> Vec<TextInput> {
            (0..n).map(|i| input(&format!("i{i}"), "texto")).collect()
        };
        let mut caps = DeterministicEmbedder::builder()
            .build()
            .expect("build")
            .capabilities;
        caps.max_batch = NonZeroUsize::new(2);
        caps.images = true;
        let limited = DeterministicEmbedder::builder()
            .capabilities(caps.clone())
            .build()
            .expect("b");
        assert!(
            limited.embed_documents(&inputs(2)).await.is_ok(),
            "boundary is accepted"
        );
        let err = limited
            .embed_queries(&inputs(3))
            .await
            .expect_err("above max_batch");
        assert!(matches!(
            err,
            EmbeddingError::BatchTooLarge { size: 3, max: 2 }
        ));
        assert_eq!(err.class(), ErrorClass::Permanent);
        let assets: Vec<AssetRef> = (0..3)
            .map(|i| AssetRef {
                id: format!("a{i}").parse().expect("id"),
                path: format!("{i}.png").into(),
            })
            .collect();
        assert!(matches!(
            limited.embed_images(&assets).await,
            Err(EmbeddingError::BatchTooLarge { size: 3, max: 2 })
        ));
        caps.batch = false;
        let single = DeterministicEmbedder::builder()
            .capabilities(caps)
            .build()
            .expect("b");
        assert!(single.embed_documents(&inputs(1)).await.is_ok());
        assert!(matches!(
            single.embed_documents(&inputs(2)).await,
            Err(EmbeddingError::BatchTooLarge { size: 2, max: 1 })
        ));
    }

    #[tokio::test]
    async fn source_scoped_faults_and_defects_only_hit_that_source() {
        let embedder = DeterministicEmbedder::builder().build().expect("build");
        embedder.fail_source_always("S1", ErrorClass::Permanent);
        assert!(
            embedder
                .embed_documents(&[input("S1-p1-text-0", "a")])
                .await
                .is_err()
        );
        assert!(
            embedder
                .embed_documents(&[input("S2-p1-text-0", "a")])
                .await
                .is_ok()
        );
        assert!(
            embedder
                .embed_documents(&[input("S10-p1-text-0", "a")])
                .await
                .is_ok(),
            "prefix is exact"
        );
        assert_eq!(embedder.calls_for_source("S1").len(), 1);
        let embedder = DeterministicEmbedder::builder().build().expect("build");
        embedder.inject_defect_for_source("S1", InjectedDefect::NaN);
        let batch = embedder
            .embed_documents(&[input("S1-p1-text-0", "a"), input("S1-p1-text-1", "b")])
            .await
            .expect("embed");
        assert!(batch.embeddings[0].vector[0].is_nan());
        assert!(
            batch.embeddings[1].vector.iter().all(|x| x.is_finite()),
            "only the first item"
        );
    }

    #[test]
    fn keyword_vectors_give_hand_derivable_scores() {
        let embedder = DeterministicEmbedder::builder()
            .with_keyword_vector("hemoglobina", &[(0, 1.0)])
            .with_keyword_vector("hematócrito", &[(0, 0.8), (1, 0.6)])
            .build()
            .expect("build");
        let query = embedder.vector_for("Onde está o resultado da hemoglobina?");
        let dot = |v: Vec<f32>| v.iter().zip(&query).map(|(a, b)| a * b).sum::<f32>();
        approx::assert_abs_diff_eq!(
            dot(embedder.vector_for("Hemoglobina | 13,5")),
            1.0,
            epsilon = 1e-6
        );
        approx::assert_abs_diff_eq!(
            dot(embedder.vector_for("Hematócrito | 40,1")),
            0.8,
            epsilon = 1e-6
        );
        approx::assert_abs_diff_eq!(
            dot(embedder.vector_for("Glicemia | 92")),
            0.0,
            epsilon = 1e-6
        );
    }

    #[test]
    fn input_budget_reserves_prefix_tokens() {
        let embedder = DeterministicEmbedder::builder()
            .prefix_policy(PrefixPolicy::E5)
            .build()
            .expect("build");
        assert_eq!(embedder.input_budget(InputKind::Query), Some(511));
    }

    #[test]
    fn engine_label_changes_the_space() {
        let a = DeterministicEmbedder::builder().build().expect("a");
        let b = DeterministicEmbedder::builder()
            .engine_label(EngineId::FastEmbed)
            .build()
            .expect("b");
        assert_eq!(b.model_info().engine, EngineId::FastEmbed);
        assert_ne!(
            a.model_info().embedding_space_id,
            b.model_info().embedding_space_id
        );
    }
}
