ID: C0-1
Severity: medium
File:line: src/embedding/mod.rs:520
Description: `l2_normalize` does not correctly normalize all finite, nonzero vectors. Squaring large components in `f32` overflows, while the epsilon cutoff leaves small nonzero vectors unchanged.
Impact: `DeterministicBuilder::with_vector` can produce non-unit vectors despite advertising `normalized: true`. These vectors pass `validate_batch`, compromising cosine calculations based on normalized dot products.
Reproduction scenario or verifiable rationale: Normalizing `[1e20_f32, 0.0]` yields `[0.0, 0.0]` because the squared norm becomes infinity. Normalizing `[1e-8_f32, 0.0]` leaves it unchanged because its norm is below `f32::EPSILON`. Both inputs should normalize to `[1.0, 0.0]`.
Validation suggestion: Compute the norm with wider or scaled arithmetic and distinguish zero from small nonzero norms. Test both examples through `with_vector` and assert unit-length output.

ID: C0-2
Severity: medium
File:line: src/embedding/deterministic.rs:329
Description: The deterministic adapter ignores both `Capabilities::batch` and `Capabilities::max_batch`. Text and image methods check modality support but accept arbitrarily many inputs. This contradicts the configurable capabilities contract and PRD §7’s requirement that unsupported operations return explicit errors.
Impact: Contract tests can pass when orchestration sends unsupported batches, masking failures that real adapters will encounter.
Reproduction scenario or verifiable rationale: Configure `batch = false` and submit two text inputs, or use the default `max_batch = 64` and submit 65 short inputs. Both calls succeed. The image path at line 401 likewise contains no batch constraint check.
Validation suggestion: Enforce batch support and maximum batch size consistently across all three embedding methods. Test rejection above each limit and success at the boundary, including image calls.

ID: C0-3
Severity: medium
File:line: src/ids.rs:236
Description: `ChunkId` parsing validates only a generic slug, rather than the `{source_id}-p{page}-{modality}-{index}` format required by PLAN §2.1. Serde uses the same permissive parser. This violates the validated-newtype boundary described by `api-parse-dont-validate`.
Impact: Successfully parsed chunk IDs need not contain source, page, modality, or index information. Invalid IDs can enter `RecordId::derive` and adapter inputs despite the type claiming to enforce the identifier contract.
Reproduction scenario or verifiable rationale: `"bogus"`, `"src-001-p0-text-3"`, and `"src-001-p2-audio-3"` all satisfy `is_slug` and therefore parse as `ChunkId`; none conforms to the required format.
Validation suggestion: Parse the structured suffix, require a nonzero page, a supported modality, and a valid index, and validate the source portion. Add rejection tests through both `FromStr` and serde, plus constructor/parser round-trip tests.

🌱 graft saved ~21,534 tokens (~$0.01) this turn, 1 call.