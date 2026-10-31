ID: CB-26
Severity: high
File:line: src/anonimizador/detect.py:307
Description: The CB-24 fix still rejects a capitalized name when the separating dash is attached to its first word.
Impact: The entire name can survive redaction when it is absent from the terms file.
Reproduction scenario or verifiable rationale: `found(line("Paciente", "-Sicrano", "Inventado"))` returns no detections. `content` retains `-Sicrano`, and `name_shaped` rejects its initial `-`. Both searches beside the label inspect the same rejected value.
Validation suggestion: Add regressions for attached hyphens and en dashes after `Paciente` and `Mãe`, retaining the negative cases for prose and separators alone.

ID: CB-27
Severity: medium
File:line: src/anonimizador/detect.py:364
Description: The four-token limit counts punctuation tokens toward the number’s length, truncating identifiers whose separators are extracted separately.
Impact: Identifier digits remain visible after redaction.
Reproduction scenario or verifiable rationale: For `line("RG:", "12", ".", "345", ".", "678", "-", "9")`, `number_value` examines only `12 . 345 .` and marks only `12` and `345`. The remaining digits match none of the standalone patterns. Since a number was found, no fallback runs.
Validation suggestion: Test separately tokenized identifier punctuation and require every digit-bearing token to be covered. Bound meaningful number components rather than counting separators against the limit.

ID: CB-28
Severity: medium
File:line: src/anonimizador/detect.py:164
Description: Numeric detection folds the document’s digits but compares them against the term’s unfurled digits, differing from `Term.count`.
Impact: A listed numeric identifier using fullwidth digits receives no term detection.
Reproduction scenario or verifiable rationale: `Term("１２３４５").count("１２３４５")` returns 1. However, `join` converts the document word to `12345`, while `term.digits` remains `１２３４５`; consequently, `term_spans` returns no spans.
Validation suggestion: Add a regression comparing `Term.count` with detections for fullwidth numeric terms and make their numeric normalization consistent.

ID: CB-29
Severity: low
File:line: tests/test_locate.py:152
Description: The CB-25 compatibility test fails on supported Pillow versions that lack `get_flattened_data`.
Impact: The test suite fails on Pillow 10.1 before exercising the compatibility fix.
Reproduction scenario or verifiable rationale: `monkeypatch.setattr` defaults to `raising=True`. Attempting to replace the absent `Image.Image.get_flattened_data` therefore raises `AttributeError`.
Validation suggestion: Use `raising=False` when installing the failing stub, and run this regression against the declared minimum Pillow version.