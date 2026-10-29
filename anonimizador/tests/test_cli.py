"""End-to-end `anonimizador verify` on synthetic PDFs, with the real tools."""

import os
from pathlib import Path

import pytest
from conftest import FIXTURES, requires_tools
from make_covered_name_pdf import build_pdf, covered_name_pdf

from anonimizador.cli import EXIT_FOUND, EXIT_OK, EXIT_USAGE, main

COVERED = FIXTURES / "covered-name.pdf"
SECRETS = ("Fulano", "Ficticio", "123.456", "789", "covered-name")


@pytest.fixture
def terms(outside_repo: Path) -> Path:
    path = outside_repo / "terms.txt"
    path.write_text("# invented identifiers\nFulano Ficticio\n123.456.789-09\n", encoding="utf-8")
    return path


def test_the_committed_fixture_is_what_the_generator_writes():
    assert COVERED.read_bytes() == covered_name_pdf()


@requires_tools
def test_a_covered_name_is_counted_and_flagged_without_being_printed(terms, capsys):
    code = main(["verify", "--terms", str(terms), str(COVERED)])
    out = capsys.readouterr()
    text = out.out + out.err
    assert code == EXIT_FOUND, text
    # The name is in the text layer, the metadata, and the raw bytes, not on the page.
    assert "term 1: metadata=1 text=1 raw=2\n" in text
    assert "hint: term 1 is in the file but not visible" in text
    # The CPF is visible: OCR finds it, so no hint.
    assert "term 2: text=1 ocr=1 raw=1\n" in text
    assert "hint: term 2" not in text
    assert "possible identifiers (patterns): cpf=1" in text
    for secret in SECRETS:
        assert secret not in text, f"the output reveals {secret}"


@requires_tools
def test_a_really_redacted_pdf_passes(terms, outside_repo, capsys):
    clean = outside_repo / "clean.pdf"
    clean.write_bytes(
        build_pdf(
            b"BT /F1 12 Tf 72 730 Td (Paciente: [REMOVIDO]) Tj ET\n"
            b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n",
            b"/Title (Laudo sintetico)",
        )
    )
    code = main(["verify", "--terms", str(terms), str(clean)])
    out = capsys.readouterr().out
    assert code == EXIT_OK, out
    assert out.splitlines()[1] == "[1] ok"


@requires_tools
def test_file_names_appear_only_on_request(terms, capsys):
    main(["verify", "--no-ocr", "--terms", str(terms), str(COVERED)])
    assert "covered-name" not in capsys.readouterr().out
    main(["verify", "--no-ocr", "--show-names", "--terms", str(terms), str(COVERED)])
    assert "[1] covered-name.pdf FOUND" in capsys.readouterr().out


@requires_tools
def test_strict_patterns_fail_on_a_pattern_alone(outside_repo, capsys):
    pdf = outside_repo / "cpf.pdf"
    pdf.write_bytes(build_pdf(b"BT /F1 12 Tf 72 700 Td (CPF: 987.654.321-00) Tj ET\n", b""))
    assert main(["verify", "--no-ocr", str(pdf)]) == EXIT_OK
    assert main(["verify", "--no-ocr", "--strict-patterns", str(pdf)]) == EXIT_FOUND
    assert "987" not in capsys.readouterr().out


def test_usage_errors_exit_2(outside_repo, capsys):
    assert main(["verify", "--no-ocr", str(outside_repo / "missing.pdf")]) == EXIT_USAGE
    assert "argument 1" in capsys.readouterr().err
    (outside_repo / "notes.txt").write_text("x", encoding="utf-8")
    assert main(["verify", "--no-ocr", str(outside_repo)]) == EXIT_USAGE


def stub(bin_dir: Path, tool: str, script: str) -> None:
    """A fake `tool` on PATH, as a crashed, hung, or misconfigured install would be."""
    path = bin_dir / tool
    path.write_text("#!/bin/sh\n" + script, encoding="utf-8")
    path.chmod(0o755)


LIST_LANGS = (
    'if [ "$1" = "--list-langs" ]; then '
    "printf 'List of available languages (2):\\neng\\npor\\n'; exit 0; fi\n"
)


@pytest.fixture
def stubs(tmp_path: Path, monkeypatch) -> Path:
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    monkeypatch.setenv("PATH", f"{bin_dir}{os.pathsep}{os.environ['PATH']}")
    return bin_dir


