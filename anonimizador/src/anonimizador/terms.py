"""Identifier terms and patterns, and how they are matched.

Matching is meant to survive the ways an identifier changes between the owner's list
and a document: case and accents are ignored, long terms are matched without spaces or
punctuation (OCR spacing errors, line breaks), and numbers are matched by their digits.
"""

from __future__ import annotations

import os
import re
import subprocess
import unicodedata
from dataclasses import dataclass, field

# Identifier patterns counted as hints. They are noisy (an exam date is a date), so
# they inform the reviewer and fail a check only on request.
PATTERNS: dict[str, re.Pattern[str]] = {
    name: re.compile(pattern)
    for name, pattern in {
        "cpf": r"(?<!\d)\d{3}\.?\d{3}\.?\d{3}-?\d{2}(?!\d)",
        "cns": r"(?<!\d)[1-2789]\d{2} ?\d{4} ?\d{4} ?\d{4}(?!\d)",
        "phone": r"(?<!\d)\(?\d{2}\)? ?9?\d{4}[- ]?\d{4}(?!\d)",
        "email": r"[\w.+-]+@[\w-]+\.[\w.-]+",
        "cep": r"(?<!\d)\d{5}-\d{3}(?!\d)",
        "date": r"(?<!\d)\d{1,2}/\d{1,2}/\d{2,4}(?!\d)",
        "crm": r"\bcrm\b",
    }.items()
}

# Terms whose letters-and-digits form is at least this long are matched without
# spaces or punctuation; shorter ones need whole words.
COMPACT_MIN = 6


class TermsError(Exception):
    """The terms file cannot be used."""


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
    """Digit runs joined across the separators numbers are written with."""
    return [re.sub(r"\D", "", m) for m in re.findall(r"\d[\d.\-/ ]*\d|\d", text)]


@dataclass(frozen=True)
class Term:
    """One identifier from the owner's list."""

    raw: str
    digits: str = field(init=False)
    numeric: bool = field(init=False)
    compact: str | None = field(init=False)
    words: str = field(init=False)

    def __post_init__(self) -> None:
        digits = re.sub(r"\D", "", self.raw)
        letters = compact(self.raw)
        object.__setattr__(self, "digits", digits)
        object.__setattr__(self, "numeric", len(digits) >= 5 and len(digits) >= 0.8 * len(letters))
        object.__setattr__(self, "compact", letters if len(letters) >= COMPACT_MIN else None)
        object.__setattr__(self, "words", words(self.raw))

    @property
    def searchable_in_binary(self) -> bool:
        """Short word terms are not searched in raw bytes: binary data is full of short
        letter runs."""
        return self.numeric or self.compact is not None

    def count(self, text: str) -> int:
        """Occurrences of the term in `text`."""
        if not text:
            return 0
        if self.numeric:
            return sum(group.count(self.digits) for group in digit_groups(text))
        if self.compact:
            return compact(text).count(self.compact)
        if self.words.strip():
            return words(text).count(self.words)
        return 0


def count_patterns(texts: list[str]) -> dict[str, int]:
    """The largest count of each pattern over `texts`, zero counts left out."""
    counts = {}
    for name, pattern in PATTERNS.items():
        n = max((len(pattern.findall(fold(t))) for t in texts), default=0)
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
    with open(path, encoding="utf-8") as f:
        lines = [line.strip() for line in f]
    terms = [Term(line) for line in lines if line and not line.startswith("#")]
    if not terms:
        raise TermsError("the terms file lists no term")
    return terms
