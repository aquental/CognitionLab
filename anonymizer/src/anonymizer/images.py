"""PNG and JPEG structure: chunks, compressed text, size, and whether the bytes are a
whole image (Codex C0-9)."""

from __future__ import annotations

import itertools
import struct
import zlib
from collections.abc import Iterator

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def png_size(path: str) -> tuple[int, int]:
    with open(path, "rb") as f:
        head = f.read(24)
    if len(head) == 24 and head[:8] == PNG_SIGNATURE:
        width, height = struct.unpack(">II", head[16:24])
        return width, height
    return 0, 0


def png_compressed_text(kind: bytes, body: bytes) -> bytes | None:
    """The inflated text of a zTXt chunk or a compressed iTXt chunk; None for an iTXt
    chunk stored as it is. Raises `ValueError` when it cannot be inflated."""
    keyword_end = body.find(b"\x00")
    if keyword_end < 0:
        raise ValueError("no keyword")
    if kind == b"zTXt":
        method, text = body[keyword_end + 1 : keyword_end + 2], body[keyword_end + 2 :]
    else:
        flag, method = (
            body[keyword_end + 1 : keyword_end + 2],
            body[keyword_end + 2 : keyword_end + 3],
        )
        language_end = body.find(b"\x00", keyword_end + 3)
        translated_end = body.find(b"\x00", language_end + 1) if language_end >= 0 else -1
        if translated_end < 0:
            raise ValueError("no translated keyword")
        if flag == b"\x00":
            return None
        text = body[translated_end + 1 :]
    if method != b"\x00":
        raise ValueError("unknown compression")
    inflater = zlib.decompressobj()
    try:
        out = inflater.decompress(text)
    except zlib.error:
        raise ValueError("bad zlib data") from None
    if not inflater.eof:
        raise ValueError("truncated zlib data")
    return out


def png_text(data: bytes) -> tuple[list[bytes], int]:
    """The inflated zTXt and compressed iTXt chunks of a PNG (tEXt is already raw), and
    how many could not be inflated: an unread place, not an empty one (Codex C0-17)."""
    texts, unread = [], 0
    for kind, body in png_chunks(data):
        if kind in (b"zTXt", b"iTXt"):
            try:
                if (text := png_compressed_text(kind, body)) is not None:
                    texts.append(text)
            except ValueError:
                unread += 1
    return texts, unread


def png_chunks(data: bytes) -> Iterator[tuple[bytes, bytes]]:
    if data[:8] != PNG_SIGNATURE:
        return
    pos = 8
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        yield kind, data[pos + 8 : pos + 8 + length]
        pos += 12 + length


# Channels per PNG color type, and the bit depths each allows.
PNG_CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
PNG_DEPTHS = {0: (1, 2, 4, 8, 16), 2: (8, 16), 3: (1, 2, 4, 8), 4: (8, 16), 6: (8, 16)}
# The seven passes of Adam7 interlacing: first column and row, column and row steps.
ADAM7 = (
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
)


def png_rows(width: int, height: int, color: int, depth: int, interlaced: bool) -> list[int]:
    """The size of each row of a PNG's filtered pixel data, pass by pass, its filter
    byte included."""
    passes = ADAM7 if interlaced else ((0, 0, 1, 1),)
    rows = []
    for x, y, dx, dy in passes:
        columns, count = -(-(width - x) // dx), -(-(height - y) // dy)
        if columns > 0 and count > 0:
            rows += [1 + -(-(columns * PNG_CHANNELS[color] * depth) // 8)] * count
    return rows


def png_is_whole(data: bytes) -> bool:
    """A PNG signature; chunks within the bytes, with their checksums; a valid header
    first and an end chunk last; a palette when the colors need one; and image data
    that inflates to exactly the pixels the header gives, each row with a known filter
    (Codex C0-9)."""
    if data[:8] != PNG_SIGNATURE:
        return False
    chunks, pos = [], 8
    while pos + 12 <= len(data) and (not chunks or chunks[-1][0] != b"IEND"):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        end = pos + 12 + length
        if end > len(data):
            return False
        body = data[pos + 8 : end - 4]
        if zlib.crc32(kind + body) != struct.unpack(">I", data[end - 4 : end])[0]:
            return False
        chunks.append((kind, body))
        pos = end
    kinds = [kind for kind, _ in chunks]
    if not kinds or kinds[0] != b"IHDR" or kinds[-1] != b"IEND" or len(chunks[0][1]) != 13:
        return False
    width, height, depth, color, method, filtering, interlace = struct.unpack(
        ">IIBBBBB", chunks[0][1]
    )
    if not (width and height) or depth not in PNG_DEPTHS.get(color, ()):
        return False
    if method or filtering or interlace > 1 or (color == 3 and b"PLTE" not in kinds):
        return False
    rows = png_rows(width, height, color, depth, bool(interlace))
    expected = sum(rows)
    inflater = zlib.decompressobj()
    try:
        pixels = inflater.decompress(b"".join(b for k, b in chunks if k == b"IDAT"), expected + 1)
    except zlib.error:
        return False
    if not inflater.eof or len(pixels) != expected:
        return False
    # Each row starts with its filter type, 0 to 4.
    starts = itertools.accumulate(rows[:-1], initial=0)
    return all(pixels[start] <= 4 for start in starts)


def jpeg_is_whole(data: bytes) -> bool:
    """A JPEG start; segments within the bytes; a frame header with a width and its
    components, and a scan header with its components, each of its own length; and an
    end after the scan."""
    if data[:2] != b"\xff\xd8":
        return False
    pos, frame = 2, False
    while pos + 4 <= len(data) and data[pos] == 0xFF:
        marker = data[pos + 1]
        if marker == 0xFF:
            pos += 1
            continue
        if marker == 0x01 or 0xD0 <= marker <= 0xD7:
            pos += 2
            continue
        (length,) = struct.unpack(">H", data[pos + 2 : pos + 4])
        segment = data[pos + 4 : pos + 2 + length]
        if length < 2 or pos + 2 + length > len(data):
            return False
        if marker == 0xDA:
            scan = len(segment) >= 1 and 1 <= segment[0] <= 4 and length == 6 + 2 * segment[0]
            return frame and scan and b"\xff\xd9" in data[pos + 2 + length :]
        if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
            if len(segment) < 6 or not segment[5] or length != 8 + 3 * segment[5]:
                return False
            frame = struct.unpack(">H", segment[3:5])[0] > 0
        pos += 2 + length
    return False
