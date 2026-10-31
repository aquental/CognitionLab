"""Words with their positions on the page (R3).

Every page is rendered to an image, which is what the redaction paints and keeps.
Words come from two readings of it: the PDF text layer (`pdftotext -bbox-layout`),
scaled to the image, and the OCR of the image (`tesseract ... tsv`). A word is never
printed or reported: only its box is.
"""

from __future__ import annotations

import csv
import io
import re
import struct
import xml.etree.ElementTree as ET
from collections.abc import Callable
from concurrent.futures import Executor
from dataclasses import dataclass, field
from pathlib import Path

from PIL import Image, ImageOps

from anonimizador.channels import ToolError, run

# (left, top, right, bottom) in pixels of the page image.
Box = tuple[int, int, int, int]

# A text layer whose page shape differs from the image by more than this (a rotated
# page) is not mapped onto it: its boxes would land in the wrong place.
MAX_ASPECT_DIFFERENCE = 0.02
# The modes of a 16-bit grayscale image (a PNG); "I" is how older Pillows open it.
SIXTEEN_BIT = ("I;16", "I;16B", "I;16L", "I;16N", "I")
# The TSV level of a word, and the columns a word needs.
TSV_WORD_LEVEL = "5"
TSV_COLUMNS = frozenset(
    ("level", "block_num", "par_num", "line_num", "left", "top", "width", "height", "text")
)


@dataclass(frozen=True)
class Word:
    text: str
    box: Box
    # Words with the same line key are one line, in reading order.
    line: tuple[int | str, ...]


@dataclass
class Page:
    number: int
    image: Path
    size: tuple[int, int]
    native: list[Word] = field(default_factory=list)
    ocr: list[Word] = field(default_factory=list)
    # The text layer could not be placed on the image (see MAX_ASPECT_DIFFERENCE).
    unaligned: bool = False


def render_pdf(path: Path, tmp: Path, dpi: int, timeout: int) -> list[Path]:
    """Every page as a PNG at `dpi`, in page order."""
    run(["pdftoppm", "-r", str(dpi), "-png", str(path), str(tmp / "page")], timeout)
    pages = sorted(tmp.glob("page-*.png"), key=lambda p: int(p.stem.rsplit("-", 1)[1]))
    if not pages:
        raise ToolError("pdftoppm rendered no page")
    return pages


# What a malformed tool output raises while it is parsed.
PARSE_ERRORS = (ET.ParseError, ValueError, KeyError, TypeError, OverflowError)
# What Pillow raises on a malformed image, besides OSError. A message can quote the
# file's bytes: a bad EXIF header raises a SyntaxError that does (Codex CB-13).
IMAGE_ERRORS = (
    SyntaxError,
    ValueError,
    TypeError,
    KeyError,
    IndexError,
    EOFError,
    struct.error,
    Image.DecompressionBombError,
)


def ocr_words(image: Path, lang: str, timeout: int) -> list[Word]:
    """The words tesseract reads in `image`, with their boxes."""
    tsv = run(["tesseract", str(image), "-", "-l", lang, "tsv"], timeout)
    try:
        return tsv_words(tsv)
    except PARSE_ERRORS:
        # The message would quote the output, which is document text (Codex CB-9).
        raise ToolError("tesseract wrote output that could not be read") from None


def tsv_words(tsv: str) -> list[Word]:
    words = []
    rows = csv.DictReader(io.StringIO(tsv), delimiter="\t", quoting=csv.QUOTE_NONE)
    # Output without the columns of a word is no reading, not an empty page (Codex CB-14).
    if not TSV_COLUMNS.issubset(rows.fieldnames or ()):
        raise ValueError("tesseract TSV without its columns")
    for row in rows:
        text = (row.get("text") or "").strip()
        if row.get("level") != TSV_WORD_LEVEL or not text:
            continue
        left, top = int(row["left"]), int(row["top"])
        box = (left, top, left + int(row["width"]), top + int(row["height"]))
        line = ("ocr", int(row["block_num"]), int(row["par_num"]), int(row["line_num"]))
        words.append(Word(text, box, line))
    return words


def strip_namespaces(root: ET.Element) -> None:
    for element in root.iter():
        element.tag = element.tag.rsplit("}", 1)[-1]


def native_words(path: Path, timeout: int) -> list[tuple[float, float, list[Word]]]:
    """Per page: its width and height in points, and the words of its text layer with
    boxes in points."""
    xhtml = run(["pdftotext", "-bbox-layout", str(path), "-"], timeout)
    try:
        return bbox_pages(xhtml)
    except PARSE_ERRORS:
        # The message would quote the output, which is document text (Codex CB-9).
        raise ToolError("pdftotext wrote output that could not be read") from None


