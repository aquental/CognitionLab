"""Where an identifier can survive anonymization, and how each place is read.

Every reader returns text to search, never writes outside a temporary directory, and
runs only local tools (poppler, tesseract).
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

# The places searched, in report order.
CHANNELS = ("name", "metadata", "text", "ocr", "images", "raw")
# Channels whose text a document shows or declares (searched for patterns).
PATTERN_CHANNELS = ("text", "ocr", "images", "metadata")
# Images smaller than this on either side are icons or masks, not readable text.
MIN_OCR_SIDE = 32
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

Texts = dict[str, list[str]]


def run(args: list[str], timeout: int = 600) -> str:
    """Stdout of a tool, or "" when it fails (a failure is a missing reading, which the
    review must cover, not a crash)."""
    result = subprocess.run(args, capture_output=True, timeout=timeout, check=False)
    return result.stdout.decode("utf-8", "replace") if result.returncode == 0 else ""


def ocr(image: str, lang: str) -> str:
    return run(["tesseract", image, "-", "-l", lang], timeout=300)


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


def decode_both(blobs: list[bytes]) -> list[str]:
    texts = []
    for blob in blobs:
        texts.append(blob.decode("utf-8", "ignore"))
        texts.append(blob.decode("latin-1"))
    return texts


def raw_texts(data: bytes) -> list[str]:
    """The file bytes and every zlib stream in it, decoded the ways text is stored:
    leftover incremental updates, uncompressed content streams, XMP packets, PNG text."""
    blobs = [data]
    for match in re.finditer(rb"stream\r?\n", data):
        start = match.end()
        end = data.find(b"endstream", start)
        if end < 0:
            continue
        with contextlib.suppress(zlib.error):
            blobs.append(zlib.decompressobj().decompress(data[start:end]))
    blobs.extend(png_text_chunks(data))
    texts = decode_both(blobs)
    texts.extend(
        blob.decode("utf-16-be", "ignore") for blob in blobs if blob.count(b"\x00") > len(blob) // 4
    )
    return texts


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
            if marker == 0xDA:  # start of scan: no metadata after this
                break
            (length,) = struct.unpack(">H", data[pos + 2 : pos + 4])
            if 0xE1 <= marker <= 0xEF or marker == 0xFE:
                blobs.append(data[pos + 4 : pos + 2 + length])
            pos += 2 + length
    return decode_both(blobs)


def read_pdf(path: str, tmp: str, lang: str, dpi: int, use_ocr: bool, pool: Executor) -> Texts:
    """Every channel of a PDF. Page renders and embedded images go to `tmp`."""
    with open(path, "rb") as f:
        raw = raw_texts(f.read())
    texts: Texts = {
        "name": [os.path.basename(path)],
        "metadata": [
            run(["pdfinfo", "-enc", "UTF-8", path]),
            run(["pdfinfo", "-meta", path]),
        ],
        "text": [run(["pdftotext", "-enc", "UTF-8", "-layout", path, "-"])],
        "ocr": [],
        "images": [],
        "raw": raw,
    }
    if use_ocr:
        run(["pdftoppm", "-r", str(dpi), "-png", path, os.path.join(tmp, "page")])
        pages = sorted(os.path.join(tmp, p) for p in os.listdir(tmp) if p.startswith("page"))
        texts["ocr"] = list(pool.map(lambda p: ocr(p, lang), pages))
        run(["pdfimages", "-png", path, os.path.join(tmp, "img")])
        images = sorted(
            os.path.join(tmp, p)
            for p in os.listdir(tmp)
            if p.startswith("img") and min(png_size(os.path.join(tmp, p))) >= MIN_OCR_SIDE
        )
        texts["images"] = list(pool.map(lambda p: ocr(p, lang), images))
    return texts


def read_image(path: str, lang: str, use_ocr: bool) -> Texts:
    """Every channel of a PNG or JPEG file."""
    with open(path, "rb") as f:
        data = f.read()
    return {
        "name": [os.path.basename(path)],
        "metadata": image_metadata(data),
        "text": [],
        "ocr": [ocr(path, lang)] if use_ocr else [],
        "images": [],
        "raw": raw_texts(data),
    }
