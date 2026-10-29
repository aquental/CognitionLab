"""The streams of PDF bytes, decoded as a reader decodes them.

Each stream is found from its object's dictionary: its filters and their parameters,
indirect references and `#xx` name escapes resolved, and its `/Length`. A stream that
cannot be decoded is reported, never taken as empty: whatever it holds is unread
(Codex C0-3, C0-13..C0-16).
"""

from __future__ import annotations

import base64
import bisect
import re
import zlib
from collections.abc import Callable
from dataclasses import dataclass


class Unread(Exception):
    """A stream that cannot be decoded, with what was decoded before the failure."""

    def __init__(self, partial: bytes = b"") -> None:
        super().__init__("stream not decoded")
        self.partial = partial


class Name(bytes):
    """A PDF name, without its slash."""


@dataclass(frozen=True)
class Ref:
    """An indirect reference (`10 0 R`)."""

    number: int
    generation: int


# One token of PDF object syntax, after whitespace and comments.
TOKEN = re.compile(
    rb"(?:[\s\x00]|%[^\r\n]*)*"
    rb"(?:(?P<dict><<)|(?P<end_dict>>>)|(?P<array>\[)|(?P<end_array>\])"
    rb"|(?P<ref>(?P<number>\d+)[\s\x00]+(?P<generation>\d+)[\s\x00]+R(?![A-Za-z0-9]))"
    rb"|(?P<numeric>[-+]?(?:\d+\.?\d*|\.\d+))"
    rb"|(?P<name>/[^\s\x00/\[\]<>(){}%]*)"
    rb"|(?P<literal>\()|(?P<hex><[0-9A-Fa-f\s\x00]*>)"
    rb"|(?P<keyword>[A-Za-z]+))",
    re.DOTALL,
)
KEYWORDS = {b"true": True, b"false": False, b"null": None}
# The bytes that nest a literal string: parentheses, and escapes, which do not.
PAREN = re.compile(rb"\\.|[()]", re.DOTALL)
# Dictionaries and arrays nest this deep at most (each level is a Python call).
MAX_DEPTH = 256
# An object header; object numbers have 10 digits at most, so a long run of digits is
# no header (and no number too long to convert).
OBJECT_HEADER = re.compile(rb"(?<![0-9])(\d{1,10})[\s\x00]+(\d{1,5})[\s\x00]+obj(?![A-Za-z0-9])")
# How many object headers before a stream are tried for its dictionary: a string in
# the dictionary can look like a header (`(9 0 obj)`).
HEADER_TRIES = 16
# The start of a stream's data: the keyword ends with CRLF, LF, or (accepted by
# readers) CR alone (Grok G0-43); `endstream` is not one.
STREAM_START = re.compile(rb"(?<![A-Za-z])stream(?:\r\n|\r|\n)")
STREAM_END = re.compile(rb"[\s\x00]*endstream")
SPACE = b" \t\r\n\f\x00"


def name_of(token: bytes) -> bytes:
    """A name without its slash, `#xx` escapes resolved (`/F#69lter` is `Filter`)."""
    return re.sub(rb"#([0-9A-Fa-f]{2})", lambda m: bytes.fromhex(m[1].decode()), token[1:])


def literal_end(data: bytes, start: int) -> int:
    """The end of the literal string that starts at `start`."""
    depth = 0
    for token in PAREN.finditer(data, start):
        if token[0] == b"(":
            depth += 1
        elif token[0] == b")":
            depth -= 1
            if not depth:
                return token.end()
    raise ValueError("unterminated literal string")


