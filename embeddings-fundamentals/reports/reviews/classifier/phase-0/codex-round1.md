## Coverage

Reviewed commit `76b5eba582b92d55d07dda047e03f859c8a44609`. No files were changed; no builds or tests were run. Execution results below come from `.review/PACKAGE.md`. The required `rust-skills` skill was unavailable; I applied `PLAN.md` §2.2–§2.3 and `CLAUDE.md` directly.

Paths below are relative to `embeddings-fundamentals/`. Section references are to `spec/CLASSIFIER.md`.

| File read | Sections checked |
|---|---|
| `Cargo.lock` | §4.2; §20 Phase 0 items 3–4 |
| `Cargo.toml` | §4.2–§4.3; §20 items 3–4; PLAN §2.3 |
| `reports/progress/PROGRESS.md` | §0; §3.4; §14.8; §20 |
| `reports/reviews/classifier/classifier-findings.md` | §5.5; §14.1; §20; prior finding closures |
| `reports/reviews/classifier/phase-0/agy-round1.md` | §0; §2–§5; §14.1; §20; prior finding rationale |
| `spec/CLASSIFIER.md` | Changed architecture and coverage requirements against §0, §2–§5, §14.1 and §20 |
| `src/classifier/categorias.rs` | §2.1; §4.3; §5.1; §14.1; §20 |
| `src/classifier/mod.rs` | §4.1–§4.3; §5; §20 |
| `src/classifier/tipos.rs` | §3; §4.3; §5.2–§5.5; §7.2; §20; PLAN §2.2–§2.3 |
| `src/lib.rs` | §4.1–§4.2; §20 |
| `tests/classifier_vectors.rs` | §0.1; §2.1; §4.2; §5.1; §14.1; §20 |
| `tests/fixtures/classifier/README.md` | §0.1; §3.4; §20 |

Also inspected `tests/fixtures/classifier/vetores.json`, omitted from the diff, against §0.1, §2.1, §3.4, §5.1 and §14.1, and read `.review/PACKAGE.md`.

## Phase 0 verification

- **Entry prerequisite:** The clean, pushed TestoDoc rename commit remains the acknowledged A0-4 owner action; not reported again.
- **1 — Source hashes:** Independently hashed TD and TV at `6cd3dd0` and all five TP files at `06f559f`; every hash matches §0.
- **2 — Fixture copy/provenance/privacy:** Local TV matches SHA-256 `f14676c78e9bf590e0cb1534c83ab456c82cb8bd67e696cc94bb239d891db22e` and 347,582 bytes. README records the privacy check; I did not access real exams or repeat it. Final source-commit provenance remains A0-4.
- **3 — Unicode dependency:** Manifest and lockfile pin `0.1.24`; cached source declares Unicode `(16, 0, 0)`. Cached `pdf-extract` and `stringprep` manifests accept this version. Inspected the Unicode-version regression test.
- **4 — Regex dependency/feature:** Confirmed `fancy-regex = "0.19.2"`, its lockfile resolution, and additive `classifier-jev = ["dep:reqwest"]`.
- **5 — Type skeleton:** Compared all §5.1–§5.5 fields, variants and exports. Provider contracts are explicitly scheduled under Phase 4.
- **6 — Conformance harness:** Inspected the registered checks, failure/missing-section self-test and exact partition: three covered, fourteen pending, two excluded. Duplicate registration catches a covered section left pending.
- **Exit — Compilation:** Package records successful Clippy checks for default and `classifier-jev` configurations.
- **Exit — Types/constants:** Inspected enum parsing/serialization, probability serialization, content predicates and direct TV checks for `meta`, `constantes`, `criterio`; package records passing tests.
- **Cross-phase acceptance (§14.8):** Formatting, both Clippy configurations and default tests are evidenced; feature-enabled test execution is not evidenced—see C0-2.

Prior closures:

- **A0-1: Agree.** `Classificacao` and exports exist; explicit probability serialization and exact-byte tests cover ordering and `None → {}`.
- **A0-2: Agree.** Each covered entry requires a check, and the runner executes every registered check.
- **A0-3: Agree.** Exact key sets, enum deserialization, unknown-value rejection and the requested `Display` checks are present.

## Findings

### C0-1 — low

**File and line:** `embeddings-fundamentals/src/classifier/tipos.rs:364`

**Description and impact:** The public documentation defines `margem` as “Chosen probability minus the next highest.” The normative calculation is the nonnegative difference between the two largest probabilities, independently of the chosen category. This gives future implementers and consumers an incorrect result contract.

**Verifiable rationale:** §7.2 specifies `max(0.0, ordenadas[0] − ordenadas[1])`. TV’s case beginning at `vetores.json:3676` chooses `mapa` with probability `0.3`, while `holter` has `0.6`, and expects margin `0.3`. Subtracting the strongest alternative from the chosen probability produces `-0.3`.

**Suggested validation:** Correct the field documentation to describe the two highest probabilities. Preserve this existing non-maximum-choice vector when implementing Phase 3.

### C0-2 — low

**File and line:** `.review/PACKAGE.md:39`; `embeddings-fundamentals/reports/progress/PROGRESS.md:372`

**Description and impact:** The acceptance evidence omits `cargo test --features classifier-jev`, although §14.8 and §20 require it for every phase. Feature-enabled compilation is evidenced, but feature-enabled test execution is not, leaving the acceptance record incomplete.

**Verifiable rationale:** The package records both Clippy configurations, default `cargo test`, a test inventory and mutants. Neither it nor the progress record contains a feature-enabled test result. Clippy does not execute test assertions.

**Suggested validation:** Run `cargo test --features classifier-jev` for the reviewed revision and record its result, or supply the existing execution evidence if it was already run.