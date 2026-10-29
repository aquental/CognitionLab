import base64
import struct
import time
import zlib

from anonimizador.channels import (
    AMBIGUOUS,
    Reading,
    image_metadata,
    image_metadata_blocks,
    mixed_readings,
    pdf_string_views,
    png_size,
    raw_readings,
    raw_texts,
    read_image,
    unescape_literal,
    worth_ocr,
)
from anonimizador.terms import Term, count_patterns
from anonimizador.verify import block_count

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
    return sum(block_count(Term(term), block) for block in raw_readings(data).blocks)


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
    # No gap is a sure break, not even other bytes (Grok G0-39).
    assert raw_count("Zuleo", b"BT (Zu) Tj ET \x00\x9c\xff BT (leo) Tj ET") == 1


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


def test_comments_nul_and_other_bytes_between_glyphs_keep_a_word():
    # Regression (Grok G0-39): a comment, a NUL (PDF whitespace), a comma, or a brace
    # between glyph strings was a sure break, so a short name drawn in a TJ array
    # counted 0.
    for gap in (b"%\n", b"\x00", b",", b"{"):
        data = b"BT /F1 24 Tf 72 700 Td [(Z) -2 (u)" + gap + b" -2 (l) -2 (e) -2 (o)] TJ ET"
        assert raw_count("Zuleo", data) == 1, gap
        assert raw_count("Zulea", data) == 0, gap
        stream = b"1 0 obj << >> stream\n" + zlib.compress(data) + b"endstream\n"
        assert raw_count("Zuleo", stream) == 1, gap
    # A comment and a NUL are whitespace: the glyphs around them simply continue.
    for gap in (b"%\n", b"\x00"):
        data = b"BT [(Z) -2 (u)" + gap + b" -2 (l) -2 (e) -2 (o)] TJ ET"
        assert pdf_string_views([data]).split == "Zuleo", gap
    # A comment's brackets change nothing: a large adjustment stays a possible space.
    data = b"BT [(Z) -2 (u) -2 (l) -2 (e) -2 (o) % ]\n -300 (A) -2 (s) -2 (s) -2 (u) -2 (n)"
    data += b" -2 (t) -2 (o)] TJ ET"
    assert raw_count("Zuleo", data) == 1
    assert raw_count("Assunto", data) == 1


def test_an_email_split_by_a_move_is_a_hint_whatever_its_length():
    # Regression (Grok G0-40): an e-mail longer than a join was whole only in the fully
    # joined reading, which was not searched for patterns.
    for address in (b"ana.silva@hospital.com", b"fulano.silva@clinica.example.com"):
        at = address.index(b"@")
        data = b"BT 72 700 Td (" + address[:at] + b") Tj 4 0 Td (" + address[at:] + b") Tj ET"
        views = pdf_string_views([data])
        hints = count_patterns(views.for_patterns, [views.mixed], [views.drawn])
        assert hints == {"email": 1}, address
    # Numbers are not searched in the fully joined reading: separate numbers stay apart.
    numbers = b"BT (123.456) Tj 9 0 Td (789-09) Tj 9 0 Td (12) Tj ET"
    views = pdf_string_views([numbers])
    assert count_patterns([], (), [views.drawn]) == {}


def test_a_tj_adjustment_that_a_reader_shows_as_a_space_is_ambiguous():
    # Regression (Grok G0-42): pdftotext splits words from about -100 (0.1 em), but only
    # -200 or less was ambiguous, so a short name glued onto the next word.
    page = b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n"
    for adjustment in (b"-150", b"-100", b"-199", b"-50"):
        array = b"BT /F1 12 Tf 72 700 Td [(Z) -2 (u) -2 (l) -2 (e) -2 (o) " + adjustment
        array += b" (A) -2 (s) -2 (s) -2 (u) -2 (n) -2 (t) -2 (o)] TJ ET"
        data = page + b"9 0 obj << >> stream\n" + zlib.compress(array) + b"endstream\n"
        assert raw_count("Zuleo", data) == 1, adjustment
        assert raw_count("Zulea", data) == 0, adjustment
        assert raw_count("Assunto", data) == 1, adjustment
    # Kerning inside a word stays a continuation: the word is whole in every reading.
    kerned = b"BT [(Z) -2 (u) -30 (l) -2 (e) -49 (o)] TJ ET"
    assert pdf_string_views([kerned]).split == "Zuleo"


