# CLAUDE.md

This file guides Claude Code when it works in this subproject.

## What this is

`anonimizador` holds local Python tools that anonymize medical PDFs and images and verify the result. They produce the anonymized corpus that `../embeddings-fundamentals` evaluates.

- `README.md` holds the status and usage.
- `docs/SPEC.md` holds the requirements.
- `docs/PLAN.md` holds the stages.
- `docs/ANONYMIZATION.md` holds the manual procedure.

Check the code before assuming a command or module exists: `verify` and `batch` (detection, redaction by rasterizing, and verification of the copies) are implemented.

The git root is the parent directory `CognitionLab/`. Reply to the owner in Portuguese.

## Commands

Run all commands from this directory:

```bash
uv sync
uv run anonimizador verify --terms <file outside the repo> <paths...>
uv run anonimizador batch --terms <file outside the repo> --out <git-ignored dir> <originals dir>
uv run pytest
uv run ruff check . && uv run ruff format --check .
RUN_SAMPLE_EXAMS=1 uv run pytest tests/test_sample_exams.py    # local only
# Exam type in the index: the binary sends to Jev only if built with `--features classifier-jev`
# (without it, --classify-remote gives `error: configuracao_invalida` on every copy).
uv run anonimizador batch --classifier ../embeddings-fundamentals/target/release/embeddings-fundamentals ...
```

External tools: poppler (`pdftotext`, `pdfinfo`, `pdftoppm`, `pdfimages`) and `tesseract` with `por` data. End-to-end tests skip themselves when a tool is missing.

## Real documents never leave this machine

The owner's real exams, with patient names in file names and content, are in `../embeddings-fundamentals/tests/sample-exams/`, which git ignores. The rules of `../embeddings-fundamentals/CLAUDE.md` apply here unchanged.

- **What never leaves this machine.** Never commit, push, upload, or send these files, or anything derived from them, to a reviewer (Grok, Codex, Antigravity) or to a remote service. "Derived" covers text, OCR, renders, file names, and words taken from file names.
- **Synthetic data is invented from scratch.** Never model a fixture on a real exam, and never reuse a word from an exam's file name. Before sending a review package, count the file-name words of 4 letters or more that it contains. Report counts only.
- **Tests on real exams are local and opt-in** (`RUN_SAMPLE_EXAMS=1`). They assert properties, name exams by position, and never print text.
- **The terms file stays outside the repository.** The verifier refuses one that git could commit.
- **No network.** The tools never use the network. Models for later stages, such as name recognition, are installed ahead of time and run locally.
- **Staging.** Stage explicit paths only, never `git add -A`, and check `git diff --cached --name-only` before every commit.

## Code conventions

- Python ≥ 3.12, with type hints. `ruff` is configured in `pyproject.toml`.
- The verifier itself depends only on the standard library. Add a dependency only when a stage needs it, and check its license; some PDF libraries are AGPL.
- Reports and errors carry counts, positions, and kinds only, never values.
- An automatic step never records an anonymization review. Only an explicit human approval writes `"ok"` with a reviewer and a date.

## Adversarial review

Each stage of `docs/PLAN.md` goes through the same review loop as `embeddings-fundamentals`:
1. Grok, then Codex, then Antigravity (`agy`), one at a time and never in parallel. Tell agy that the code is **Python** and that it must judge it by **Python best practices and idiomatic Python** (owner, 2026-10-29). Do not ask it for the `rust-dev` plugin, which `embeddings-fundamentals` uses.
2. Fix every valid finding with a regression test, or refute it with evidence.
3. Repeat until no reviewer has a valid finding.

Run reviewers from a clean worktree of the committed branch. Record the reports in `reports/reviews/stage-N/`.
