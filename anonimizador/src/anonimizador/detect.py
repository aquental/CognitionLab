"""Identifiers on a page, with their boxes (R4, first part).

Three detectors run over each reading of a page (the text layer and the OCR):
- `terms`: the owner's terms, matched as `verify` counts them (case, accents, spacing,
  and digits);
- `patterns`: CPF and CNS with valid check digits, phone, e-mail, CEP, and dates;
- `labels`: the value after a field label such as `Paciente:` or `CRM`.

A detection holds its page, box, kind, detector, and reading, never its text.
"""

from __future__ import annotations

import re
from collections import Counter
from dataclasses import asdict, dataclass

from anonimizador.locate import Box, Page, Word
from anonimizador.terms import DIGIT_RUN, PATTERNS, Term, fold

# Patterns redacted without a label. "crm" is a label here, and a date only under the
# "remove" date policy (R6).
REDACTED_PATTERNS = ("cpf", "cns", "phone", "email", "cep")
# Field labels (folded) whose value runs to the end of the line, or to the next label.
# They need a colon, so the same word in running text is not a label.
COLON_LABELS = (
    "paciente",
    "nome do paciente",
    "nome",
    "nome social",
    "nome da mae",
    "mae",
    "nome do pai",
    "pai",
    "responsavel",
    "prontuario",
    "atendimento",
    "pedido",
    "requisicao",
    "registro",
    "matricula",
    "carteirinha",
    "carteira",
    "convenio",
    "plano",
    "guia",
    "senha",
    "medico",
    "medica",
    "medico solicitante",
    "medico responsavel",
    "solicitante",
    "requisitante",
    "data de nascimento",
    "nascimento",
    "dt nasc",
    "d n",
    "endereco",
    "telefone",
    "celular",
    "e mail",
    "email",
    "cartao sus",
)
# Labels of a number: the words after them that hold a digit (with a state's two
# letters and separators) are their value, colon or not.
NUMBER_LABELS = ("crm", "cpf", "rg", "cns", "cnes", "crbm", "crf", "coren")
# How many words after a number label can belong to its value: "CRM/SP 123456",
# "CRM 123.456 - SP".
NUMBER_VALUE_WORDS = 4
# The states a council number is registered in ("CRM/SP").
STATES = "ac al ap am ba ce df es go ma mt ms mg pa pb pr pe pi rj rn rs ro rr sc sp se to"
# A word of a number's value: digits (an RG may end in the check letter X, Codex CB-6),
# a state, "nº", or a separator.
NUMBER_PART = re.compile(
    r"^[\W_]*(?:(?:\d[\d\W_]*(?:x[\W_]*)?|(?:" + "|".join(STATES.split()) + r")|n[o°º]?)[\W_]*)?$"
)
SEPARATOR = re.compile(r"^[\W_]+$")
LABEL_PREFIX = r"(?<![^\W_])"
COLON_LABEL = re.compile(
    LABEL_PREFIX
    + r"(?:"
    + "|".join(r"[\W_]*".join(map(re.escape, label.split())) for label in COLON_LABELS)
    + r")[\s.]*:"
)
NUMBER_LABEL = re.compile(
    LABEL_PREFIX + r"(?:" + "|".join(map(re.escape, NUMBER_LABELS)) + r")(?![^\W_\d])"
)


@dataclass(frozen=True)
class Detection:
    page: int
    box: Box
    kind: str
    detector: str
    reading: str


@dataclass(frozen=True)
class Text:
    """Words joined into folded text, lines by newlines, with the word each character
    came from (-1 for a separator)."""

    folded: str
    owner: tuple[int, ...]

    def words(self, start: int, end: int) -> set[int]:
        return {w for w in self.owner[start:end] if w >= 0}


def lines_of(words: list[Word]) -> list[list[int]]:
    """Word indices grouped by line, lines in the order their first word comes."""
    lines: dict[tuple, list[int]] = {}
    for index, word in enumerate(words):
        lines.setdefault(word.line, []).append(index)
    return list(lines.values())


def join(words: list[Word], lines: list[list[int]]) -> Text:
    chars: list[str] = []
    owner: list[int] = []
    for n, line in enumerate(lines):
        for k, index in enumerate(line):
            if n or k:
                chars.append(" " if k else "\n")
                owner.append(-1)
            for c in words[index].text:
                folded = fold(c)
                chars.append(folded)
                owner.extend([index] * len(folded))
    return Text("".join(chars), tuple(owner))


def alnum_positions(text: str) -> list[int]:
    return [k for k, c in enumerate(text) if c.isalnum()]


def term_spans(term: Term, text: Text) -> list[tuple[int, int]]:
    """Where `term` is in `text`, by the rules of `Term.count`."""
    folded = text.folded
    spans = []
    if term.numeric:
        for run in DIGIT_RUN.finditer(folded):
            positions = [k for k in range(run.start(), run.end()) if folded[k].isdigit()]
            digits = "".join(folded[k] for k in positions)
            spans += spans_of(term.digits, digits, positions)
    elif term.compact:
        positions = alnum_positions(folded)
        spans += spans_of(term.compact, "".join(folded[k] for k in positions), positions)
    elif parts := term.words.split():
        # In a lookahead, so overlapping occurrences ("li li li" holds "li li" twice) are
        # all found, as `Term.count` counts them (Codex CB-10).
        pattern = LABEL_PREFIX + r"[\W_]+".join(map(re.escape, parts)) + r"(?![^\W_])"
        spans += [m.span(1) for m in re.finditer(f"(?=({pattern}))", folded)]
    return spans


