ID: C4-1
Severity: high
File:line: src/experiment.rs:529
Description: `run_combined` does not preserve incomplete ingestion status from either leg. It checks corpus/evaluation provenance and requires at least one pinned revision per space, but neither the snapshot nor `CombinedResult` carries the failed-source status.
Impact: A combined run can return exit 0 and publish apparently complete evidence despite a required source having failed, violating PLAN §4.3.
Reproduction scenario or verifiable rationale: Fail ingestion of a required source that has no relevance labels while allowing other sources to succeed. Its absence does not invalidate the remaining labels. Combining that leg passes the checks shown here and returns a successful `CombinedResult`.
Validation suggestion: Add a regression with an unlabeled required source failing in one leg. Persist and check leg completion status; refuse the combination or propagate an incomplete result and exit 2.

ID: C4-2
Severity: medium
File:line: src/cli.rs:595
Description: The remote-permission helper checks the supplied manifest without establishing that it belongs to the indexed corpus. The new combined-search and label-drafting paths subsequently load snapshots but never compare their corpus checksums with that manifest.
Impact: Permission for one corpus authorizes remote query embedding against another corpus, violating the documented per-corpus permission boundary.
Reproduction scenario or verifiable rationale: Supply a remotely permitted manifest B while searching a combined snapshot from corpus A, whose current manifest refuses remote processing. Fusion and space checks do not inspect the supplied manifest, so the remote adapter is constructed and receives the query.
Validation suggestion: Use two synthetic manifests and a recording remote adapter. Verify that an unrelated permitted manifest is refused before adapter construction or query embedding. Bind permission checks to every selected snapshot’s corpus provenance.

ID: C4-3
Severity: medium
File:line: src/report.rs:453
Description: A combined leg is matched to its reported standalone run using only experiment ID and embedding-space ID. Those values do not identify the revisions searched: extraction and chunking can change while the embedding space remains unchanged, and experiment IDs are unique only within a store.
Impact: The report can calculate “combined against its legs” deltas against a different run, producing misleading quality and latency comparisons.
Reproduction scenario or verifiable rationale: Generate a combined result in store A. In fresh store B, reuse the leg experiment IDs and models with different chunking settings on the same manifest and dataset. Supply B’s standalone results with A’s combined result. The existing checks accept them despite different revision sets.
Validation suggestion: Record revision provenance or its canonical fingerprint in each combined leg and verify it against standalone results. Add a regression mixing results from two stores with identical experiment IDs and spaces but different revisions.

ID: C4-4
Severity: medium
File:line: src/drafts.rs:148
Description: Recursive corpus discovery embeds the current directory’s full path in `DraftError::Read`. A directory containing a personal name therefore escapes through the error path before filename findings can refuse it.
Impact: `draft-manifest` can print identifying directory names, contradicting its privacy guarantees and PLAN §2.3’s `obs-no-sensitive-data` rule.
Reproduction scenario or verifiable rationale: Place a synthetic PDF under `corpus/FULANO DE TAL/`, make that directory unreadable, and invoke `draft-manifest` as an unprivileged user. `read_dir` fails and the propagated error includes the identifying directory path.
Validation suggestion: Add an unreadable-directory regression asserting that stderr contains no identifying path component. Return a neutral traversal error or opaque source reference instead of the directory value.

ID: C4-5
Severity: low
File:line: README.md:118
Description: The README states that each phase passed its independent adversarial review loop, including Antigravity from Phase 4, although Phase 4’s progress record explicitly says those reviews remain in progress.
Impact: The publication overstates completed validation and presents outstanding review work as finished.
Reproduction scenario or verifiable rationale: Compare this assertion with the supplied Phase 4 status and the current request identifying this as Codex round 1; Antigravity has not yet completed the required sequence.
Validation suggestion: Describe Phase 4 reviews as pending until all required reviewers have confirmed closure, and keep the README consistent with the progress record.