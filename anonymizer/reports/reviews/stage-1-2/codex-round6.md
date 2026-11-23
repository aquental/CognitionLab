ID: CB-30
Severity: high
File:line: src/anonimizador/detect.py:392
Description: A bare label glued to its value loses the identifying text inside the label’s word. Unlike colon labels, bare labels never mark that word.
Impact: A patient’s first name can survive redaction.
Reproduction scenario or verifiable rationale: `found(line("Paciente-Sicrano", "Inventado"))` marks only `"Inventado"`. The regex recognizes `"Paciente"`, but `bare_value` searches after its containing word. With `"Paciente-Sicrano"` alone, nothing is detected. CB-26 tests a dash attached to a separate value word and does not cover this case.
Validation suggestion: Test both examples without terms and require the word containing the glued name to be boxed.

ID: CB-31
Severity: medium
File:line: src/anonimizador/detect.py:374
Description: The four-word number limit still counts state prefixes and number introducers, truncating valid identifiers split around punctuation.
Impact: Trailing identifier digits remain visible despite the CB-27 fix.
Reproduction scenario or verifiable rationale: `found(line("RG:", "MG", "12", ".", "345", ".", "678", "-", "9"))` takes `"MG"`, `"12"`, `"345"`, and `"678"`, then stops before `"9"`. Similarly, `"CPF", "Nº", "123", ".", "456", ".", "789", "-", "09"` leaves `"09"`. The standalone punctuation prevents the CPF pattern from covering that omission.
Validation suggestion: Add both regressions and require every numeric component to be detected. Account for permitted prefixes separately from the limit on numeric components.