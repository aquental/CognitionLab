"""`anonimizador batch` on synthetic originals: the redacted copies, the index, its
links, and privacy (R5, R7, R8, R11)."""

import contextlib
import io
import json
import re
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
from pathlib import Path
from urllib.parse import unquote

import pytest
from conftest import requires_tools, stub
from make_covered_name_pdf import build_pdf, covered_name_pdf
from PIL import Image, ImageDraw, ImageFont, PngImagePlugin

import anonimizador.batch
from anonimizador.batch import link, neutral_name, run_batch
from anonimizador.channels import read_image, read_pdf, run
from anonimizador.cli import EXIT_FOUND, EXIT_OK, EXIT_USAGE, main
from anonimizador.terms import count_patterns
from anonimizador.verify import FileResult, Options, VerifyError

# Invented originals: names with spaces and Markdown characters, as owners name files.
NAMES = ("Relatorio Fulano Ficticio.pdf", "hemograma [2] | x.pdf")
SECRETS = ("Fulano", "Ficticio", "Relatorio", "hemograma")
NOW = datetime(2026, 10, 30, 14, 5, 9)
LINK = re.compile(r"\[(?:[^\]\\]|\\.)*\]\(([^)]+)\)")
# Invented identifiers planted in every place an original can hold them: the text layer,
# text under a drawn box, metadata, a scanned page, an image's pixels and its chunks.
TERMS = "Fulano Ficticio\nSicrano Inventado\nBeltrano Inventado\n123.456.789-09\n"
PLANTED = ("Fulano", "Ficticio", "Sicrano", "Beltrano", "Inventado", "98765", "exemplo")
LABELLED = (
    b"BT /F1 14 Tf 72 730 Td (Paciente: Beltrano Inventado) Tj ET\n"
    b"BT /F1 14 Tf 72 700 Td (CPF 123.456.789-09   Dt. Nasc.: 01/02/1980) Tj ET\n"
    b"BT /F1 14 Tf 72 670 Td (Telefone: \\(11\\) 98765-4321   b@exemplo.com) Tj ET\n"
    b"BT /F1 14 Tf 72 640 Td (Hemoglobina 13,5 g/dL   CRM/SP 123456) Tj ET\n"
)
SCANNED_LINES = (
    "Paciente: Sicrano Inventado",
    "CNS 898001234567891",
    "Hemoglobina 13,5 g/dL",
)


def drawn(lines: tuple[str, ...]) -> Image.Image:
    """A white page with `lines` drawn on it, as a scanner would give it."""
    page = Image.new("L", (1240, 600), 255)
    draw = ImageDraw.Draw(page)
    font = ImageFont.load_default(size=44)
    for n, text in enumerate(lines):
        draw.text((80, 80 + 90 * n), text, fill=0, font=font)
    return page


def write_originals(exams: Path) -> None:
    exams.mkdir()
    (exams / "Relatorio Fulano Ficticio.pdf").write_bytes(covered_name_pdf())
    (exams / "ficha [2] | x.pdf").write_bytes(build_pdf(LABELLED, b"/Author (Beltrano Inventado)"))
    drawn(SCANNED_LINES).save(exams / "scan Sicrano.pdf", "PDF", resolution=150.0)
    chunks = PngImagePlugin.PngInfo()
    chunks.add_text("Author", "Sicrano Inventado")
    exif = Image.Exif()
    exif[0x013B] = "Sicrano Inventado"  # Artist
    drawn(("Sicrano Inventado", "Raio X torax")).save(
        exams / "imagem.png", "PNG", pnginfo=chunks, exif=exif
    )
    (exams / "notes.txt").write_text("not an exam", encoding="utf-8")


@pytest.fixture(scope="module")
def redacted(tmp_path_factory) -> tuple[Path, int, str]:
    """One batch over every synthetic original, with OCR: the outputs, the exit status,
    and the console."""
    root = tmp_path_factory.mktemp("redacted")
    write_originals(root / "exams")
    (root / "terms.txt").write_text(TERMS, encoding="utf-8")
    out = root / "exams" / "anonymized"
    console = io.StringIO()
    with contextlib.redirect_stdout(console), contextlib.redirect_stderr(console):
        code = main(
            ["batch", "--terms", str(root / "terms.txt"), "--out", str(out), str(root / "exams")]
        )
    return out, code, console.getvalue()


def shown_patterns(copy: Path) -> dict[str, int]:
    """Identifier patterns in what OCR reads on the pages of `copy`."""
    with tempfile.TemporaryDirectory() as tmp, ThreadPoolExecutor(2) as pool:
        if copy.suffix == ".pdf":
            reading = read_pdf(str(copy), copy.name, tmp, "por", 300, True, 120, pool)
        else:
            reading = read_image(str(copy), copy.name, "por", True, 120)
    return count_patterns(["\n".join(reading.texts["ocr"]), *reading.texts["images"]])