def test_a_stream_whose_keyword_ends_with_cr_alone_is_inflated():
    # Regression (Grok G0-43): only LF and CRLF started a stream, though readers accept
    # a bare CR, so a short name left in such a Flate stream counted 0.
    page = b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n"
    for eol in (b"\r", b"\n", b"\r\n"):
        body = zlib.compress(b"BT /F1 12 Tf 72 700 Td (Zuleo) Tj ET\n")
        data = page + b"9 0 obj << /Filter /FlateDecode >> stream" + eol + body + b"endstream\n"
        assert raw_count("Zuleo", data) == 1, eol
        assert raw_count("Zulea", data) == 0, eol


def test_strings_that_are_not_drawn_stay_out_of_a_drawn_word():
    # Regression (Grok G0-44): a string in a comment, or a dictionary value, was spliced
    # between the pieces of a drawn word, so a short name counted 0 in raw bytes.
    page = b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n"
    for data in (
        b"BT [(Z) -2 (u) -2 (l) % (XX)\n -2 (e) -2 (o)] TJ ET",
        b"BT (Zu) Tj % (XX)\n (leo) Tj ET",
        b"BT (Zu) Tj % <4142>\n (leo) Tj ET",
        b"BT (Zu) Tj /Span << /Lang (pt) >> BDC (leo) Tj EMC ET",
        b"BT (Zu) Tj /Span << /Alt <4142> /P << /Q (r) >> >> BDC (leo) Tj EMC ET",
        # Without a string, as before.
        b"BT [(Z) -2 (u) -2 (l) % nota\n -2 (e) -2 (o)] TJ ET",
        b"BT (Zu) Tj /Span << /MCID 1 >> BDC (leo) Tj EMC ET",
    ):
        assert raw_count("Zuleo", data) == 1, data
        assert raw_count("Zulea", data) == 0, data
        stream = b"9 0 obj << /Filter /FlateDecode >> stream\n" + zlib.compress(data)
        assert raw_count("Zuleo", page + stream + b"endstream\n") == 1, data
    # A longer word, which needs no boundary, is whole too.
    assert raw_count("Quirvo", b"BT (Qui) Tj % (XX)\n (rvo) Tj ET") == 1
    # A string only a comment holds is still text of the file.
    assert raw_count("Zul\xe9o", b"BT (Laudo) Tj % (Zul\\351o)\nET") == 1
    assert raw_count("Zul\xe9o", b"BT (Laudo) Tj % <5A756CE96F>\nET") == 1
    # A dictionary left open by binary data ends at the stream keyword.
    data = b"<< \x8f\x02 endstream\nBT (Zu) Tj 7 0 Td (leo) Tj ET"
    assert pdf_string_views([data]).drawn == "Zuleo"


def test_a_number_whose_space_is_drawn_at_a_move_is_a_pattern_hint():
    # Regression (Grok G0-45): the space in a phone or a CPF, drawn at the edge of a
    # piece beside a move, was dropped from every join.
    def hints(data: bytes) -> dict[str, int]:
        raw = raw_readings(data)
        return count_patterns(raw.strings, raw.joins, raw.drawn)

    page = b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n"
    for data, kind in (
        (b"BT (11) Tj 20 0 Td ( 98765-4321) Tj ET", "phone"),
        (b"BT (11) Tj 20 0 Td (98765-4321) Tj ET", "phone"),
        (b"BT (11) Tj 20 0 Td ( ) Tj 4 0 Td (98765-4321) Tj ET", "phone"),
        (b"BT (123.456.789- ) Tj 0 -14 Td (09) Tj ET", "cpf"),
        (b"BT [(123.456.789-) -300 (09)] TJ ET", "cpf"),
    ):
        assert hints(data) == {kind: 1}, data
        stream = b"9 0 obj << /Filter /FlateDecode >> stream\n" + zlib.compress(data)
        assert hints(page + stream + b"endstream\n") == {kind: 1}, data
    # Numbers with a word between them, or a space on each side of the move, make no
    # number.
    assert hints(b"BT (Lote 123.456) Tj 9 0 Td ( mg 789-09) Tj ET") == {}
    assert hints(b"BT (11 ) Tj 20 0 Td ( 98765-4321) Tj ET") == {}
    assert mixed_readings(f"a 11 {AMBIGUOUS} 9876 x") == " 9876\n11  9876"
    # One space at a boundary joins; two spaces end the join.
    assert mixed_readings(f"Zu{AMBIGUOUS}  {AMBIGUOUS}leo") == "Zu \nleo"
    # A join starts after the last space too, so a long last word uses up no length.
    long = f"Telefone: {AMBIGUOUS}(11){AMBIGUOUS}98765-4321"
    assert "(11)98765-4321" in mixed_readings(long).split("\n")


