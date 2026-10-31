# Classifier findings

Adversarial review findings for the classifier port (`spec/CLASSIFIER.md`), phase by phase.

## Process
- **Order per phase:** Antigravity reviews the phase's changes first. Codex reviews them after Antigravity's fixes, and only after 03:30.
- **Handling:** every finding is checked against the code and the sources. A valid finding is fixed, with a regression test. A false one is refuted with evidence.
- **Moving on:** after 3 cycles, the work may move to the next phase if only low-severity findings remain.
- **Raw reports:** `reports/reviews/classifier/phase-N/{agy,codex}-roundK.md`.

Severity: high, medium, or low, as the reviewer gave it. A changed severity is noted in the solution column.

## Spec (before Phase 0)

The spec was reviewed by Antigravity in 2 rounds:
- round 1 had 12 findings, all valid and all fixed;
- round 2 closed all of them and returned NO FINDINGS.

The full table is in `spec/CLASSIFIER.md` §21.2, and the raw reports are in `spec/`.

## Phase 0 — preparation

| ID | Reviewer | Round | Severity | Finding | Valid? | Solution (summary) | Status |
|---|---|---|---|---|---|---|---|
| A0-1 | Antigravity | 1 | medium | `Classificacao` (§5.5) was missing from `tipos.rs` and its re-exports, with its rule that `probabilidades` serialize in the order of `Categoria::TODAS` and `None` as `{}`. | Yes | Added `Classificacao` as §5.5 defines it, and `serializar_probabilidades` for `serialize_with`. The serializer writes the keys in the order of `TODAS`, without depending on `preserve_order`. Two tests: `None` gives `{}`, and the keys come in the order of `TODAS`. Mutants (`null` for `None`; reversed order) are detected. | Fixed; closure confirmed by Antigravity round 2 |
| A0-2 | Antigravity | 1 | medium | Nothing checked that a section in `COVERED` was actually tested. A section moved from `PENDING` to `COVERED` without a test passed in silence. | Yes | `COVERED` is now a registry of (section, check) pairs, and one test runs every entry. Each check gets only the value of its own section. A covered section without a check can no longer be written. A self-test checks that the runner reports a failing section and a missing one. Mutants (failures ignored; missing sections ignored; a section dropped from the registry) are detected. Spec §20, Phase 0, item 6, describes the registry. | Fixed; closure confirmed by Antigravity round 2 |
| A0-3 | Antigravity | 1 | low | No exact key sets for `meta`, `constantes`, `criterio` and each option. The enum tests never deserialized, and `Display` was not tested. | Yes | The checks now compare the exact key sets of `meta` (6), `constantes` (9), `criterio` (3) and each option (4). Every enum now round-trips through `serde_json::from_str`, and an unknown value is refused. `Display` of `Categoria` and `PaginasModo` is tested. Mutants (a smaller key set; a different deserialize name; a wrong `Display`) are detected. | Fixed; closure confirmed by Antigravity round 2 |
| A0-4 | Antigravity | 1 | low | The phase's entry is not met: TestoDoc's rename commit has not been pushed with a clean message, so the README records a description of the commit, not its final SHA. | Yes | This is blocked on an owner action in TestoDoc: amend the message of `6cd3dd0` and push. After the push, record the final SHA in `tests/fixtures/classifier/README.md` and in spec §0.1/§3.4. The vectors' bytes do not depend on it, because the file is pinned by sha256. | Open (owner) |

Round 2 (Antigravity, commit `76b5eba`): it confirmed that A0-1, A0-2 and A0-3 are closed, and gave NO FINDINGS on the whole phase (`phase-0/agy-round2.md`). Codex reviews Phase 0 after 03:30.

Note on round 1: the first answer was a bare `NO FINDINGS`, with no coverage. It was not accepted, and the review was run again with a required Coverage section and one verification line per criterion. That second run is `phase-0/agy-round1.md`.

## Phase 1 — primitives and Unicode

| ID | Reviewer | Round | Severity | Finding | Valid? | Solution (summary) | Status |
|---|---|---|---|---|---|---|---|
