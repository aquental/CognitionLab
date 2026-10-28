//! Deterministic identifiers and fingerprints (PLAN §2.1).
//!
//! Every hash is SHA-256 over a canonical JSON encoding. Object keys are sorted by
//! their UTF-8 bytes, there is no whitespace, and every string is NFC-normalized.
//! Hashes never use `Debug` output or delimiter-free concatenation, so the same
//! configuration always produces the same identifier.

use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::domain::Modality;

/// Errors raised while building or parsing identifiers.
#[derive(Debug, thiserror::Error)]
pub enum IdError {
    /// The value does not match the identifier's format.
    #[error("invalid {kind} identifier {value:?}: {reason}")]
    Invalid {
        /// Identifier kind, such as `source`.
        kind: &'static str,
        /// The rejected value. Identifiers never contain document content.
        value: String,
        /// Why the value was rejected.
        reason: &'static str,
    },
    /// The value could not be converted into canonical JSON.
    #[error("cannot canonicalize value for hashing")]
    Canonical(#[source] serde_json::Error),
    /// Two object keys become identical after NFC normalization, so no single
    /// canonical encoding exists.
    #[error("object keys collide after NFC normalization: {key:?}")]
    DuplicateKey {
        /// The normalized key.
        key: String,
    },
}

/// Encodes `value` as canonical JSON: sorted keys, no whitespace, NFC strings.
///
/// # Errors
///
/// Returns [`IdError::Canonical`] when `value` cannot be serialized to JSON.
pub fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<String, IdError> {
    let value = serde_json::to_value(value).map_err(IdError::Canonical)?;
    let mut out = String::new();
    write_canonical(&value, &mut out)?;
    Ok(out)
}

fn write_canonical(value: &Value, out: &mut String) -> Result<(), IdError> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => out.push_str(&value.to_string()),
        Value::String(s) => write_string(s, out)?,
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            // Sort explicitly: the iteration order of `serde_json::Map` depends on the
            // crate's `preserve_order` feature, which another dependency could enable.
            let mut entries: Vec<(String, &Value)> =
                map.iter().map(|(k, v)| (k.nfc().collect(), v)).collect();
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            if let Some(pair) = entries.windows(2).find(|pair| pair[0].0 == pair[1].0) {
                return Err(IdError::DuplicateKey {
                    key: pair[0].0.clone(),
                });
            }
            out.push('{');
            for (i, (key, item)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(key, out)?;
                out.push(':');
                write_canonical(item, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_string(s: &str, out: &mut String) -> Result<(), IdError> {
    let normalized: String = s.nfc().collect();
    let encoded = serde_json::to_string(&normalized).map_err(IdError::Canonical)?;
    out.push_str(&encoded);
    Ok(())
}

/// Lowercase hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Lowercase hex SHA-256 of the canonical JSON encoding of `value`.
///
/// # Errors
///
/// Returns [`IdError::Canonical`] when `value` cannot be serialized to JSON.
pub fn hash_canonical<T: Serialize + ?Sized>(value: &T) -> Result<String, IdError> {
    Ok(sha256_hex(canonical_json(value)?.as_bytes()))
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Declares a validated string newtype with serde, `Display`, and `FromStr` support.
macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident, $kind:literal, $check:expr, $reason:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Returns the identifier as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                let check: fn(&str) -> bool = $check;
                if check(&value) {
                    Ok(Self(value))
                } else {
                    Err(IdError::Invalid { kind: $kind, value, reason: $reason })
                }
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::try_from(s.to_owned())
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// Stable identifier of an input document, assigned by the corpus manifest.
    SourceId,
    "source",
    is_slug,
    "expected 1-128 ASCII letters, digits, '-', '_' or '.'"
);

string_id!(
    /// Identifier of a saved experiment run.
    ExperimentId,
    "experiment",
    is_slug,
    "expected 1-128 ASCII letters, digits, '-', '_' or '.'"
);

string_id!(
    /// Identifier of an item submitted to an embedding adapter (chunk, query, or asset).
    ItemId,
    "item",
    is_slug,
    "expected 1-128 ASCII letters, digits, '-', '_' or '.'"
);

string_id!(
    /// Identifier of a frozen evaluation query.
    QueryId,
    "query",
    is_slug,
    "expected 1-128 ASCII letters, digits, '-', '_' or '.'"
);

string_id!(
    /// SHA-256 checksum of a source file, formatted as `sha256:<64 hex>`.
    Checksum,
    "checksum",
    |s| s.strip_prefix("sha256:").is_some_and(|h| is_hex(h, 64)),
    "expected sha256:<64 lowercase hex>"
);

string_id!(
    /// Full SHA-256 fingerprint of a configuration (64 lowercase hex characters).
    Fingerprint,
    "fingerprint",
    |s| is_hex(s, 64),
    "expected 64 lowercase hex characters"
);

string_id!(
    /// Immutable source revision: checksum + extraction + chunking fingerprints.
    RevisionId,
    "revision",
    |s| s.strip_prefix("rev-").is_some_and(|h| is_hex(h, 16)),
    "expected rev-<16 lowercase hex>"
);

string_id!(
    /// Identity of an embedding space. Equal dimensions never imply equal spaces.
    EmbeddingSpaceId,
    "embedding space",
    |s| s.strip_prefix("space-").is_some_and(|h| is_hex(h, 32)),
    "expected space-<32 lowercase hex>"
);

string_id!(
    /// Unique identifier of one indexed record in one embedding space.
    RecordId,
    "record",
    |s| is_hex(s, 16),
    "expected 16 lowercase hex characters"
);

string_id!(
    /// Identifier of a chunk, unique within a source revision across modalities.
    ChunkId,
    "chunk",
    is_chunk_id,
    "expected {source_id}-p{page}-{text|image}-{index} with page >= 1 in canonical decimal"
);

/// Canonical decimal: digits only, no sign, no leading zero (except "0" itself).
fn is_canonical_u32(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && (s == "0" || !s.starts_with('0'))
        && s.parse::<u32>().is_ok()
}

/// `{source_id}-p{page}-{modality}-{index}`, parsed from the right because source ids
/// may themselves contain '-'.
fn is_chunk_id(s: &str) -> bool {
    let mut parts = s.rsplitn(4, '-');
    let (Some(index), Some(modality), Some(page), Some(source)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let page_ok = page
        .strip_prefix('p')
        .is_some_and(|n| is_canonical_u32(n) && n != "0");
    s.len() <= 128
        && is_slug(source)
        && page_ok
        && matches!(modality, "text" | "image")
        && is_canonical_u32(index)
}

impl Checksum {
    /// Computes the checksum of `bytes`.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(format!("sha256:{}", sha256_hex(bytes)))
    }
}

impl Fingerprint {
    /// Fingerprints the canonical JSON encoding of `value`.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Canonical`] when `value` cannot be serialized to JSON.
    pub fn of<T: Serialize + ?Sized>(value: &T) -> Result<Self, IdError> {
        Ok(Self(hash_canonical(value)?))
    }
}

impl RevisionId {
    /// Derives the revision of a source from its checksum and the extraction and
    /// chunking fingerprints. Changing any of the three changes the revision.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Canonical`] if hashing fails.
    pub fn derive(
        checksum: &Checksum,
        extraction: &Fingerprint,
        chunking: &Fingerprint,
    ) -> Result<Self, IdError> {
        #[derive(Serialize)]
        struct Preimage<'a> {
            checksum: &'a Checksum,
            extraction_fingerprint: &'a Fingerprint,
            chunking_fingerprint: &'a Fingerprint,
        }
        let hash = hash_canonical(&Preimage {
            checksum,
            extraction_fingerprint: extraction,
            chunking_fingerprint: chunking,
        })?;
        Ok(Self(format!("rev-{}", &hash[..16])))
    }
}

impl EmbeddingSpaceId {
    /// Derives a space identity from a serializable descriptor of everything that
    /// determines vector compatibility (engine, model, revision, dimensions, ...).
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Canonical`] if hashing fails.
    pub fn derive<T: Serialize + ?Sized>(descriptor: &T) -> Result<Self, IdError> {
        let hash = hash_canonical(descriptor)?;
        Ok(Self(format!("space-{}", &hash[..32])))
    }
}

impl RecordId {
    /// Derives a record identifier from its revision, chunk, and embedding space.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Canonical`] if hashing fails.
    pub fn derive(
        revision: &RevisionId,
        chunk: &ChunkId,
        space: &EmbeddingSpaceId,
    ) -> Result<Self, IdError> {
        #[derive(Serialize)]
        struct Preimage<'a> {
            revision_id: &'a RevisionId,
            chunk_id: &'a ChunkId,
            embedding_space_id: &'a EmbeddingSpaceId,
        }
        let hash = hash_canonical(&Preimage {
            revision_id: revision,
            chunk_id: chunk,
            embedding_space_id: space,
        })?;
        Ok(Self(hash[..16].to_owned()))
    }
}

