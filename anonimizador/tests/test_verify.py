"""`verify` internals without the external tools: directory traversal and counting."""

import os
import re
import zlib
from pathlib import Path

import pytest
from conftest import check_report_privacy
from make_covered_name_pdf import build_pdf

from anonimizador import verify
from anonimizador.channels import Reading, raw_readings
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
    assert str(error.value).startswith("cannot read everything under argument 1 (")
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


def test_a_link_that_leads_nowhere_is_refused_without_its_name(tmp_path: Path):
    # Regression (Grok G0-11): os.walk listed a dangling directory link as a file, and
    # without a corpus suffix it was dropped, so the clean file beside it was "ok".
    root = tmp_path / "corpus"
    pdf(root / "clean.pdf")
    (root / "Zuleo").symlink_to(tmp_path / "unmounted", target_is_directory=True)
    with pytest.raises(VerifyError) as error:
        collect([str(root)])
    message = str(error.value)
    assert message == "cannot read everything under argument 1 (a symbolic link leads nowhere)"
    loop = tmp_path / "looped"
    pdf(loop / "clean.pdf")
    (loop / "self").symlink_to(loop / "self")
    with pytest.raises(VerifyError, match="leads nowhere"):
        collect([str(loop)])


def test_every_argument_that_reaches_a_file_names_it(tmp_path: Path):
    # Regression (Grok G0-13): the first argument's name was kept, so checking a
    # subfolder before its parent hid the subfolder's name.
    root = tmp_path / "corpus"
    clean = pdf(root / "Zuleo" / "clean.pdf")
    for order in ([root / "Zuleo", root], [root, root / "Zuleo"], [clean, root]):
        files = collect([str(p) for p in order])
        assert len(files) == 1
        assert "Zuleo/clean.pdf" in (files[0].name, *files[0].aliases), order


def test_a_short_name_left_only_in_raw_bytes_is_found(monkeypatch):
    # Regression (Grok G0-10), through the raw readings of real byte layouts.
    stream = zlib.compress("Zuleo".encode("utf-16-le"))
    for data in (
        "Zuleo".encode("utf-16-le"),
        b"%PDF-1.4\n<< /Author (Zuleo) >>\n",
        b"%PDF-1.4\n1 0 obj << >> stream\n" + stream + b"endstream\n",
    ):
        reading = Reading()
        reading.texts["raw"], reading.strings, reading.raw_joins = raw_readings(data)
        monkeypatch.setattr(verify, "read", lambda *_, r=reading: r)
        file = CorpusFile("x.pdf", "x.pdf")
        for term, expected in (("Zuleo", [1]), ("Zulea", [])):
            result = check_file(1, file, [verify.Term(term)], Options(use_ocr=False), None)
            assert [hit.counts["raw"] for hit in result.hits] == expected, (data[:12], term)


def test_hidden_entries_are_searched_and_appledouble_files_are_not(tmp_path: Path):
    # Regression (Grok G0-15): hidden directories and files were skipped, so a clean
    # file beside them was "ok".
    root = tmp_path / "corpus"
    pdf(root / "clean.pdf")
    pdf(root / ".Zuleo" / "lab.pdf")
    pdf(root / ".hidden.pdf")
    (root / "._clean.pdf").write_bytes(b"\x00\x05\x16\x07")
    names = [f.name for f in collect([str(root)])]
    assert names == [".Zuleo/lab.pdf", ".hidden.pdf", "clean.pdf"]
    (root / ".gone").symlink_to(tmp_path / "missing")
    with pytest.raises(VerifyError, match="leads nowhere"):
        collect([str(root)])


def hints_of(monkeypatch, name: str, texts: dict[str, list[str]]) -> dict[str, int]:
    reading = Reading()
    reading.texts.update(texts)
    monkeypatch.setattr(verify, "read", lambda *_: reading)
    return check_file(1, CorpusFile("x.pdf", name), [], Options(), None).hints


def test_patterns_are_searched_in_names_and_across_pages(monkeypatch):
    # Regression (Grok G0-16): a CPF in the file name alone gave no hint.
    assert hints_of(monkeypatch, "123.456.789-09.pdf", {}) == {"cpf": 1}
    assert hints_of(monkeypatch, "123.456.789-0.pdf", {}) == {}
    # Regression (Grok G0-18): pages were searched one at a time, so a number split by
    # a page break was no hint, and one per page counted once.
    assert hints_of(monkeypatch, "x.pdf", {"ocr": ["CPF 123.456.789-", "09"]}) == {"cpf": 1}
    two = {"ocr": ["CPF 123.456.789-09", "CPF 987.654.321-00"]}
    assert hints_of(monkeypatch, "x.pdf", two) == {"cpf": 2}


def strict_result(monkeypatch, data: bytes):
    reading = Reading()
    reading.texts["raw"], reading.strings, reading.raw_joins = raw_readings(data)
    monkeypatch.setattr(verify, "read", lambda *_: reading)
    options = Options(use_ocr=False, strict_patterns=True)
    return check_file(1, CorpusFile("x.pdf", "x.pdf"), [], options, None), options


def test_patterns_left_in_pdf_strings_are_hints(monkeypatch):
    # Regression (Grok G0-21): decoded PDF strings reached no pattern channel, so a CPF
    # in an old object gave no hint, even with --strict-patterns.
    cpf = "123.456.789-09"
    hexed = "FEFF" + cpf.encode("utf-16-be").hex().upper()
    for data in (
        b"9 0 obj << /Subject (" + cpf.encode() + b") >> endobj\n",
        b"9 0 obj << /Subject <" + hexed.encode() + b"> >> endobj\n",
        # Drawn glyph group by glyph group (Grok G0-23).
        b"BT [(123)(.)(456)(.)(789)(-)(09)] TJ ET\n",
    ):
        result, options = strict_result(monkeypatch, data)
        assert result.hints == {"cpf": 1}, data[:24]
        assert result.status(options.strict_patterns) == "FOUND"


def test_a_clean_page_gains_no_pattern_from_its_strings(monkeypatch):
    clean = build_pdf(
        b"BT /F1 12 Tf 72 730 Td (Paciente: [REMOVIDO]) Tj ET\n"
        b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n",
        b"/Title (Assunto sintetico)",
    )
    result, options = strict_result(monkeypatch, clean)
    assert result.hints == {}
    assert result.status(options.strict_patterns) == "ok"


def test_check_file_searches_the_joins_of_ambiguous_moves(monkeypatch):
    # Regression (Grok G0-29), through check_file: only a join of the first two pieces
    # makes the short word whole, and that join counts once.
    stream = b"BT 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 86 700 Tm (leo) Tj "
    stream += b"1 0 0 1 200 700 Tm (Assunto) Tj ET"
    reading = Reading()
    reading.texts["raw"], reading.strings, reading.raw_joins = raw_readings(stream)
    monkeypatch.setattr(verify, "read", lambda *_: reading)
    terms = [verify.Term("Zuleo"), verify.Term("Zulea"), verify.Term("Assunto")]
    result = check_file(1, CorpusFile("x.pdf", "x.pdf"), terms, Options(use_ocr=False), None)
    assert {hit.number: hit.counts["raw"] for hit in result.hits} == {1: 1, 3: 1}
