You are a rigorous systems engineer. Your task is to turn the file docs/system-design/classificador.md into a precise, complete, and verifiable implementation specification, good enough for another system (or agent) to implement the classifier in Rust without ambiguity and without guessing.

INVIOLABLE RULES:
- Never invent behavior, file names, prompts, thresholds, schemas, or algorithms. If you cannot find the evidence in the code, prompts, or tests, state the gap explicitly.
- Every statement about what the classifier does must be traceable to a specific file or prompt.
- Before any edit, read the current document and the real artifacts.
- Produce a clear diff (or a final version plus a changelog), and justify each change with evidence.
- If something is unclear or contradictory, stop and list the questions instead of filling the gaps.

MANDATORY STEPS (run them in order):

1. READING AND INVENTORY
   - Read docs/system-design/classificador.md in full.
   - Find and list ALL the code files, prompts, configs, schemas, tests, and dependencies that implement or define the classifier. Search the repository (names such as classifier, classificador, intent, routing, etc.). Give paths that are absolute or relative to the root.
   - Identify the prompts in use (.md and .txt files, embedded system prompts, etc.) and extract the relevant parts.

2. FIDELITY VALIDATION
   - Compare the current document against the real code and prompts.
   - Produce a "Divergences and gaps" section that lists:
     - What the document states but the code does not do.
     - What the code does but the document omits or describes incorrectly.
     - Ambiguities that would prevent a correct Rust implementation.
   - Continue only when the validation is complete. If there are serious divergences, fix them first.

3. ENRICHING THE DOCUMENT
   Update docs/system-design/classificador.md so that it contains at least:
   - The classifier's objective and context (what it decides and why).
   - Explicit references (paths) to every file and prompt used, with a short description of the role of each.
   - Input and output contracts (types, JSON/struct schemas, required and optional fields, possible values).
   - The decision flow or algorithm (steps, conditions, fallbacks, rule priority).
   - Business rules, thresholds, and input/output examples (positive and negative).
   - Handling of errors, edge cases, timeouts, and missing or malformed data.
   - External dependencies (models, APIs, databases, configs) and how they are injected.
   - Non-functional requirements that matter for a Rust implementation (expected latency, concurrency, thread safety, allocations, etc.).
   - Every invariant that the implementation must preserve.
   - A "What a Rust implementer needs to know" section with an explicit checklist.

4. ADVERSARIAL AUDIT (Antigravity)
   Once the enriched version is validated locally:
   - Prepare a clear package for Antigravity (the updated document, the list of source files, and the critical prompt excerpts).
   - Instruct Antigravity to run an adversarial audit focused on:
     a) Ambiguities that would lead to divergent Rust implementations.
     b) Contradictions inside the document or with the real code.
     c) Edge cases that are not covered.
     d) Hidden assumptions or undocumented dependencies.
     e) What is missing for a complete and correct implementation.
   - Collect Antigravity's criticisms.
   - Validate each criticism against the code and prompts. If it is real, put the fix into the document. If it is false or irrelevant, record why it was rejected.
   - Produce a final changelog of the changes made in response to the audit.

5. DELIVERY
   - The final version of the document (or the diff).
   - The list of file and prompt references.
   - A summary of the divergences found and fixed.
   - Antigravity's criticisms and how each one was handled.
   - An explicit statement: "This document is sufficient for a Rust implementation", or the residual list of gaps that still prevent it.

Start now with Step 1. Do not edit anything until you have the inventory and the fidelity validation.
