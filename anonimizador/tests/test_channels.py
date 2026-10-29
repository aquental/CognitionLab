import struct
import time
import zlib

from anonimizador.channels import image_metadata, png_size, raw_texts, unescape_literal, worth_ocr
from anonimizador.terms import Term, count_patterns

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


def test_jpeg_fill_bytes_and_standalone_markers_are_skipped():
    # Regression (Grok G0-12): a fill byte (0xFF) before a marker was read as a marker
    # with a length, so the walker jumped over the EXIF segment.
    body = b"Exif\x00\x00Artist=Fulano Ficticio"
    app1 = b"\xe1" + struct.pack(">H", len(body) + 2) + body
    for fill in (b"\xff", b"\xff\xff\xff"):
        jpeg = b"\xff\xd8" + fill + app1 + b"\xff\xda"
        assert max(NAME.count(t) for t in image_metadata(jpeg)) == 1, fill
    restart = b"\xff\xd8\xff\xd0\xff" + app1 + b"\xff\xda"
    assert max(NAME.count(t) for t in image_metadata(restart)) == 1


def test_pdf_hex_and_escaped_strings_are_decoded_in_raw_bytes():
    # Regression (Grok G0-14): a name stored as a UTF-16 hex string or with an octal
    # escape held none of its letters in the bytes, so a leftover one counted 0.
    short = Term("Zuleo")

    def count(data: bytes) -> int:
        return max(short.count(t) for t in raw_texts(data))

    assert count(b"<< /Author <FEFF005A0075006C0065006F> >>") == 1
    assert count(b"<< /Author <FEFF 005A 0075 006C\n0065 006F> >>") == 1
    assert count(b"<< /Author <FEFF005A0075006C00650061> >>") == 0
    assert count(b"<< /Author (Zul\\351o) >>") == 1
    assert count(b"<< /Author (\\376\\377\\000Z\\000u\\000l\\000e\\000o) >>") == 1
    assert count(b"<< /Author (Zul\\351a) >>") == 0
    stream = zlib.compress(b"BT <FEFF005A0075006C0065006F> Tj ET")
    assert count(b"1 0 obj << >> stream\n" + stream + b"endstream\n") == 1


def test_literal_escapes_follow_the_pdf_rules():
    assert unescape_literal(rb"a\8b\(c\)\\d\101\0e\n") == b"a8b(c)\\dA\x00e\n"
    assert unescape_literal(b"split\\\nline") == b"splitline"


def test_a_thin_image_is_read_and_an_icon_is_not():
    # Regression (Grok G0-17): both sides had to reach 32 pixels, so a strip with a
    # line of text was never read.
    assert worth_ocr((400, 20))
    assert worth_ocr((20, 400))
    assert not worth_ocr((16, 16))
    assert not worth_ocr((31, 31))


def test_a_word_drawn_glyph_by_glyph_is_whole_in_raw_bytes():
    # Regression (Grok G0-19): content streams often draw a word as several strings,
    # and each piece was its own word, so a term under 6 letters counted 0.
    def count(term: str, data: bytes) -> int:
        return max(Term(term).count(t) for t in raw_texts(data))

    kerned = b"BT [(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ ET"
    assert count("Zuleo", b"(Z)(u)(l)(e)(o)") == 1
    assert count("Zuleo", kerned) == 1
    assert count("Zuleo", b"BT [<5A> -2 <75> -2 <6C> <65> <6F>] TJ ET") == 1
    assert count("Zuleo", b"BT (Zu) Tj 4 0 Td (leo) Tj ET") == 1
    assert count("Zuleo", b"(Zu\nleo)") == 1
    stream = zlib.compress(kerned)
    assert count("Zuleo", b"1 0 obj << >> stream\n" + stream + b"endstream\n") == 1
    assert count("Zulea", kerned) == 0
    assert count("Rosa", b"[(R) 5 (o) 5 (s) 5 (a)] TJ") == 1
    assert count("Fulano", b"[(F)(u)(l)(a)(n)(o)] TJ") == 1
    # Strings of separate drawings stay separate words.
    assert count("Zuleo", b"(Zu) Tj ET BT /F1 9 Tf (leo) Tj") == 0


def test_a_literal_with_balanced_parentheses_is_one_string():
    # Regression (Grok G0-20): the string stopped at the first parenthesis, so the
    # escapes before it were never decoded.
    def count(term: str, data: bytes) -> int:
        return max(Term(term).count(t) for t in raw_texts(data))

    assert count("Zuleição", b"<< /Author (Zulei\\347\\343o (mae)) >>") == 1
    assert count("Zuleição", b"<< /Author (Zulei\\347\\343a (mae)) >>") == 0
    assert count("Fulano", b"<< /Author (Ful\\343no (Dr)) >>") == 1
    utf16 = b"(\\376\\377\\000Z\\000u\\000l\\000e\\000o (Dr))"
    assert count("Zuleo", b"<< /Author " + utf16 + b" >>") == 1


def test_a_long_run_without_at_sign_is_fast_to_search():
    # The e-mail pattern was quadratic on a long run of word characters, which decoded
    # binary strings hold (found while fixing Grok G0-21).
    started = time.perf_counter()
    assert count_patterns(["a" * 200_000 + " b@c.com"]) == {"email": 1}
    assert time.perf_counter() - started < 2