def spans_of(needle: str, haystack: str, positions: list[int]) -> list[tuple[int, int]]:
    """Every occurrence of `needle` in `haystack`, as spans of the text whose characters
    `positions` lists."""
    spans = []
    start = haystack.find(needle)
    while needle and start >= 0:
        spans.append((positions[start], positions[start + len(needle) - 1] + 1))
        start = haystack.find(needle, start + 1)
    return spans


def cpf_valid(digits: str) -> bool:
    if len(digits) != 11 or len(set(digits)) == 1:
        return False
    for n in (9, 10):
        total = sum(int(d) * w for d, w in zip(digits[:n], range(n + 1, 1, -1), strict=True))
        if total * 10 % 11 % 10 != int(digits[n]):
            return False
    return True


def cns_valid(digits: str) -> bool:
    return len(digits) == 15 and sum(int(d) * (15 - i) for i, d in enumerate(digits)) % 11 == 0


VALIDATORS = {"cpf": cpf_valid, "cns": cns_valid}


def pattern_spans(text: Text, remove_dates: bool) -> list[tuple[str, tuple[int, int]]]:
    names = (*REDACTED_PATTERNS, "date") if remove_dates else REDACTED_PATTERNS
    found = []
    for name in names:
        valid = VALIDATORS.get(name)
        for m in PATTERNS[name].finditer(text.folded):
            if valid is None or valid(re.sub(r"\D", "", m.group())):
                found.append((name, m.span()))
    return found


def colon_value(words: list[Word], line: list[int], last: int) -> list[int]:
    """The words after the label word `last` in `line`, up to the next label."""
    value = []
    for index in (i for i in line if i > last):
        if words[index].text.endswith(":"):
            break
        value.append(index)
    return value


def label_values(words: list[Word], lines: list[list[int]]) -> list[int]:
    """The words that are the value of a field label."""
    values = []
    for k, line in enumerate(lines):
        text = join(words, [line])
        for m in COLON_LABEL.finditer(text.folded):
            # The value starts after the word holding the colon, or in it when it is
            # glued to the colon ("Paciente:Fulano").
            last = max(text.words(m.start(), m.end()))
            glued = any(c.isalnum() for c in words[last].text.partition(":")[2])
            value = colon_value(words, line, last)
            if glued:
                values.append(last)
            elif not value and k + 1 < len(lines):
                # A label that ends its line has its value on the next line of the same
                # page (a date-of-birth label above the date, Codex CB-7).
                below = lines[k + 1]
                if words[below[0]].line[0] == words[last].line[0]:
                    value = colon_value(words, below, below[0] - 1)
            values += value
        for m in NUMBER_LABEL.finditer(text.folded):
            label_words = text.words(m.start(), m.end())
            after = [i for i in line if i > max(label_words)][:NUMBER_VALUE_WORDS]
            taken = []
            for index in after:
                if not NUMBER_PART.match(fold(words[index].text)):
                    break
                taken.append(index)
            # A value holds a digit: "CRM SP" alone, or a label in prose, is none.
            if any(any(c.isdigit() for c in words[i].text) for i in taken):
                values += [i for i in taken if not SEPARATOR.match(words[i].text)]
            # A number glued to its label ("CRM:123") is in the label's word, however
            # short (Codex CB-11).
            values += [i for i in label_words if any(c.isdigit() for c in words[i].text)]
    return values


def detect_reading(
    reading: str, pages: list[tuple[int, list[Word]]], terms: list[Term], remove_dates: bool
) -> list[Detection]:
    """Detections in one reading of every page. The pages are searched as one text, a
    line break between pages, so a term split across a page break is found, as `verify`
    finds it (Codex CB-5). A word's line key starts with its page number."""
    words = [Word(w.text, w.box, (number, *w.line)) for number, ws in pages for w in ws]
    if not words:
        return []
    lines = lines_of(words)
    text = join(words, lines)
    marked: list[tuple[int, str, str]] = []
    for term in terms:
        for start, end in term_spans(term, text):
            marked += [(w, "term", "terms") for w in text.words(start, end)]
    for name, (start, end) in pattern_spans(text, remove_dates):
        marked += [(w, name, "patterns") for w in text.words(start, end)]
    marked += [(w, "label", "labels") for w in label_values(words, lines)]
    unique = dict.fromkeys(marked)
    return [
        Detection(words[w].line[0], words[w].box, kind, det, reading) for w, kind, det in unique
    ]


def detect(pages: list[Page], terms: list[Term], remove_dates: bool = True) -> list[Detection]:
    """Every detection on every page, in both readings."""
    native = [(p.number, p.native) for p in pages]
    ocr = [(p.number, p.ocr) for p in pages]
    return detect_reading("native", native, terms, remove_dates) + detect_reading(
        "ocr", ocr, terms, remove_dates
    )


def summary(detections: list[Detection]) -> dict[str, int]:
    """Detections per kind, the only form in which they are reported."""
    return dict(sorted(Counter(d.kind for d in detections).items()))


def as_json(detections: list[Detection], pages: list[Page]) -> dict:
    return {
        "pages": [
            {"number": p.number, "size": list(p.size), "text_layer_unaligned": p.unaligned}
            for p in pages
        ],
        "detections": [asdict(d) for d in detections],
        "counts": summary(detections),
    }