def assert_unreadable(code: int, text: str, reason: str) -> None:
    assert code == EXIT_USAGE, text
    assert "[1] ERROR" in text and "[1] ok" not in text, text
    assert f"    unreadable: {reason}" in text, text
    assert "Traceback" not in text
    for secret in SECRETS:
        assert secret not in text, f"the output reveals {secret}"


@requires_tools
def test_a_failed_ocr_is_an_error_not_a_clean_file(terms, stubs, capsys):
    # Regression (Grok G4-5): a tesseract that exits non-zero used to leave the OCR
    # channel empty and the file "ok".
    stub(stubs, "tesseract", LIST_LANGS + "exit 1\n")
    code = main(["verify", "--terms", str(terms), str(COVERED)])
    out = capsys.readouterr()
    assert_unreadable(code, out.out + out.err, "ocr: tesseract failed (exit 1)")


@requires_tools
def test_a_hung_tool_times_out_without_printing_the_path(terms, stubs, capsys):
    stub(stubs, "tesseract", LIST_LANGS + "sleep 5\n")
    code = main(["verify", "--timeout", "1", "--terms", str(terms), str(COVERED)])
    out = capsys.readouterr()
    assert_unreadable(code, out.out + out.err, "ocr: tesseract timed out after 1 s")


@requires_tools
def test_missing_page_renders_are_an_error(terms, stubs, capsys):
    stub(stubs, "pdftoppm", "exit 0\n")
    code = main(["verify", "--terms", str(terms), str(COVERED)])
    out = capsys.readouterr()
    assert_unreadable(code, out.out + out.err, "ocr: pdftoppm rendered 0 of 1 pages")


@requires_tools
def test_an_unreadable_pdf_reports_every_channel_it_could_not_read(terms, stubs, capsys):
    # An encrypted PDF makes pdfinfo and pdftotext fail.
    stub(stubs, "pdfinfo", "exit 1\n")
    stub(stubs, "pdftotext", "exit 3\n")
    code = main(["verify", "--no-ocr", "--terms", str(terms), str(COVERED)])
    text = capsys.readouterr().out
    assert_unreadable(code, text, "metadata: pdfinfo failed (exit 1)")
    assert "    unreadable: text: pdftotext failed (exit 3)" in text


@requires_tools
def test_missing_ocr_language_data_is_refused_before_any_file(terms, stubs, capsys):
    stub(stubs, "tesseract", 'if [ "$1" = "--list-langs" ]; then echo eng; exit 0; fi\nexit 1\n')
    assert main(["verify", "--terms", str(terms), str(COVERED)]) == EXIT_USAGE
    out = capsys.readouterr()
    assert "tesseract has no data for: por" in out.err
    assert out.out == ""