def test_an_inline_image_or_a_stray_dictionary_opener_hides_no_drawn_string():
    # Regression (Grok G0-46): a "<<" in an inline image's samples opened a dictionary
    # that nothing closed, so every later string was left out of the drawn readings.
    page = b"BT /F1 12 Tf 72 740 Td (Hemoglobina 13,5 g/dL) Tj ET\n"
    for samples in (b"\x10\x20<<\x30\x40\x50\x60", b"\x10%(\x20<<\x30\x40"):
        image = b"BI /W 8 /H 1 /CS /G /BPC 8 ID " + samples + b" EI\n"
        for text in (
            b"BT /F1 12 Tf 72 700 Td (Zu) Tj (leo) Tj ET",
            b"BT [(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ ET",
            b"BT (Zu) Tj % (XX)\n (leo) Tj ET",
            b"BT (Zu) Tj /Span << /Lang (pt) >> BDC (leo) Tj EMC ET",
        ):
            content = page + image + text
            assert raw_count("Zuleo", content) == 1, (samples, text)
            assert raw_count("Zulea", content) == 0, (samples, text)
            stream = b"4 0 obj << /Filter /FlateDecode >> stream\n" + zlib.compress(content)
            assert raw_count("Zuleo", stream + b"endstream\n") == 1, (samples, text)
        assert raw_count("Quirvo", page + image + b"BT (Qui) Tj (rvo) Tj ET") == 1
        cpf = b"BT 72 700 Td (123) Tj 18 0 Td (.) Tj 6 0 Td (456) Tj 22 0 Td (.) Tj"
        cpf += b" 6 0 Td (789) Tj 22 0 Td (-) Tj 6 0 Td (09) Tj ET"
        raw = raw_readings(page + image + cpf)
        assert count_patterns(raw.strings, raw.joins, raw.drawn) == {"cpf": 1}, samples
    # A "<<" that no ">>" closes is no dictionary, inline image or not.
    assert pdf_string_views([b"BT (Zu) Tj \x10<<\x30 (leo) Tj ET"]).drawn == "Zuleo"
    # Samples are skipped: a comment that ends the line, and a ">>" that would close an
    # earlier image's "<<", hide nothing.
    same_line = b"BI /W 4 /H 1 /CS /G /BPC 8 ID \x10%\x20\x30 EI BT (Zu) Tj (leo) Tj ET"
    assert raw_count("Zuleo", same_line) == 1
    two = b"BI /W 2 /H 1 /CS /G /BPC 8 ID <<\x30 EI\nBT (Zu) Tj (leo) Tj ET\n"
    two += b"BI /W 2 /H 1 /CS /G /BPC 8 ID \x30>> EI\n"
    assert raw_count("Zuleo", two) == 1
    # A stream keyword drops a "<<" its binary data left open.
    data = b"stream\n\x10<<\x20\nendstream\nBT (Zu) Tj (leo) Tj ET\nstream\n\x30>>\x40\nendstream"
    assert pdf_string_views([data]).drawn.startswith("Zuleo")


