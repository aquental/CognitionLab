"""`verify` internals without the external tools: directory traversal and counting."""

import os
import re
from pathlib import Path

import pytest
from conftest import check_report_privacy

from anonimizador import verify
from anonimizador.channels import Reading
from anonimizador.verify import CorpusFile, Options, VerifyError, check_file, collect

TERM = "Fulano Ficticio"


def pdf(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"%PDF-1.4\n")
    return path


@pytest.mark.skipif(os.geteuid() == 0, reason="root reads any directory")
def test_a_directory_that_cannot_be_listed_is_refused_without_its_name(tmp_path: Path):
    # Regression (Grok G0-3): os.walk skipped it, so the clean files beside it made
    # the run "ok".
    root = tmp_path / "corpus"
    pdf(root / "clean.pdf")
    locked = root / "locked"
    pdf(locked / "lab.pdf")
    locked.chmod(0)
    try:
        with pytest.raises(VerifyError) as error:
            collect([str(root)])
    finally:
        locked.chmod(0o755)
    assert str(error.value).startswith("cannot list a directory under argument 1 (")
    assert "locked" not in str(error.value) and str(tmp_path) not in str(error.value)


def test_symlinked_directories_are_followed_once(tmp_path: Path):
    # Regression (Grok G0-3): os.walk did not follow a symlinked directory.
    outside = tmp_path / "elsewhere"
    pdf(outside / "lab.pdf")
    root = tmp_path / "corpus"
    pdf(root / "clean.pdf")
    (root / "linked").symlink_to(outside, target_is_directory=True)
    (root / "loop").symlink_to(root, target_is_directory=True)
    files = collect([str(root)])
    assert [f.name for f in files] == ["clean.pdf", "linked/lab.pdf"]
    # The loop's name is searched too, as an alias of every file under it.
    assert [f.aliases for f in files] == [("loop/clean.pdf",), ("loop/linked/lab.pdf",)]


def test_a_second_link_to_a_directory_keeps_its_name(tmp_path: Path, monkeypatch):
    # Regression (Grok G0-7): a directory reached again through a symlink was skipped,
    # so the link's name was never searched: a neutral name that sorted first hid it.
    root = tmp_path / "corpus"
    pdf(root / "laudos" / "clean.pdf")
    (root / "sicrano").symlink_to(root / "laudos", target_is_directory=True)
    files = collect([str(root)])
    assert [(f.name, f.aliases) for f in files] == [("laudos/clean.pdf", ("sicrano/clean.pdf",))]
    reading = Reading()
    monkeypatch.setattr(verify, "read", lambda *_: reading)
    result = check_file(1, files[0], [verify.Term("Sicrano")], Options(), None)
    assert [hit.counts["name"] for hit in result.hits] == [1]
    assert result.path == "laudos/clean.pdf"


def counts_of(monkeypatch, texts: dict[str, list[str]]) -> dict[str, int]:
    reading = Reading()
    reading.texts.update(texts)
    monkeypatch.setattr(verify, "read", lambda *_: reading)
    result = check_file(1, CorpusFile("x.pdf", "x.pdf"), [verify.Term(TERM)], Options(), None)
    return result.hits[0].counts if result.hits else {}


def test_every_page_and_image_counts_and_a_split_term_is_found(monkeypatch):
    # Regression (Grok G0-6): the largest count of any one page was reported, and a
    # term divided by a page break was missed.
    assert counts_of(monkeypatch, {"ocr": [TERM, TERM], "images": [TERM, "x", TERM]}) == {
        "name": 0,
        "metadata": 0,
        "text": 0,
        "ocr": 2,
        "images": 2,
        "raw": 0,
    }
    assert counts_of(monkeypatch, {"ocr": ["... Fulano", "Ficticio ..."]})["ocr"] == 1
    # Alternative readings of the same bytes still count an occurrence once.
    assert counts_of(monkeypatch, {"raw": [TERM, TERM, TERM]})["raw"] == 1


REPORT = re.compile(r"^\[\d+\] ok$")


def test_a_failed_privacy_check_names_counts_and_positions_only():
    # Regression (Grok G0-5): pytest printed the leaked words and the line, the
    # operands of the failed asserts.
    with pytest.raises(pytest.fail.Exception) as error:
        check_report_privacy("[1] ok\n[2] beltrano\n", {"beltrano"}, REPORT)
    assert str(error.value) == "1 file-name word(s) reached the report"
    with pytest.raises(pytest.fail.Exception) as error:
        check_report_privacy("[1] ok\nsicrano\n", {"beltrano"}, REPORT)
    assert str(error.value) == "report line(s) [1] have an unexpected shape"
    check_report_privacy("[1] ok\n", {"beltrano"}, REPORT)
