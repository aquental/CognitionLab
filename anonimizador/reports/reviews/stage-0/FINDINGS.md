# Stage 0: adversarial review findings

Stage 0 is the project structure plus `verify`, moved from `embeddings-fundamentals`. The review order follows `CLAUDE.md`: Grok, then Codex, then Antigravity, repeated until no reviewer has a valid finding.

## Carried over from `embeddings-fundamentals` Phase 4, Grok round 4

The verifier was reviewed in `embeddings-fundamentals` before it moved here. The verbatim report is `embeddings-fundamentals/reports/reviews/phase-4/grok-round4.md`. Its two findings on the verifier are fixed here.

| ID | Reviewer | Severity | Description | Resolution | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| G4-5 | Grok | high | A failed tool (non-zero exit, encrypted PDF, missing `por` data) left its channel empty, and the file was reported `ok` with exit 0. A timeout raised `TimeoutExpired`, whose message holds the file path | `channels.run` raises `ToolError`, which names the tool and never the arguments, for a non-zero exit, a timeout (`--timeout`), or a tool that cannot start. Each channel records its own error, and the file is reported `ERROR` with `unreadable: <place>: <tool failure>`. A run with an unreadable file exits 2. Pages that did not all render are an error. Missing OCR language data is refused before any file. An `OSError` is reported by its reason only | `src/anonimizador/channels.py`, `verify.py`, `cli.py` | `tests/test_cli.py`, with stub tools on PATH: a failing tesseract, a hung tesseract (timeout), a pdftoppm that renders nothing, failing pdfinfo and pdftotext, and missing `por` data. None prints `ok`, a path, or a traceback. The "failure is empty text" mutant fails 2 of them | pending (stage 0 review) | fixed |
| G4-6 (verifier part) | Grok | medium | The `name` channel searched only the file's base name, so a name in a directory (`MARIA SILVA/lab-001.pdf`) was never counted | A file found under a directory is named by its path relative to that directory, directories included, and that name is the `name` channel. `--show-names` prints the same relative name | `src/anonimizador/verify.py` | `tests/test_cli.py::test_a_name_in_a_directory_is_searched_and_never_printed` | pending (stage 0 review) | fixed |