def parse_object(data: bytes, pos: int, depth: int = 0) -> tuple[object, int]:
    """The PDF object at `pos` and where it ends: a dictionary (bytes keys), a list, a
    name (bytes), a number, a `Ref`, a boolean, None, or a string (raw bytes). Raises
    `ValueError` on anything else."""
    token = TOKEN.match(data, pos)
    if token is None or depth > MAX_DEPTH:
        raise ValueError("not a PDF object")
    kind, end = token.lastgroup, token.end()
    if kind == "dict":
        entries: dict[bytes, object] = {}
        while (close := TOKEN.match(data, end)) and close.lastgroup != "end_dict":
            key, end = parse_object(data, end, depth + 1)
            if not isinstance(key, Name):
                raise ValueError("a dictionary key is not a name")
            entries[key], end = parse_object(data, end, depth + 1)
        if close is None:
            raise ValueError("unterminated dictionary")
        return entries, close.end()
    if kind == "array":
        items = []
        while (close := TOKEN.match(data, end)) and close.lastgroup != "end_array":
            item, end = parse_object(data, end, depth + 1)
            items.append(item)
        if close is None:
            raise ValueError("unterminated array")
        return items, close.end()
    if kind == "ref":
        return Ref(int(token["number"]), int(token["generation"])), end
    if kind == "numeric":
        value = token["numeric"]
        return (float(value) if b"." in value else int(value)), end
    if kind == "name":
        return Name(name_of(token["name"])), end
    if kind == "literal":
        string_end = literal_end(data, token.start("literal"))
        return data[token.end() : string_end - 1], string_end
    if kind == "hex":
        return token["hex"], end
    if kind == "keyword" and token["keyword"] in KEYWORDS:
        return KEYWORDS[token["keyword"]], end
    raise ValueError("not a PDF object")


class Document:
    """The objects of PDF bytes, found by their headers (`10 0 obj`)."""

    def __init__(self, data: bytes) -> None:
        self.data = data
        headers = list(OBJECT_HEADER.finditer(data))
        self.starts = [header.start() for header in headers]
        self.headers = headers
        # The last definition of an object wins, as in an incremental update.
        self.objects = {(int(h[1]), int(h[2])): h.end() for h in headers}

    def resolve(self, value: object, depth: int = 0) -> object:
        """`value`, or the object an indirect reference points to; `ValueError` when it
        cannot be found (an object in an object stream, a missing one)."""
        while isinstance(value, Ref):
            pos = self.objects.get((value.number, value.generation))
            if pos is None or depth > MAX_DEPTH:
                raise ValueError("unresolved reference")
            value, _ = parse_object(self.data, pos)
            depth += 1
        return value

    def stream_dictionary(self, keyword: int) -> dict[bytes, object] | None:
        """The dictionary of the stream whose keyword starts at `keyword`: the object
        before it, when it is a dictionary that ends right at the keyword."""
        last = bisect.bisect_right(self.starts, keyword) - 1
        for index in range(last, max(-1, last - HEADER_TRIES), -1):
            try:
                value, end = parse_object(self.data, self.headers[index].end())
            except ValueError:
                continue
            between = self.data[end:keyword]
            if isinstance(value, dict) and not between.strip(SPACE):
                return value
        return None


def lzw_decode(data: bytes, early_change: int = 1) -> bytes:
    """LZW as PDF writes it: 9 to 12-bit codes, a clear code, and the code size growing
    one code early unless `/EarlyChange 0`."""
    out = bytearray()
    table = [bytes([i]) for i in range(256)] + [b"", b""]
    width, buffer, buffered, previous = 9, 0, 0, b""
    for byte in data:
        buffer, buffered = (buffer << 8) | byte, buffered + 8
        while buffered >= width:
            buffered -= width
            code = buffer >> buffered
            buffer &= (1 << buffered) - 1
            if code == 256:
                del table[258:]
                width, previous = 9, b""
                continue
            if code == 257:
                return bytes(out)
            if code < len(table):
                entry = table[code]
            elif code == len(table) and previous:
                entry = previous + previous[:1]
            else:
                raise Unread(bytes(out))
            out += entry
            if previous:
                table.append(previous + entry[:1])
            previous = entry
            if len(table) + early_change >= 1 << width and width < 12:
                width += 1
    # No end-of-data code: the data stops short.
    raise Unread(bytes(out))


