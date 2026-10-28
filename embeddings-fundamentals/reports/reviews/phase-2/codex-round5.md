C2-17: CLOSED — Name matching now requires uniqueness among extracted lines as well as sampled rows. The regression test checks both extracted-line orders.

C2-18: CLOSED — Clock reads no longer advance time. Model loading and embedding advance the clock inside their operations, so misplaced timing boundaries produce incorrect measurements.

ID: C2-19
Severity: medium
File:line: src/evaluation.rs:492
Description: `evaluate` scores repeated occurrences of the same chunk independently without rejecting duplicate retrieved items.
Impact: Duplicate hits inflate precision and can produce nDCG greater than 1, invalidating reported retrieval quality.
Reproduction scenario or verifiable rationale: Construct a dataset with one query and one text chunk labeled 2. Supply three copies of its valid, anchor-matching `SearchHit`. The evaluator reports Precision@3 = 1 and nDCG@3 ≈ 2.13093 because the numerator counts the same judgment three times while IDCG counts it once.
Validation suggestion: Reject duplicate chunk identities within a query’s evaluated ranking. Add a regression using repeated hits and property tests requiring successful evaluations to keep nDCG within [0, 1].

ID: C2-20
Severity: medium
File:line: src/embedding/local.rs:116
Description: The async adapter performs batch tokenization synchronously on the runtime worker before entering `spawn_blocking`. The new OpenAI adapter follows the same pattern. This violates PLAN §2.3’s treatment of substantial CPU work (`async-spawn-blocking`).
Impact: Large batches or oversized inputs can stall a runtime worker during validation, delaying unrelated tasks and timers. Input-length rejection does not prevent this because tokenization happens first.
Reproduction scenario or verifiable rationale: Invoke an adapter on a current-thread runtime with a large input while a timer runs concurrently. Every `TokenCounter::count` completes synchronously before the embedding future reaches its first await; the backend’s blocking closure does not cover this work.
Validation suggestion: Move batch tokenization and input preparation onto the blocking pool. Use a deliberately slow test tokenizer and verify that a concurrent timer progresses while validation runs.

ID: C2-21
Severity: low
File:line: src/embedding/local.rs:38
Description: `LocalTextAdapter` stores `Box<dyn ModelTokenizer>`, introducing dynamic dispatch for another component despite PLAN §2.2 specifying static dispatch for token counters (`trait-dyn-vs-generic`).
Impact: The new shared adapter violates the phase’s explicit dispatch contract. Making the backend generic does not make the tokenizer statically dispatched.
Reproduction scenario or verifiable rationale: The tokenizer field and constructor parameter are trait objects, and both local loaders box their concrete `HfTokenCounter`. The required `EmbeddingStrategy::token_counter` trait-object return does not require boxed storage internally.
Validation suggestion: Parameterize `LocalTextAdapter` over its tokenizer type, store that concrete type, and coerce its borrowed reference only at the required `EmbeddingStrategy` interface.