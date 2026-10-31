ID: CB-32
Severity: high
File:line: src/anonimizador/detect.py:412
Description: Colon labels are recognized in folded text, but their glued values are detected using an ASCII colon in the original text. A fullwidth colon (`U+FF1A`) therefore hides a glued value.
Impact: A patient name can survive both detection readings and remain visible in the copy.
Reproduction scenario or verifiable rationale: A single word `Paciente：Sicrano` folds to `paciente:sicrano`, so it matches `COLON_LABEL` and skips bare-label handling. However, `partition(":")[2]` on the original word is empty. With no surrounding words, neither fallback supplies a value, and no detection is produced.
Validation suggestion: Add a regression using this single word without terms. Require its box to be detected in both readings, and determine the glued suffix using the folded match’s position rather than an ASCII delimiter in the original word.

ID: CB-33
Severity: high
File:line: src/anonimizador/detect.py:88
Description: The number-value pattern accepts a standalone number introducer (`nº`) but rejects it when attached to the number.
Impact: A council or document number following a recognized label can remain completely unredacted.
Reproduction scenario or verifiable rationale: For the two words `CRM` and `Nº123456`, folding produces `crm no123456`. `NUMBER_PART` accepts `no` alone but cannot consume its following digits. Consequently, `number_value` returns nothing, the fallback condition fails, and the glued-label check examines only `CRM`. No independent identifier pattern matches the six-digit value.
Validation suggestion: Add regressions for `CRM Nº123456` and `CRM N°123456` without terms. Require the complete number word to be detected, while retaining rejection of ordinary prose after a number label.