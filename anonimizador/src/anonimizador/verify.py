"""`anonimizador verify`: count the owner's identifiers left in corpus files.

The report holds counts only, never a term, a match, or document text, and names
files by position unless asked otherwise.
"""

from __future__ import annotations

import os
import shutil
import tempfile
from collections.abc import Iterator
from concurrent.futures import Executor, ThreadPoolExecutor
from dataclasses import dataclass

from anonimizador.channels import CHANNELS, PATTERN_CHANNELS, read_image, read_pdf
from anonimizador.terms import Term, count_patterns

SUFFIXES = {".pdf": "pdf", ".png": "image", ".jpg": "image", ".jpeg": "image"}


class VerifyError(Exception):
    """The verification cannot start."""


@dataclass(frozen=True)
class Options:
    lang: str = "por+eng"
    dpi: int = 300
    use_ocr: bool = True
    show_names: bool = False
    strict_patterns: bool = False


@dataclass(frozen=True)
class TermHit:
    """Where one term (by its position in the terms file) was found in one file."""

    number: int
    counts: dict[str, int]

    @property
    def hidden(self) -> bool:
        """In the file but not on the page: text under a drawn box."""
        c = self.counts
        return bool(c["text"] or c["raw"]) and not (c["ocr"] or c["images"])


@dataclass(frozen=True)
class FileResult:
    index: int
    path: str
    hits: list[TermHit]
    hints: dict[str, int]

    def failed(self, strict_patterns: bool) -> bool:
        return bool(self.hits) or (strict_patterns and bool(self.hints))


def missing_tools(use_ocr: bool) -> list[str]:
    tools = ["pdftotext", "pdfinfo"]
    if use_ocr:
        tools += ["pdftoppm", "pdfimages", "tesseract"]
    return [t for t in tools if shutil.which(t) is None]


def collect(paths: list[str]) -> list[str]:
    """The PDF, PNG, and JPEG files named or found under directories (hidden entries
    skipped), in path order."""
    files = []
    for position, path in enumerate(paths, start=1):
        if os.path.isdir(path):
            for root, dirs, names in os.walk(path):
                dirs[:] = sorted(d for d in dirs if not d.startswith("."))
                for name in sorted(names):
                    if not name.startswith(".") and os.path.splitext(name)[1].lower() in SUFFIXES:
                        files.append(os.path.join(root, name))
        elif os.path.isfile(path):
            files.append(path)
        else:
            raise VerifyError(f"not a file or directory: argument {position}")
    if not files:
        raise VerifyError("no PDF, PNG, or JPEG file to check")
    return sorted(set(files))


def check_file(
    index: int, path: str, terms: list[Term], options: Options, pool: Executor
) -> FileResult:
    with tempfile.TemporaryDirectory() as tmp:
        if SUFFIXES.get(os.path.splitext(path)[1].lower()) == "pdf":
            texts = read_pdf(path, tmp, options.lang, options.dpi, options.use_ocr, pool)
        else:
            texts = read_image(path, options.lang, options.use_ocr)
    hits = []
    for number, term in enumerate(terms, start=1):
        counts = {}
        for channel in CHANNELS:
            searchable = channel != "raw" or term.searchable_in_binary
            counts[channel] = (
                max((term.count(t) for t in texts[channel]), default=0) if searchable else 0
            )
        if any(counts.values()):
            hits.append(TermHit(number, counts))
    hints = count_patterns([t for c in PATTERN_CHANNELS for t in texts[c]])
    return FileResult(index, path, hits, hints)


def verify(files: list[str], terms: list[Term], options: Options) -> Iterator[FileResult]:
    with ThreadPoolExecutor(max_workers=os.cpu_count() or 2) as pool:
        for index, path in enumerate(files, start=1):
            yield check_file(index, path, terms, options, pool)


def render(result: FileResult, options: Options) -> list[str]:
    """Report lines for one file: counts, positions, and kinds only."""
    label = f"[{result.index}] {result.path}" if options.show_names else f"[{result.index}]"
    failed = result.failed(options.strict_patterns)
    lines = [f"{label} {'FOUND' if failed else 'ok'}"]
    for hit in result.hits:
        where = " ".join(f"{c}={hit.counts[c]}" for c in CHANNELS if hit.counts[c])
        lines.append(f"    term {hit.number}: {where}")
        if options.use_ocr and hit.hidden:
            lines.append(
                f"    hint: term {hit.number} is in the file but not visible on the page: "
                "text under a drawn box? Redact for real (docs/ANONYMIZATION.md)."
            )
    if result.hints:
        lines.append(
            "    possible identifiers (patterns): "
            + " ".join(f"{k}={v}" for k, v in sorted(result.hints.items()))
        )
    return lines
