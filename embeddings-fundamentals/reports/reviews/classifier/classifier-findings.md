# Classifier findings

Adversarial review findings for the classifier port (`spec/CLASSIFIER.md`), phase by phase.

## Process
- **Order per phase:** Antigravity reviews the phase's changes first. Codex reviews them after Antigravity's fixes, and only after 03:30.
- **Handling:** every finding is checked against the code and the sources. A valid finding is fixed, with a regression test. A false one is refuted with evidence.
- **Moving on:** after 3 cycles, the work may move to the next phase if only low-severity findings remain.
- **Raw reports:** `reports/reviews/classifier/phase-N/{agy,codex}-roundK.md`.
- **Owner decision, 2026-10-31 (Antigravity quota):** while Antigravity has no quota (until about 2026-11-05 07:00), each phase goes on with Codex only, for up to 3 cycles. When the quota returns, Antigravity reviews every phase it missed, and its findings are handled as usual. This is a temporary waiver of the Antigravity step, not an approval.
- **Owner decision, 2026-10-31 11:30 (every reviewer out of quota):** Codex hit its usage limit (until 2026-11-04 03:38) and Grok has no balance (402), so the Codex round 2 of Phase 1 and round 1 of Phase 2 did not run; nothing was sent. The owner chose to go on with the next phases (TDD, local commits, no push) and to run the pending reviews in phase order when Codex returns, then Antigravity's. Findings go into the phases already done. No phase counts as approved until its reviews ran.
- **Owner decision, 2026-10-31 11:35: review suspended.** No review runs, not even when the quotas return, until the owner resumes it. The pending reviews are listed in `reports/progress/PROGRESS.md`.

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

| C0-1 | Codex | 1 | low | The doc of `Classificacao::margem` said "chosen probability minus the next highest"; spec §7.2 defines it as `max(0, highest − second highest)`, whichever type was chosen (TV has a case where the chosen type is not the highest). | Yes | Fixed the doc comment (commit `08c9cbf`). The calculation itself is Phase 3, and the TV case keeps it honest. | Fixed |
| C0-2 | Codex | 1 | low | The evidence lacked `cargo test --features classifier-jev`, one of the §14.8 acceptance commands. | Yes | Ran the five §14.8 commands in clean worktrees. At `76b5eba` (Phase 0): all exit 0, 222 tests with and without the feature. At `08c9cbf` (Phase 0 + 1 + C0-1): all exit 0, 246 tests both ways. The review packages now include the feature test. A first try at `08c9cbf` failed only because two worktrees shared one `CARGO_TARGET_DIR`; it passed with its own. | Fixed |

Codex round 1 (commit `76b5eba`, `phase-0/codex-round1.md`): it agreed with the closures of A0-1, A0-2 and A0-3 and found C0-1 and C0-2, both low.

Codex round 2 (commit `08c9cbf`, `phase-0/codex-round2.md`): C0-1 and C0-2 closed, NO FINDINGS. **Phase 0 is closed.** A0-4 still waits for the owner's push in TestoDoc.

Note on round 1: the first answer was a bare `NO FINDINGS`, with no coverage. It was not accepted, and the review was run again with a required Coverage section and one verification line per criterion. That second run is `phase-0/agy-round1.md`.

## Phase 1 — primitives and Unicode

**Antigravity did not review this phase.** Its run on 2026-10-31 at 02:56 ended with `RESOURCE_EXHAUSTED` (429, "Individual quota reached", reset in 124 h, about 2026-11-05 07:00) and no review. Nothing was simulated. Under the owner's decision above, Codex reviews the phase now, and Antigravity reviews it after the quota resets.

| ID | Reviewer | Round | Severity | Finding | Valid? | Solution (summary) | Status |
|---|---|---|---|---|---|---|---|
| C1-1 | Codex | 1 | medium | `lower()` and the lowercase branch of `title()` mapped each code point alone; CPython lowers a capital sigma by context (`"ΟΣ".lower() == "ος"`, `"ΟΣ".title() == "Ος"`). | Yes | The generator now writes a Case_Ignorable table, read from CPython's own `str.lower()` (unicodedata does not expose the property). `lower` and `title` apply CPython's `handle_capital_sigma`; `dobrar` keeps folding each code point alone, as TestoDoc does. Unit test with the reviewer's cases plus others. Sigma variants and Case_Ignorable characters were added to the Python differential. The mutant "sigma by table" is detected by both. Spec §6.3 updated. | Fixed |
| C1-2 | Codex | 1 | medium | `json_canonico` refused floats, but spec §7.4 accepts any `Value`. | Yes, and the spec was also wrong: it said floats go out "com `serde_json`", whose format is not Python's (`1e-9` instead of `1e-09`, `1e16` instead of `1e+16`). | `json_canonico` and `digest` can no longer fail. A float is written as Python's `repr`: the shortest round-trip digits, ties to the even digit, and Python's notation switch. A new opt-in differential compares 20 000 random floats plus the boundary values with `json.dumps` on Python 3.14; it found the tie case (`1898340296299833.25`), which is now fixed and kept in the test. Spec §7.4 corrected. | Fixed |