def run_length_decode(data: bytes) -> bytes:
    """Run-length data, which must end with its end-of-data byte (128)."""
    out = bytearray()
    pos = 0
    while pos < len(data) and (length := data[pos]) != 128:
        if length < 128:
            out += data[pos + 1 : pos + 2 + length]
            pos += 2 + length
        else:
            out += data[pos + 1 : pos + 2] * (257 - length)
            pos += 2
    if pos >= len(data):
        raise Unread(bytes(out))
    return bytes(out)


def flate_decode(data: bytes) -> bytes:
    """Zlib data, which must end: a truncated or corrupt stream is unread (with what
    came before the failure)."""
    inflater = zlib.decompressobj()
    try:
        out = inflater.decompress(data)
    except zlib.error:
        raise Unread() from None
    if not inflater.eof:
        raise Unread(out)
    return out


def ascii_hex_decode(data: bytes) -> bytes:
    """ASCIIHex data, which must end with ">"."""
    body, end, _ = data.partition(b">")
    digits = re.sub(rb"[\s\x00]", b"", body)
    try:
        out = bytes.fromhex((digits + b"0" * (len(digits) % 2)).decode("ascii"))
    except (UnicodeDecodeError, ValueError):
        raise Unread() from None
    if not end:
        raise Unread(out)
    return out


def ascii85_decode(data: bytes) -> bytes:
    """ASCII85 data, which must end with "~>"."""
    body, end, _ = data.partition(b"~>")
    try:
        out = base64.a85decode(body.strip().removeprefix(b"<~"), ignorechars=b" \t\n\r\v\f\x00")
    except ValueError:
        raise Unread() from None
    if not end:
        raise Unread(out)
    return out


def number(parameters: dict[bytes, object], key: bytes, default: int) -> int:
    value = parameters.get(key, default)
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise Unread()
    return value


