"""Detectors over words with boxes: what is found, and what is left alone."""

from anonimizador.detect import Detection, cns_valid, cpf_valid, detect_reading
from anonimizador.locate import Word
from anonimizador.terms import Term

# A valid CNS (weighted sum divisible by 11), invented.
CNS = "898001234567891"


def line(*texts: str, number: int = 0) -> list[Word]:
    """Words of one line, laid out left to right, each 100 px wide."""
    return [
        Word(t, (100 * i, 20 * number, 100 * i + 90, 20 * number + 15), ("ocr", 1, 1, number))
        for i, t in enumerate(texts)
    ]


def found(words: list[Word], terms=(), remove_dates=True) -> dict[str, set[str]]:
    """The words detected, by kind (the test knows the words; detections do not)."""
    by_box = {w.box: w.text for w in words}
    result: dict[str, set[str]] = {}
    for d in detect_reading(1, "ocr", words, [Term(t) for t in terms], remove_dates):
        result.setdefault(d.kind, set()).add(by_box[d.box])
    return result


def test_a_term_is_found_across_words_lines_and_ocr_spacing():
    words = line("Exame", "de", "Fula", "no", number=0) + line("Ficticio", "ok", number=1)
    assert found(words, ["Fulano Ficticio"]) == {"term": {"Fula", "no", "Ficticio"}}


def test_a_numeric_term_is_found_by_its_digits_whatever_the_punctuation():
    words = line("CPF:", "123.456.789-", "09")
    assert found(words, ["12345678909"])["term"] == {"123.456.789-", "09"}


def test_a_short_term_must_be_a_whole_word():
    assert found(line("Anabela", "Ana"), ["Ana"]) == {"term": {"Ana"}}


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


def test_a_number_label_without_a_number_takes_nothing():
    assert found(line("CRM", "SP", "assinatura")) == {}
    assert found(line("o", "crm", "do", "estado")) == {}


def test_laboratory_values_are_left_alone():
    words = line("Hemoglobina", "13,5", "g/dL", "12,0", "-", "16,0", number=0) + line(
        "Leucocitos", "7.500", "/mm3", "4.000", "a", "11.000", number=1
    )
    assert found(words, ["Fulano Ficticio", "12345678909"]) == {}


def test_a_detection_holds_no_text():
    [d] = detect_reading(1, "ocr", line("Paciente:", "Sicrano"), [], True)
    assert d == Detection(1, (100, 0, 190, 15), "label", "labels", "ocr")
