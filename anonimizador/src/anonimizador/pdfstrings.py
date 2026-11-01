"""The strings of PDF bytes: where they are, how they decode, and how a content stream
draws them. A name left in the bytes is often a hex string or an escaped literal, and a
word is often drawn glyph by glyph, so each string is read several ways (Grok G0-14,
G0-19, G0-22)."""

from __future__ import annotations

import contextlib
import re
import zlib
from dataclasses import dataclass

# Hex digits with PDF whitespace (NUL included) between them, at least one digit: `<5>`
# is the byte 0x50 (Codex C0-6). Each repetition ends with a digit, so a long run
# without ">" fails fast.
PDF_HEX_STRING = re.compile(rb"<((?:[\s\x00]*[0-9A-Fa-f])+[\s\x00]*)>")
# A literal string without nested parentheses: read as well as the full parse below,
# because a "(" in binary data can start a long false string.
PDF_INNER_LITERAL = re.compile(rb"\(((?:\\.|[^\\()])*)\)", re.DOTALL)
PDF_STRING_OPENER = re.compile(rb"[(<]")
# A literal string whose balanced parentheses nest one level deep, as nearly every one
# does (Grok G0-20). A regular expression keeps binary data fast to scan, and a "(" in
# it starts no long false string.
PDF_LITERAL = re.compile(rb"\(((?:\\.|[^\\()]|\((?:\\.|[^\\()])*\))*)\)", re.DOTALL)
# The bytes that nest a literal string: parentheses, and escapes, which do not.
PDF_PAREN = re.compile(rb"\\.|[()]", re.DOTALL)
# Where a stream starts or ends: the keywords as the file structure has them (a
# dictionary before `stream`, `endobj` after `endstream`), not the words in a text
# (`(upstream)`, `(endstream)`, Codex C0-5).
STREAM_BOUNDARY = re.compile(rb">>\s*stream(?:\r\n|\r|\n)|endstream[\s\x00]*endobj")
# The longest literal string read with deeper nesting (Grok G0-47). Binary data pairs
# its parentheses over any length; text strings are short.
MAX_NESTED_LITERAL = 1024
# An inline image's dictionary up to its samples: `BI`, its entries, and `ID` with the
# one whitespace byte after it. A `BI` with no `ID` after it starts no image, such as one
# in another image's samples (Antigravity A0-4).
INLINE_IMAGE_HEAD = rb"(?<![\w/])BI[\s\x00/].{0,4096}?(?<![\w/])ID[\s\x00]"
# What surrounds the strings of PDF bytes: comments (whose "(" starts no string),
# dictionary delimiters, stream keywords (at which binary data that looked like an open
# dictionary ends), an inline image's dictionary up to its samples (`BI` ... `ID`,
# see `inline_image_end`), and string openers. The lookahead on the first bytes lets the
# regular expression engine skip the other bytes fast.
PDF_STRUCTURE = re.compile(
    rb"(?=[%<>(esB])(?:(?P<comment>%[^\r\n]*)|(?P<open><<)|(?P<close>>>)"
    rb"|(?P<stream>(?:end)?stream\b)"
    rb"|(?P<inline>" + INLINE_IMAGE_HEAD + rb")"
    rb"|(?P<string>[(<]))",
    re.DOTALL,
)
# The end of an inline image: "EI" before whitespace or the end, after its samples and
# any whitespace, or (searched for) after whitespace.
INLINE_IMAGE_END = re.compile(rb"[\s\x00]*EI(?![^\s\x00])")
INLINE_IMAGE_EI = re.compile(rb"[\s\x00]EI(?![^\s\x00])")
INLINE_IMAGE_START = re.compile(INLINE_IMAGE_HEAD, re.DOTALL)
# The entries of an inline image's dictionary that give its samples' size.
INLINE_IMAGE_ENTRY = re.compile(
    rb"/(?P<key>[A-Za-z]+)[\s\x00]*"
    rb"(?P<value>/[A-Za-z0-9]+|\d+|true|false|\[[\s\x00]*/[A-Za-z0-9]+)"
)
# The abbreviations of the keys of an inline image's dictionary.
INLINE_IMAGE_KEYS = {
    b"Width": b"W",
    b"Height": b"H",
    b"BitsPerComponent": b"BPC",
    b"ColorSpace": b"CS",
    b"ImageMask": b"IM",
    b"Filter": b"F",
}
# Color components per color space (abbreviated and full names); an indexed space has
# one.
INLINE_IMAGE_COMPONENTS = (
    dict.fromkeys((b"/G", b"/DeviceGray", b"/CalGray", b"[/I", b"[/Indexed"), 1)
    | dict.fromkeys((b"/RGB", b"/DeviceRGB", b"/CalRGB"), 3)
    | dict.fromkeys((b"/CMYK", b"/DeviceCMYK"), 4)
)
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
    digits = re.sub(rb"[\s\x00]", b"", body).decode("ascii")
    return bytes.fromhex(digits + "0" * (len(digits) % 2))


