#!/usr/bin/env bash
# Builds a self-contained adversarial review package for one phase (execute prompt §7.1).
# Usage: scripts/review-package.sh <phase-number> <base-ref> <reviewer-name> <out-file> [history-file]
# The package inlines the phase scope, PLAN contracts, the complete phase diff (text files;
# binary fixtures are listed, not inlined), and validation evidence, so the reviewer needs
# no tool access. It never includes secrets or real medical documents.
set -euo pipefail
phase="$1"; base="$2"; reviewer="$3"; out="$4"; history="${5:-}"
cd "$(git rev-parse --show-toplevel)/embeddings-fundamentals"
head_ref="$(git rev-parse --short HEAD)"

section() { awk -v start="$1" -v stop="$2" '$0 ~ start {p=1} p && $0 ~ stop && $0 !~ start {exit} p' "$3"; }

{
cat <<HDR
You are ${reviewer}, an ADVERSARIAL code reviewer, independent of the implementer.
Do NOT use any tools, do NOT run commands, do NOT edit files: everything you need is inlined below.

Review Phase ${phase} of the embeddings-fundamentals project (Rust 2024, Cucumber BDD).
Reviewed state: commit ${head_ref} compared with base ${base}.

Look for, and only report what you are confident is real:
- Correctness defects and missing requirements of this phase (PLAN phase scope + exit criteria).
- Violations of PRD and PLAN contracts (PLAN §2.2 trait dispatch, §2.3 Rust conventions, §4 reliability, §5 testing).
- Tests that pass without proving the intended behavior (tautologies, expected values copied from code, steps that do not invoke real operations).
- Ownership, concurrency, async-blocking, and error-handling problems; unwrap/expect/panic in production code.
- Integration, data-integrity, and reproducibility failures.
- Regressions and untested failure paths.
- Violations of rust-skills rules (cite rule IDs such as async-no-lock-await, err-no-unwrap-prod).

For EACH finding output exactly these fields:
ID (${reviewer:0:1}${phase}-1, ${reviewer:0:1}${phase}-2, ...), Severity (critical/high/medium/low), File:line, Description, Impact,
Reproduction scenario or verifiable rationale, Validation suggestion.
If you find nothing valid, output exactly: NO FINDINGS.
Do not praise. Do not restate the code. End with nothing else.

HDR
if [[ -n "$history" && -f "$history" ]]; then
  echo "===== FINDINGS HISTORY AND RESOLUTIONS (earlier review rounds) ====="
  cat "$history"; echo
fi
echo "===== PLAN: phase ${phase} scope and exit criteria ====="
section "^### Phase ${phase} " "^### Phase |^---" spec/PLAN.md
echo; echo "===== PLAN §2 Core contracts (2.1–2.3) ====="
section "^## 2\\. Core contracts" "^## 3\\. Phases" spec/PLAN.md
echo; echo "===== PLAN §4 Reliability & data handling ====="
section "^## 4\\. Reliability" "^## 5\\. Testing" spec/PLAN.md
echo; echo "===== PLAN §5 Testing layers ====="
section "^## 5\\. Testing" "^## 6\\. CI" spec/PLAN.md
echo; echo "===== PRD §5.7 BDD contract ====="
section "^### 5\\.7 " "^## 6\\. " spec/PRD.md
echo; echo "===== PROGRESS RECORD (implementer's claims + validation evidence) ====="
cat reports/progress/PROGRESS.md
echo; echo "===== BINARY / LOCK / RESULT-JSON FILES CHANGED (not inlined; result JSONs are rendered in the inlined comparison.md) ====="
git diff --stat "${base}...HEAD" -- '*.pdf' '*.png' '*.jpg' '*Cargo.lock' 'reports/phase-*/*.json' | cat
echo; echo "===== COMPLETE PHASE DIFF (${base}...HEAD, text files) ====="
git diff "${base}...HEAD" -- . ':(exclude)*.pdf' ':(exclude)*.png' ':(exclude)*.jpg' ':(exclude)*Cargo.lock' ':(exclude)reports/reviews/**' ':(exclude)reports/phase-*/*.json' | cat
git diff "${base}...HEAD" -- ../.github | cat
} > "$out"
wc -c "$out"