def copies(out: Path) -> list[Path]:
    """The copies of every run under `out`: each run writes into its own directory."""
    return sorted(out.glob("*/exam-*"))


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
def test_every_copy_verifies_clean_and_keeps_what_is_not_an_identifier(redacted):
    out, code, console = redacted
    assert code == EXIT_OK, console
    assert "4 of 4 copy(ies) verified ok" in console
    assert [p.suffix for p in copies(out)].count(".png") == 1
    for log in out.glob("*/logs/*-verify-anonymized.txt"):
        text = log.read_text(encoding="utf-8")
        assert text.splitlines()[0].endswith(" ok"), text
        assert "term " not in text, text
    # What the copies show holds no identifier pattern: the CRM label stays (its number
    # goes). Pattern hints that `verify` takes from a copy's pixel bytes are noise: gray
    # levels such as 0x40 and 0x2E read as "@" and ".".
    for copy in copies(out):
        assert set(shown_patterns(copy)) <= {"crm"}, copy.name
    # The lab value survives redaction: the boxes cover identifiers, not the page.
    scans = [p for p in copies(out) if p.suffix == ".pdf"]
    pages = [
        run(["pdftoppm", "-r", "150", "-png", str(p), str(out / f"check-{p.stem}")], 60)
        for p in scans
    ]
    assert pages
    text = "".join(
        run(["tesseract", str(png), "-", "-l", "por"], 60)
        for png in sorted(out.glob("check-*.png"))
    )
    assert text.count("Hemoglobina") >= 2


@requires_tools
def test_a_copy_keeps_only_pixels(redacted):
    out, _, _ = redacted
    for copy in copies(out):
        if copy.suffix == ".pdf":
            assert run(["pdftotext", str(copy), "-"], 60).strip() == ""
            info = run(["pdfinfo", str(copy)], 60)
            assert not re.search(r"^(Title|Author|Subject|Keywords|Creator|Producer):", info, re.M)
            assert "Metadata" not in run(["pdfinfo", "-meta", str(copy)], 60)
            raw = copy.read_bytes()
            assert b"/Font" not in raw and b"/Metadata" not in raw and b"/Info" not in raw
            # Lossless pages: JPEG artifacts change what OCR reads.
            assert b"/FlateDecode" in raw and b"/DCTDecode" not in raw
        else:
            with Image.open(copy) as im:
                assert set(im.info) <= {"dpi"} and not im.getexif()
        assert not any(secret.encode() in copy.read_bytes() for secret in PLANTED)


@requires_tools
def test_the_index_links_every_file_and_nothing_names_an_original_but_the_index(redacted):
    out, _, console = redacted
    [report] = out.glob("*-anonimizer.md")
    assert re.fullmatch(r"\d{8}-\d{6}-anonimizer\.md", report.name)
    targets = links(report)
    source = out.parent
    originals = sorted(p for p in source.iterdir() if p.is_file() and p.suffix != ".txt")
    assert all(p.resolve() in targets for p in originals)
    assert all(t.is_file() for t in targets)
    kinds = sorted(
        re.sub(r"^\d\d-exam-[0-9a-f]{12}", "", t.name) for t in targets if t.parent.name == "logs"
    )
    assert kinds == sorted(
        ["console.txt"] + ["-detections.json", "-verify-anonymized.txt", "-verify-original.txt"] * 4
    )
    assert sum(t.parent.parent == out and t.name.startswith("exam-") for t in targets) == 4
    # Logs, detections, and the console carry positions, kinds, and counts only.
    saved = [t.read_text(encoding="utf-8") for t in targets if t.parent.name == "logs"]
    for text in [console, *saved]:
        for secret in (*PLANTED, *SECRETS):
            assert secret not in text, f"a log reveals {secret}"
    detections = [json.loads(t.read_text()) for t in targets if t.suffix == ".json"]
    assert all(
        set(d) == {"page", "box", "kind", "detector", "reading"}
        for f in detections
        for d in f["detections"]
    )


@requires_tools
def test_the_originals_are_never_written(tmp_path):
    exams = tmp_path / "exams"
    write_originals(exams)
    before = {p.name: p.read_bytes() for p in exams.iterdir()}
    run_batch(exams, exams / "anonymized", [], Options(), NOW)
    assert {p.name: p.read_bytes() for p in exams.iterdir() if p.is_file()} == before


