"""Reading pages: what a malformed image or tool output gives (Codex CB-13 to CB-15)."""

import io
import shutil
import struct
import zlib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import pytest
from conftest import requires_tools, stub
from PIL import Image

from anonimizador.channels import ToolError
from anonimizador.cli import EXIT_USAGE, main
from anonimizador.locate import read_pages, tsv_words

SENTINEL = b"Fulano!!"
# The real tesseract, found before a test puts a stub ahead of it on PATH.
TESSERACT = shutil.which("tesseract")


def png_with_chunk(kind: bytes, payload: bytes) -> bytes:
    """A white PNG with one more chunk, its checksum valid, after the header."""
    buffer = io.BytesIO()
    Image.new("L", (40, 20), 255).save(buffer, "PNG")
    data = buffer.getvalue()
    chunk = struct.pack(">I", len(payload)) + kind + payload
    chunk += struct.pack(">I", zlib.crc32(kind + payload))
    # The signature (8 bytes) and the IHDR chunk (25 bytes) come first.
    return data[:33] + chunk + data[33:]


def read(path: Path, tmp: Path, use_ocr: bool = False):
    with ThreadPoolExecutor(1) as pool:
        return read_pages(path, tmp, "por", 150, 60, pool, use_ocr)


def test_malformed_exif_is_a_tool_error_without_its_bytes(tmp_path):
    # Codex CB-13: Pillow's SyntaxError quotes the bad EXIF header.
    image = tmp_path / "a.png"
    image.write_bytes(png_with_chunk(b"eXIf", SENTINEL))
    with pytest.raises(ToolError) as error:
        read(image, tmp_path)
    assert "Fulano" not in str(error.value)
    assert error.value.__cause__ is None and error.value.__suppress_context__


@requires_tools
def test_malformed_exif_in_a_batch_is_an_error_row(tmp_path, capsys):
    exams = tmp_path / "exams"
    exams.mkdir()
    (exams / "a.png").write_bytes(png_with_chunk(b"eXIf", SENTINEL))
    code = main(["batch", "--out", str(tmp_path / "out"), str(exams)])
    text = "".join(capsys.readouterr())
    logs = "".join(p.read_text(encoding="utf-8") for p in (tmp_path / "out").rglob("*.txt"))
    assert code == EXIT_USAGE, text
    assert "[1] anonymized: ERROR" in text
    assert "Fulano" not in text + logs and "Traceback" not in text


def test_tsv_without_its_columns_is_no_reading():
    # Codex CB-14: garbage was read as an empty page.
    with pytest.raises(ValueError):
        tsv_words("garbage\n")
    columns = "level page_num block_num par_num line_num word_num left top width height conf text"
    header = columns.replace(" ", "\t") + "\n"
    assert tsv_words(header) == []


@requires_tools
def test_malformed_tsv_in_a_batch_is_an_error_row(tmp_path, stubs, capsys):
    exams = tmp_path / "exams"
    exams.mkdir()
    Image.new("L", (40, 20), 255).save(exams / "a.png")
    stub(
        stubs,
        "tesseract",
        f'case "$*" in *tsv*) echo garbage; exit 0;; esac\nexec "{TESSERACT}" "$@"\n',
    )
    code = main(["batch", "--out", str(tmp_path / "out"), str(exams)])
    text = "".join(capsys.readouterr())
    assert code == EXIT_USAGE, text
    assert "not written: tesseract wrote output that could not be read" in text


def test_a_transparent_background_is_read_as_white(tmp_path):
    # Codex CB-15: black under transparent pixels turned the whole page black.
    rgba = Image.new("RGBA", (40, 20), (0, 0, 0, 0))
    for x in range(10, 30):
        rgba.putpixel((x, 10), (0, 0, 0, 255))
    rgba.save(tmp_path / "a.png")
    (tmp_path / "pages").mkdir()
    [page] = read(tmp_path / "a.png", tmp_path / "pages")
    with Image.open(page.image) as im:
        gray = im.convert("L")
        assert gray.getpixel((0, 0)) == 255
        assert gray.getpixel((15, 10)) == 0


def test_a_16_bit_image_keeps_its_contrast(tmp_path):
    # Antigravity AB-4: converting to 8 bits clipped every value above 255 to white.
    image = Image.new("I;16", (40, 20), 60000)
    for x in range(10, 30):
        image.putpixel((x, 10), 2000)
    image.save(tmp_path / "a.png")
    (tmp_path / "pages").mkdir()
    [page] = read(tmp_path / "a.png", tmp_path / "pages")
    with Image.open(page.image) as im:
        gray = im.convert("L")
        assert gray.getpixel((0, 0)) > 200
        assert gray.getpixel((15, 10)) < 20


def test_a_16_bit_image_with_a_transparent_value_keeps_its_contrast(tmp_path):
    # Codex CB-22: transparency was flattened first, to 8 bits, before scaling.
    image = Image.new("I;16", (40, 20), 60000)
    for x in range(10, 30):
        image.putpixel((x, 10), 2000)
    for x in range(5):
        image.putpixel((x, 0), 0)
    image.save(tmp_path / "a.png", transparency=0)
    (tmp_path / "pages").mkdir()
    [page] = read(tmp_path / "a.png", tmp_path / "pages")
    with Image.open(page.image) as im:
        gray = im.convert("L")
        assert gray.getpixel((20, 5)) > 200
        assert gray.getpixel((15, 10)) < 20
        assert gray.getpixel((1, 0)) == 255


@requires_tools
def test_a_rendered_page_past_the_pixel_limit_is_a_tool_error(tmp_path, monkeypatch):
    # Codex CB-21: Pillow's DecompressionBombError escaped every handler.
    from make_covered_name_pdf import covered_name_pdf

    pdf = tmp_path / "a.pdf"
    pdf.write_bytes(covered_name_pdf())
    monkeypatch.setattr(Image, "MAX_IMAGE_PIXELS", 100)
    (tmp_path / "pages").mkdir()
    with pytest.raises(ToolError, match="too many pixels; lower --dpi"):
        read(pdf, tmp_path / "pages")


def test_a_transparent_16_bit_image_needs_no_pillow_newer_than_declared(tmp_path, monkeypatch):
    # Codex CB-25: `get_flattened_data` exists only in Pillow 11.3+; the project allows
    # 10.1. Every pixel API newer than that is made to fail here.
    def unavailable(self, *args, **kwargs):
        pytest.fail("a pixel API newer than Pillow 10.1 was called")

    for name in ("get_flattened_data", "getdata"):
        monkeypatch.setattr(Image.Image, name, unavailable, raising=False)
    test_a_16_bit_image_with_a_transparent_value_keeps_its_contrast(tmp_path)