def test_a_literal_nested_deeper_than_one_level_is_one_string():
    # Regression (Grok G0-47): a literal was read one parenthesis level deep, so an
    # escaped name in a string nested two levels was never decoded.
    for literal in (
        rb"(Zul\351o (obs (2)))",
        rb"(Zul\351o (a (b (c))))",
        rb"(Zul\351o \( (a (b)))",
        rb"(Zul\351o (obs))",
        rb"(obs (2) Zul\351o)",
        rb"((Zul\351o))",
        rb"(Zul\351o (a) (b))",
        rb"(A (B (Zul\351o)))",
    ):
        data = b"BT /F1 12 Tf 72 700 Td " + literal + b" Tj ET"
        assert raw_count("Zul\xe9o", data) == 1, literal
        assert raw_count("Zul\xe9a", data) == 0, literal
        stream = b"4 0 obj << /Filter /FlateDecode >> stream\n" + zlib.compress(data)
        assert raw_count("Zul\xe9o", stream + b"endstream\n") == 1, literal
    # It is drawn whole: a short name split before it is whole in the drawn readings.
    assert raw_count("Zul\xe9o", rb"BT (Zu) Tj (l\351o (a (b))) Tj ET") == 1
    # A deeper literal holds its text outside the strings in it, which are read too, and
    # is read only when it is short and within one stream: binary data pairs
    # parentheses over any length.
    assert pdf_string_views([b"(a (b (c)) d)"]).lines == "a \n d\nb (c)"
    assert pdf_string_views([b"(" + b"a" * 1100 + b" (b (c)))"]).lines == "b (c)"
    assert pdf_string_views([b"(a (endstream\nendobj (c)))"]).lines == "endstream\nendobj (c)"


def test_a_deeper_literal_may_hold_the_word_stream():
    # Regression (Codex C0-5): any "stream" in a deeper literal, a word in its text
    # included, made it unread.
    for literal in (
        rb"(Zul\351o (upstream (2)))",
        rb"(Zul\351o (a stream (2)))",
        rb"(Zul\351o (endstream (2)))",
    ):
        data = b"9 0 obj << /Author " + literal + b" >> endobj\n"
        assert raw_count("Zul\xe9o", data) == 1, literal
    # The keywords of the file structure still end it.
    assert pdf_string_views([b"(a >> stream\n (b (c)))"]).lines == "b (c)"
    assert pdf_string_views([b"(a endstream endobj (b (c)))"]).lines == "b (c)"


def test_hex_strings_with_nul_spaces_or_one_digit_are_read():
    # Regression (Codex C0-6): NUL is PDF whitespace, and "<5>" is the byte 0x50.
    assert raw_count("Zuleo", b"9 0 obj << /Author <5A\x0075\x006C\x0065\x006F> >> endobj") == 1
    assert raw_count("Puleo", b"BT [<5><75><6C><65><6F>] TJ ET") == 1
    assert raw_count("Zulea", b"BT [<5><75><6C><65><6F>] TJ ET") == 0


def test_a_text_string_is_read_as_pdfdocencoding_too():
    # Regression (Codex C0-7): 0x93 is the ligature "fi" in PDFDocEncoding, a control
    # character in Latin-1, so an old author's name did not match.
    data = b"9 0 obj << /Author (Fulano \\223cticio) >> endobj\n"
    assert raw_count("Fulano Ficticio", data) == 1
    assert raw_count("Fulano Ficticio", data.replace(b"\\223", b"\\224")) == 0


