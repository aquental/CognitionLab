ID: CB-1
Severity: high
File:line: src/anonimizador/batch.py:222
Description: Output files are overwritten without requiring a new output directory or checking whether the destination aliases an original.
Impact: A batch can modify originals. Subsequent runs also overwrite copies referenced by earlier indexes, invalidating their recorded verification results.
Reproduction scenario or verifiable rationale: Outside git, place `a.pdf` and a different PDF named `neutral_name(a.pdf)` in the source directory. Run with `out=source`. Processing `a.pdf` overwrites the second original before its turn. A pre-existing output symlink or hard link to an original has the same effect.
Validation suggestion: Test destination collisions and links to originals; require refusal before any original changes. Create outputs exclusively within a fresh run directory.

ID: CB-2
Severity: high
File:line: src/anonimizador/batch.py:153
Description: The index’s git check examines its lexical path without checking an existing file symlink’s target.
Impact: An index containing original filenames can be written into a committable repository file.
Reproduction scenario or verifiable rationale: Create an output directory outside git and a symlink named for the upcoming timestamped report pointing to a file inside a repository. Both checks run from directories outside git and return false. Creating the new log directory succeeds, and `run.report.write_text()` follows the report symlink.
Validation suggestion: Add this case with a fixed `now` and assert that no identifying index reaches the repository. Check resolved destinations and create the report exclusively.

ID: CB-3
Severity: high
File:line: src/anonimizador/batch.py:351
Description: The final console message includes the output directory’s complete resolved path.
Impact: Patient names or other identifying directory components leak into both stdout and `console.txt`.
Reproduction scenario or verifiable rationale: Use an allowed output directory such as `/private/Fulano Ficticio/anonymized`. The final message reproduces the name verbatim. The real-exam privacy test explicitly replaces the output path before checking, so it cannot detect this leak.
Validation suggestion: Test an output path containing a synthetic secret without replacing it in captured output. Print only the neutral report basename or a path-free completion message.

ID: CB-4
Severity: high
File:line: src/anonimizador/batch.py:239
Description: `--no-ocr` also disables pixel verification of the rasterized output, even though pixels are now its only document-content channel.
Impact: Unredacted identifiers can survive while the batch reports a verified-clean copy and exits 0.
Reproduction scenario or verifiable rationale: Process a PNG containing a visible listed name, with no identifying metadata, using `--no-ocr`. `read_image_page()` supplies no words, so detection produces no boxes. The rebuilt PNG retains the name, and verification skips OCR. The same gap affects scanned PDF pages.
Validation suggestion: Add an integration test with a visible synthetic name and `--no-ocr`. Require pixel verification before declaring a copy verified, or explicitly report verification as incomplete.

ID: CB-5
Severity: high
File:line: src/anonimizador/detect.py:248
Description: Each page is searched independently, so terms and patterns cannot match across page boundaries as required by the verifier’s matching rules.
Impact: Identifiers split between pages receive no redaction boxes and remain in the copy.
Reproduction scenario or verifiable rationale: Put `Fulano` at the end of page 1 and `Ficticio` at the start of page 2, with `Fulano Ficticio` as the sole term and no labels. Neither per-page call contains the complete compact term. The verifier’s combined page reading can find it.
Validation suggestion: Add a two-page fixture and assert that both fragments receive correctly assigned boxes and disappear after redaction.

ID: CB-6
Severity: high
File:line: src/anonimizador/detect.py:216
Description: Numeric field values containing an alphabetic check digit are rejected in their entirety.
Impact: An RG ending in `X` survives despite its explicit `RG` label.
Reproduction scenario or verifiable rationale: For words `RG:` and `12.345.671-X`, `NUMBER_PART` rejects the value because its numeric branch excludes letters. The value is not attached to the label, and there is no RG pattern fallback. No detection is emitted without an owner-supplied term.
Validation suggestion: Add tests for RG values with numeric and `X` check digits, both attached to and separated from their labels, and assert that the complete value is covered.

ID: CB-7
Severity: high
File:line: src/anonimizador/detect.py:198
Description: Field labels cannot associate a value on the next line, including birth dates that are promised to be removed under `--keep-dates`.
Impact: Birth dates remain visible while the report states that birth dates were removed.
Reproduction scenario or verifiable rationale: Put `Data de nascimento:` on one line and `01/02/1980` on the next. With `remove_dates=False`, the pattern detector ignores the date and the label detector examines only the label’s line. Neither reading produces a box for the value.
Validation suggestion: Add native-text and OCR fixtures with birth-date values below their labels. Verify removal under both date policies.

ID: CB-8
Severity: medium
File:line: src/anonimizador/batch.py:203
Description: Errors returned by verification of the original are recorded in its status but never added to the run’s error totals.
Impact: A failed verification step can be followed by an exit status of 0 and a summary claiming zero errors.
Reproduction scenario or verifiable rationale: An original can have an unreadable raw or metadata channel while its pages still render successfully. If the rebuilt copy verifies clean, only the copy’s errors are counted at line 243; the original’s errors have been discarded from the exit-status calculation.
Validation suggestion: Test an original verification result containing an error followed by successful rendering and clean output verification. Require exit 2 and a nonzero error total.

ID: CB-9
Severity: medium
File:line: src/anonimizador/locate.py:87
Description: Parsing failures from external-tool output are not converted into sanitized step errors. `ET.ParseError`, numeric-conversion `ValueError`, and missing-field `KeyError` escape the batch’s exception handlers.
Impact: The command can terminate with a traceback instead of an error row, completed index, and controlled exit 2.
Reproduction scenario or verifiable rationale: A `pdftotext` stub that exits successfully but returns malformed XHTML reaches `ET.fromstring()` and raises `ET.ParseError`. Neither `anonymize()` nor `run_batch_command()` catches it. Malformed bounding-box attributes and TSV coordinates follow equivalent paths.
Validation suggestion: Add successful-exit stubs returning malformed XHTML and TSV. Assert sanitized error reporting, continued processing of other originals, and exit 2.

ID: CB-10
Severity: medium
File:line: src/anonimizador/detect.py:152
Description: Short multiword terms use non-overlapping regex matches, whereas `Term.count()` counts overlapping occurrences through a lookahead.
Impact: Words belonging to a verifier-recognized occurrence can remain unredacted.
Reproduction scenario or verifiable rationale: `Term("Li Li").count("Li Li Li")` returns two occurrences. The detector matches only the first `Li Li`, resumes after that match, and leaves the third word without a box.
Validation suggestion: Add this overlapping short-term case and assert that every word participating in either occurrence is marked. Use overlapping span enumeration consistent with `Term.count()`.

ID: CB-11
Severity: medium
File:line: src/anonimizador/detect.py:223
Description: A number attached to its field label is ignored unless the same word contains at least four digits.
Impact: Short council registration numbers survive depending solely on tokenization.
Reproduction scenario or verifiable rationale: `CRM:123` produces no detection: there are no following words, and the attached value fails the four-digit threshold. The equivalent two-word input `CRM:` followed by `123` is detected.
Validation suggestion: Test short registration values with attached and detached labels and require equivalent redaction coverage.