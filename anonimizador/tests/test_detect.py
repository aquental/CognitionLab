"""Detectors over words with boxes: what is found, and what is left alone."""

from pathlib import Path

from anonimizador.detect import Detection, cns_valid, cpf_valid, detect, detect_reading
from anonimizador.locate import Page, Word
from anonimizador.terms import Term

# A valid CNS (weighted sum divisible by 11), invented.
CNS = "898001234567891"


def line(*texts: str, number: int = 0) -> list[Word]:
    """Words of one line, laid out left to right, each 100 px wide."""
    return [
        Word(t, (100 * i, 20 * number, 100 * i + 90, 20 * number + 15), ("ocr", 1, 1, number))
        for i, t in enumerate(texts)
    ]


def found(words: list[Word], terms=(), remove_dates=True, pages=None) -> dict[str, set[str]]:
    """The words detected, by kind (the test knows the words; detections do not).
    `pages` splits the words into pages; by default they are all on page 1."""
    pages = pages or [(1, words)]
    by_box = {(n, w.box): w.text for n, ws in pages for w in ws}
    result: dict[str, set[str]] = {}
    for d in detect_reading("ocr", pages, [Term(t) for t in terms], remove_dates):
        result.setdefault(d.kind, set()).add(by_box[d.page, d.box])
    return result


def test_a_term_is_found_across_words_lines_and_ocr_spacing():
    words = line("Exame", "de", "Fula", "no", number=0) + line("Ficticio", "ok", number=1)
    assert found(words, ["Fulano Ficticio"]) == {"term": {"Fula", "no", "Ficticio"}}


def test_a_numeric_term_is_found_by_its_digits_whatever_the_punctuation():
    words = line("CPF:", "123.456.789-", "09")
    assert found(words, ["12345678909"])["term"] == {"123.456.789-", "09"}


def test_a_short_term_must_be_a_whole_word():
    assert found(line("Anabela", "Ana"), ["Ana"]) == {"term": {"Ana"}}


def test_overlapping_occurrences_of_a_short_term_are_all_found():
    # Codex CB-10: "Li Li Li" holds "Li Li" twice, as Term.count counts it.
    words = line("Li", "Li", "Li", "x")
    assert Term("Li Li").count("Li Li Li") == 2
    assert [d.box for d in detect_reading("ocr", [(1, words)], [Term("Li Li")], True)] == [
        w.box for w in words[:3]
    ]


def test_a_term_split_across_a_page_break_is_found_on_both_pages():
    # Codex CB-5: verify searches the pages together, so the detector does too.
    first, second = line("laudo", "Fulano", number=9), line("Ficticio", "ok", number=0)
    pages = [(1, first), (2, second)]
    detections = detect_reading("ocr", pages, [Term("Fulano Ficticio")], True)
    assert sorted((d.page, d.box) for d in detections) == [(1, first[1].box), (2, second[0].box)]
    # And `detect`, over whole pages, hands every page of a reading to it at once.
    rendered = [
        Page(1, Path("p1.png"), (900, 900), ocr=first),
        Page(2, Path("p2.png"), (900, 900), ocr=second),
    ]
    assert sorted((d.page, d.box) for d in detect(rendered, [Term("Fulano Ficticio")])) == [
        (1, first[1].box),
        (2, second[0].box),
    ]


def test_only_cpf_and_cns_with_valid_check_digits_are_patterns():
    assert cpf_valid("12345678909") and not cpf_valid("12345678900")
    assert not cpf_valid("11111111111")
    assert cns_valid(CNS) and not cns_valid("898001234567893")
    assert found(line("valor", "123.456.789-09")) == {"cpf": {"123.456.789-09"}}
    assert found(line("codigo", "12345678900")) == {}
    assert found(line("cartao", CNS)) == {"cns": {CNS}}


def test_contact_patterns_are_found():
    words = line("(11)", "98765-4321", "x@exemplo.com", "01234-567")
    assert found(words) == {
        "phone": {"(11)", "98765-4321"},
        "email": {"x@exemplo.com"},
        "cep": {"01234-567"},
    }


def test_dates_follow_the_date_policy():
    assert found(line("coleta", "05/06/2021")) == {"date": {"05/06/2021"}}
    assert found(line("coleta", "05/06/2021"), remove_dates=False) == {}


def test_a_label_value_runs_to_the_next_label():
    words = line("Paciente:", "Sicrano", "Inventado", "Idade:", "45", "anos")
    assert found(words) == {"label": {"Sicrano", "Inventado"}}


def test_a_label_with_a_detached_or_glued_colon():
    assert found(line("Nome", ":", "Sicrano")) == {"label": {"Sicrano"}}
    assert found(line("Paciente:Sicrano", "Idade:", "45")) == {"label": {"Paciente:Sicrano"}}


def test_a_label_word_in_running_text_is_no_label():
    assert found(line("o", "paciente", "relata", "dor")) == {}
    assert found(line("sobrenome:", "x")) == {}