def test_an_inline_image_ends_after_its_samples_whatever_they_hold():
    # Regression (Codex C0-4): the samples ended at their first " EI ", so the rest of
    # them was read as syntax and a "%" there hid the text after the image.
    text = b" EI BT /F1 12 Tf 72 700 Td (Zu) Tj (leo) Tj ET"
    for head, samples in (
        (b"BI /W 16 /H 1 /CS /G /BPC 8 ID ", b" EI %abcdefghijk"),
        (b"BI /Width 4 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 ID ", b"%" * 24),
        (b"BI /W 9 /H 2 /IM true ID ", b"%\x01 EI\n"),
        (b"BI /W 2 /H 1 /CS [/I /RGB 1 <00FF00FF00FF>] /BPC 8 ID ", b"%("),
        (b"BI /W 4 /H 2 /CS /RGB /BPC 8 ID ", b"\x00" * 8 + b" EI %" + b"\x00" * 11),
        # Filtered data ends at its filter's end-of-data mark.
        (b"BI /W 2 /H 2 /CS /G /BPC 8 /F /DCT ID ", b"\xff\xd8 EI %x\xff\xd9"),
        (b"BI /W 2 /H 2 /CS /G /BPC 8 /F [/A85 /DCT] ID ", b"9jqo EI %^BlbD~>"),
        (b"BI /W 2 /H 2 /CS /G /BPC 8 /Filter /RunLengthDecode ID ", b"\x05 EI %a\x80"),
        (b"BI /W 1 /H 1 /CS /G /BPC 8 /F /AHx ID ", b"4 EI %1>"),
        # Level 0 stores the bytes as they are, " EI %" included.
        (b"BI /W 8 /H 1 /CS /G /BPC 8 /F /Fl ID ", zlib.compress(b" EI %abc", 0)),
    ):
        data = head + samples + text
        assert raw_count("Zuleo", data) == 1, head
        assert raw_count("Zulea", data) == 0, head
        stream = b"4 0 obj << /Filter /FlateDecode >> stream\n" + zlib.compress(data)
        assert raw_count("Zuleo", stream + b"endstream\n") == 1, head
    # Without a size that "EI" confirms (a filter, a wrong size), the first "EI" ends it.
    for head in (b"BI /W 4 /H 1 /CS /G /BPC 8 /F /AHx ID ", b"BI /W 40 /H 1 /CS /G /BPC 8 ID "):
        assert raw_count("Zuleo", head + b"%0102" + text) == 1, head


JPEG_START = b"\xff\xd8\xff\xc0\x00\x0b\x08\x00\x01\x00\x01\x01\x01\x11\x00"
JPEG_SCAN = b"\xff\xda\x00\x08\x01\x01\x00\x00\x3f\x00\x12\x34\xff\xd9"


def test_an_image_that_is_not_whole_is_an_error_without_ocr(tmp_path):
    # Regression (Codex C0-9): with OCR off, nothing read an image as one, so bytes
    # that are no image were reported clean.
    # IHDR: 40 x 30, RGB, 8 bits: 30 rows of a filter byte and 120 bytes.
    idat = (b"IDAT", zlib.compress(b"\x00" * 30 * 121))
    whole = {
        "a.png": png([IHDR, idat, (b"IEND", b"")]),
        "b.jpg": JPEG_START + JPEG_SCAN,
    }
    broken = {
        "c.jpg": b"not an image",
        "d.png": png([IHDR, idat]),
        "e.png": png([IHDR, idat, (b"IEND", b"")])[:-20],
        "f.png": png([IHDR, (b"IEND", b"")]),
        "g.jpg": JPEG_START + JPEG_SCAN[:-2],
        "h.jpg": JPEG_START[:4],
        "i.jpg": b"\xff\xd8" + JPEG_SCAN,
        # Codex round 2: empty chunks, data that does not inflate, too few pixels, a
        # wrong checksum, a palette image without a palette.
        "j.png": png([(b"IHDR", b""), (b"IDAT", b""), (b"IEND", b"")]),
        "k.png": png([IHDR, (b"IDAT", b"broken"), (b"IEND", b"")]),
        "l.png": png([IHDR, (b"IDAT", zlib.compress(b"\x00" * 121)), (b"IEND", b"")]),
        "m.png": png([IHDR, idat, (b"IEND", b"")]).replace(
            b"IEND\xaeB`\x82", b"IEND\x00\x00\x00\x00"
        ),
        "n.png": png(
            [
                (b"IHDR", struct.pack(">IIBBBBB", 40, 30, 8, 3, 0, 0, 0)),
                (b"IDAT", zlib.compress(b"\x00" * 30 * 41)),
                (b"IEND", b""),
            ]
        ),
    }
    for name, data in {**whole, **broken}.items():
        (tmp_path / name).write_bytes(data)
        reading = read_image(str(tmp_path / name), name, "por", False, 10)
        expected = ["file: not a whole PNG or JPEG image"] if name in broken else []
        assert reading.errors == expected, name


