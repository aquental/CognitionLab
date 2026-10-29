import struct
import zlib

from anonimizador.channels import image_metadata, png_size, raw_texts
from anonimizador.terms import Term

NAME = Term("Fulano Ficticio")


def png(chunks: list[tuple[bytes, bytes]]) -> bytes:
    out = b"\x89PNG\r\n\x1a\n"
    for kind, body in chunks:
        out += struct.pack(">I", len(body)) + kind + body
        out += struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    return out


IHDR = (b"IHDR", struct.pack(">IIBBBBB", 40, 30, 8, 2, 0, 0, 0))


def test_png_text_metadata_is_read_plain_and_compressed(tmp_path):
    plain = png([IHDR, (b"tEXt", b"Author\x00Fulano Ficticio"), (b"IEND", b"")])
    packed = png(
        [IHDR, (b"zTXt", b"Comment\x00\x00" + zlib.compress(b"Fulano Ficticio")), (b"IEND", b"")]
    )
    clean = png([IHDR, (b"IEND", b"")])
    assert max(NAME.count(t) for t in image_metadata(plain)) == 1
    assert max(NAME.count(t) for t in image_metadata(packed)) == 1
    assert image_metadata(clean) == []
    path = tmp_path / "x.png"
    path.write_bytes(clean)
    assert png_size(str(path)) == (40, 30)


def test_jpeg_metadata_segments_are_read_until_the_scan():
    def segment(marker: int, body: bytes) -> bytes:
        return bytes([0xFF, marker]) + struct.pack(">H", len(body) + 2) + body

    jpeg = (
        b"\xff\xd8"
        + segment(0xE1, b"Exif\x00\x00Artist=Fulano Ficticio")
        + segment(0xFE, b"comment")
        + segment(0xDA, b"\x00")
        + b"Fulano Ficticio in pixel data is not metadata"
    )
    texts = image_metadata(jpeg)
    assert max(NAME.count(t) for t in texts) == 1
    assert sum(NAME.count(t) for t in texts) == 2  # utf-8 and latin-1 views of one block


def test_raw_bytes_include_inflated_streams_and_utf16():
    stream = zlib.compress(b"BT (Fulano Ficticio) Tj ET")
    pdf = b"%PDF-1.4\n1 0 obj << /Length 9 >> stream\n" + stream + b"endstream\n"
    assert max(NAME.count(t) for t in raw_texts(pdf)) == 1
    utf16 = "﻿Fulano Ficticio".encode("utf-16-be")
    assert max(NAME.count(t) for t in raw_texts(b"(" + utf16 + b")")) == 1


def test_a_short_name_in_utf16_metadata_is_a_whole_word():
    # Regression (Grok G0-8): UTF-16 put a NUL between letters, so a term shorter than
    # the compact length was searched as separate one-letter words and counted 0.
    def jpeg(text: str, encoding: str) -> bytes:
        # NUL-terminated ASCII on both sides: each byte order glues a neighbor on.
        body = b"Exif\x00\x00UserComment\x00" + text.encode(encoding) + b"Make\x00"
        return b"\xff\xd8\xff\xe1" + struct.pack(">H", len(body) + 2) + body + b"\xff\xda"

    short = Term("Zuleo")
    for encoding in ("utf-16-le", "utf-16-be"):
        assert max(short.count(t) for t in image_metadata(jpeg("Zuleo", encoding))) == 1
        assert max(short.count(t) for t in image_metadata(jpeg("Zulea", encoding))) == 0
        assert max(short.count(t) for t in image_metadata(jpeg("Zuleotti Ito", encoding))) == 0
        # Each occurrence counts once, whatever byte order reads it.
        assert max(short.count(t) for t in image_metadata(jpeg("Zuleo Ito", encoding))) == 1
    # A neighbor that both byte orders glue on at the start, then the end.
    glued = b"\xff\xd8\xff\xe1"
    body = b"Exif\x00\x00t\x00" + "Zuleo".encode("utf-16-le") + b"Make\x00"
    glued += struct.pack(">H", len(body) + 2) + body + b"\xff\xda"
    assert max(short.count(t) for t in image_metadata(glued)) == 1
    body = b"Exif\x00\x00" + "Zuleo".encode("utf-16-le") + b"M\x00\x00"
    glued = b"\xff\xd8\xff\xe1" + struct.pack(">H", len(body) + 2) + body + b"\xff\xda"
    assert max(short.count(t) for t in image_metadata(glued)) == 1
    # EXIF UserComment: an 8-byte charset code, then UTF-16.
    body = b"Exif\x00\x00UNICODE\x00" + "Zuleo".encode("utf-16-le") + b"\x00\x00Make\x00"
    exif = b"\xff\xd8\xff\xe1" + struct.pack(">H", len(body) + 2) + body + b"\xff\xda"
    assert max(short.count(t) for t in image_metadata(exif)) == 1
