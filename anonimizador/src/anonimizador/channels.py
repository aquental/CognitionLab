"""Where an identifier can survive anonymization, and how each place is read.

Every reader returns text to search, never writes outside a temporary directory, and
runs only local tools (poppler, tesseract). A place that could not be read is reported
as an error, never as an empty place: an unread channel proves nothing (Grok G4-5).
"""

from __future__ import annotations

import contextlib
import os
import re
import struct
import subprocess
from collections.abc import Iterator
from concurrent.futures import Executor
from dataclasses import dataclass

from anonimizador.images import jpeg_is_whole, png_chunks, png_is_whole, png_size, png_text
from anonimizador.pdfstreams import decoded_streams
from anonimizador.pdfstrings import pdf_string_views

# The places searched, in report order.
CHANNELS = ("name", "metadata", "text", "ocr", "images", "raw")
# Channels whose texts are separate parts of the file (pages, embedded images), not
# alternative readings of the same bytes. Their parts are searched together, so every
# occurrence counts and a term split across a page break is found (Grok G0-6).
PARTS_CHANNELS = ("ocr", "images")
# Channels whose text a document shows or declares (searched for patterns).
# The file name too: an identifier can be the name of the file (Grok G0-16).
PATTERN_CHANNELS = ("name", "text", "ocr", "images", "metadata")
# Images smaller than this on both sides are icons, not readable text. A thin strip
# (a header bar, a label) can hold a line of text, so one long side is enough (Grok
# G0-17).
MIN_OCR_SIDE = 32

Texts = dict[str, list[str]]


class ToolError(Exception):
    """A tool failed or timed out. The message names the tool, never the file."""


# Poppler reports what it could not read on stderr, often with exit 0: a stream it
# could not decode leaves its text out (Codex C0-2). Warnings are not errors.
POPPLER_TOOLS = frozenset(("pdftotext", "pdfinfo", "pdftoppm", "pdfimages"))
POPPLER_ERROR = re.compile(
    rb"^(?:Syntax |Internal |I/O |Permission |Command Line )?Error\b", re.MULTILINE
)