# The PDFDocEncoding characters that differ from Latin-1 (PDF 32000, annex D), such as
# the ligature "ﬁ" at 0x93 (Codex C0-7).
PDFDOC = str.maketrans(
    dict(zip(range(0x18, 0x20), "\u02d8\u02c7\u02c6\u02d9\u02dd\u02db\u02da\u02dc", strict=True))
    | dict(
        zip(
            range(0x80, 0xA1),
            "\u2022\u2020\u2021\u2026\u2014\u2013\u0192\u2044\u2039\u203a\u2212\u2030"
            "\u201e\u201c\u201d\u2018\u2019\u201a\u2122\ufb01\ufb02\u0141\u0152\u0160"
            "\u0178\u017d\u0131\u0142\u0153\u0161\u017e\ufffd\u20ac",
            strict=True,
        )
    )
)


def decode_pdf_string(raw: bytes) -> str:
    """A PDF text string: UTF-16 or UTF-8 after a byte order mark; UTF-16 when at least
    half of the code units have a NUL high byte, as two-byte (Identity-H) content
    strings store text without a mark, a dash or a symbol among Latin letters (Grok
    G0-30, G0-33); Latin-1 otherwise, which content strings (WinAnsi) and most
    PDFDocEncoding strings share (`PDFDOC` reads the rest)."""
    codec, mark = pdf_string_codec(raw)
    return raw[mark:].decode(codec, "ignore")


def pdf_string_codec(raw: bytes) -> tuple[str, int]:
    """The codec `decode_pdf_string` reads `raw` with, and the length of its byte order
    mark."""
    for mark, codec in BYTE_ORDER_MARKS:
        if raw.startswith(mark):
            return codec, len(mark)
    if len(raw) >= 2 and len(raw) % 2 == 0:
        units = len(raw) // 2
        if raw[0::2].count(0) * 2 >= units and any(raw[1::2]):
            return "utf-16-be", 0
        if raw[1::2].count(0) * 2 >= units and any(raw[0::2]):
            return "utf-16-le", 0
    return "latin-1", 0


BYTE_ORDER_MARKS = (
    (b"\xfe\xff", "utf-16-be"),
    (b"\xff\xfe", "utf-16-le"),
    (b"\xef\xbb\xbf", "utf-8"),
)


@dataclass(frozen=True)
class PdfString:
    """One string of a PDF: where it is in the bytes, and its bytes."""

    start: int
    end: int
    value: bytes


def literal_ends(blob: bytes) -> dict[int, int]:
    """Where the literal string that each "(" of `blob` opens ends, for those that close,
    with balanced parentheses at any depth. One pass with a stack keeps binary data, full
    of unbalanced parentheses, fast to scan."""
    ends: dict[int, int] = {}
    opens = []
    for token in PDF_PAREN.finditer(blob):
        if token.group() == b"(":
            opens.append(token.start())
        elif token.group() == b")" and opens:
            ends[opens.pop()] = token.end()
    return ends