def unpredict(data: bytes, parameters: dict[bytes, object]) -> bytes:
    """The data before a Flate or LZW predictor (TIFF 2 at 8 bits, PNG 10 to 15)."""
    predictor = number(parameters, b"Predictor", 1)
    if predictor == 1:
        return data
    colors = number(parameters, b"Colors", 1)
    bits = number(parameters, b"BitsPerComponent", 8)
    columns = number(parameters, b"Columns", 1)
    row = (colors * bits * columns + 7) // 8
    pixel = max(1, (colors * bits + 7) // 8)
    if not 0 < row <= len(data) or bits not in (1, 2, 4, 8, 16):
        raise Unread(data)
    out = bytearray()
    if predictor == 2 and bits == 8:
        for start in range(0, len(data), row):
            line = bytearray(data[start : start + row])
            for i in range(pixel, len(line)):
                line[i] = (line[i] + line[i - pixel]) & 0xFF
            out += line
        return bytes(out)
    if predictor < 10:
        raise Unread(data)
    above = bytearray(row)
    for start in range(0, len(data), row + 1):
        kind, line = data[start], bytearray(data[start + 1 : start + 1 + row])
        for i, value in enumerate(line):
            left = line[i - pixel] if i >= pixel else 0
            up = above[i] if i < len(above) else 0
            corner = above[i - pixel] if i >= pixel and i - pixel < len(above) else 0
            if kind == 1:
                value += left
            elif kind == 2:
                value += up
            elif kind == 3:
                value += (left + up) // 2
            elif kind == 4:
                guess = left + up - corner
                distances = (abs(guess - left), abs(guess - up), abs(guess - corner))
                value += (left, up, corner)[distances.index(min(distances))]
            elif kind != 0:
                raise Unread(bytes(out))
            line[i] = value & 0xFF
        out += line
        above = line
    return bytes(out)


Decoder = Callable[[bytes, dict[bytes, object]], bytes]
# The filters a stream's data can be decoded with, by full and abbreviated name.
DECODERS: dict[bytes, Decoder] = {
    **dict.fromkeys((b"FlateDecode", b"Fl"), lambda d, p: unpredict(flate_decode(d), p)),
    **dict.fromkeys(
        (b"LZWDecode", b"LZW"),
        lambda d, p: unpredict(lzw_decode(d, number(p, b"EarlyChange", 1)), p),
    ),
    **dict.fromkeys((b"ASCIIHexDecode", b"AHx"), lambda d, p: ascii_hex_decode(d)),
    **dict.fromkeys((b"ASCII85Decode", b"A85"), lambda d, p: ascii85_decode(d)),
    **dict.fromkeys((b"RunLengthDecode", b"RL"), lambda d, p: run_length_decode(d)),
}
# Image codecs: their streams are images, which `pdfimages` extracts for OCR.
IMAGE_FILTERS = frozenset(b"DCTDecode DCT JPXDecode JBIG2Decode CCITTFaxDecode CCF".split())


def as_list(value: object) -> list[object]:
    return value if isinstance(value, list) else [value]


def decode_stream(document: Document, entries: dict[bytes, object], body: bytes) -> bytes | None:
    """`body` through the stream's filters; None when they end in an image codec.
    Raises `Unread`."""
    try:
        filters = [document.resolve(f) for f in as_list(document.resolve(entries.get(b"Filter")))]
        parameters = as_list(document.resolve(entries.get(b"DecodeParms")))
        parameters = [document.resolve(p) for p in parameters]
    except ValueError:
        raise Unread() from None
    parameters += [None] * (len(filters) - len(parameters))
    if entries.get(b"Subtype") == b"Image":
        # An image's samples are pixels, which the image channel reads: they are
        # inflated, without undoing a predictor, which would only cost time.
        parameters = [None] * len(filters)
    for filter_name, given in zip(filters, parameters, strict=False):
        if not isinstance(filter_name, Name) or not isinstance(given, dict | None):
            raise Unread()
        if filter_name in IMAGE_FILTERS:
            return None
        identity = given is None or given.get(b"Name", b"Identity") == b"Identity"
        if filter_name == b"Crypt" and identity:
            continue
        decoder = DECODERS.get(filter_name)
        if decoder is None:
            raise Unread()
        body = decoder(body, given or {})
    return body


def stream_end(document: Document, entries: dict[bytes, object] | None, start: int) -> int:
    """Where a stream's data ends: after `/Length` bytes when `endstream` follows them,
    so the data may hold `endstream` itself (Codex C0-16); at the first `endstream`
    otherwise, as readers do with a wrong length; -1 if there is none."""
    if entries is not None:
        try:
            length = document.resolve(entries.get(b"Length"))
        except ValueError:
            length = None
        size = len(document.data) - start
        whole = isinstance(length, int) and not isinstance(length, bool) and 0 <= length <= size
        if whole and STREAM_END.match(document.data, start + length):
            return start + length
    return document.data.find(b"endstream", start)


def decoded_streams(data: bytes) -> tuple[list[bytes], int]:
    """Every stream of `data` decoded, and how many could not be. A stream with no
    filter, or whose dictionary is not found, is still tried as zlib data, silently:
    some writers leave the filter out. But a dictionary that cannot be read (nested too
    deep, malformed) before data that is no zlib stream is an unread stream. A stream the
    file is cut in, with no `endstream`, runs to the end of the file; unless it decodes
    whole, it is unread too (Antigravity round 1)."""
    document = Document(data)
    decoded, unread = [], 0
    pos = 0
    while keyword := STREAM_START.search(data, pos):
        start = keyword.end()
        entries = document.stream_dictionary(keyword.start())
        end = stream_end(document, entries, start)
        cut = end < 0
        if cut:
            end = len(data)
        pos = end
        body = data[start:end]
        if entries is None or entries.get(b"Filter") is None:
            try:
                decoded.append(flate_decode(body))
            except Unread as e:
                decoded.extend([e.partial] if e.partial else [])
                unread += cut or (
                    entries is None and data[: keyword.start()].rstrip(SPACE).endswith(b">>")
                )
            continue
        try:
            if (stream := decode_stream(document, entries, body)) is not None:
                decoded.append(stream)
        except Unread as e:
            unread += 1
            decoded.extend([e.partial] if e.partial else [])
    return decoded, unread
