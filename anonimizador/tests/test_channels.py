import struct
import time
import zlib

from anonimizador.channels import (
    AMBIGUOUS,
    image_metadata,
    mixed_readings,
    pdf_string_views,
    png_size,
    raw_readings,
    raw_texts,
    unescape_literal,
    worth_ocr,
)
from anonimizador.terms import Term, count_patterns

NAME = Term("Fulano Ficticio")


def png(chunks: list[tuple[bytes, bytes]]) -> bytes:
    out = b"\x89PNG\r\n\x1a\n"
    for kind, body in chunks:
        out += struct.pack(">I", len(body)) + kind + body
        out += struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    return out


IHDR = (b"IHDR", struct.pack(">IIBBBBB", 40, 30, 8, 2, 0, 0, 0))


def raw_count(term: str, data: bytes) -> int:
    """The raw count `check_file` gives `term` in `data`."""
    texts, _, word_texts = raw_readings(data)
    found = Term(term)
    return max(found.count(t) for t in [*texts, *(word_texts if found.whole_word else [])])


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
    assert raw_count("Zuleo", b"<< /Author <FEFF005A0075006C0065006F> >>") == 1
    assert raw_count("Zuleo", b"<< /Author <FEFF 005A 0075 006C\n0065 006F> >>") == 1
    assert raw_count("Zuleo", b"<< /Author <FEFF005A0075006C00650061> >>") == 0
    assert raw_count("Zuleo", b"<< /Author (Zul\\351o) >>") == 1
    assert raw_count("Zuleo", b"<< /Author (\\376\\377\\000Z\\000u\\000l\\000e\\000o) >>") == 1
    assert raw_count("Zuleo", b"<< /Author (Zul\\351a) >>") == 0
    stream = zlib.compress(b"BT <FEFF005A0075006C0065006F> Tj ET")
    assert raw_count("Zuleo", b"1 0 obj << >> stream\n" + stream + b"endstream\n") == 1


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

    kerned = b"BT [(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ ET"
    assert raw_count("Zuleo", b"(Z)(u)(l)(e)(o)") == 1
    assert raw_count("Zuleo", kerned) == 1
    assert raw_count("Zuleo", b"BT [<5A> -2 <75> -2 <6C> <65> <6F>] TJ ET") == 1
    assert raw_count("Zuleo", b"BT (Zu) Tj 4 0 Td (leo) Tj ET") == 1
    assert raw_count("Zuleo", b"(Zu\nleo)") == 1
    stream = zlib.compress(kerned)
    assert raw_count("Zuleo", b"1 0 obj << >> stream\n" + stream + b"endstream\n") == 1
    assert raw_count("Zulea", kerned) == 0
    assert raw_count("Rosa", b"[(R) 5 (o) 5 (s) 5 (a)] TJ") == 1
    assert raw_count("Fulano", b"[(F)(u)(l)(a)(n)(o)] TJ") == 1
    # Strings of another text object may still be the same word (Grok G0-38).
    assert raw_count("Zuleo", b"(Zu) Tj ET BT /F1 9 Tf (leo) Tj") == 1


def test_a_literal_with_balanced_parentheses_is_one_string():
    # Regression (Grok G0-20): the string stopped at the first parenthesis, so the
    # escapes before it were never decoded.

    assert raw_count("Zuleição", b"<< /Author (Zulei\\347\\343o (mae)) >>") == 1
    assert raw_count("Zuleição", b"<< /Author (Zulei\\347\\343a (mae)) >>") == 0
    assert raw_count("Fulano", b"<< /Author (Ful\\343no (Dr)) >>") == 1
    utf16 = b"(\\376\\377\\000Z\\000u\\000l\\000e\\000o (Dr))"
    assert raw_count("Zuleo", b"<< /Author " + utf16 + b" >>") == 1


def test_a_long_run_without_at_sign_is_fast_to_search():
    # The e-mail pattern was quadratic on a long run of word characters, which decoded
    # binary strings hold (found while fixing Grok G0-21).
    started = time.perf_counter()
    assert count_patterns(["a" * 200_000 + " b@c.com"]) == {"email": 1}
    assert time.perf_counter() - started < 2


