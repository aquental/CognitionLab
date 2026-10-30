"""Local, opt-in checks over the owner's REAL sample exams (never in CI).

The exams live in `../embeddings-fundamentals/tests/sample-exams/`, which git ignores.
These tests assert properties only and never print a file name or any document text:
messages name an exam by its position.

    RUN_SAMPLE_EXAMS=1 uv run pytest tests/test_sample_exams.py
"""

import os
import re
from pathlib import Path

import pytest
from conftest import check_report_privacy, requires_tools

from anonimizador.cli import EXIT_OK, main
from anonimizador.terms import fold

# The only lines a report without --show-names may hold.
REPORT_LINE = re.compile(
    r"^(anonimizador verify: .*|\[\d+\] (ok|FOUND)|"
    r"    possible identifiers \(patterns\):( [a-z]+=\d+)+)$"
)
EXAMS = Path(__file__).resolve().parents[2] / "embeddings-fundamentals" / "tests" / "sample-exams"

pytestmark = [
    pytest.mark.skipif(
        os.environ.get("RUN_SAMPLE_EXAMS") != "1",
        reason="set RUN_SAMPLE_EXAMS=1 to run the real sample-exam checks (local only)",
    ),
    requires_tools,
]


def name_words() -> set[str]:
    words = set()
    # Files only: the batch's output directory under the exams is not an exam.
    for path in (p for p in EXAMS.iterdir() if p.is_file()):
        words.update(w for w in re.findall(r"[^\W\d_]+", fold(path.stem)) if len(w) >= 4)
    return words


def test_the_report_on_real_exams_reveals_no_file_name(capsys):
    """Without --show-names, nothing of a file name reaches the output: only positions,
    channel names, counts, and pattern kinds."""
    assert EXAMS.is_dir(), "the sample exams are missing (owner-supplied, local only)"
    code = main(["verify", "--no-ocr", str(EXAMS)])
    out = capsys.readouterr().out
    assert code == EXIT_OK
    pdfs = [p for p in EXAMS.iterdir() if p.suffix.lower() == ".pdf"]
    if not out.startswith(f"anonimizador verify: {len(pdfs)} file(s)"):
        pytest.fail(f"the report does not start with the count of {len(pdfs)} files", pytrace=False)
    check_report_privacy(out, name_words(), REPORT_LINE)


# The only lines `batch --no-ocr` without terms may print.
BATCH_LINE = re.compile(
    r"^(anonimizador batch: \d+ original\(s\), 0 term\(s\), no OCR|"
    r"anonimizador batch: warning: no terms file: .*|"
    r"anonimizador batch: index written to .*-anonimizer\.md|"
    r"\[\d+\] original: (ok|FOUND)|\[\d+\] anonymized: pending \(stage 2\)|"
    r"    possible identifiers \(patterns\):( [a-z]+=\d+)+)$"
)


def test_the_batch_console_on_real_exams_reveals_no_file_name(tmp_path, capsys):
    """The batch prints positions and counts only; the index, outside the repository,
    links every original."""
    out = tmp_path / "anonymized"
    code = main(["batch", "--no-ocr", "--out", str(out), str(EXAMS)])
    console = capsys.readouterr().out
    assert code == EXIT_OK
    # The index path is the test's own temporary directory: drop it before the check.
    check_report_privacy(console.replace(str(out), "<out>"), name_words(), BATCH_LINE)
    [index] = out.glob("*-anonimizer.md")
    pdfs = [p for p in EXAMS.iterdir() if p.suffix.lower() == ".pdf"]
    rows = [line for line in index.read_text(encoding="utf-8").splitlines() if line[:2] == "| "]
    if len(rows) != len(pdfs) + 1:
        pytest.fail(f"the index has {len(rows) - 1} rows for {len(pdfs)} exams", pytrace=False)