@requires_tools
def test_a_detector_miss_is_caught_by_verify_and_fails_the_run(tmp_path, monkeypatch, capsys):
    # R8: verify reads the copy by another path than the detector. With every detection
    # dropped, the copy still shows the name, and the run says so.
    exams = tmp_path / "exams"
    exams.mkdir()
    (exams / "a.pdf").write_bytes(build_pdf(LABELLED, b""))
    (tmp_path / "terms.txt").write_text(TERMS, encoding="utf-8")
    monkeypatch.setattr(anonimizador.batch, "detect", lambda *args: [])
    code = main(
        ["batch", "--terms", str(tmp_path / "terms.txt"), "--out", str(exams / "out"), str(exams)]
    )
    console = capsys.readouterr().out
    assert code == EXIT_FOUND, console
    assert "[1] anonymized: FOUND" in console
    assert "0 of 1 copy(ies) verified ok, 1 with something left" in console


@requires_tools
def test_a_failed_step_is_an_error_row_not_a_traceback(source, stubs, capsys):
    stub(stubs, "pdftoppm", "exit 3\n")
    code = main(["batch", "--out", str(source / "out"), str(source)])
    out = capsys.readouterr()
    text = out.out + out.err
    assert code == EXIT_USAGE, text
    assert "[1] anonymized: ERROR\n    not written: pdftoppm failed (exit 3)" in text
    assert "Traceback" not in text and "Fulano" not in text
    [report] = (source / "out").glob("*-anonimizer.md")
    assert "| ERROR |" in report.read_text(encoding="utf-8")


@requires_tools
def test_outputs_under_the_source_are_not_taken_for_originals(source):
    out = source / "anonymized"
    options = Options()
    assert run_batch(source, out, [], options, NOW).exams == 2
    (out / "exam-leftover.pdf").write_bytes(covered_name_pdf())
    assert run_batch(source, out, [], options, NOW.replace(second=10)).exams == 2
    assert len(list(out.glob("*-anonimizer.md"))) == 2


@requires_tools
def test_a_run_without_terms_warns_that_ok_proves_no_name_is_gone(source, capsys):
    out = source / "anonymized"
    run_batch(source, out, [], Options(), NOW)
    [report] = out.glob("*-anonimizer.md")
    assert "no terms file: only identifier patterns" in report.read_text(encoding="utf-8")
    assert "warning: no terms file" in capsys.readouterr().out


@requires_tools
def test_a_second_run_in_the_same_second_leaves_the_first_index_alone(source):
    out = source / "anonymized"
    run_batch(source, out, [], Options(), NOW)
    [report] = out.glob("*-anonimizer.md")
    first = report.read_bytes()
    with pytest.raises(VerifyError, match="already exists"):
        run_batch(source, out, [], Options(), NOW)
    assert report.read_bytes() == first


def test_an_output_directory_git_could_commit_is_refused_before_anything_is_written(source, capsys):
    # Not created yet, inside this repository, and not ignored: the index would be
    # committable (the check runs from the nearest existing directory).
    out = Path(__file__).parent / "batch-output-that-must-not-exist" / "nested"
    code = main(["batch", "--out", str(out), str(source)])
    assert code == EXIT_USAGE
    assert "git cannot commit" in capsys.readouterr().err
    assert not out.parent.exists()


def test_a_source_without_originals_is_a_usage_error(outside_repo, capsys):
    code = main(["batch", "--out", str(outside_repo / "out"), str(outside_repo)])
    assert code == EXIT_USAGE
    assert "no PDF, PNG, or JPEG original" in capsys.readouterr().err
    assert not (outside_repo / "out").exists()


def test_neutral_names_come_from_the_bytes_not_the_name(outside_repo):
    a, b = outside_repo / "Fulano.PDF", outside_repo / "other.pdf"
    a.write_bytes(b"%PDF same")
    b.write_bytes(b"%PDF same")
    assert neutral_name(a) == neutral_name(b)
    assert re.fullmatch(r"exam-[0-9a-f]{12}\.pdf", neutral_name(a))
    c = outside_repo / "photo.JPG"
    c.write_bytes(b"jpeg bytes")
    # An image's copy is a PNG.
    assert neutral_name(c).endswith(".png")


def test_a_link_escapes_markdown_and_encodes_its_target(tmp_path):
    target = tmp_path / "a b" / "x [1] | y.pdf"
    assert (
        link(target, tmp_path, target.name)
        == "[x \\[1\\] \\| y.pdf](a%20b/x%20%5B1%5D%20%7C%20y.pdf)"
    )


def test_a_run_without_ocr_is_refused_before_anything_is_written(source, capsys):
    # Codex CB-4: a copy holds only pixels; without OCR it would pass unread.
    code = main(["batch", "--no-ocr", "--out", str(source / "out"), str(source)])
    assert code == EXIT_USAGE
    assert "batch needs OCR" in capsys.readouterr().err
    assert not (source / "out").exists()


