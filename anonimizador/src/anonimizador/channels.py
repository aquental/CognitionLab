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
import zlib
from collections.abc import Iterator
from concurrent.futures import Executor
from dataclasses import dataclass

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
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

Texts = dict[str, list[str]]


class ToolError(Exception):
    """A tool failed or timed out. The message names the tool, never the file."""


def run(args: list[str], timeout: int) -> str:
    """Stdout of a tool. A non-zero exit, a timeout, or a tool that cannot start raises
    `ToolError`, whose message holds no argument (arguments hold file paths)."""
    tool = args[0]
    try:
        result = subprocess.run(args, capture_output=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        raise ToolError(f"{tool} timed out after {timeout} s") from None
    except OSError as e:
        raise ToolError(f"{tool} could not start ({e.strerror})") from None
    if result.returncode != 0:
        raise ToolError(f"{tool} failed (exit {result.returncode})")
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
        self.errors: list[str] = []
        # The raw bytes read every way: the raw channel searches `raw.texts`; patterns
        # also search the decoded PDF strings, since an identifier left in an old object
        # reaches no other pattern channel (Grok G0-21).
        self.raw = RawReading([], [], [], [])

    def take_raw(self, raw: RawReading) -> None:
        self.raw = raw
        self.texts["raw"] = raw.texts

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


def png_size(path: str) -> tuple[int, int]:
    with open(path, "rb") as f:
        head = f.read(24)
    if len(head) == 24 and head[:8] == PNG_SIGNATURE:
        width, height = struct.unpack(">II", head[16:24])
        return width, height
    return 0, 0


def inflate_tail(body: bytes) -> bytes | None:
    """The zlib stream that ends a zTXt or compressed iTXt chunk, if any."""
    for start in range(len(body)):
        try:
            return zlib.decompress(body[start:])
        except zlib.error:
            continue
    return None


def png_chunks(data: bytes) -> Iterator[tuple[bytes, bytes]]:
    if data[:8] != PNG_SIGNATURE:
        return
    pos = 8
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        yield kind, data[pos + 8 : pos + 8 + length]
        pos += 12 + length


def png_text_chunks(data: bytes) -> list[bytes]:
    """Inflated zTXt and compressed iTXt chunks of a PNG (tEXt is already raw)."""
    out = []
    for kind, body in png_chunks(data):
        if kind in (b"zTXt", b"iTXt"):
            inflated = inflate_tail(body)
            if inflated is not None:
                out.append(inflated)
    return out


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


PDF_HEX_STRING = re.compile(rb"<([0-9A-Fa-f\s]{2,})>")
# A literal string without nested parentheses: read as well as the full parse below,
# because a "(" in binary data can start a long false string.
PDF_INNER_LITERAL = re.compile(rb"\(((?:\\.|[^\\()])*)\)", re.DOTALL)
PDF_STRING_OPENER = re.compile(rb"[(<]")
# A literal string whose balanced parentheses nest one level deep, so
# `(Concei\\347\\343o (mae))` is one string (Grok G0-20). Deeper nesting is rare, and a
# regular expression keeps binary streams fast to scan.
PDF_LITERAL = re.compile(rb"\(((?:\\.|[^\\()]|\((?:\\.|[^\\()])*\))*)\)", re.DOTALL)
# The tokens between two strings of a content stream: whitespace, array brackets,
# names, numbers, operators, and anything else (which ends the drawn text).
GAP_TOKEN = re.compile(
    rb"(?P<space>[\s\x00]+|%[^\r\n]*)|(?P<bracket>[\[\]])|(?P<name>/[^\s/\[\]()<>{}%]*)"
    rb"|(?P<number>[-+]?(?:\d+\.?\d*|\.\d+))|(?P<op>[A-Za-z*'\"]+)|(?P<other>.)",
    re.DOTALL,
)
# Text-showing, text-state, and color operators: the text continues where it was.
SAME_TEXT_OPS = frozenset(b"Tj TJ Tc Tw Tz TL Tf Tr Ts rg RG g G k K cs CS sc SC scn SCN".split())
# Anything else between two strings (a move, a new text object, a saved graphics state,
# marked content, other bytes) may or may not separate words: only the fonts' widths
# and the positions, which are not read, tell. It is ambiguous; no gap is a sure break
# (Grok G0-25, G0-26, G0-29, G0-32, G0-37, G0-38, G0-39). So is a number at or below
# this: in a TJ array (thousandths of an em, moving right) it may be a word space, and
# elsewhere it is an operand, whose operator is ambiguous anyway. `pdftotext -layout`
# splits words from about -100 (0.1 em; -110 at 8 and 10 pt); -50 leaves a margin of
# half, and smaller kerning inside a word stays a continuation (Grok G0-42).
WORD_SPACE_ADJUSTMENT = -50
# A gap longer than this is ambiguous without being tokenized (binary-sized gaps stay
# fast).
MAX_TEXT_GAP = 256
PDF_ESCAPES = {b"n": b"\n", b"r": b"\r", b"t": b"\t", b"b": b"\b", b"f": b"\f"}


def unescape_literal(body: bytes) -> bytes:
    """The bytes of a PDF literal string: octal (`\\347`) and character escapes
    resolved, escaped line breaks removed."""

    def replace(match: re.Match[bytes]) -> bytes:
        escape = match.group(1)
        if escape[:1] in b"01234567":
            return bytes([int(escape, 8) & 0xFF])
        if escape[:1] in b"\r\n":
            return b""
        return PDF_ESCAPES.get(escape, escape)

    return re.sub(rb"\\([0-7]{1,3}|\r\n|.)", replace, body, flags=re.DOTALL)


def hex_string(body: bytes) -> bytes:
    """The bytes of a PDF hex string; a missing final digit is 0, as the format says."""
    digits = re.sub(rb"\s", b"", body).decode("ascii")
    return bytes.fromhex(digits + "0" * (len(digits) % 2))


def decode_pdf_string(raw: bytes) -> str:
    """A PDF text string: UTF-16 or UTF-8 after a byte order mark; UTF-16 when at least
    half of the code units have a NUL high byte, as two-byte (Identity-H) content
    strings store text without a mark, a dash or a symbol among Latin letters (Grok
    G0-30, G0-33); PDFDocEncoding (read as Latin-1) otherwise."""
    if raw.startswith(b"\xfe\xff"):
        return raw[2:].decode("utf-16-be", "ignore")
    if raw.startswith(b"\xff\xfe"):
        return raw[2:].decode("utf-16-le", "ignore")
    if raw.startswith(b"\xef\xbb\xbf"):
        return raw[3:].decode("utf-8", "ignore")
    if len(raw) >= 2 and len(raw) % 2 == 0:
        units = len(raw) // 2
        if raw[0::2].count(0) * 2 >= units and any(raw[1::2]):
            return raw.decode("utf-16-be", "ignore")
        if raw[1::2].count(0) * 2 >= units and any(raw[0::2]):
            return raw.decode("utf-16-le", "ignore")
    return raw.decode("latin-1")


@dataclass(frozen=True)
class PdfString:
    """One string of a PDF: where it is in the bytes, and its bytes."""

    start: int
    end: int
    value: bytes


def pdf_string_spans(blob: bytes) -> list[PdfString]:
    """The hex and literal strings of `blob`, in order."""
    spans = []
    pos = 0
    while opener := PDF_STRING_OPENER.search(blob, pos):
        start = opener.start()
        pos = start + 1
        if blob[start] == ord("("):
            if literal := PDF_LITERAL.match(blob, start):
                spans.append(PdfString(start, literal.end(), unescape_literal(literal.group(1))))
                pos = literal.end()
        elif hexed := PDF_HEX_STRING.match(blob, start):
            spans.append(PdfString(start, hexed.end(), hex_string(hexed.group(1))))
            pos = hexed.end()
    return spans


# Marks a boundary between strings that only the fonts' widths could settle.
AMBIGUOUS = "\x1f"
# The longest join of pieces read across ambiguous boundaries: longer than a short term
# and than any identifier pattern (a CPF is 14 characters, a CNS 18).
MAX_MIXED_READING = 20


def gap_joint(gap: bytes) -> str:
    """How the strings before and after `gap` are joined: "" when the text simply
    continues (whitespace, comments, brackets, small TJ kerning, text-showing,
    text-state, and color operators), `AMBIGUOUS` otherwise."""
    if len(gap) > MAX_TEXT_GAP:
        return AMBIGUOUS
    for token in GAP_TOKEN.finditer(gap):
        kind, value = token.lastgroup, token.group()
        if (
            kind == "other"
            or (kind == "op" and value not in SAME_TEXT_OPS)
            or (kind == "number" and float(value) <= WORD_SPACE_ADJUSTMENT)
        ):
            return AMBIGUOUS
    return ""


def drawn_text(blob: bytes, spans: list[PdfString]) -> str:
    """The strings of `blob` as drawn, with `AMBIGUOUS` wherever they might not be one
    word. Content streams often draw a word glyph by glyph (`[(Z) -2 (u)] TJ`), and a
    short term must be a whole word (Grok G0-19); a word space is often only a move
    (Grok G0-22). Line breaks inside a string are dropped."""
    parts = []
    for index, span in enumerate(spans):
        if index:
            parts.append(gap_joint(blob[spans[index - 1].end : span.start]))
        parts.append(re.sub(r"[\r\n]", "", decode_pdf_string(span.value)))
    return "".join(parts)


def mixed_readings(marked: str) -> str:
    """Every way the ambiguous boundaries of `marked` could join its pieces, one reading
    per line, each up to `MAX_MIXED_READING` characters: the end of one piece (after its
    last space), the pieces after it, and the start of the last one (before its first
    space). A short term, or a number drawn group by group, is whole in one of them
    whatever each move is (Grok G0-29, G0-34, G0-35). Longer terms need no boundary."""
    readings = []
    for line in marked.split("\n"):
        pieces = line.split(AMBIGUOUS)
        for first in range(len(pieces) - 1):
            reading = re.split(r"\s", pieces[first])[-1]
            for piece in pieces[first + 1 :]:
                head = re.split(r"\s", piece, maxsplit=1)
                reading += head[0]
                if len(reading) > MAX_MIXED_READING:
                    break
                readings.append(reading)
                if len(head) > 1:
                    break
    return "\n".join(readings)


@dataclass(frozen=True)
class PdfStringViews:
    """The PDF strings of some bytes, read five ways."""

    # One string per line.
    lines: str
    # As drawn, with every move joined, with every move a break, and every join a mix
    # of the two makes: an ambiguous boundary can add a match, never hide one.
    drawn: str
    split: str
    mixed: str
    # Innermost literals only, robust to a false "(" in binary data.
    inner: str

    @property
    def for_patterns(self) -> list[str]:
        """The readings searched for every identifier pattern (Grok G0-21, G0-23, G0-34),
        with `mixed` counted by distinct match. The fully joined reading glues the
        numbers of a whole page together, so it is searched for text patterns only (an
        e-mail longer than a join, Grok G0-40)."""
        return [self.lines, self.split]


def pdf_string_views(blobs: list[bytes]) -> PdfStringViews:
    """Every PDF string in `blobs`, decoded. A name stored as a hex string
    (`<FEFF005A…>`) or with octal escapes (`(Jo\\343o)`), the usual forms for a non-ASCII
    name, holds none of its letters in the bytes (Grok G0-14)."""
    lines, drawn, split, mixed, inner = [], [], [], [], []
    for blob in blobs:
        spans = pdf_string_spans(blob)
        lines.extend(decode_pdf_string(span.value) for span in spans)
        marked = drawn_text(blob, spans)
        drawn.append(marked.replace(AMBIGUOUS, ""))
        split.append(marked.replace(AMBIGUOUS, "\n"))
        mixed.append(mixed_readings(marked))
        inner.extend(
            decode_pdf_string(unescape_literal(m)) for m in PDF_INNER_LITERAL.findall(blob)
        )
    return PdfStringViews(
        "\n".join(lines), "\n".join(drawn), "\n".join(split), "\n".join(mixed), "\n".join(inner)
    )


def raw_blobs(data: bytes) -> list[bytes]:
    """The file bytes, every zlib stream in it, and PNG compressed text."""
    blobs = [data]
    for match in re.finditer(rb"stream\r?\n", data):
        start = match.end()
        end = data.find(b"endstream", start)
        if end < 0:
            continue
        with contextlib.suppress(zlib.error):
            blobs.append(zlib.decompressobj().decompress(data[start:end]))
    blobs.extend(png_text_chunks(data))
    return blobs


@dataclass(frozen=True)
class RawReading:
    """What the bytes of a file hold, read every way."""

    # Searched for every term.
    texts: list[str]
    # PDF strings, searched for every pattern.
    strings: list[str]
    # Joins of ambiguous boundaries: searched for terms that must be whole words, and
    # for patterns by distinct match.
    joins: list[str]
    # PDF strings fully joined: searched for text patterns.
    drawn: list[str]


def raw_readings(data: bytes) -> RawReading:
    """The raw bytes of a PDF or an image file, read every way (Grok G0-41: an image's
    bytes can hold content-stream text too)."""
    blobs = raw_blobs(data)
    texts = decode_both(blobs)
    texts.extend(
        blob.decode("utf-16-be", "ignore") for blob in blobs if blob.count(b"\x00") > len(blob) // 4
    )
    strings = pdf_string_views(blobs)
    texts.extend((strings.lines, strings.drawn, strings.split, strings.inner))
    return RawReading(texts, strings.for_patterns, [strings.mixed], [strings.drawn])


def raw_texts(data: bytes) -> list[str]:
    """The file bytes and every zlib stream in it, decoded the ways text is stored:
    leftover incremental updates, uncompressed content streams, XMP packets, PNG text,
    and PDF strings (hex and escaped literals)."""
    return raw_readings(data).texts


def image_metadata(data: bytes) -> list[str]:
    """Metadata blocks of an image: PNG text and EXIF chunks; JPEG APPn and COM
    segments (EXIF, XMP, IPTC, comments)."""
    blobs = []
    for kind, body in png_chunks(data):
        if kind in (b"tEXt", b"iTXt", b"eXIf"):
            blobs.append(body)
        if kind in (b"zTXt", b"iTXt"):
            inflated = inflate_tail(body)
            if inflated is not None:
                blobs.append(inflated)
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
    return decode_both(blobs)


def read_pdf(
    path: str, name: str, tmp: str, lang: str, dpi: int, use_ocr: bool, timeout: int, pool: Executor
) -> Reading:
    """Every channel of a PDF. `name` is the path as the corpus records it (relative to
    the directory searched). Page renders and embedded images go to `tmp`."""
    reading = Reading()
    texts = reading.texts
    texts["name"] = [name]
    with open(path, "rb") as f:
        reading.take_raw(raw_readings(f.read()))
    pages = None
    with reading.channel("metadata"):
        info = run(["pdfinfo", "-enc", "UTF-8", path], timeout)
        pages = page_count(info)
        texts["metadata"] = [info, run(["pdfinfo", "-meta", path], timeout)]
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
    """Every channel of a PNG or JPEG file."""
    reading = Reading()
    with open(path, "rb") as f:
        data = f.read()
    reading.texts["name"] = [name]
    reading.texts["metadata"] = image_metadata(data)
    reading.take_raw(raw_readings(data))
    if use_ocr:
        with reading.channel("ocr"):
            reading.texts["ocr"] = [ocr(path, lang, timeout)]
    return reading


def missing_languages(langs: str, timeout: int) -> list[str]:
    """The tesseract languages of `langs` (`por+eng`) with no installed data."""
    installed = set(run(["tesseract", "--list-langs"], timeout).split())
    return [lang for lang in langs.split("+") if lang not in installed]