impl ChunkId {
    /// Builds `{source}-p{page}-{modality}-{index}`, for example `src-001-p2-text-3`.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Invalid`] if the result exceeds the identifier length limit.
    pub fn new(
        source: &SourceId,
        page: NonZeroU32,
        modality: Modality,
        index: u32,
    ) -> Result<Self, IdError> {
        Self::try_from(format!("{source}-p{page}-{}-{index}", modality.as_str()))
    }
}

impl From<&ChunkId> for ItemId {
    fn from(chunk: &ChunkId) -> Self {
        // Chunk ids satisfy the same slug rules as item ids.
        Self(chunk.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_keys_and_drops_whitespace() {
        let value = json!({ "b": 1, "a": { "d": [true, null], "c": "x" } });
        assert_eq!(
            canonical_json(&value).expect("canonical"),
            r#"{"a":{"c":"x","d":[true,null]},"b":1}"#
        );
    }

    #[test]
    fn canonical_json_normalizes_strings_to_nfc() {
        let decomposed = "Hemoglobina e\u{301}"; // e + combining acute accent
        let composed = "Hemoglobina \u{e9}";
        assert_eq!(
            canonical_json(&decomposed).expect("decomposed"),
            canonical_json(&composed).expect("composed")
        );
        // NFD would also make the two equal; require the composed (NFC) bytes.
        assert_eq!(
            canonical_json(&decomposed).expect("decomposed"),
            "\"Hemoglobina \u{e9}\""
        );
    }

    #[test]
    fn revision_id_is_frozen_for_known_inputs() {
        let checksum = Checksum::of_bytes(b"fixture");
        let extraction = Fingerprint::of(&json!({ "tool": "pdftotext" })).expect("fp");
        let chunking = Fingerprint::of(&json!({ "strategy": "structure-aware-v1" })).expect("fp");
        let revision = RevisionId::derive(&checksum, &extraction, &chunking).expect("revision");
        // Golden value computed independently with Python (json.dumps sort_keys, compact
        // separators, hashlib.sha256). Changing the encoding must be a deliberate decision.
        assert_eq!(revision.as_str(), "rev-3d454748b6c6373a");
    }

    #[test]
    fn revision_changes_when_any_fingerprint_changes() {
        let checksum = Checksum::of_bytes(b"fixture");
        let a = Fingerprint::of(&json!({ "v": 1 })).expect("fp");
        let b = Fingerprint::of(&json!({ "v": 2 })).expect("fp");
        let base = RevisionId::derive(&checksum, &a, &a).expect("rev");
        assert_ne!(base, RevisionId::derive(&checksum, &b, &a).expect("rev"));
        assert_ne!(base, RevisionId::derive(&checksum, &a, &b).expect("rev"));
        let other = Checksum::of_bytes(b"other");
        assert_ne!(base, RevisionId::derive(&other, &a, &a).expect("rev"));
    }

    #[test]
    fn chunk_id_includes_page_and_modality() {
        let source: SourceId = "src-001".parse().expect("source");
        let page = NonZeroU32::new(2).expect("page");
        let text = ChunkId::new(&source, page, Modality::Text, 3).expect("chunk");
        let image = ChunkId::new(&source, page, Modality::Image, 3).expect("chunk");
        assert_eq!(text.as_str(), "src-001-p2-text-3");
        assert_ne!(text, image);
    }

    #[test]
    fn record_id_depends_on_revision_and_space() {
        let source: SourceId = "src-001".parse().expect("source");
        let chunk = ChunkId::new(&source, NonZeroU32::MIN, Modality::Text, 0).expect("chunk");
        let rev_a: RevisionId = "rev-0000000000000001".parse().expect("rev");
        let rev_b: RevisionId = "rev-0000000000000002".parse().expect("rev");
        let space_a = EmbeddingSpaceId::derive(&json!({ "model": "a" })).expect("space");
        let space_b = EmbeddingSpaceId::derive(&json!({ "model": "b" })).expect("space");
        let id = RecordId::derive(&rev_a, &chunk, &space_a).expect("record");
        assert_ne!(
            id,
            RecordId::derive(&rev_b, &chunk, &space_a).expect("record")
        );
        assert_ne!(
            id,
            RecordId::derive(&rev_a, &chunk, &space_b).expect("record")
        );
    }

    #[test]
    fn chunk_ids_must_follow_the_structured_format() {
        for bad in [
            "bogus",
            "src-001-p0-text-3",
            "src-001-p2-audio-3",
            "src-001-p02-text-3",
            "src-001-p2-text-03",
            "-p2-text-3",
            "src-001-p2-text-",
        ] {
            assert!(bad.parse::<ChunkId>().is_err(), "{bad} must be rejected");
            assert!(
                serde_json::from_str::<ChunkId>(&format!("\"{bad}\"")).is_err(),
                "{bad} via serde"
            );
        }
        let source: SourceId = "lab-searchable-001".parse().expect("source");
        for (page, modality, index) in [(1, Modality::Text, 0), (12, Modality::Image, 7)] {
            let built = ChunkId::new(
                &source,
                NonZeroU32::new(page).expect("page"),
                modality,
                index,
            )
            .expect("chunk");
            assert_eq!(
                built.as_str().parse::<ChunkId>().expect("round trip"),
                built
            );
        }
    }

    #[test]
    fn invalid_identifiers_are_rejected() {
        assert!("".parse::<SourceId>().is_err());
        assert!("has space".parse::<SourceId>().is_err());
        assert!("sha256:abc".parse::<Checksum>().is_err());
        assert!("rev-XYZ".parse::<RevisionId>().is_err());
        assert!("space-00".parse::<EmbeddingSpaceId>().is_err());
    }

    #[test]
    fn identifiers_round_trip_through_serde() {
        let id: RevisionId = "rev-0123456789abcdef".parse().expect("rev");
        let json = serde_json::to_string(&id).expect("serialize");
        assert_eq!(json, "\"rev-0123456789abcdef\"");
        let back: RevisionId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, id);
        assert!(serde_json::from_str::<RevisionId>("\"bogus\"").is_err());
    }

    /// The dev-dependency enables `serde_json/preserve_order`, so in tests a `Map`
    /// keeps insertion order and this test fails if `write_canonical` stops sorting.
    #[test]
    fn canonical_json_sorts_keys_inserted_out_of_order() {
        let mut map = serde_json::Map::new();
        map.insert("b".to_owned(), json!(1));
        map.insert("a".to_owned(), json!(2));
        assert_eq!(
            map.keys().next().map(String::as_str),
            Some("b"),
            "preserve_order is active"
        );
        assert_eq!(
            canonical_json(&Value::Object(map)).expect("canonical"),
            r#"{"a":2,"b":1}"#
        );
    }

    #[test]
    fn keys_colliding_after_nfc_are_rejected_in_any_order_and_depth() {
        let composed = "\u{e9}";
        let decomposed = "e\u{301}";
        for (first, second) in [(composed, decomposed), (decomposed, composed)] {
            let mut map = serde_json::Map::new();
            map.insert(first.to_owned(), json!(1));
            map.insert(second.to_owned(), json!(2));
            let flat = Value::Object(map.clone());
            assert!(matches!(
                canonical_json(&flat),
                Err(IdError::DuplicateKey { .. })
            ));
            let nested = json!({ "outer": [ { "x": Value::Object(map) } ] });
            assert!(matches!(
                canonical_json(&nested),
                Err(IdError::DuplicateKey { .. })
            ));
        }
    }

    #[test]
    fn record_id_is_frozen_for_known_inputs() {
        let revision: RevisionId = "rev-0123456789abcdef".parse().expect("rev");
        let chunk: ChunkId = "src-001-p2-text-3".parse().expect("chunk");
        let space: EmbeddingSpaceId = "space-35007f7bfcdac3ce4c66ebe17fa1793b"
            .parse()
            .expect("space");
        // Golden value computed independently with Python (json.dumps sort_keys, compact
        // separators, ensure_ascii=False; hashlib.sha256, first 16 hex characters).
        assert_eq!(
            RecordId::derive(&revision, &chunk, &space)
                .expect("record")
                .as_str(),
            "3281093a6a8c6280"
        );
    }

    proptest! {
        #[test]
        fn canonical_json_is_independent_of_key_insertion_order(
            entries in proptest::collection::btree_map("[a-z]{1,6}", 0i64..1000, 0..8)
        ) {
            // With preserve_order enabled (dev-dependency), these maps differ in order.
            let forward: serde_json::Map<String, Value> =
                entries.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
            let reversed: serde_json::Map<String, Value> =
                entries.iter().rev().map(|(k, v)| (k.clone(), json!(v))).collect();
            let sorted: String = format!(
                "{{{}}}",
                entries.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect::<Vec<_>>().join(",")
            );
            prop_assert_eq!(canonical_json(&Value::Object(forward)).expect("forward"), sorted.clone());
            prop_assert_eq!(canonical_json(&Value::Object(reversed)).expect("reversed"), sorted);
        }
    }
}
