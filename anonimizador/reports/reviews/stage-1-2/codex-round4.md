ID: CB-23
Severity: high
File:line: src/anonimizador/detect.py:80
Description: `NUMBER_PART` rejects identifiers with a state prefix attached to their digits, including the Brazilian RG format `MG-12.345.678`. CB-19 added state suffixes only.
Impact: A complete RG can survive redaction and pass verification when it is absent from the terms file.
Reproduction scenario or verifiable rationale: For the words `RG:` and `MG-12.345.678`, `number_value` rejects the second word: it starts with neither a digit nor a standalone state. The fallback condition also fails. No other configured pattern matches this value.
Validation suggestion: Add detection and batch regressions for state-prefixed RG values without supplied terms. Require the entire identifier’s box to be redacted.

ID: CB-24
Severity: high
File:line: src/anonimizador/detect.py:286
Description: A detached dash between a bare `Paciente` or `Mãe` label and its value causes the entire name to be rejected by `name_shaped`.
Impact: Names in ordinary fields such as `Paciente - Sicrano Inventado` survive despite recognition of the label.
Reproduction scenario or verifiable rationale: With these four words on one row, `beside` returns `-`, `Sicrano`, and `Inventado`. The dash is neither capitalized nor a particle, so `name_shaped` rejects the whole value. With nothing beneath the label, every fallback fails and no detection is emitted.
Validation suggestion: Test detached hyphens and en dashes after both labels, without terms. Skip field separators before validating the name, while retaining the existing negative tests for running prose.

ID: CB-25
Severity: medium
File:line: src/anonimizador/locate.py:228
Description: The CB-22 fix calls `Image.get_flattened_data()`, which is unavailable in Pillow 10.1 despite the declared dependency allowing `pillow>=10.1`.
Impact: A supported installation crashes with an uncaught `AttributeError` when processing a 16-bit PNG with transparency, aborting the batch without its intended error handling.
Reproduction scenario or verifiable rationale: Install Pillow 10.1 on Python 3.12 and exercise the existing transparent 16-bit fixture. Its integer transparency key reaches this call. Neither `IMAGE_ERRORS` nor the batch handlers catch `AttributeError`.
Validation suggestion: Run the regression against the minimum supported Pillow version. Use a compatible pixel-reading API or raise the dependency minimum to a version providing this method.