"""Identifier terms and patterns, and how they are matched.

Matching is meant to survive the ways an identifier changes between the owner's list
and a document: case and accents are ignored, long terms are matched without spaces or
punctuation (OCR spacing errors, line breaks), and numbers are matched by their digits.
"""

from __future__ import annotations

import functools
import os
import re
import subprocess
import unicodedata
from collections.abc import Sequence
from dataclasses import dataclass, field

# Identifier patterns counted as hints. They are noisy (an exam date is a date), so
# they inform the reviewer and fail a check only on request. A number may break after
# a separator (`\s?` takes a line or page break), as OCR and page joins write it (Grok
# G0-18).
PATTERNS: dict[str, re.Pattern[str]] = {
    name: re.compile(pattern)
    for name, pattern in {
        "cpf": r"(?<!\d)\d{3}\.?\s?\d{3}\.?\s?\d{3}-?\s?\d{2}(?!\d)",
        "cns": r"(?<!\d)[1-2789]\d{2}\s?\d{4}\s?\d{4}\s?\d{4}(?!\d)",
        "phone": r"(?<!\d)\(?\d{2}\)?\s?9?\d{4}[-\s]?\s?\d{4}(?!\d)",
        # Anchored at the start of a run and bounded (64 is the longest local part), so
        # a long run with no "@" is tried once, not from every position.
        "email": r"(?<![\w.+-])[\w.+-]{1,64}@[\w-]+\.[\w.-]+",
        "cep": r"(?<!\d)\d{5}-\s?\d{3}(?!\d)",
        "date": r"(?<!\d)\d{1,2}/\s?\d{1,2}/\s?\d{2,4}(?!\d)",
        "crm": r"\bcrm\b",
    }.items()
}

# Terms whose letters-and-digits form is at least this long are matched without
# spaces or punctuation; shorter ones need whole words.
COMPACT_MIN = 6
# Terms with fewer letters and digits than this are not searched in raw bytes: binary
# data holds short letter runs by chance. Measured on the synthetic fixtures (0.5 MB of
# PDF and image bytes), 9 of 10 three-letter names matched by chance and no name of 4
# letters or more did (Grok G0-10).
RAW_MIN_LENGTH = 4
# Terms with at least this many digits are matched by their digits alone, whatever
# label or punctuation surrounds them ("CPF 123.456.789-09" is its digits).
NUMERIC_MIN_DIGITS = 5
# Digit runs separated by at most this many non-alphanumeric characters (spaces,
# punctuation, parentheses, line and page breaks) are one number: "(11) 98765-4321",
# a number broken across lines.
DIGIT_GAP_MAX = 3
DIGIT_RUN = re.compile(rf"\d(?:[\W_]{{0,{DIGIT_GAP_MAX}}}\d)*")


class TermsError(Exception):
    """The terms file cannot be used."""


# Every term and pattern folds the same large texts (raw bytes, pages): fold each once.
@functools.lru_cache(maxsize=256)
def fold(text: str) -> str:
    """Lower case without accents: what OCR and typing variants have in common."""
    decomposed = unicodedata.normalize("NFKD", text)
    return "".join(c for c in decomposed if not unicodedata.combining(c)).casefold()


def words(text: str) -> str:
    """The folded words of `text`, space-separated and space-padded."""
    return " " + " ".join(re.findall(r"[^\W_]+", fold(text))) + " "


def compact(text: str) -> str:
    """The folded letters and digits of `text`, nothing else."""
    return "".join(re.findall(r"[^\W_]+", fold(text)))


def digit_groups(text: str) -> list[str]:
    """Digit runs joined across the separators numbers are written with (Grok G0-1)."""
    return [re.sub(r"\D", "", m) for m in DIGIT_RUN.findall(text)]