def test_word_spaces_and_line_moves_split_a_drawn_text():
    # Regression (Grok G0-22): every piece of a text object was glued, so a short name
    # drawn glyph by glyph next to another word was never a whole word.

    name = b"(Z) -2 (u) -2 (l) -2 (e) -2 (o)"
    other = b"(A) -2 (s) -2 (s) -2 (u) -2 (n) -2 (t) -2 (o)"
    spaced = b"BT [" + name + b" -300 " + other + b"] TJ ET"
    next_line = b"BT [" + name + b"] TJ 0 -20 Td [" + other + b"] TJ ET"
    for data in (spaced, next_line):
        assert raw_count("Zuleo", data) == 1, data
        assert raw_count("Zulea", data) == 0, data
        assert raw_count("Assunto", data) == 1, data
        stream = b"1 0 obj << >> stream\n" + zlib.compress(data) + b"endstream\n"
        assert raw_count("Zuleo", stream) == 1, data


def test_text_state_and_same_baseline_keep_a_word_whole():
    # Regression (Grok G0-24): Tm, Tc, and other text-state operators ended the word.

    same_line = b"BT 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 90 700 Tm (leo) Tj ET"
    assert raw_count("Zuleo", same_line) == 1
    assert raw_count("Zulea", same_line) == 0
    assert raw_count("Zuleo", b"BT (Zu) Tj 0.1 Tc (leo) Tj ET") == 1
    assert raw_count("Zuleo", b"BT (Zu) Tj 1 0 0 rg (leo) Tj ET") == 1
    # Every move is ambiguous, a new line too: counting is the safe error (the earlier
    # refutations for another baseline and for ' and " gave way to this, Grok G0-32).
    for moved in (
        b"BT 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 72 680 Tm (leo) Tj ET",
        b'BT 0 0 (Zu) " 0 0 (leo) " ET',
        b"BT (Zu) Tj (leo) ' (Assunto) Tj ET",
    ):
        assert raw_count("Zuleo", moved) == 1, moved
    # Only bytes that are no content-stream syntax (another stream's binary data)
    # separate strings for sure.
    assert raw_count("Zuleo", b"BT (Zu) Tj ET \x00\x9c\xff BT (leo) Tj ET") == 0


def test_a_number_drawn_glyph_by_glyph_is_a_pattern_hint():
    # Regression (Grok G0-23): patterns saw one string per line only.
    def hints(data: bytes) -> dict[str, int]:
        views = pdf_string_views([data])
        return count_patterns(views.for_patterns, [views.mixed])

    for data in (
        b"BT [(123)(.)(456)(.)(789)(-)(09)] TJ ET",
        b"BT (123) Tj (.) Tj (456) Tj (.) Tj (789) Tj (-) Tj (09) Tj ET",
        b"BT [(12) -2 (3.456.789-09)] TJ ET",
    ):
        assert hints(data) == {"cpf": 1}, data
    assert hints(b"BT [(123)(.)(456)(.)(789)(-)(0)] TJ ET") == {}


def glyphs(word: str) -> bytes:
    """A word drawn glyph by glyph in a TJ array."""
    return b"[" + b" -2 ".join(b"(" + bytes([c]) + b")" for c in word.encode()) + b"] TJ"


def test_a_horizontal_move_is_read_both_as_a_break_and_as_glyph_positioning():
    # Regression (Grok G0-25, G0-26): a horizontal Td or a Tm on the same baseline
    # always joined, so two words drawn glyph by glyph became one. Without the glyph
    # widths a move can be either, so both readings are searched.

    words = glyphs("Zuleo"), glyphs("Assunto")
    td = b"BT /F1 12 Tf 72 700 Td " + words[0] + b" 20 0 Td " + words[1] + b" ET"
    tm = b"BT 1 0 0 1 72 700 Tm " + words[0] + b" 1 0 0 1 200 700 Tm " + words[1] + b" ET"
    for data in (td, tm):
        assert raw_count("Zuleo", data) == 1, data
        assert raw_count("Assunto", data) == 1, data
        assert raw_count("Zulea", data) == 0, data
        stream = b"1 0 obj << >> stream\n" + zlib.compress(data) + b"endstream\n"
        assert raw_count("Zuleo", stream) == 1, data
    # Glyph positioning stays a word: an ambiguous boundary adds a match, never hides one.
    assert raw_count("Zuleo", b"BT 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 86 700 Tm (leo) Tj ET") == 1


