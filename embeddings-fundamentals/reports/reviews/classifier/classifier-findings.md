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
