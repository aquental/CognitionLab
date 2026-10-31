## Coverage

Reviewed commit `08c9cbf2e355b38a5a9e2685677a1c281a5ffe5a`. No files were modified; no builds or tests were run. Execution results below come from `.review/PACKAGE.md`. CPython observations used read-only commands with Python 3.14.10 / Unicode 16.0.0.

Paths below are relative to `embeddings-fundamentals/`. All 15 files represented in `.review/phase.diff` were read:

| File | Specification checked |
|---|---|
| `reports/progress/PROGRESS.md` | §3, §14.4, §14.8, §20 |
| `reports/reviews/classifier/classifier-findings.md` | §3, §20; findings history |
| `reports/reviews/classifier/phase-0/agy-round2.md` | §20; recorded Phase 0 closures |
| `scripts/gen-pyclasses.py` | §4.2, §6.3, §14.4, §20 |
| `spec/CLASSIFIER.md` | §3–§4, §6.1–§6.6, §7.4, §14.1–§14.4, §20 |
| `src/classifier/canonico.rs` | §4.3, §7.4, §14.1–§14.2, §20 |
| `src/classifier/estado.rs` | §3.3, §6.1–§6.2, §14.1, §20 |
| `src/classifier/listas.rs` | §3.3, §14.1, §20 |
| `src/classifier/mascaramento/conhecidos.rs` | §6.1–§6.3, §14.1, §20 |
| `src/classifier/mascaramento/mod.rs` | §3, §4.1, §20 |
| `src/classifier/mod.rs` | §4.1–§4.3, §20 |
| `src/classifier/texto.rs` | §4.2–§4.3, §6.1–§6.4, §6.6, §14.2, §14.4, §20 |
| `src/classifier/tipos.rs` | §5.5, §7.2; corrected `margem` documentation |
| `tests/classifier_pyclasses.rs` | §6.3, §14.4, §20 |
| `tests/classifier_vectors.rs` | §4.2, §14.1, §20 |

Also inspected the omitted generated `pyclasses.rs`: metadata, all table structures and ordering, mapping entries relevant to edge cases, and its generator hash. Read all Phase 1 fixture cases and all 13 lists directly from `vetores.json`; its SHA-256 matches the pin.

Rust review used `PLAN.md` §2.2–§2.3, `CLAUDE.md`, and the available local `rust-dev` guidance. No additional privacy or lint violation was identified in the changed code.

## Phase 1 work and exit criteria

| Item | Verification |
|---|---|
| Work 1: generator, tables, synchronization | Inspected every generator predicate/mapping, generated table structure, matching generator hash, synchronization tests, and recorded regeneration success. |
| Work 2: text primitives and offsets | Traced normalization, folding, predicates, casing, whitespace, line breaks, and offset boundaries; CPython confirms the casing defect in C1-1. |
| Work 3: canonical JSON and digest | Inspected recursive sorting, escaping, integer encoding, preservation of decomposed strings, and SHA-256; float handling violates §7.4, as C1-2 describes. |
| Work 4: name parts and permitted terms | Checked composition/folding order, token classes, minimum length, particles, vocabulary filtering, deduplication, and occurrence order against every relevant vector. |
| Work 5: §14.4 tests | Inspected Unicode edge-case assertions and differential-test inputs; IGNORECASE and patient-hint integration are explicitly deferred to Phase 2. Contextual sigma coverage is missing. |
| Exit: TV `dobrar` | Read all 5 cases and their registered byte-equality check; package records success. |
| Exit: TV `compor` | Read all 3 cases and their registered check, plus line-break unit assertions; package records success. |
| Exit: TV `partes_de_nome` | Read all 5 cases and their registered check; package records success. |
| Exit: TV `termos_permitidos` | Read all 6 cases and their registered check; package records success. |
| Exit: TV `json_canonico` | Read all 3 cases and JSON/digest assertions; package records success. These cases contain no floats. |
| Exit: `dobrar` proptest | Inspected the code-point-count invariant and generator; package records the passing unit suite. |
| Exit: `json_canonico` proptest | Inspected reversed insertion-order comparison with unique keys and confirmed `preserve_order` is enabled for tests; package records success. |
| Common phase gates | Package records all five §14.8 commands succeeding at the reviewed commit. Antigravity’s Phase 1 review remains explicitly open. |

## Findings

### C1-1 — medium — String casing omits CPython’s contextual final sigma

**File and line:** `src/classifier/texto.rs:107` and `:129–134`; related generator mapping at `scripts/gen-pyclasses.py:97`.

**Description and impact:** `lower()` and the lowercase branch of `title()` apply isolated-character mappings. CPython applies contextual handling to capital sigma, producing final sigma `ς` when appropriate. The generated `Σ → σ` entry is correct for an isolated character but insufficient for string casing. These public primitives therefore violate Python compatibility and can produce different normalized patient hints.

**Reproduction / rationale:** Read-only evaluation on CPython 3.14.10, UCD 16.0.0:

| Operation | CPython | Rust result implied by implementation |
|---|---|---|
| `"ΟΣ".lower()` | `"ος"` | `"οσ"` |
| `"ΟΣ".title()` | `"Ος"` | `"Οσ"` |
| `"ΟΣ\u0301".lower()` | `"ος\u0301"` | `"οσ\u0301"` |

The differential alphabet at `tests/classifier_pyclasses.rs:93–99` does not explicitly include `Σ`; relying on `any::<char>()` makes this defect easy to miss.

**Suggested validation:** Add deterministic CPython-backed cases for final and non-final sigma, including intervening case-ignorable characters: `"ΟΣ"`, `"ΟΣΑ"`, `"ΟΣ\u0301"`, `"Ο\u0301Σ"`, and `"AΣ\u0301B"`. Implement contextual string casing using Unicode 16 properties while preserving `dobrar`’s specified per-character semantics.

### C1-2 — medium — Canonical JSON rejects values explicitly supported by §7.4

**File and line:** `src/classifier/canonico.rs:67–74`; incorrect regression expectation at `:142–147`.

**Description and impact:** §7.4 explicitly requires `json_canonico` to accept any `serde_json::Value` and serialize floats through `serde_json`. The implementation instead returns `NaoInteiro` for every floating-point number, including floats nested inside otherwise valid objects. Consequently, canonical serialization and digest generation fail for inputs the contract permits.

**Reproduction / rationale:** The existing `floats_are_refused` test passes `{"a":[1,0.5]}` and asserts an error. CPython with the specified options produces:

```json
{"a":[1,0.5]}
```

The three conformance vectors contain no floats, so their success does not validate this behavior. The current unit test enforces the contract violation.

**Suggested validation:** Replace the rejection expectation with scalar and nested float serialization checks, including `0.5`, `1.0`, and `-0.0`, following §7.4’s prescribed serialization. Check that `digest` hashes the resulting UTF-8 bytes.