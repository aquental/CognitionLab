import os
import re
import shutil
import sys
from pathlib import Path

import pytest

from anonimizador.terms import fold

FIXTURES = Path(__file__).parent / "fixtures"
sys.path.insert(0, str(FIXTURES))

TOOLS = ("pdftotext", "pdfinfo", "pdftoppm", "pdfimages", "tesseract")

requires_tools = pytest.mark.skipif(
    any(shutil.which(t) is None for t in TOOLS),
    reason="needs poppler and tesseract (with Portuguese data) on PATH",
)


@pytest.fixture
def outside_repo(tmp_path: Path) -> Path:
    """A directory no git work tree contains (pytest's tmp_path is outside the repo)."""
    return tmp_path


def check_report_privacy(out: str, words: set[str], allowed: re.Pattern[str]) -> None:
    """Fails when a word of `words` reaches the report `out`, or a line does not match
    `allowed`. The failure names counts and line positions only: the words and the
    lines are never an assertion operand, which pytest would print (Grok G0-5)."""
    report = fold(out)
    leaked = sum(1 for w in words if re.search(rf"(?<![a-z]){re.escape(w)}(?![a-z])", report))
    if leaked:
        pytest.fail(f"{leaked} file-name word(s) reached the report", pytrace=False)
    bad = [i for i, line in enumerate(out.splitlines()) if not allowed.match(line)]
    if bad:
        pytest.fail(f"report line(s) {bad} have an unexpected shape", pytrace=False)


def stub(bin_dir: Path, tool: str, script: str) -> None:
    """A fake `tool` on PATH, as a crashed, hung, or misconfigured install would be."""
    path = bin_dir / tool
    path.write_text("#!/bin/sh\n" + script, encoding="utf-8")
    path.chmod(0o755)


@pytest.fixture
def stubs(tmp_path: Path, monkeypatch) -> Path:
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    monkeypatch.setenv("PATH", f"{bin_dir}{os.pathsep}{os.environ['PATH']}")
    return bin_dir