def string_at(blob: bytes, start: int) -> PdfString | None:
    """The string that starts at `start` ("(" or "<"), if one does. A literal's balanced
    parentheses belong to it one level deep, as in `(Concei\\347\\343o (mae))` (Grok
    G0-20)."""
    if literal := PDF_LITERAL.match(blob, start):
        return PdfString(start, literal.end(), unescape_literal(literal.group(1)))
    if hexed := PDF_HEX_STRING.match(blob, start):
        return PdfString(start, hexed.end(), hex_string(hexed.group(1)))
    return None


def nested_literal(blob: bytes, start: int, ends: dict[int, int]) -> PdfString | None:
    """The literal string at `start` whose parentheses nest deeper than `PDF_LITERAL`
    reads, as in `(Zul\\351o (obs (2)))` (Grok G0-47): up to `MAX_NESTED_LITERAL` bytes,
    and within one stream, since binary data pairs its parentheses over any length."""
    end = ends.get(start, 0)
    if not start < end <= start + MAX_NESTED_LITERAL or STREAM_BOUNDARY.search(blob, start, end):
        return None
    return PdfString(start, end, unescape_literal(blob[start + 1 : end - 1]))


def pdf_string_spans(blob: bytes) -> list[PdfString]:
    """The hex and literal strings of `blob`, in order. A deeper nested literal does not
    hide the strings in it, so a false one in binary data hides nothing: it holds only
    its text outside them, which is then read once."""
    ends = literal_ends(blob)
    spans, nested = [], []
    pos = 0
    while opener := PDF_STRING_OPENER.search(blob, pos):
        pos = opener.start() + 1
        if span := string_at(blob, opener.start()):
            spans.append(span)
            pos = span.end
        elif span := nested_literal(blob, opener.start(), ends):
            nested.append(span)
    inner = sorted([*spans, *nested], key=lambda span: span.start)
    outer = [outer_text(blob, literal, inner) for literal in nested]
    return sorted([*spans, *outer], key=lambda span: span.start)


def outer_text(blob: bytes, literal: PdfString, spans: list[PdfString]) -> PdfString:
    """`literal` without the strings in it, each gap on its own line: a line break in the
    string's own encoding, so a UTF-16 string stays aligned (Antigravity A0-3)."""
    pieces = []
    pos = literal.start + 1
    for span in spans:
        if pos <= span.start and span.end < literal.end:
            pieces.append(unescape_literal(blob[pos : span.start]))
            pos = span.end
    pieces.append(unescape_literal(blob[pos : literal.end - 1]))
    codec, _ = pdf_string_codec(b"".join(pieces))
    return PdfString(literal.start, literal.end, "\n".encode(codec).join(pieces))


def inline_image_entries(head: bytes) -> dict[bytes, bytes]:
    """The entries of an inline image's dictionary (`head`, `BI` ... `ID`) that give its
    data's size, keys abbreviated; an array value is its first name (`[/AHx`)."""
    return {
        INLINE_IMAGE_KEYS.get(e.group("key"), e.group("key")): re.sub(
            rb"[\s\x00]", b"", e.group("value")
        )
        for e in INLINE_IMAGE_ENTRY.finditer(head)
    }


def samples_size(entries: dict[bytes, bytes]) -> int | None:
    """The bytes of an unfiltered inline image's samples; None when they cannot be
    told."""
    if entries.get(b"IM") == b"true":
        components, bits = 1, b"1"
    else:
        components = INLINE_IMAGE_COMPONENTS.get(entries.get(b"CS", b""), 0)
        bits = entries.get(b"BPC", b"")
    width, height = entries.get(b"W", b""), entries.get(b"H", b"")
    if not (components and bits.isdigit() and width.isdigit() and height.isdigit()):
        return None
    return (int(width) * components * int(bits) + 7) // 8 * int(height)