@dataclass(frozen=True)
class Term:
    """One identifier from the owner's list."""

    raw: str
    digits: str = field(init=False)
    numeric: bool = field(init=False)
    compact: str | None = field(init=False)
    words: str = field(init=False)
    word_pattern: re.Pattern[str] = field(init=False, repr=False, compare=False)

    def __post_init__(self) -> None:
        digits = re.sub(r"\D", "", self.raw)
        letters = compact(self.raw)
        object.__setattr__(self, "digits", digits)
        # A label is not part of the identifier: any term with enough digits is matched
        # by them (Grok G0-2).
        object.__setattr__(self, "numeric", len(digits) >= NUMERIC_MIN_DIGITS)
        object.__setattr__(self, "compact", letters if len(letters) >= COMPACT_MIN else None)
        object.__setattr__(self, "words", words(self.raw))
        # A lookahead does not consume the space after a match, which the next adjacent
        # match needs: "a a" holds two "a" (Grok G0-28).
        object.__setattr__(self, "word_pattern", re.compile(f"(?={re.escape(self.words)})"))

    @property
    def whole_word(self) -> bool:
        """Matched as whole words: neither a number nor long enough for its compact form."""
        return not self.numeric and self.compact is None

    @property
    def searchable_in_binary(self) -> bool:
        """Terms shorter than `RAW_MIN_LENGTH` are not searched in raw bytes: binary
        data is full of short letter runs."""
        return self.numeric or len(compact(self.raw)) >= RAW_MIN_LENGTH

    def count(self, text: str) -> int:
        """Occurrences of the term in `text`."""
        if not text:
            return 0
        if self.numeric:
            return sum(group.count(self.digits) for group in digit_groups(text))
        if self.compact:
            return compact(text).count(self.compact)
        if self.words.strip():
            return len(self.word_pattern.findall(words(text)))
        return 0


# Patterns that are not numbers: searched in fully joined text too, which glues the
# numbers of a whole page together (Grok G0-40).
TEXT_PATTERNS = frozenset(("email", "crm"))


def count_patterns(
    texts: list[str], joins: Sequence[str] = (), joined: Sequence[str] = ()
) -> dict[str, int]:
    """The largest count of each pattern over `texts`, zero counts left out. In `joins`
    (readings that hold one occurrence in several overlapping joins, see
    `channels.mixed_readings`), distinct matches are counted. `joined` is searched for
    `TEXT_PATTERNS` only."""
    counts = {}
    for name, pattern in PATTERNS.items():
        plain = [*texts, *joined] if name in TEXT_PATTERNS else texts
        n = max(
            max((len(pattern.findall(fold(t))) for t in plain), default=0),
            max((len(set(pattern.findall(fold(t)))) for t in joins), default=0),
        )
        if n:
            counts[name] = n
    return counts


def committable(path: str) -> bool:
    """True when `path` lies in a git work tree and git does not ignore it."""
    directory = os.path.dirname(os.path.abspath(path))
    inside = subprocess.run(
        ["git", "-C", directory, "rev-parse", "--is-inside-work-tree"],
        capture_output=True,
        check=False,
    )
    if inside.returncode != 0 or inside.stdout.strip() != b"true":
        return False
    ignored = subprocess.run(
        ["git", "-C", directory, "check-ignore", "-q", os.path.abspath(path)],
        capture_output=True,
        check=False,
    )
    return ignored.returncode != 0


def load_terms(path: str) -> list[Term]:
    """Reads one term per line; blank lines and lines starting with # are skipped.

    The terms file is itself identifying, so one that git could commit is refused.
    """
    if committable(path):
        raise TermsError(
            "the terms file is inside a git work tree and not ignored; "
            "keep it outside the repository"
        )
    try:
        with open(path, encoding="utf-8") as f:
            lines = [line.strip() for line in f]
    except UnicodeDecodeError:
        # Its message holds bytes of a term: report the encoding only (Grok G0-4).
        raise TermsError("the terms file is not UTF-8 text; save it as UTF-8") from None
    terms = [Term(line) for line in lines if line and not line.startswith("#")]
    if not terms:
        raise TermsError("the terms file lists no term")
    return terms
