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
from conftest import requires_tools

from anonimizador.cli import EXIT_OK, main
from anonimizador.terms import fold

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
    for path in EXAMS.iterdir():
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
    assert out.splitlines()[0].startswith(f"anonimizador verify: {len(pdfs)} file(s)")
    report = fold(out)
    leaked = [w for w in name_words() if re.search(rf"(?<![a-z]){re.escape(w)}(?![a-z])", report)]
    assert not leaked, f"{len(leaked)} file-name word(s) reached the report"
    allowed = re.compile(
        r"^(anonimizador verify: .*|\[\d+\] (ok|FOUND)|"
        r"    possible identifiers \(patterns\):( [a-z]+=\d+)+)$"
    )
    for i, line in enumerate(out.splitlines()):
        assert allowed.match(line), f"report line {i} has an unexpected shape"
