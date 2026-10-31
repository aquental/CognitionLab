"""Identifiers on a page, with their boxes (R4, first part).

Three detectors run over each reading of a page (the text layer and the OCR):
- `terms`: the owner's terms, matched as `verify` counts them (case, accents, spacing,
  and digits);
- `patterns`: CPF and CNS with valid check digits, phone, e-mail, CEP, and dates;
- `labels`: the value after a field label such as `Paciente:` or `CRM`, beside it or
  under it; a `Nome`, `Paciente`, or date-of-birth label that starts its line needs no
  colon.

A detection holds its page, box, kind, detector, and reading, never its text.
"""

from __future__ import annotations

import re
from collections import Counter
from collections.abc import Callable
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
    "data nasc",
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
# A word of a number's value: digits, which may start with a state ("MG-12.345.678",
# Codex CB-23) or "nº" ("Nº123456", Codex CB-33) and end in the check letter X of an
# RG (Codex CB-6) or in a state ("123456-SP", Codex CB-19); a state; "nº"; or a
# separator.
STATE = "(?:" + "|".join(STATES.split()) + ")"
NUMBER_PART = re.compile(
    r"^[\W_]*(?:(?:(?:(?:"
    + STATE
    + r"|n[o°º]?)[\W_]*)?\d[\d\W_]*(?:(?:x|"
    + STATE
    + r")[\W_]*)?|"
    + STATE
    + r"|n[o°º]?)[\W_]*)?$"
)
SEPARATOR = re.compile(r"^[\W_]+$")
# The words a label alone is no value of ("Nome do exame:").
PARTICLES = ("de", "do", "da", "dos", "das")
# The widest gap between the words of a value, in heights of the label: a wider one
# separates the value from the next field.
GAP_IN_LABEL_HEIGHTS = 2
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
            # The text is folded, so the term's digits are too: fullwidth digits
            # become ASCII, as `Term.count` compares them (Codex CB-28).
            spans += spans_of(fold(term.digits), digits, positions)
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


def ends_field(words: list[Word], seq: list[int], k: int) -> bool:
    """Whether `seq[k]` starts the next field: it holds a colon ("Idade:", "Idade:45",
    ":"), or the word after it starts with one ("Idade" ":") (Antigravity AB-1)."""
    # Folded, as labels are matched: a fullwidth colon is a colon (Codex CB-34).
    return ":" in fold(words[seq[k]].text) or (
        k + 1 < len(seq) and fold(words[seq[k + 1]].text).startswith(":")
    )


def colon_value(words: list[Word], line: list[int], last: int) -> list[int]:
    """The words after the label word `last` in `line`, up to the next field."""
    after = [i for i in line if i > last]
    value = []
    for k, index in enumerate(after):
        if ends_field(words, after, k):
            break
        value.append(index)
    return value


def row_after(words: list[Word], index: int) -> list[int]:
    """The words of the page of `index` to its right whose middle lies within its height,
    left to right: its row, whatever line the extractor put each one on (Codex CB-12)."""
    _, top, right, bottom = words[index].box
    row = [
        i
        for i, w in enumerate(words)
        if w.line[0] == words[index].line[0]
        and (w.box[0] + w.box[2]) / 2 > right
        and top <= (w.box[1] + w.box[3]) / 2 <= bottom
    ]
    return sorted(row, key=lambda i: words[i].box[0])


def run_from(words: list[Word], row: list[int], edge: int, reach: float, gap: float) -> list[int]:
    """The words of `row` up to a label or a gap wider than `gap`; the first one must
    start within `reach` of `edge`."""
    value: list[int] = []
    for k, index in enumerate(row):
        box = words[index].box
        if box[0] - edge > (gap if value else reach) or ends_field(words, row, k):
            break
        value.append(index)
        edge = box[2]
    return value


def column_below(words: list[Word], label: int, gap: float) -> list[int]:
    """The value under a column heading: the nearest word below `label` on its page that
    starts where the heading starts, and the words after it on its row."""
    left, top, _, bottom = words[label].box
    under = [
        i
        for i, w in enumerate(words)
        if w.line[0] == words[label].line[0]
        and w.box[1] > (top + bottom) / 2
        and w.box[1] - bottom <= gap
        and abs(w.box[0] - left) <= gap / 2
    ]
    if not under:
        return []
    first = min(under, key=lambda i: words[i].box[1])
    return [first, *run_from(words, row_after(words, first), words[first].box[2], gap, gap)]


def label_gap(words: list[Word], label: int) -> float:
    return GAP_IN_LABEL_HEIGHTS * (words[label].box[3] - words[label].box[1])


def beside(words: list[Word], last: int, reach: float) -> list[int]:
    """The value to the right of the label word `last`, on its row."""
    row = row_after(words, last)
    return run_from(words, row, words[last].box[2], reach, label_gap(words, last))


def content(words: list[Word], value: list[int]) -> list[int]:
    """The words of `value` that are not particles or separators: a dash between a label
    and its value is neither a name nor its absence (Codex CB-24)."""
    return [
        i
        for i in value
        if fold(words[i].text) not in PARTICLES and not SEPARATOR.match(words[i].text)
    ]


def not_particles(words: list[Word], value: list[int]) -> bool:
    return bool(content(words, value))


def name_shaped(words: list[Word], value: list[int]) -> bool:
    """Capitalized words, with particles and separators, as a name is written; running
    text is not."""
    words_of_name = content(words, value)
    # A dash glued to the first word ("-Sicrano") is not its initial (Codex CB-26).
    return bool(words_of_name) and all(
        re.sub(r"^[\W_]+", "", words[i].text)[:1].isupper() for i in words_of_name
    )