def lzw_encode(data: bytes, early_change: int = 1) -> bytes:
    """LZW as PDF reads it: 9 to 12-bit codes, the size growing one code early unless
    `early_change` is 0."""
    table = {bytes([i]): i for i in range(256)}
    out, buffer, buffered, width, next_code = bytearray(), 0, 0, 9, 258

    def emit(code: int) -> None:
        nonlocal buffer, buffered
        buffer, buffered = (buffer << width) | code, buffered + width
        while buffered >= 8:
            buffered -= 8
            out.append((buffer >> buffered) & 0xFF)

    emit(256)
    word = b""
    for byte in data:
        if word + bytes([byte]) in table:
            word += bytes([byte])
            continue
        emit(table[word])
        table[word + bytes([byte])], next_code = next_code, next_code + 1
        if next_code == (1 << width) + 1 - early_change and width < 12:
            width += 1
        if next_code >= 4094:
            emit(256)
            table, width, next_code = {bytes([i]): i for i in range(256)}, 9, 258
        word = bytes([byte])
    emit(table[word])
    emit(257)
    if buffered:
        out.append((buffer << (8 - buffered)) & 0xFF)
    return bytes(out)


def run_length_encode(data: bytes) -> bytes:
    """Literal runs of up to 128 bytes, and one repeated run, as RunLengthDecode reads."""
    out = bytearray()
    for start in range(0, len(data) - 1, 128):
        chunk = data[start : start + 128]
        out += bytes([len(chunk) - 1]) + chunk
    return bytes(out) + bytes([257 - 5]) + data[-1:] + b"\x80"


def test_a_stream_is_decoded_through_its_chain_of_filters():
    # Regression (Codex C0-3): only zlib was tried, so a name in an old stream encoded
    # with ASCII85, ASCIIHex, LZW, or RunLength (alone or before Flate) counted 0.
    page = b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n"
    content = b"BT (Zu) Tj (leo) Tj ET\n" + b"BT 72 600 Td (linha de teste abc) Tj ET\n" * 300
    # Run-length literal runs hold the text as it is: the bytes and the decoded stream
    # both count it.
    for filters, body, count in (
        (b"[/ASCII85Decode /FlateDecode]", base64.a85encode(zlib.compress(content)) + b"~>", 1),
        (b"/A85", b"<~" + base64.a85encode(content) + b"~>", 1),
        (b"[/AHx /Fl]", zlib.compress(content).hex().encode() + b">", 1),
        (b"/LZWDecode", lzw_encode(content), 1),
        (b"[/ASCIIHexDecode /LZWDecode]", lzw_encode(content).hex().encode() + b">", 1),
        (b"/RunLengthDecode", run_length_encode(content), 2),
    ):
        data = page + b"9 0 obj << /Length 1 /Filter " + filters + b" >> stream\n" + body
        data += b"\nendstream endobj\n"
        raw = raw_readings(data)
        assert raw_count("Zuleo", data) == count, filters
        assert raw_count("Zulea", data) == 0, filters
        assert raw.unread == 0, filters
    # A filter that is not read makes the raw bytes unread; an image codec does not.
    for filters, unread in (
        (b"/Crypt /DecodeParms << /Name /StdCF >>", 1),
        (b"[/A85 /Unknown]", 1),
        (b"/DCTDecode", 0),
        # Crypt without parameters is the identity.
        (b"/Crypt", 0),
    ):
        data = page + b"9 0 obj << /Filter " + filters + b" >> stream\nxyz\nendstream endobj\n"
        assert raw_readings(data).unread == unread, filters


def test_independent_places_add_up_and_readings_of_one_place_do_not():
    # Regression (Codex C0-12): the largest count of any reading was reported, so two
    # streams, or two metadata segments, holding a name counted 1.
    xmp = zlib.compress(b"<author>Zuleo</author>")
    stream = b"9 0 obj << /Filter /FlateDecode >> stream\n" + xmp + b"\nendstream endobj\n"
    assert raw_count("Zuleo", stream) == 1
    assert raw_count("Zuleo", stream + stream) == 2
    segment = b"\xff\xfe\x00\x07Zuleo"
    jpeg = JPEG_START + segment + segment + JPEG_SCAN
    assert [max(Term("Zuleo").count(t) for t in b) for b in image_metadata_blocks(jpeg)] == [1, 1]


