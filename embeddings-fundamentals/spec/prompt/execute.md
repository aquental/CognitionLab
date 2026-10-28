# Mission

Act as a Senior Rust Engineer and fully implement phases 0, 1, 2,
3, and 4 of the embeddings-fundamentals project, using Rust and
Cucumber for BDD tests.

Execute the phases sequentially:
0 → 1 → 2 → 3 → 4.

Each phase requires implementation, validation, and sequential
adversarial reviews by Grok and Codex. Do not advance with unresolved
valid findings or stop between approved phases.

# 1. Sources of Truth

Read before implementing:
- Applicable AGENTS.md instructions.
- embeddings-fundamentals/spec/PRD.md.
- embeddings-fundamentals/spec/PLAN.md.
- embeddings-fundamentals/README.md.
- Existing code, tests, and configuration.

The PRD defines requirements and acceptance criteria.
The PLAN guides architecture, execution, and phase exit criteria.
The README must reflect the actual project state.

Identify inconsistencies between documentation and implementation.
Resolve technical details with documented justification while
preserving requirements. Do not reduce scope or weaken acceptance
criteria to obtain approval.

# 2. Autonomy and Continuity

Automatically proceed to the next phase once the current phase
satisfies every approval criterion. Do not request confirmation
between phases or end execution merely to report progress.

You are authorized to delegate adversarial reviews to Grok and
Codex and to run the tools required for those reviews.

Preserve the user's pre-existing changes. Do not push, merge,
or publish externally without authorization.

Do not invent credentials, remote-processing authorization,
test results, benchmarks, or reviewer reports.

If a genuine external blocker arises:
- Complete any independent work possible within the current phase.
- Document the blocker and the action required to resolve it.
- Do not bypass the affected approval criterion.
- Do not declare the phase or project complete.

Maintain a persistent record that allows work to resume without
repeating completed steps. Session interruptions do not change
the phase order or waive review requirements.

# 3. Rust Engineering Guidelines

Implement idiomatic, safe code proportional to the problem.

Ownership and lifetimes:
- Define data ownership and reference lifetimes before structuring
  interfaces.
- Prefer borrowing when it keeps the code simple and correct.
- Use clone when ownership requires it or when it provides a
  justified clarity benefit; avoid unnecessary copies.
- Do not introduce complex lifetimes solely to eliminate a copy.

Traits and abstractions:
- Use cohesive traits and small contracts.
- Preserve separation between extraction, chunking, embeddings,
  storage, retrieval, evaluation, and orchestration.
- Avoid artificial hierarchies and abstractions without concrete use.
- Prefer zero-cost abstractions where appropriate while respecting
  the planned use of dynamic dispatch and asynchronous adapters.

Error handling:
- Use Result for failures and Option for legitimate absence.
- Prefer typed errors with thiserror in domain contracts.
- Use anyhow where appropriate at application boundaries.
- Preserve sufficient diagnostic context without exposing
  credentials or sensitive content.
- Do not use unwrap() or expect() in production code.
- Do not replace recoverable errors with panic!.
- Intentional failures in tests must produce useful messages.

Safety:
- Avoid unsafe whenever an appropriate safe alternative exists.
- When indispensable, keep unsafe blocks minimal and document
  their invariants with SAFETY comments.
- Pay particular attention to FFI boundaries and platform differences.

Asynchronous execution:
- Follow Tokio ecosystem conventions.
- Do not block runtime threads with CPU-intensive work,
  blocking calls, or synchronous waits.
- Use appropriate isolation mechanisms and bounded concurrency.
- Preserve the specified retry limits and failure states.

Validation:
- Actually run cargo fmt, Clippy, and relevant tests.
- Reasoning or visual inspection does not replace execution.
- Do not add warning suppressions to hide problems.
- Do not add dependencies without checking their necessity
  and compatibility with the project.

# 4. Preparation

Before phase 0:

1. Inspect the repository state, including untracked files,
   and identify pre-existing changes.

2. Establish a baseline for producing complete phase-specific
   diffs without mixing in unrelated changes.

3. Verify that the Grok and Codex executors are available.
   Consult their help and configuration to use actual commands.
   Do not invent flags, models, tools, or capabilities.

4. Verify prerequisites for every phase:
   - Rust toolchain.
   - PDF and OCR tools.
   - Dependencies and models.
   - Anonymized corpus.
   - Relevance labels.
   - Permissions and credentials for remote integrations.

5. Create a progress record for each phase containing:
   implementation, acceptance criteria, validation, reviews,
   findings, resolutions, and blockers.

6. Prepare a location for adversarial review reports,
   preferably reports/reviews/phase-N/.

Provide reviewers with code, specifications, and synthetic fixtures.
Do not send secrets or medical documents.

Synthetic fixtures support development and deterministic tests.
They must not silently replace the corpus or real integrations
required by the acceptance criteria.

# 5. Implementation by Phase

Follow the PLAN's details and exit criteria. The items below
summarize the scope and do not replace the full specification.

Phase 0 — Project structure and test infrastructure:
- Library and binary structure, dependencies, and features.
- Deterministic adapter and fixtures.
- Cucumber runner and scenario inventory.
- Runner self-tests.
- Initial CI.

Phase 1 — Text baseline:
- Manifest and data eligibility.
- Native extraction and per-page OCR.
- Structure-aware chunking and token limits.
- Revisioned storage and atomic publication.
- Validation, bounded retries, and explicit failures.
- EmbedAnything integration.
- Traceable text retrieval.