def run(args: list[str], timeout: int) -> str:
    """Stdout of a tool. A non-zero exit, a timeout, a tool that cannot start, or a
    Poppler error on stderr raises `ToolError`, whose message holds no argument
    (arguments hold file paths) and no stderr (it can quote the file)."""
    tool = args[0]
    try:
        result = subprocess.run(args, capture_output=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        raise ToolError(f"{tool} timed out after {timeout} s") from None
    except OSError as e:
        raise ToolError(f"{tool} could not start ({e.strerror})") from None
    if result.returncode != 0:
        raise ToolError(f"{tool} failed (exit {result.returncode})")
    if tool in POPPLER_TOOLS and (errors := len(POPPLER_ERROR.findall(result.stderr))):
        raise ToolError(f"{tool} reported {errors} error(s) reading the file")
    return result.stdout.decode("utf-8", "replace")


def ocr(image: str, lang: str, timeout: int) -> str:
    return run(["tesseract", image, "-", "-l", lang], timeout)


def ocr_all(images: list[str], lang: str, timeout: int, pool: Executor) -> list[str]:
    """OCR of every image; the first failure raises `ToolError`."""
    return list(pool.map(lambda p: ocr(p, lang, timeout), images))


def page_count(info: str) -> int | None:
    for line in info.splitlines():
        if line.startswith("Pages:"):
            with contextlib.suppress(ValueError):
                return int(line.split(":", 1)[1])
    return None


class Reading:
    """The texts of every channel of one file, and the channels that failed."""

    def __init__(self) -> None:
        self.texts: Texts = {channel: [] for channel in CHANNELS}
        # Independent places of a channel (two metadata segments, two streams), each
        # read several ways: a term counts once per place, in its best reading, and the
        # places add up (Codex C0-12). A channel without blocks is one place.
        self.blocks: dict[str, list[list[str]]] = {}
        self.errors: list[str] = []
        # The raw bytes read every way: the raw channel searches `raw.texts`; patterns
        # also search the decoded PDF strings, since an identifier left in an old object
        # reaches no other pattern channel (Grok G0-21).
        self.raw = RawReading([], [], [], [])

    def take_raw(self, raw: RawReading) -> None:
        self.raw = raw
        self.texts["raw"] = raw.texts
        self.blocks["raw"] = [block.texts for block in raw.blocks]
        if raw.unread:
            self.errors.append(f"raw: {raw.unread} stream(s) cannot be decoded")
        if raw.unclear_images:
            self.errors.append(f"raw: {raw.unclear_images} inline image(s) end where it is unclear")

    @contextlib.contextmanager
    def channel(self, name: str) -> Iterator[None]:
        """Records a `ToolError` raised while reading `name` as an error of that
        channel, so the other channels are still read."""
        try:
            yield
        except ToolError as e:
            self.errors.append(f"{name}: {e}")


def worth_ocr(size: tuple[int, int]) -> bool:
    return max(size) >= MIN_OCR_SIDE


# Runs of UTF-16 text (EXIF and Windows metadata): letters with a NUL beside each.
UTF16_RUNS = {
    "utf-16-le": re.compile(rb"(?:[^\x00]\x00){2,}"),
    "utf-16-be": re.compile(rb"(?:\x00[^\x00]){2,}"),
}


def split_ends(text: str, first: bool, last: bool) -> str:
    if first and len(text) > 1:
        text = text[0] + "\n" + text[1:]
    if last and len(text) > 1:
        text = text[:-1] + "\n" + text[-1]
    return text


def utf16_views(blob: bytes) -> list[str]:
    """The UTF-16 runs of `blob`, decoded, one run per line, one view per byte order
    (Grok G0-8). Read as single bytes, UTF-16 puts a NUL between letters, so a short
    term was never a whole word. A NUL-terminated ASCII character beside a run reads as
    one more character of it, and the bytes cannot tell the two apart, so each view
    also comes with the first and the last character of each run set apart. That can
    only add matches, never hide one."""
    views = []
    for encoding, pattern in UTF16_RUNS.items():
        runs = [run.decode(encoding) for run in pattern.findall(blob)]
        if runs:
            views.extend(
                "\n".join(split_ends(run, first, last) for run in runs)
                for first in (False, True)
                for last in (False, True)
            )
    return views


def decode_both(blobs: list[bytes]) -> list[str]:
    """Each blob read as UTF-8, as Latin-1, and through its UTF-16 runs."""
    texts = []
    for blob in blobs:
        texts.append(blob.decode("utf-8", "ignore"))
        texts.append(blob.decode("latin-1"))
        texts.extend(utf16_views(blob))
    return texts


def raw_blobs(data: bytes) -> tuple[list[bytes], int]:
    """The file bytes, every stream decoded through its filters (`pdfstreams`), and PNG
    compressed text; and how many streams could not be decoded."""
    streams, unread = decoded_streams(data)
    return [data, *streams, *png_text(data)[0]], unread


@dataclass(frozen=True)
class RawBlock:
    """One part of a file's bytes (the file itself, a decoded stream), read every way."""

    # Searched for every term.
    texts: list[str]
    # Joins of ambiguous boundaries: also searched for terms that must be whole words.
    joins: str


@dataclass(frozen=True)
class RawReading:
    """What the bytes of a file hold, read every way."""

    # Each part of the bytes.
    blocks: list[RawBlock]
    # PDF strings, searched for every pattern.
    strings: list[str]
    # Joins of ambiguous boundaries: searched for terms that must be whole words, and
    # for patterns by distinct match.
    joins: list[str]
    # PDF strings fully joined: searched for text patterns.
    drawn: list[str]
    # Streams that cannot be decoded, and inline images whose end is not clear.
    unread: int = 0
    unclear_images: int = 0

    @property
    def texts(self) -> list[str]:
        return [text for block in self.blocks for text in block.texts]


def raw_readings(data: bytes) -> RawReading:
    """The raw bytes of a PDF or an image file, read every way (Grok G0-41: an image's
    bytes can hold content-stream text too)."""
    blobs, unread = raw_blobs(data)
    blocks, views = [], []
    for blob in blobs:
        texts = decode_both([blob])
        if blob.count(b"\x00") > len(blob) // 4:
            texts.append(blob.decode("utf-16-be", "ignore"))
        view = pdf_string_views([blob])
        texts.extend((view.lines, view.drawn, view.split, view.inner, view.pdfdoc))
        blocks.append(RawBlock(texts, view.mixed))
        views.append(view)
    strings = [
        "\n".join(getattr(view, reading) for view in views) for reading in ("lines", "split")
    ]
    joins = "\n".join(view.mixed for view in views)
    drawn = "\n".join(view.drawn for view in views)
    unclear = sum(view.unclear_images for view in views)
    return RawReading(blocks, strings, [joins], [drawn], unread, unclear)


def raw_texts(data: bytes) -> list[str]:
    """The file bytes and every zlib stream in it, decoded the ways text is stored:
    leftover incremental updates, uncompressed content streams, XMP packets, PNG text,
    and PDF strings (hex and escaped literals)."""
    return raw_readings(data).texts


def image_metadata(data: bytes) -> list[str]:
    """Every reading of every metadata block of an image."""
    return [text for block in image_metadata_blocks(data) for text in block]


def image_metadata_blocks(data: bytes) -> list[list[str]]:
    """Metadata blocks of an image, each read every way: PNG text and EXIF chunks;
    JPEG APPn and COM segments (EXIF, XMP, IPTC, comments)."""
    blobs = [body for kind, body in png_chunks(data) if kind in (b"tEXt", b"iTXt", b"eXIf")]
    blobs.extend(png_text(data)[0])
    if data[:2] == b"\xff\xd8":
        pos = 2
        while pos + 4 <= len(data) and data[pos] == 0xFF:
            marker = data[pos + 1]
            if marker == 0xFF:  # a fill byte before a marker: it has no length (Grok G0-12)
                pos += 1
                continue
            if marker == 0xDA:  # start of scan: no metadata after this
                break
            if marker == 0x01 or 0xD0 <= marker <= 0xD7:  # standalone markers: no length
                pos += 2
                continue
            (length,) = struct.unpack(">H", data[pos + 2 : pos + 4])
            if 0xE1 <= marker <= 0xEF or marker == 0xFE:
                blobs.append(data[pos + 4 : pos + 2 + length])
            pos += 2 + length
    return [decode_both([blob]) for blob in blobs]


# What reading malformed bytes can raise: they are unread, never a traceback.
PARSE_ERRORS = (ValueError, TypeError, OverflowError, RecursionError, IndexError, KeyError)


def take_raw(reading: Reading, data: bytes) -> None:
    """The raw readings of `data` into `reading`, or the raw channel's error."""
    try:
        reading.take_raw(raw_readings(data))
    except PARSE_ERRORS as e:
        reading.errors.append(f"raw: the bytes cannot be parsed ({type(e).__name__})")


def read_pdf(
    path: str, name: str, tmp: str, lang: str, dpi: int, use_ocr: bool, timeout: int, pool: Executor
) -> Reading:
    """Every channel of a PDF. `name` is the path as the corpus records it (relative to
    the directory searched). Page renders and embedded images go to `tmp`."""
    reading = Reading()
    texts = reading.texts
    texts["name"] = [name]
    with open(path, "rb") as f:
        data = f.read()
    take_raw(reading, data)
    pages = None
    with reading.channel("metadata"):
        info = run(["pdfinfo", "-enc", "UTF-8", path], timeout)
        pages = page_count(info)
        texts["metadata"] = [info, run(["pdfinfo", "-meta", path], timeout)]
        # The information dictionary and the XMP packet are two places.
        reading.blocks["metadata"] = [[text] for text in texts["metadata"]]
    with reading.channel("text"):
        texts["text"] = [run(["pdftotext", "-enc", "UTF-8", "-layout", path, "-"], timeout)]
    if use_ocr:
        with reading.channel("ocr"):
            run(["pdftoppm", "-r", str(dpi), "-png", path, os.path.join(tmp, "page")], timeout)
            rendered = sorted(os.path.join(tmp, p) for p in os.listdir(tmp) if p.startswith("page"))
            if pages is None or len(rendered) != pages:
                expected = "an unknown number of" if pages is None else str(pages)
                raise ToolError(f"pdftoppm rendered {len(rendered)} of {expected} pages")
            texts["ocr"] = ocr_all(rendered, lang, timeout, pool)
        with reading.channel("images"):
            run(["pdfimages", "-png", path, os.path.join(tmp, "img")], timeout)
            images = sorted(
                os.path.join(tmp, p)
                for p in os.listdir(tmp)
                if p.startswith("img") and worth_ocr(png_size(os.path.join(tmp, p)))
            )
            texts["images"] = ocr_all(images, lang, timeout, pool)
    return reading


def read_image(path: str, name: str, lang: str, use_ocr: bool, timeout: int) -> Reading:
    """Every channel of a PNG or JPEG file. Bytes that are not a whole PNG or JPEG
    image are an error, OCR or not: no channel read them as one (Codex C0-9)."""
    reading = Reading()
    with open(path, "rb") as f:
        data = f.read()
    if not (png_is_whole(data) or jpeg_is_whole(data)):
        reading.errors.append("file: not a whole PNG or JPEG image")
    if unread := png_text(data)[1]:
        reading.errors.append(f"metadata: {unread} compressed PNG text chunk(s) cannot be read")
    reading.texts["name"] = [name]
    reading.blocks["metadata"] = image_metadata_blocks(data)
    reading.texts["metadata"] = [text for block in reading.blocks["metadata"] for text in block]
    take_raw(reading, data)
    if use_ocr:
        with reading.channel("ocr"):
            reading.texts["ocr"] = [ocr(path, lang, timeout)]
    return reading


def missing_languages(langs: str, timeout: int) -> list[int]:
    """The positions (from 1) of the tesseract languages of `langs` (`por+eng`) with no
    installed data."""
    # One language per line after a header that names the data directory, a path
    # (Codex C0-1): only the language lines count.
    lines = run(["tesseract", "--list-langs"], timeout).splitlines()
    installed = {line.strip() for line in lines if not line.startswith("List of")}
    return [n for n, lang in enumerate(langs.split("+"), start=1) if lang not in installed]