def test_a_line_shown_with_a_quote_operator_continues_after_it():
    # Regression (Grok G0-27): ' and " show the string before them on a new line, and
    # the text after them continues that line; the break was put after the string.

    for data in (
        b"BT 72 720 Td (Zu) ' (leo) Tj ET",
        b"BT 72 720 Td (Zu) ' " + glyphs("leo") + b" ET",
        b'BT 72 720 Td 0 0 (Zu) " (leo) Tj ET',
    ):
        assert raw_count("Zuleo", data) == 1, data
        assert raw_count("Zulea", data) == 0, data


def test_a_short_word_placed_piece_by_piece_beside_another_word_is_found():
    # Regression (Grok G0-29): each horizontal move was read the same way, so a word
    # whose pieces are positioned one by one, next to a word placed after another move,
    # was whole in no reading.

    def at(x: int, text: bytes) -> bytes:
        return b"1 0 0 1 %d 700 Tm %s Tj " % (x, text)

    stream = b"BT " + at(72, b"(Zu)") + at(86, b"(leo)") + at(200, b"(Assunto)") + b"ET"
    assert raw_count("Zuleo", stream) == 1
    assert raw_count("Zulea", stream) == 0
    assert raw_count("Assunto", stream) == 1
    compressed = b"1 0 obj << >> stream\n" + zlib.compress(stream) + b"endstream\n"
    assert raw_count("Zuleo", compressed) == 1
    alone = b"BT " + at(72, b"(Zu)") + at(86, b"(leo)") + b"ET"
    assert raw_count("Zuleo", alone) == 1
    td = b"BT 72 700 Td (Z) Tj 7 0 Td (u) Tj 7 0 Td (l) Tj 7 0 Td (e) Tj 7 0 Td (o) Tj"
    assert raw_count("Zuleo", td + b" 30 0 Td (Assunto) Tj ET") == 1


def test_a_long_word_without_ambiguous_marks_is_fast_to_read():
    # Found while fixing Grok G0-29: the run pattern backtracked over every start and
    # length of a long word with no mark.
    started = time.perf_counter()
    assert mixed_readings("a" * 30_000 + " Zu" + AMBIGUOUS + "leo") == "Zuleo"
    assert time.perf_counter() - started < 1


def test_two_byte_strings_without_a_mark_are_read_as_utf16():
    # Regression (Grok G0-30): Identity-H content strings hold UTF-16 code units with no
    # byte order mark; read as Latin-1 they had a NUL between letters.

    for data in (
        b"BT <005A0075006C0065006F> Tj ET",
        b"BT [<005A> -2 <0075> -2 <006C> -2 <0065> -2 <006F>] TJ ET",
        b"BT (\\000Z\\000u\\000l\\000e\\000o) Tj ET",
        b"BT <FEFF005A0075006C0065006F> Tj ET",
    ):
        assert raw_count("Zuleo", data) == 1, data
        assert raw_count("Zulea", data) == 0, data
    hexed = "123.456.789-09".encode("utf-16-be").hex().encode()
    views = pdf_string_views([b"BT <" + hexed + b"> Tj ET"])
    assert count_patterns(views.for_patterns, [views.mixed]) == {"cpf": 1}


def test_any_move_is_ambiguous_without_state():
    # Regression (Grok G0-32, G0-36): the baseline was known only after a Tm, and was
    # lost in a long first gap, so a line opened with Td or at the start of an
    # uncompressed stream and continued with Tm was a hard break.
    opened_with_td = b"BT /F1 12 Tf 72 700 Td (Zu) Tj 1 0 0 1 86 700 Tm (leo) Tj ET"
    assert raw_count("Zuleo", opened_with_td) == 1
    assert raw_count("Zulea", opened_with_td) == 0
    page = b"%PDF-1.4\n" + b"1 0 obj << /Type /Catalog >> endobj\n" * 20
    stream = b"BT 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 86 700 Tm (leo) Tj "
    stream += b"1 0 0 1 200 700 Tm (Assunto) Tj ET"
    uncompressed = page + b"9 0 obj << >> stream\n" + stream + b"\nendstream endobj\n"
    assert raw_count("Zuleo", uncompressed) == 1
    assert raw_count("Zulea", uncompressed) == 0
    assert raw_count("Assunto", uncompressed) == 1


