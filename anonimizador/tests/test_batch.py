"""`anonimizador batch` on synthetic originals: the index, its links, and privacy."""

import re
from datetime import datetime
from pathlib import Path
from urllib.parse import unquote

import pytest
from conftest import requires_tools
from make_covered_name_pdf import build_pdf, covered_name_pdf

from anonimizador.batch import link, neutral_name, run_batch
from anonimizador.cli import EXIT_OK, EXIT_USAGE, main
from anonimizador.verify import Options

# Invented originals: names with spaces and Markdown characters, as owners name files.
NAMES = ("Relatorio Fulano Ficticio.pdf", "hemograma [2] | x.pdf")
SECRETS = ("Fulano", "Ficticio", "Relatorio", "hemograma")
NOW = datetime(2026, 10, 30, 14, 5, 9)
LINK = re.compile(r"\[(?:[^\]\\]|\\.)*\]\(([^)]+)\)")


@pytest.fixture
def source(outside_repo: Path) -> Path:
    exams = outside_repo / "exams"
    exams.mkdir()
    (exams / NAMES[0]).write_bytes(covered_name_pdf())
    (exams / NAMES[1]).write_bytes(
        build_pdf(b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n", b"")
    )
    (exams / "notes.txt").write_text("not an exam", encoding="utf-8")
    return exams


@pytest.fixture
def terms(outside_repo: Path) -> Path:
    path = outside_repo / "terms.txt"
    path.write_text("Fulano Ficticio\n", encoding="utf-8")
    return path


def links(report: Path) -> list[Path]:
    """The files the index links, resolved from its directory."""
    text = report.read_text(encoding="utf-8")
    return [(report.parent / unquote(href)).resolve() for href in LINK.findall(text)]


@requires_tools
def test_the_index_links_every_original_and_log_and_the_console_names_none(source, terms, capsys):
    out = source / "anonymized"
    before = {name: (source / name).read_bytes() for name in NAMES}
    code = main(["batch", "--no-ocr", "--terms", str(terms), "--out", str(out), str(source)])
    console = capsys.readouterr()
    assert code == EXIT_OK, console.out + console.err

    [report] = out.glob("*-anonimizer.md")
    assert re.fullmatch(r"\d{8}-\d{6}-anonimizer\.md", report.name)
    targets = links(report)
    for name in NAMES:
        assert (source / name).resolve() in targets
    logs = [t for t in targets if t.parent.parent.name == "logs"]
    assert sorted(t.name for t in logs) == sorted(
        [
            f"{n:02d}-{neutral_name(source / name)[:-4]}-verify-original.txt"
            for n, name in enumerate(sorted(NAMES), start=1)
        ]
        + ["console.txt"]
    )
    assert all(t.is_file() for t in targets)

    # The covered name is counted in the first original, by term number only.
    first = next(t for t in logs if t.name.startswith("01-"))
    assert first.read_text(encoding="utf-8").startswith("[1] FOUND\n    term 1:")
    # The console, and its saved copy, name originals by position, never by name.
    saved = next(t for t in logs if t.name == "console.txt").read_text(encoding="utf-8")
    for text in (console.out + console.err, saved):
        for secret in SECRETS:
            assert secret not in text, f"the console reveals {secret}"
    assert "[2] anonymized: pending (stage 2)" in saved
    # The originals are read, never written.
    assert {name: (source / name).read_bytes() for name in NAMES} == before


@requires_tools
def test_outputs_under_the_source_are_not_taken_for_originals(source):
    out = source / "anonymized"
    options = Options(use_ocr=False)
    assert run_batch(source, out, [], options, NOW) == (2, 0)
    (out / "exam-leftover.pdf").write_bytes(covered_name_pdf())
    assert run_batch(source, out, [], options, NOW.replace(second=10)) == (2, 0)
    assert len(list(out.glob("*-anonimizer.md"))) == 2


@requires_tools
def test_a_run_without_terms_warns_that_ok_proves_no_name_is_gone(source, capsys):
    out = source / "anonymized"
    run_batch(source, out, [], Options(use_ocr=False), NOW)
    [report] = out.glob("*-anonimizer.md")
    assert "no terms file: only identifier patterns" in report.read_text(encoding="utf-8")
    assert "warning: no terms file" in capsys.readouterr().out


@requires_tools
def test_a_second_run_in_the_same_second_leaves_the_first_index_alone(source):
    out = source / "anonymized"
    run_batch(source, out, [], Options(use_ocr=False), NOW)
    [report] = out.glob("*-anonimizer.md")
    first = report.read_bytes()
    with pytest.raises(FileExistsError):
        run_batch(source, out, [], Options(use_ocr=False), NOW)
    assert report.read_bytes() == first


def test_an_output_directory_git_could_commit_is_refused_before_anything_is_written(source, capsys):
    # Not created yet, inside this repository, and not ignored: the index would be
    # committable (the check runs from the nearest existing directory).
    out = Path(__file__).parent / "batch-output-that-must-not-exist" / "nested"
    code = main(["batch", "--no-ocr", "--out", str(out), str(source)])
    assert code == EXIT_USAGE
    assert "git cannot commit" in capsys.readouterr().err
    assert not out.parent.exists()


def test_a_source_without_originals_is_a_usage_error(outside_repo, capsys):
    code = main(["batch", "--no-ocr", "--out", str(outside_repo / "out"), str(outside_repo)])
    assert code == EXIT_USAGE
    assert "no PDF, PNG, or JPEG original" in capsys.readouterr().err
    assert not (outside_repo / "out").exists()


def test_neutral_names_come_from_the_bytes_not_the_name(outside_repo):
    a, b = outside_repo / "Fulano.PDF", outside_repo / "other.pdf"
    a.write_bytes(b"%PDF same")
    b.write_bytes(b"%PDF same")
    assert neutral_name(a) == neutral_name(b)
    assert re.fullmatch(r"exam-[0-9a-f]{12}\.pdf", neutral_name(a))


def test_a_link_escapes_markdown_and_encodes_its_target(tmp_path):
    target = tmp_path / "a b" / "x [1] | y.pdf"
    assert (
        link(target, tmp_path, target.name)
        == "[x \\[1\\] \\| y.pdf](a%20b/x%20%5B1%5D%20%7C%20y.pdf)"
    )
