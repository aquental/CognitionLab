ID: CB-34
Severity: high
File:line: src/anonimizador/detect.py:221
Description: `ends_field` checks original text for ASCII colons, although label matching recognizes fullwidth colons through folding. Consequently, another field can be mistaken for a heading’s value, preventing detection of the name underneath.
Impact: A patient name absent from the terms file survives redaction.
Reproduction scenario or verifiable rationale: Place `Nome` at `(0, 0, 40, 15)`, `Data：05/06` at `(50, 0, 150, 15)`, and `Sicrano` underneath at `(0, 40, 90, 55)`. The fullwidth colon is not recognized by `ends_field`; `bare_value` accepts `Data：05/06` beside `Nome` and returns without selecting `Sicrano`. With an ASCII colon, the field boundary is recognized and the name underneath is selected.
Validation suggestion: Extend the AB-1 regression to fullwidth glued and detached colons. Normalize field-boundary checks consistently with label matching and assert that the name underneath receives a box.