def png_predict(data: bytes, columns: int) -> bytes:
    """PNG prediction, one filter type per row in turn (none, sub, up, average, Paeth)."""
    out, above = bytearray(), bytes(columns)
    for row, start in enumerate(range(0, len(data), columns)):
        line = data[start : start + columns].ljust(columns, b" ")
        kind = row % 5
        out.append(kind)
        for i, value in enumerate(line):
            left, up = (line[i - 1] if i else 0), above[i]
            corner = above[i - 1] if i else 0
            guess = left + up - corner
            distances = [abs(guess - left), abs(guess - up), abs(guess - corner)]
            paeth = (left, up, corner)[distances.index(min(distances))]
            out.append((value - (0, left, up, (left + up) // 2, paeth)[kind]) & 0xFF)
        above = line
    return bytes(out)


def tiff_predict(data: bytes, columns: int) -> bytes:
    out = bytearray()
    for start in range(0, len(data), columns):
        line = data[start : start + columns]
        out += line[:1] + bytes((line[i] - line[i - 1]) & 0xFF for i in range(1, len(line)))
    return bytes(out)


def zlib_holding(raw: bytes, rest: bytes) -> bytes:
    """A zlib stream whose first block stores `raw` as it is, then deflates `rest`."""
    stored = b"\x00" + struct.pack("<HH", len(raw), len(raw) ^ 0xFFFF) + raw
    deflater = zlib.compressobj(9, zlib.DEFLATED, -15)
    deflated = deflater.compress(rest) + deflater.flush()
    return b"\x78\x01" + stored + deflated + struct.pack(">I", zlib.adler32(raw + rest))


def test_a_stream_is_decoded_as_its_dictionary_says():
    # Regression (Codex C0-3, C0-13..C0-16): a failed decoder was silent, a chain of
    # Flate filters was inflated once, an indirect or escaped /Filter or one far from
    # the data was missed, /DecodeParms was ignored, and the data ended at the first
    # "endstream" whatever /Length said.
    page = b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n"
    # The name is drawn in the fifth 40-byte row, which PNG prediction filters with
    # Paeth, after rows filtered every other way.
    content = b"%" + b" " * 158 + b"\n" + b"BT (Zu) Tj (leo) Tj ET\n"
    content += b"BT 72 600 Td (linha de teste abc) Tj ET\n" * 30
    padded = content.ljust(-(-len(content) // 40) * 40, b" ")
    a85 = base64.a85encode(zlib.compress(content)) + b"~>"
    held = zlib_holding(b"% endstream\n", content)
    for entries, body, extra in (
        (b"/Filter [/FlateDecode /FlateDecode]", zlib.compress(zlib.compress(content)), b""),
        (b"/Filter 10 0 R", a85, b"10 0 obj [/ASCII85Decode /FlateDecode] endobj\n"),
        (b"/F#69lter [/A#385 /Fl]", a85, b""),
        (b"/Filter [/A85 /Fl] /Pad (" + b"x" * 4200 + b")", a85, b""),
        (
            b"/Filter /FlateDecode /DecodeParms << /Predictor 15 /Columns 40 >>",
            zlib.compress(png_predict(padded, 40)),
            b"",
        ),
        (
            b"/Filter /Fl /DecodeParms << /Predictor 2 /Columns 40 >>",
            zlib.compress(tiff_predict(padded, 40)),
            b"",
        ),
        (b"/Filter /LZW /DecodeParms << /EarlyChange 0 >>", lzw_encode(content, 0), b""),
        (b"/Filter /FlateDecode /Length %d" % len(held), held, b""),
        (b"/Filter /FlateDecode /Length 11 0 R", held, b"11 0 obj %d endobj\n" % len(held)),
    ):
        data = page + extra + b"9 0 obj << " + entries + b" >> stream\n" + body
        data += b"\nendstream endobj\n"
        raw = raw_readings(data)
        assert raw_count("Zuleo", data) == 1, entries[:40]
        assert raw_count("Zulea", data) == 0, entries[:40]
        assert raw.unread == 0, entries[:40]
    # A declared filter that fails, or data that ends too soon, is unread.
    for entries, body in (
        (b"/Filter /FlateDecode", b"invalidcompressedbytes"),
        (b"/Filter /FlateDecode", zlib.compress(content)[:-30]),
        (b"/Filter /A85", b"<~vwxyz~>"),
        (b"/Filter /AHx", b"4G>"),
        (b"/Filter /LZW", b"\xff\xff\xff"),
        (b"/Filter /Fl /DecodeParms << /Predictor 15 /Columns 0 >>", zlib.compress(b"ab")),
        (b"/Filter 12 0 R", zlib.compress(content)),
    ):
        data = page + b"9 0 obj << " + entries + b" >> stream\n" + body + b"\nendstream endobj\n"
        assert raw_readings(data).unread == 1, (entries, body[:10])


def test_a_compressed_png_text_that_cannot_be_inflated_is_unread(tmp_path):
    # Regression (Codex C0-17): a zTXt or iTXt chunk that did not inflate was dropped
    # as if it held no metadata.
    idat = (b"IDAT", zlib.compress(b"\x00" * 30 * 121))
    good = [
        (b"zTXt", b"Comment\x00\x00" + zlib.compress(b"Zuleo")),
        (b"iTXt", b"Comment\x00\x01\x00pt\x00\x00" + zlib.compress(b"Zuleo")),
        (b"iTXt", b"Comment\x00\x00\x00pt\x00\x00Zuleo"),
    ]
    for chunk in good:
        (tmp_path / "a.png").write_bytes(png([IHDR, chunk, idat, (b"IEND", b"")]))
        reading = read_image(str(tmp_path / "a.png"), "a.png", "por", False, 10)
        assert reading.errors == [], chunk[0]
        assert sum(Term("Zuleo").count(t) for t in reading.texts["metadata"]) >= 1, chunk[0]
    for chunk in (
        (b"zTXt", b"Comment\x00\x00invalidcompressedbytes"),
        (b"zTXt", b"Comment\x00\x00" + zlib.compress(b"Zuleo")[:-3]),
        (b"iTXt", b"Comment\x00\x01\x00pt\x00\x00broken"),
        (b"zTXt", b"no keyword end"),
    ):
        (tmp_path / "b.png").write_bytes(png([IHDR, chunk, idat, (b"IEND", b"")]))
        reading = read_image(str(tmp_path / "b.png"), "b.png", "por", False, 10)
        assert reading.errors == ["metadata: 1 compressed PNG text chunk(s) cannot be read"]


def test_an_inline_image_whose_end_cannot_be_told_makes_the_raw_bytes_unread():
    # Regression (Codex round 2, C0-4): with a color space named in the resources,
    # the samples' size is unknown, and the first " EI " in them was taken as the end.
    text = b" EI BT /F1 12 Tf 72 700 Td (Zu) Tj (leo) Tj ET"
    head = b"BI /W 16 /H 1 /CS /Cs1 /BPC 8 ID "
    raw = raw_readings(head + b" EI %abcdefghijk" + text)
    assert raw.unclear_images == 1
    reading = Reading()
    reading.take_raw(raw)
    assert reading.errors == ["raw: 1 inline image(s) end where it is unclear"]
    # One "EI" only: the end is clear, and the text after it is read.
    raw = raw_readings(head + b"\x01\x02%abcdefghijklm" + text)
    assert raw.unclear_images == 0
    assert raw_count("Zuleo", head + b"\x01\x02%abcdefghijklm" + text) == 1
    # A later image's own "EI" does not make the first one unclear; no "EI" at all does.
    two = head + b"\x01%" + text + b"\n" + head + b"\x02%" + text
    assert raw_readings(two).unclear_images == 0
    assert raw_readings(head + b"\x01\x02 no end").unclear_images == 1