@requires_tools
def test_a_name_in_a_directory_is_searched_and_never_printed(outside_repo, capsys):
    # Regression (Grok G4-6): only the file name used to be searched.
    root = outside_repo / "corpus"
    folder = root / "BELTRANO DA SILVA"
    folder.mkdir(parents=True)
    (folder / "lab-001.pdf").write_bytes(
        build_pdf(b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n", b"")
    )
    terms = outside_repo / "names.txt"
    terms.write_text("Beltrano da Silva\n", encoding="utf-8")
    code = main(["verify", "--no-ocr", "--terms", str(terms), str(root)])
    out = capsys.readouterr().out
    assert code == EXIT_FOUND, out
    assert "    term 1: name=1\n" in out
    assert "BELTRANO" not in out and "Beltrano" not in out


def test_a_terms_file_that_is_not_utf8_exits_2_without_a_traceback(outside_repo, capsys):
    # Regression (Grok G0-4): exit 1 meant "found", and the traceback showed the bytes.
    terms = outside_repo / "terms.txt"
    terms.write_bytes("Fulano Conceição\n".encode("latin-1"))
    pdf = outside_repo / "a.pdf"
    pdf.write_bytes(b"%PDF-1.4\n")
    assert main(["verify", "--no-ocr", "--terms", str(terms), str(pdf)]) == EXIT_USAGE
    out = capsys.readouterr()
    assert "the terms file is not UTF-8 text" in out.err
    assert "Traceback" not in out.err and out.out == ""


@pytest.mark.skipif(os.geteuid() == 0, reason="root reads any directory")
def test_an_unlisted_directory_fails_the_run(terms, outside_repo, capsys):
    # Regression (Grok G0-3), end to end: a clean file beside an unreadable directory.
    root = outside_repo / "corpus"
    locked = root / "locked"
    locked.mkdir(parents=True)
    (root / "clean.pdf").write_bytes(build_pdf(b"BT /F1 12 Tf 72 700 Td (ok) Tj ET\n", b""))
    locked.chmod(0)
    try:
        code = main(["verify", "--no-ocr", "--terms", str(terms), str(root)])
    finally:
        locked.chmod(0o755)
    out = capsys.readouterr()
    assert code == EXIT_USAGE
    assert "cannot read everything under argument 1" in out.err
    assert "ok" not in out.out and "locked" not in out.out + out.err


@pytest.mark.parametrize("option", ["--timeout", "--dpi"])
@pytest.mark.parametrize("value", ["-1", "0", "ten", "2147483648"])
def test_a_nonpositive_number_is_a_usage_error(option, value, outside_repo, capsys):
    # Regression (Grok G0-9, G0-31): a negative timeout raised ValueError in subprocess,
    # and a huge one OverflowError: a traceback with exit 1 ("found").
    pdf = outside_repo / "a.pdf"
    pdf.write_bytes(b"%PDF-1.4\n")
    with pytest.raises(SystemExit) as exit_:
        main(["verify", "--no-ocr", option, value, str(pdf)])
    assert exit_.value.code == EXIT_USAGE
    err = capsys.readouterr().err
    assert f"argument {option}: must be a whole number from 1 to" in err
    assert "Traceback" not in err


@requires_tools
def test_a_leftover_hex_string_is_found_in_raw_bytes(outside_repo, capsys):
    # Regression (Grok G0-14), end to end: an object no trailer references, as an
    # incremental update leaves it, holding a UTF-16 hex author.
    old = b"9 0 obj << /Author <FEFF005A0075006C0065006F> >> endobj\n"
    pdf = outside_repo / "a.pdf"
    pdf.write_bytes(build_pdf(b"BT /F1 12 Tf 72 700 Td (Laudo) Tj ET\n", b"") + old)
    terms = outside_repo / "terms.txt"
    terms.write_text("Zuleo\n", encoding="utf-8")
    code = main(["verify", "--no-ocr", "--terms", str(terms), str(pdf)])
    out = capsys.readouterr().out
    assert code == EXIT_FOUND, out
    assert "    term 1: raw=1\n" in out
    assert "Zuleo" not in out and "FEFF" not in out


@requires_tools
def test_strict_patterns_fail_on_a_pattern_in_the_name(outside_repo, capsys):
    # Regression (Grok G0-16), end to end.
    pdf = outside_repo / "123.456.789-09.pdf"
    pdf.write_bytes(build_pdf(b"BT /F1 12 Tf 72 700 Td (Laudo) Tj ET\n", b""))
    assert main(["verify", "--no-ocr", "--strict-patterns", str(pdf)]) == EXIT_FOUND
    out = capsys.readouterr().out
    assert "possible identifiers (patterns): cpf=1" in out
    assert "123" not in out.replace("[1]", "")


@requires_tools
def test_a_leftover_kerned_word_and_cpf_are_found(outside_repo, capsys):
    # Regression (Grok G0-19, G0-21), end to end: an unreferenced old content stream
    # draws a short name glyph by glyph, and an old object holds an unlisted CPF.
    old = (
        b"9 0 obj << >> stream\nBT [(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ ET\nendstream endobj\n"
        b"10 0 obj << /Subject (123.456.789-09) >> endobj\n"
    )
    pdf = outside_repo / "a.pdf"
    pdf.write_bytes(build_pdf(b"BT /F1 12 Tf 72 700 Td (Assunto) Tj ET\n", b"") + old)
    terms = outside_repo / "terms.txt"
    terms.write_text("Zuleo\n", encoding="utf-8")
    code = main(["verify", "--no-ocr", "--strict-patterns", "--terms", str(terms), str(pdf)])
    out = capsys.readouterr().out
    assert code == EXIT_FOUND, out
    assert "    term 1: raw=1\n" in out
    assert "possible identifiers (patterns): cpf=1" in out
    assert "Zuleo" not in out and "123" not in out.replace("[1]", "")