def filtered_size(data: bytes, name: bytes) -> int | None:
    """How many bytes of `data` the filter `name` reads, up to its end-of-data mark;
    None for a filter whose end is not found that way."""
    name = name.lstrip(b"[/")
    if name in (b"DCTDecode", b"DCT"):
        end = data.find(b"\xff\xd9")
        return end + 2 if end >= 0 else None
    if name in (b"ASCIIHexDecode", b"AHx"):
        end = data.find(b">")
        return end + 1 if end >= 0 else None
    if name in (b"ASCII85Decode", b"A85"):
        end = data.find(b"~>")
        return end + 2 if end >= 0 else None
    if name in (b"FlateDecode", b"Fl"):
        inflater = zlib.decompressobj()
        with contextlib.suppress(zlib.error):
            inflater.decompress(data)
            return len(data) - len(inflater.unused_data) if inflater.eof else None
        return None
    if name in (b"RunLengthDecode", b"RL"):
        pos = 0
        while pos < len(data) and data[pos] != 128:
            pos += 2 + data[pos] if data[pos] < 128 else 2
        return pos + 1 if pos < len(data) else None
    return None


def inline_image_end(blob: bytes, head: re.Match[bytes]) -> tuple[int, bool]:
    """Where an inline image ends, and whether that is clear: after its data, when "EI"
    follows it, since the data may hold " EI " itself (Codex C0-4). Unfiltered samples
    are as many bytes as the dictionary gives; filtered data ends at its filter's
    end-of-data mark. Otherwise (a color space named in the resources, a filter whose
    end is not marked) the image ends at the first "EI", which is clear only when no
    other "EI" follows before the next image (Codex round 2)."""
    entries = inline_image_entries(head.group())
    if b"F" in entries:
        size = filtered_size(blob[head.end() :], entries[b"F"])
    else:
        size = samples_size(entries)
    if size is not None and (end := INLINE_IMAGE_END.match(blob, head.end() + size)):
        return end.end(), True
    if not (end := INLINE_IMAGE_EI.search(blob, head.end())):
        return head.end(), False
    following = INLINE_IMAGE_START.search(blob, end.end())
    later = INLINE_IMAGE_EI.search(blob, end.end(), following.start() if following else len(blob))
    return end.end(), later is None


def drawn_spans(blob: bytes) -> tuple[list[PdfString], int]:
    """The strings of `blob` a reader may draw: not in a comment, not in an inline
    image's samples, and not in a dictionary (a marked-content property such as
    `<< /Lang (pt) >> BDC`) (Grok G0-44). Only a `<<` that a `>>` closes opens a
    dictionary, so a stray one in binary data hides nothing (Grok G0-46). Also how many
    inline images end where it is not clear."""
    ends = literal_ends(blob)
    tokens: list[tuple[str, PdfString | None]] = []
    unclear = 0
    pos = 0
    while token := PDF_STRUCTURE.search(blob, pos):
        kind = token.lastgroup or ""
        pos = token.end()
        if kind == "string":
            start = token.start()
            if span := string_at(blob, start) or nested_literal(blob, start, ends):
                tokens.append((kind, span))
                pos = span.end
        elif kind == "inline":
            pos, clear = inline_image_end(blob, token)
            unclear += not clear
        elif kind in ("open", "close", "stream"):
            tokens.append((kind, None))
    closed = closed_dictionaries([kind for kind, _ in tokens])
    spans = []
    depth = 0
    for index, (kind, span) in enumerate(tokens):
        if index in closed:
            depth += 1 if kind == "open" else -1
        elif span and not depth:
            spans.append(span)
    return spans, unclear


def closed_dictionaries(kinds: list[str]) -> set[int]:
    """The positions in `kinds` of the `open` and `close` delimiters that pair up; a
    stream keyword drops the delimiters still open."""
    opens: list[int] = []
    closed = set()
    for index, kind in enumerate(kinds):
        if kind == "open":
            opens.append(index)
        elif kind == "close" and opens:
            closed.update((opens.pop(), index))
        elif kind == "stream":
            opens.clear()
    return closed


