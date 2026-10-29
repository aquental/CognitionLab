"""End-to-end `anonimizador verify` on synthetic PDFs, with the real tools."""

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
    assert f"[1] {COVERED} FOUND" in capsys.readouterr().out


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