@requires_tools
def test_the_source_itself_as_output_writes_nothing_over_an_original(source):
    # Codex CB-1: every output goes into the run's own new directory, created
    # exclusively, so no original and no earlier output is written over.
    before = {p.name: p.read_bytes() for p in source.iterdir()}
    run_batch(source, source, [], Options(), NOW)
    assert {
        p.name: p.read_bytes() for p in source.iterdir() if p.is_file() and p.name in before
    } == before
    assert len(copies(source)) == 2
    assert all(c.parent == source / "20261030-140509" for c in copies(source))


@requires_tools
def test_an_index_path_that_links_into_the_repository_is_refused(source):
    # Codex CB-2: the index would follow the link into a committable place.
    out = source / "out"
    out.mkdir()
    target = Path(__file__).parent / "index-that-must-not-exist.md"
    (out / "20261030-140509-anonimizer.md").symlink_to(target)
    with pytest.raises(VerifyError, match="git cannot commit"):
        run_batch(source, out, [], Options(), NOW)
    assert not target.exists()


@requires_tools
def test_the_console_never_prints_the_output_path(outside_repo, capsys):
    # Codex CB-3: the directories above the outputs can name a person.
    exams = outside_repo / "Fulano Ficticio"
    exams.mkdir()
    (exams / "a.pdf").write_bytes(build_pdf(b"BT /F1 12 Tf 72 700 Td (Hemoglobina) Tj ET\n", b""))
    code = main(["batch", "--out", str(exams / "anonymized"), str(exams)])
    console = capsys.readouterr()
    assert code == EXIT_OK, console.out + console.err
    assert "Fulano" not in console.out + console.err
    assert re.search(r"index written: \d{8}-\d{6}-anonimizer\.md$", console.out.strip())


@requires_tools
def test_an_unreadable_original_fails_the_run_even_with_a_clean_copy(source, monkeypatch):
    # Codex CB-8: the original's errors count, not only the copy's.
    verify = anonimizador.batch.verify

    def failing_originals(files, terms, options):
        for result in verify(files, terms, options):
            if not result.path.startswith("exam-"):
                result = FileResult(result.index, result.path, [], {}, ["raw: injected"])
            yield result

    monkeypatch.setattr(anonimizador.batch, "verify", failing_originals)
    totals = run_batch(source, source / "out", [], Options(), NOW)
    assert totals.unreadable == 2 and totals.found == 0


@requires_tools
def test_malformed_tool_output_is_an_error_row_not_a_traceback(source, stubs, capsys):
    # Codex CB-9: pdftotext exits 0 with output that is no XHTML.
    stub(
        stubs,
        "pdftotext",
        'case "$*" in *-bbox-layout*) echo "<html><page"; exit 0;; esac\n'
        'exec /usr/bin/env -u PATH "$(command -v -p pdftotext)" "$@"\n',
    )
    code = main(["batch", "--out", str(source / "out"), str(source)])
    text = "".join(capsys.readouterr())
    assert code == EXIT_USAGE, text
    assert "not written: pdftotext wrote output that could not be read" in text
    assert "Traceback" not in text


@requires_tools
def test_the_same_original_gives_the_same_copy(source):
    run_batch(source, source / "out", [], Options(), NOW)
    run_batch(source, source / "out", [], Options(), NOW.replace(second=10))
    first, second = (sorted(d.glob("exam-*")) for d in sorted((source / "out").glob("2026*/")))
    assert [p.read_bytes() for p in first] == [p.read_bytes() for p in second]


def test_copies_git_could_commit_are_refused_before_anything_is_written(source, tmp_path):
    # Antigravity AB-5: rules ignoring only the index and the logs left the copies,
    # which are unreviewed, committable.
    repo = tmp_path / "repo"
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    (repo / ".gitignore").write_text("*-anonimizer.md\nlogs/\n", encoding="utf-8")
    with pytest.raises(VerifyError, match="git cannot commit"):
        run_batch(source, repo / "out", [], Options(), NOW)
    assert not (repo / "out").exists()


def test_a_link_among_the_originals_that_leads_nowhere_stops_the_run(source, capsys):
    # Antigravity AB-6: it was skipped, and the run reported every exam done.
    (source / "gone.pdf").symlink_to(source / "nowhere.pdf")
    code = main(["batch", "--out", str(source / "out"), str(source)])
    assert code == EXIT_USAGE
    assert "a link among the originals leads nowhere" in capsys.readouterr().err
    assert not (source / "out").exists()