Phase 2 — Configuration comparison:
- FastEmbed and OpenAI adapters.
- Embedding-space isolation.
- Relevance and extraction-fidelity metrics.
- Performance, resource, and cost measurements.
- Reports and real integrations.

Phase 3 — Image retrieval:
- Model compatibility spike.
- Image extraction and preparation.
- Multimodal embeddings.
- Image retrieval using Portuguese queries.
- Image-only evaluation.

Phase 4 — Combined retrieval and final evidence:
- Reciprocal Rank Fusion.
- Combined retrieval.
- Comparison across all three modes.
- Reports and reproducible configurations.
- Execution instructions and documented limitations.
- Verification of all deliverables and acceptance criteria.

# 6. Tests and Evidence

Implement Gherkin scenarios alongside their corresponding behavior,
using Cucumber for Rust and the following attributes:
#[given], #[when], and #[then].

Steps must:
- Establish controlled preconditions.
- Invoke actual application operations.
- Verify observable outcomes.
- Fail when behavior differs from expectations.

Do not replace the logic under test with predetermined answers.
Deterministic test doubles must control external dependencies
without bypassing the orchestration the scenario is meant to verify.

Derive the cumulative inventory of scenarios and Examples rows
from the PRD and its milestone tags.

Respect phase 0's bootstrap rules and runner self-tests. Starting
with phase 1, reject empty required suites, missing scenarios,
undefined or ambiguous steps, and skipped required scenarios.

Keep evidence separate for:
- Unit and property tests.
- Deterministic BDD, without network access or downloads.
- Integrations with real tools and providers.

Run formatting, Clippy, tests, and the feature matrix required
by the PLAN. Record commands, results, and limitations.

An integration that has not been executed remains outstanding.
Passing deterministic tests does not establish real model quality
or prove that external integrations work.

# 7. Mandatory Adversarial Review for Each Phase

The order is:

implementation → validation → Grok → fixes and confirmation
→ Codex → fixes and confirmation → phase approval.

Reviewers must operate independently of the implementer,
preferably with read-only access.
Do not run Grok and Codex reviews in parallel.

## 7.1 Review Package

Prepare:
- The phase objective and acceptance criteria.
- The complete diff since the phase began, including new files.
- An identifiable reference to the reviewed state.
- Necessary context from earlier phases.
- Executed tests and their evidence.
- Technical decisions, risks, and limitations.

Provide complete files when the diff does not supply enough context.

## 7.2 Grok First

Request an adversarial review looking for:
- Correctness defects and missing requirements.
- Violations of PRD and PLAN contracts.
- Tests that pass without proving the intended behavior.
- Ownership, concurrency, and error-handling problems.
- Integration, data-integrity, and reproducibility failures.
- Regressions and untested failure paths.

Each finding must include:
ID, severity, file/line, description, impact, a reproduction
scenario or verifiable rationale, and a validation suggestion.

Address every finding, including low-severity findings:
- Fix valid findings.
- Add regression tests where appropriate.
- Refute false positives with verifiable evidence.
- Do not defer valid findings to the next phase.
- Rerun affected checks.
- Ask Grok to confirm closure.

## 7.3 Codex Next

Only after Grok's findings are closed, request an adversarial
review from Codex.

Use a separate Codex agent or process, distinct from the implementer.
The implementer's self-review does not replace this step.

Provide:
- The complete, updated phase diff.
- The same review criteria required of Grok.
- The findings history and their resolutions.
- Updated validation evidence.

Request an independent review of the entire phase, not merely
verification of Grok's requested fixes.

Address every Codex finding with the same rigor and request
confirmation of closure after applying fixes.

## 7.4 Repeating the Cycle

If fixes requested by Codex change behavior, contracts, or evidence
already reviewed by Grok, resubmit the updated state to Grok and
then to Codex, preserving that order.

Continue until there are no unresolved valid findings and the
reviews cover the final phase state.

Do not close findings merely because you disagree: document
evidence and obtain confirmation from the respective reviewer.
Unresolved disagreements must remain explicit.

Never simulate an external review. An unavailable reviewer
blocks phase approval.

# 8. Records and Phase Approval

Save the original reviewer reports and a tracking table containing:
- Finding ID and reviewer.
- Severity and description.
- Resolution or evidence-based refutation.
- Affected files.
- Validation evidence.
- Reviewer confirmation.
- Final status.

A phase is approved only when:
- Its scope is implemented.
- Its exit criteria and cumulative requirements are satisfied.
- Required checks pass against the final state.
- Grok and Codex have reviewed in the required order.
- No valid findings or unresolved disagreements remain.
- Documentation and progress records reflect the actual state.

Once a phase is approved, record its completion and immediately
begin the next phase without requesting confirmation.

# 9. Completion

After phase 4:
- Run final cumulative validation.
- Check every PRD deliverable and acceptance criterion.
- Link each criterion to its supporting evidence.
- Update the README with implemented functionality, measured
  results, reproducible instructions, and limitations.

If this validation requires further changes, submit them to the
applicable adversarial review cycle before declaring completion.

Deliver:
- A summary of the implementation across all five phases.
- Execution and reproduction commands.
- Test and integration evidence.
- Links to reviewer reports and finding resolutions.
- Experimental results and actual limitations.
- Any blockers, without presenting them as completed work.

Do not declare the project complete while any mandatory approval
criterion remains outstanding.