def test_a_string_with_a_character_beyond_latin1_is_read_as_utf16():
    # Regression (Grok G0-33): one code unit above U+00FF made the whole string Latin-1.
    for data in (
        b"BT <005A0075006C0065006F2014> Tj ET",
        b"BT (\\000Z\\000u\\000l\\000e\\000o\\040\\024) Tj ET",
        b"BT <005A0075006C0065006F00202265> Tj ET",
    ):
        assert raw_count("Zuleo", data) == 1, data
        assert raw_count("Zulea", data) == 0, data
    assert raw_count("Zuleo", b"BT (Zuleo) Tj ET") == 1


def test_numbers_across_ambiguous_moves_are_pattern_hints():
    # Regression (Grok G0-34, G0-35): a CPF kerned and followed by a moved number, or
    # drawn a group at a time, was whole in no reading searched for patterns.
    def hints(data: bytes) -> dict[str, int]:
        views = pdf_string_views([data])
        return count_patterns(views.for_patterns, [views.mixed])

    kerned = b"BT 1 0 0 1 72 700 Tm [(123) -2 (.) -2 (456) -2 (.) -2 (789) -2 (-) -2 (09)] TJ"
    groups = b"BT 72 700 Td (123) Tj 18 0 Td (.) Tj 6 0 Td (456) Tj 22 0 Td (.) Tj"
    groups += b" 6 0 Td (789) Tj 22 0 Td (-) Tj 6 0 Td (09) Tj"
    for data in (kerned, groups):
        assert hints(data + b" ET") == {"cpf": 1}, data
        assert hints(data + b" 30 0 Td (99) Tj ET") == {"cpf": 1}, data
    # A digit short, with nothing after it, is no CPF in any reading.
    assert hints(groups.replace(b"(09)", b"(0)") + b" ET") == {}


def test_any_operator_between_strings_is_ambiguous():
    # Regression (Grok G0-38): ET/BT, q, and marked content ended the word for sure,
    # though pdftotext reads such strings as one word.
    for data in (
        b"BT /F1 12 Tf 72 700 Td (Zu) Tj ET BT /F1 12 Tf 84 700 Td (leo) Tj ET",
        b"BT (Zu) Tj q (leo) Tj Q ET",
        b"BT (Zu) Tj /Span BMC (leo) Tj EMC ET",
        b"BT (Zu) Tj /Span <</MCID 3>> BDC (leo) Tj EMC ET",
    ):
        assert raw_count("Zuleo", data) == 1, data
        assert raw_count("Zulea", data) == 0, data
        stream = b"1 0 obj << >> stream\n" + zlib.compress(data) + b"endstream\n"
        assert raw_count("Zuleo", stream) == 1, data


def test_a_long_gap_keeps_the_open_array_and_stays_ambiguous():
    # Regression (Grok G0-37): a gap over 256 bytes was a break that also dropped the
    # TJ array it opened, so a word-space adjustment inside it glued two words.
    page = b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n"
    filler = b"1 0 obj << /Type /Catalog >> endobj\n" * 20
    array = b"BT [(Z) -2 (u) -2 (l) -2 (e) -2 (o) -300 (A) -2 (s) -2 (s) -2 (u) -2 (n)"
    array += b" -2 (t) -2 (o)] TJ ET"
    data = page + filler + b"9 0 obj << >> stream\n" + array + b"\nendstream endobj\n"
    assert raw_count("Zuleo", data) == 1
    assert raw_count("Zulea", data) == 0
    cpf = b"BT [(123) -2 (.) -2 (456) -2 (.) -2 (789) -2 (-) -2 (09) -300 (99)] TJ ET"
    views = pdf_string_views([page + filler + cpf])
    assert count_patterns(views.for_patterns, [views.mixed]) == {"cpf": 1}