def bbox_pages(xhtml: str) -> list[tuple[float, float, list[Word]]]:
    # The XHTML header names a DTD that is never fetched; ElementTree does not resolve it.
    root = ET.fromstring(re.sub(r"<!DOCTYPE[^>]*>", "", xhtml, count=1))
    strip_namespaces(root)
    pages = []
    for page in root.iter("page"):
        words = []
        for b, block in enumerate(page.iter("block")):
            for n, line in enumerate(block.iter("line")):
                for word in line.iter("word"):
                    text = (word.text or "").strip()
                    if not text:
                        continue
                    edges = [float(word.attrib[k]) for k in ("xMin", "yMin", "xMax", "yMax")]
                    words.append(Word(text, scale_box(edges, 1.0, 1.0), ("native", b, n)))
        pages.append((float(page.attrib["width"]), float(page.attrib["height"]), words))
    return pages


def scale_box(edges: list[float], sx: float, sy: float) -> Box:
    left, top, right, bottom = edges
    return (int(left * sx), int(top * sy), int(right * sx + 0.999), int(bottom * sy + 0.999))


def placed(words: list[Word], width: float, height: float, size: tuple[int, int]) -> list[Word]:
    """Words in points, placed on an image of `size` pixels."""
    sx, sy = size[0] / width, size[1] / height
    return [Word(w.text, scale_box(list(w.box), sx, sy), w.line) for w in words]


def aligned(width: float, height: float, size: tuple[int, int]) -> bool:
    return abs(width / height - size[0] / size[1]) <= MAX_ASPECT_DIFFERENCE * (width / height)


def read_pdf_pages(
    path: Path, tmp: Path, lang: str, dpi: int, timeout: int, pool: Executor, use_ocr: bool
) -> list[Page]:
    images = render_pdf(path, tmp, dpi, timeout)
    layer = native_words(path, timeout)
    if len(layer) != len(images):
        raise ToolError(f"pdftotext read {len(layer)} pages, pdftoppm rendered {len(images)}")
    pages = []
    for number, (image, (width, height, words)) in enumerate(zip(images, layer, strict=True), 1):
        with Image.open(image) as im:
            size = im.size
        page = Page(number, image, size)
        if words and not aligned(width, height, size):
            page.unaligned = True
        elif words:
            page.native = placed(words, width, height, size)
        pages.append(page)
    if use_ocr:
        read_ocr(pages, lambda p: ocr_words(p.image, lang, timeout), pool)
    return pages


def read_image_page(path: Path, tmp: Path, lang: str, timeout: int, use_ocr: bool) -> list[Page]:
    """An image file as one page: its pixels, without anything else the file holds."""
    image = tmp / "page-1.png"
    try:
        with Image.open(path) as im:
            # Turned as a viewer shows it, then copied as bare pixels: nothing of the
            # file's chunks or segments (EXIF, XMP, ICC, text) is carried over (R7).
            shown = eight_bit(flattened(ImageOps.exif_transpose(im)))
            mode = pixel_mode(shown)
            pixels = Image.frombytes(mode, shown.size, shown.convert(mode).tobytes())
    except IMAGE_ERRORS:
        raise ToolError("the image could not be decoded") from None
    pixels.save(image, "PNG")
    page = Page(1, image, pixels.size)
    if use_ocr:
        page.ocr = ocr_words(image, lang, timeout)
    return [page]


def flattened(im: Image.Image) -> Image.Image:
    """The image as a viewer shows it: transparent pixels over white. Dropping the alpha
    channel instead would show the color under it, which can be black (Codex CB-15)."""
    if im.mode not in ("RGBA", "LA", "PA") and "transparency" not in im.info:
        return im
    white = Image.new("RGBA", im.size, (255, 255, 255, 255))
    return Image.alpha_composite(white, im.convert("RGBA"))


def eight_bit(im: Image.Image) -> Image.Image:
    """A 16-bit grayscale image scaled to 8 bits. Converting it directly clips every
    value above 255, which turns a page white (Antigravity AB-4)."""
    if im.mode not in SIXTEEN_BIT:
        return im
    return im.convert("I").point(lambda v: v / 256).convert("L")


def pixel_mode(im: Image.Image) -> str:
    return "L" if im.mode in ("1", "L", "I;16", "I") else "RGB"


def read_ocr(pages: list[Page], read: Callable[[Page], list[Word]], pool: Executor) -> None:
    for page, words in zip(pages, pool.map(read, pages), strict=True):
        page.ocr = words


def read_pages(
    path: Path, tmp: Path, lang: str, dpi: int, timeout: int, pool: Executor, use_ocr: bool = True
) -> list[Page]:
    """The pages of a PDF, PNG, or JPEG, rendered into `tmp`, with their words. Without
    OCR, only a PDF's text layer gives words."""
    if path.suffix.lower() == ".pdf":
        return read_pdf_pages(path, tmp, lang, dpi, timeout, pool, use_ocr)
    return read_image_page(path, tmp, lang, timeout, use_ocr)