def test_a_number_label_takes_the_number_after_it():
    assert found(line("CRM/SP", "123456", "Hemoglobina")) == {"label": {"123456"}}
    assert found(line("CRM", "123.456", "-", "SP", "Dr")) == {"label": {"123.456", "SP"}}
    assert found(line("RG:", "12.345.678-9")) == {"label": {"12.345.678-9"}}
    assert found(line("CPF:12345678900")) == {"label": {"CPF:12345678900"}}


def test_an_rg_with_the_check_letter_x_is_taken():
    # Codex CB-6.
    assert found(line("RG:", "12.345.671-X")) == {"label": {"12.345.671-X"}}
    assert found(line("RG", "12.345.671-x", "emissor")) == {"label": {"12.345.671-x"}}


def test_a_short_number_glued_to_its_label_is_taken():
    # Codex CB-11: "CRM:123" and "CRM:" "123" are the same value.
    assert found(line("CRM:123")) == {"label": {"CRM:123"}}
    assert found(line("CRM:", "123")) == {"label": {"123"}}


def test_a_label_that_ends_its_line_takes_the_next_line():
    # Codex CB-7: the date of birth under its label goes, whatever the date policy.
    words = line("Dt.", "Nasc.:", number=0) + line("01/02/1980", "Sexo:", "F", number=1)
    assert found(words, remove_dates=False) == {"label": {"01/02/1980"}}
    # Only when the label ends its line, and never across a page break.
    assert found(line("Nome:", "Sicrano", number=0) + line("Fulano", number=1)) == {
        "label": {"Sicrano"}
    }
    pages = [(1, line("Paciente:", number=5)), (2, line("Hemoglobina", "13,5", number=0))]
    assert found([], pages=pages) == {}


def test_a_number_label_without_a_number_takes_nothing():
    assert found(line("CRM", "SP", "assinatura")) == {}
    assert found(line("o", "crm", "do", "estado")) == {}


def test_laboratory_values_are_left_alone():
    words = line("Hemoglobina", "13,5", "g/dL", "12,0", "-", "16,0", number=0) + line(
        "Leucocitos", "7.500", "/mm3", "4.000", "a", "11.000", number=1
    )
    assert found(words, ["Fulano Ficticio", "12345678909"]) == {}


def test_a_detection_holds_no_text():
    [d] = detect_reading("ocr", [(1, line("Paciente:", "Sicrano"))], [], True)
    assert d == Detection(1, (100, 0, 190, 15), "label", "labels", "ocr")


def placed(*words: tuple[str, int], number: int = 0, height: int = 15) -> list[Word]:
    """Words of one line at the given left edges, each 90 px wide."""
    top = 40 * number
    return [Word(t, (x, top, x + 90, top + height), ("ocr", 1, 1, number)) for t, x in words]


def test_a_name_label_without_a_colon_takes_the_name_beside_it():
    # A form that prints "Nome" and the name, then a wide gap to the next field.
    words = placed(
        ("Nome", 0), ("Sicrano", 100), ("Inventado", 200), ("Data", 900), ("05/06", 1000)
    )
    assert found(words, remove_dates=False) == {"label": {"Sicrano", "Inventado"}}
    assert found(line("Nome", "do", "paciente", "Beltrano")) == {"label": {"Beltrano"}}


def test_a_name_label_without_a_colon_heads_the_column_of_the_name():
    heading = placed(("Nome", 0), ("Data", 600), ("Altura", 900), number=0)
    values = placed(("Sicrano", 0), ("Inventado", 100), ("05/06/2021", 600), number=1)
    assert found(heading + values, remove_dates=False) == {"label": {"Sicrano", "Inventado"}}


def test_a_name_label_without_a_colon_far_from_its_name_still_takes_it():
    words = placed(("Nome", 0), ("Sicrano", 700), ("Inventado", 800))
    assert found(words) == {"label": {"Sicrano", "Inventado"}}


def test_a_name_word_that_is_no_bare_label_is_left_alone():
    assert found(line("o", "nome", "do", "exame", "consta")) == {}
    assert found(line("Nomenclatura", "padrao")) == {}
    assert found(line("Nome", "do", "exame:", "Relatorio")) == {}
    # With a colon, the colon label takes it.
    assert found(line("Nome:", "Sicrano")) == {"label": {"Sicrano"}}


def test_a_column_value_must_start_under_its_heading():
    heading = placed(("Nome", 0), number=0)
    assert found(heading + placed(("Hemoglobina", 600), number=1)) == {}


def test_a_name_beside_its_label_on_another_extracted_line_is_taken():
    # Codex CB-12: the extractor can put a label and its value in separate blocks.
    label = Word("Nome", (0, 0, 40, 15), ("ocr", 1, 1, 0))
    value = [
        Word("Sicrano", (200, 0, 290, 15), ("ocr", 2, 1, 0)),
        Word("Inventado", (300, 0, 390, 15), ("ocr", 2, 1, 0)),
    ]
    assert found([label, *value]) == {"label": {"Sicrano", "Inventado"}}


def test_a_blank_field_followed_by_another_field_takes_nothing_below():
    # Codex CB-17: "Paciente:" does not end its line, so the next line is no value.
    words = line("Paciente:", "Idade:", "45", number=0) + line("Hemoglobina", "13,5", number=1)
    assert found(words) == {}
