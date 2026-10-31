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
    # The originals only: the batch writes its copies under the exams directory.
    pdfs = sorted(p for p in EXAMS.iterdir() if p.is_file() and p.suffix.lower() == ".pdf")
    code = main(["verify", "--no-ocr", *map(str, pdfs)])
    out = capsys.readouterr().out
    assert code == EXIT_OK
    if not out.startswith(f"anonimizador verify: {len(pdfs)} file(s)"):
        pytest.fail(f"the report does not start with the count of {len(pdfs)} files", pytrace=False)
    check_report_privacy(out, name_words(), REPORT_LINE)


# The only lines `batch` without terms may print.
BATCH_LINE = re.compile(
    r"^(anonimizador batch: \d+ original\(s\), 0 term\(s\), OCR at \d+ dpi \(por\+eng\)|"
    r"anonimizador batch: warning: no terms file: .*|"
    r"anonimizador batch: \d+ of \d+ copy\(ies\) verified ok, \d+ with something left, "
    r"\d+ with an error|"
    r"anonimizador batch: index written: \d{8}-\d{6}-anonimizer\.md|"
    r"\[\d+\] (original|anonymized): (ok|FOUND|ERROR)|"
    r"\[\d+\] detections: (none|( ?[a-z]+=\d+)+) on \d+ page\(s\)|"
    r"    text layer not placed on \d+ page\(s\): OCR boxes only|"
    r"    not written: [a-z]+ (failed \(exit \d+\)|reported \d+ error\(s\) reading the file|"
    r"wrote output that could not be read)|"
    r"    term \d+:( [a-z]+=\d+)+|"
    r"    possible identifiers \(patterns\):( [a-z]+=\d+)+)$"
)


# The batch runs OCR three times per page: the check uses the smallest exams only.
BATCH_EXAMS = 2


def test_the_batch_console_on_real_exams_reveals_no_file_name(tmp_path, capsys):
    """The batch prints positions and counts only; the index, outside the repository,
    links every original."""
    pdfs = sorted(
        (p for p in EXAMS.iterdir() if p.suffix.lower() == ".pdf"), key=lambda p: p.stat().st_size
    )
    source = tmp_path / "originals"
    source.mkdir()
    for pdf in pdfs[:BATCH_EXAMS]:
        (source / pdf.name).symlink_to(pdf)
    out = tmp_path / "anonymized"
    code = main(["batch", "--out", str(out), str(source)])
    console = capsys.readouterr().out
    if code != EXIT_OK:
        pytest.fail(f"batch exited {code}", pytrace=False)
    check_report_privacy(console, name_words(), BATCH_LINE)
    [index] = out.glob("*-anonimizer.md")
    rows = [line for line in index.read_text(encoding="utf-8").splitlines() if line[:2] == "| "]
    if len(rows) != BATCH_EXAMS + 1:
        pytest.fail(f"the index has {len(rows) - 1} rows for {BATCH_EXAMS} exams", pytrace=False)