# Marks a boundary between strings that only the fonts' widths could settle.
AMBIGUOUS = "\x1f"
# The longest join of pieces read across ambiguous boundaries: longer than a short term
# and than any identifier pattern (a CPF is 14 characters, a CNS 18).
MAX_MIXED_READING = 20
# What a piece adds to a join: its first word, after at most one space.
MIXED_HEAD = re.compile(r"\s?\S*")


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


def drawn_text(blob: bytes) -> tuple[str, int]:
    """The strings of `blob` as drawn, with `AMBIGUOUS` wherever they might not be one
    word. Content streams often draw a word glyph by glyph (`[(Z) -2 (u)] TJ`), and a
    short term must be a whole word (Grok G0-19); a word space is often only a move
    (Grok G0-22). Strings that are not drawn are left out, and their gap is read like
    any other (Grok G0-44). Line breaks inside a string are dropped. Also how many
    inline images end where it is not clear."""
    parts = []
    spans, unclear = drawn_spans(blob)
    for index, span in enumerate(spans):
        if index:
            parts.append(gap_joint(blob[spans[index - 1].end : span.start]))
        parts.append(re.sub(r"[\r\n]", "", decode_pdf_string(span.value)))
    return "".join(parts), unclear


def mixed_readings(marked: str) -> str:
    """Every way the ambiguous boundaries of `marked` could join its pieces, one reading
    per line, each up to `MAX_MIXED_READING` characters: the end of one piece (see
    `join_starts`), the pieces after it, and the start of the last one (its first word,
    after at most one space). A short term, or a number drawn group by group, is whole
    in one of them whatever each move is (Grok G0-29, G0-34, G0-35), and so is a number
    whose own space is drawn at the move (Grok G0-45). Longer terms need no boundary."""
    readings = []
    for line in marked.split("\n"):
        pieces = line.split(AMBIGUOUS)
        for first in range(len(pieces) - 1):
            for reading in join_starts(pieces[first]):
                for piece in pieces[first + 1 :]:
                    head = MIXED_HEAD.match(piece)
                    reading += head.group()
                    if len(reading) > MAX_MIXED_READING:
                        break
                    readings.append(reading)
                    if head.end() < len(piece):
                        break
    return "\n".join(readings)


def join_starts(piece: str) -> list[str]:
    """Where a join may start in `piece`: after its last space and, when the piece ends
    with one space, at its last word too, so that a number keeps its own space (Grok
    G0-45) and a long last word does not use up the length of a join."""
    starts = [re.split(r"\s", piece)[-1]]
    if piece[-1:].isspace() and not piece[-2:-1].isspace():
        starts.append(re.split(r"\s", piece[:-1])[-1] + piece[-1])
    return starts


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
    # One string per line read as PDFDocEncoding, where it differs from Latin-1 (text
    # strings such as an old `/Author`, Codex C0-7).
    pdfdoc: str
    # Inline images whose end is not clear: what follows them may be misread.
    unclear_images: int = 0

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
    lines, drawn, split, mixed, inner, pdfdoc = [], [], [], [], [], []
    unclear = 0
    for blob in blobs:
        for span in pdf_string_spans(blob):
            text = decode_pdf_string(span.value)
            lines.append(text)
            if text == span.value.decode("latin-1"):
                pdfdoc.append(text.translate(PDFDOC))
        marked, images = drawn_text(blob)
        unclear += images
        drawn.append(marked.replace(AMBIGUOUS, ""))
        split.append(marked.replace(AMBIGUOUS, "\n"))
        mixed.append(mixed_readings(marked))
        inner.extend(
            decode_pdf_string(unescape_literal(m)) for m in PDF_INNER_LITERAL.findall(blob)
        )
    views = ("\n".join(view) for view in (lines, drawn, split, mixed, inner, pdfdoc))
    return PdfStringViews(*views, unclear_images=unclear)