def has_digit(words: list[Word], value: list[int]) -> bool:
    return any(c.isdigit() for i in value for c in words[i].text)


# A test of a value, by its words.
Accepts = Callable[[list[Word], list[int]], bool]
# Labels taken without a colon, but only as the first words of their line: a form can
# print "Nome" with the name beside it, or above it as a column heading. Each takes only
# a value its test accepts: "Paciente" starts running text too, a name does not.
BARE_LABELS: dict[str, Accepts] = {
    "nome do paciente": not_particles,
    "nome da mae": not_particles,
    "nome do pai": not_particles,
    "nome social": not_particles,
    "nome": not_particles,
    "paciente": name_shaped,
    "mae": name_shaped,
    "data de nascimento": has_digit,
    "data nasc": has_digit,
    "dt nasc": has_digit,
    "nascimento": has_digit,
}
# Border or bullet characters OCR reads before a label ("| Nome") are skipped
# (Antigravity AB-10). Every bare label is a colon label too, so it keeps its value
# when it has a colon (Antigravity AB-8).
BARE_LABEL = re.compile(
    r"[\W_]*(?P<label>"
    + "|".join(
        r"[\W_]+".join(map(re.escape, label.split()))
        for label in sorted(BARE_LABELS, key=len, reverse=True)
    )
    + r")(?![^\W_])"
)


def bare_value(words: list[Word], label: set[int], accepts: Accepts) -> list[int]:
    """The value of a label without a colon: beside it, else under it as under a column
    heading, else the next words of its row, however far; the first that `accepts`."""
    first, last = min(label), max(label)
    gap = label_gap(words, first)
    for value in (
        beside(words, last, gap),
        column_below(words, first, gap),
        beside(words, last, float("inf")),
    ):
        if value and accepts(words, value):
            return value
    return []


def number_value(words: list[Word], seq: list[int]) -> list[int]:
    """The words that start `seq` and make a number, without its separators. A value
    holds a digit: "CRM SP" alone, or a label in prose, is none."""
    taken: list[int] = []
    for index in seq:
        # Only words with digits count toward the limit: not separators read as words
        # of their own (Codex CB-27), nor a state or "nº" before them (Codex CB-31).
        if not NUMBER_PART.match(fold(words[index].text)) or (
            has_digit(words, [index])
            and sum(has_digit(words, [i]) for i in taken) >= NUMBER_VALUE_WORDS
        ):
            break
        taken.append(index)
    if not has_digit(words, taken):
        return []
    return [i for i in taken if not SEPARATOR.match(words[i].text)]


def after_label(word: str, folded_length: int) -> str:
    """What follows, in `word`, the first `folded_length` characters of its folded form,
    without the separators between them."""
    prefix = next(k for k in range(len(word) + 1) if len(fold(word[:k])) >= folded_length)
    return re.sub(r"^[\W_]+", "", word[prefix:])


def end_of(text: Text, start: int) -> int:
    """Where the word that owns the character at `start - 1` ends in the folded text."""
    word, end = text.owner[start - 1], start
    while end < len(text.owner) and text.owner[end] == word:
        end += 1
    return end


def label_values(words: list[Word], lines: list[list[int]]) -> list[int]:
    """The words that are the value of a field label."""
    values = []
    for line in lines:
        text = join(words, [line])
        # A label with a colon is a colon label, whose value is searched the same way.
        m = BARE_LABEL.match(text.folded)
        if m and not COLON_LABEL.match(text.folded, m.start("label")):
            accepts = BARE_LABELS[" ".join(re.findall(r"[^\W_]+", m.group("label")))]
            label = text.words(m.start("label"), m.end("label"))
            last = max(label)
            # A value glued to the label ("Paciente-Sicrano") is in the label's word,
            # as with a colon label (Codex CB-30).
            glued = after_label(words[last].text, m.end("label") - text.owner.index(last))
            if glued and accepts([Word(glued, words[last].box, words[last].line)], [0]):
                values.append(last)
            values += bare_value(words, label, accepts)
        for m in COLON_LABEL.finditer(text.folded):
            # The value starts after the word holding the colon, or in it when it is
            # glued to the colon ("Paciente:Fulano").
            label = text.words(m.start(), m.end())
            first, last = min(label), max(label)
            # Read from the folded text, where a fullwidth colon is a colon (Codex CB-32).
            glued = any(c.isalnum() for c in text.folded[m.end() : end_of(text, m.end())])
            value = colon_value(words, line, last)
            if glued:
                values.append(last)
            elif not any(i > last for i in line):
                # A label that ends its extracted line has its value on its row, in
                # another block (Antigravity AB-2), or under it, as a date of birth
                # under its label (Codex CB-7, Antigravity AB-3). Beside it within a gap
                # is its value; else both places are taken, since a far word on its row
                # can be another heading (Antigravity AB-9). A blank field followed by
                # another field on its line has none (Codex CB-17).
                gap = label_gap(words, first)
                value = beside(words, last, gap) or [
                    *column_below(words, first, gap),
                    *beside(words, last, float("inf")),
                ]
            values += value
        for m in NUMBER_LABEL.finditer(text.folded):
            label_words = text.words(m.start(), m.end())
            first, last = min(label_words), max(label_words)
            after = [i for i in line if i > last]
            number = number_value(words, after)
            if not number and all(NUMBER_PART.match(fold(words[i].text)) for i in after):
                # A label whose line holds no number after it, at most a state or a
                # colon ("CRM SP", "RG :"): its number is on its row, in another block,
                # else under it (Codex CB-18, Antigravity AB-9).
                number = number_value(words, row_after(words, last)) or number_value(
                    words, column_below(words, first, label_gap(words, first))
                )
            values += number
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
