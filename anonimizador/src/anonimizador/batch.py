"""`anonimizador batch`: run the anonymization steps over every original in a directory
and write a Markdown index of the run.

Each original gets a neutral output name, `exam-<digest>.<suffix>`, from a digest of
its bytes. The console names originals by position and carries counts only, like
`verify`. The index, `<YYYYMMDD-HHMMSS>-anonimizer.md`, links every original by its
name, so it and the run's logs are written only where git cannot commit them.

Stage 0 has no redaction yet: the index marks each anonymized copy as pending, and only
the originals are verified.
"""

from __future__ import annotations

import hashlib
import os
import sys
from collections.abc import Iterator
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import TextIO
from urllib.parse import quote

from anonimizador.terms import Term, committable
from anonimizador.verify import (
    APPLEDOUBLE,
    SUFFIXES,
    CorpusFile,
    FileResult,
    Options,
    VerifyError,
    render,
    verify,
)

REPORT_SUFFIX = "-anonimizer.md"
STAMP_FORMAT = "%Y%m%d-%H%M%S"
# Hex digits of the content digest in a neutral name: enough to tell a few hundred
# exams apart, short enough to read.
DIGEST_LENGTH = 12
PENDING = "pending (stage 2)"
NO_TERMS = (
    "no terms file: only identifier patterns were counted, so `ok` does not mean that "
    "no name is left (pass --terms)"
)


@dataclass(frozen=True)
class Exam:
    """An original to anonymize: its position in the run, its path, and the neutral
    name of its anonymized copy."""

    position: int
    original: Path
    neutral: str

    @property
    def log_prefix(self) -> str:
        return f"{self.position:02d}-{Path(self.neutral).stem}"


@dataclass
class Outcome:
    """What the run did with one exam, as the index links it."""

    exam: Exam
    original_status: str = ""
    original_log: Path | None = None
    anonymized: Path | None = None
    anonymized_status: str = PENDING
    anonymized_log: Path | None = None


@dataclass
class Run:
    stamp: str
    out: Path
    logs: Path = field(init=False)
    report: Path = field(init=False)

    def __post_init__(self) -> None:
        self.logs = self.out / "logs" / self.stamp
        self.report = self.out / f"{self.stamp}{REPORT_SUFFIX}"


class Console:
    """Writes each line to the terminal and to the run's console log."""

    def __init__(self, terminal: TextIO, log: TextIO) -> None:
        self.terminal = terminal
        self.log = log

    def print(self, line: str) -> None:
        for stream in (self.terminal, self.log):
            print(line, file=stream, flush=True)


def neutral_name(path: Path) -> str:
    """`exam-<digest>.<suffix>`: names the copy by its original's bytes, never its name."""
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return f"exam-{digest.hexdigest()[:DIGEST_LENGTH]}{path.suffix.lower()}"


def originals(source: Path) -> list[Exam]:
    """The PDF, PNG, and JPEG files directly in `source`, in name order. Directories
    are not walked: the outputs usually live in one under `source`."""
    try:
        paths = sorted(
            p
            for p in source.iterdir()
            if p.is_file() and not p.name.startswith(APPLEDOUBLE) and p.suffix.lower() in SUFFIXES
        )
    except OSError as e:
        # The message of an OSError holds the path: report its reason only.
        raise VerifyError(f"cannot list the originals ({e.strerror or type(e).__name__})") from None
    if not paths:
        raise VerifyError("no PDF, PNG, or JPEG original to anonymize")
    return [Exam(n, p, neutral_name(p)) for n, p in enumerate(paths, start=1)]


def start(out: Path, now: datetime) -> Run:
    """Makes the output and log directories, refusing any place git could commit:
    the index and the logs name the originals."""
    run = Run(now.strftime(STAMP_FORMAT), out)
    if committable(str(run.report)) or committable(str(run.logs / "console.txt")):
        raise VerifyError(
            "the output directory is inside a git work tree and not ignored; "
            "the index names the originals, so keep it where git cannot commit it"
        )
    run.logs.mkdir(parents=True, exist_ok=False)
    return run


def write_log(path: Path, lines: list[str]) -> None:
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def verify_originals(
    exams: list[Exam], terms: list[Term], options: Options, run: Run, console: Console
) -> Iterator[tuple[Outcome, FileResult]]:
    files = [CorpusFile(str(e.original), e.original.name) for e in exams]
    for exam, result in zip(exams, verify(files, terms, options), strict=True):
        lines = render(result, options)
        log = run.logs / f"{exam.log_prefix}-verify-original.txt"
        write_log(log, lines)
        for line in lines:
            console.print(line.replace(f"[{result.index}]", f"[{exam.position}] original:", 1))
        console.print(f"[{exam.position}] anonymized: {PENDING}")
        outcome = Outcome(exam, result.status(options.strict_patterns), log)
        yield outcome, result


def link(target: Path, base: Path, text: str) -> str:
    """A Markdown link to `target`, relative to the directory `base`."""
    href = quote(os.path.relpath(target, base).replace(os.sep, "/"))
    label = text.replace("\\", "\\\\").replace("|", "\\|").replace("[", "\\[").replace("]", "\\]")
    return f"[{label}]({href})"


def index(run: Run, outcomes: list[Outcome], header: list[str], console_log: Path) -> str:
    base = run.report.parent
    rows = []
    for o in outcomes:
        original = link(o.exam.original, base, o.exam.original.name)
        anonymized = link(o.anonymized, base, o.exam.neutral) if o.anonymized else PENDING
        verified = link(o.original_log, base, o.original_status) if o.original_log else "—"
        after = link(o.anonymized_log, base, o.anonymized_status) if o.anonymized_log else "—"
        rows.append(f"| {o.exam.position} | {original} | {anonymized} | {verified} | {after} |")
    return "\n".join(
        [
            f"# Anonymization run {run.stamp}",
            "",
            "> **This index names the originals.** It stays on this machine: never commit, "
            "push, or send it, or the logs beside it.",
            "",
            *header,
            f"- Console output of the whole run: {link(console_log, base, console_log.name)}",
            "",
            "| # | Original | Anonymized | Verify original | Verify anonymized |",
            "|---:|---|---|---|---|",
            *rows,
            "",
            "A verify log holds counts per term (by its line in the terms file) and per place, "
            "never a value. `FOUND` on an original is expected: it is the count before "
            "anonymization.",
            "",
            "## Before using an anonymized copy",
            "",
            "- **A person reviews every copy.** No detector is complete: handwriting, stamps, "
            "poor scans, and unusual names defeat OCR and name recognition, and rare "
            "combinations of diagnosis, date, and institution can identify a person.",
            "- Only an explicit human approval records the review in the manifest "
            "(`docs/SPEC.md` R10).",
            "",
        ]
    )


def run_batch(
    source: Path, out: Path, terms: list[Term], options: Options, now: datetime | None = None
) -> tuple[int, int]:
    """Runs every step over the originals in `source` and writes the index under `out`.
    Returns the number of exams and of unreadable ones; raises `VerifyError` when the
    run cannot start."""
    exams = originals(source)
    now = now or datetime.now()
    run = start(out, now)
    console_log = run.logs / "console.txt"
    mode = "no OCR" if not options.use_ocr else f"OCR at {options.dpi} dpi ({options.lang})"
    header = [
        f"- Started: {now.isoformat(timespec='seconds')}",
        f"- Originals: {len(exams)}; terms: {len(terms)}; {mode}",
    ]
    if not terms:
        header.append(f"- **{NO_TERMS}**")
    unreadable = 0
    outcomes = []
    with console_log.open("w", encoding="utf-8") as log:
        console = Console(sys.stdout, log)
        console.print(f"anonimizador batch: {len(exams)} original(s), {len(terms)} term(s), {mode}")
        if not terms:
            console.print(f"anonimizador batch: warning: {NO_TERMS}")
        for outcome, result in verify_originals(exams, terms, options, run, console):
            unreadable += bool(result.errors)
            outcomes.append(outcome)
        header.append(f"- Finished: {datetime.now().isoformat(timespec='seconds')}")
        run.report.write_text(index(run, outcomes, header, console_log), encoding="utf-8")
        console.print(f"anonimizador batch: index written to {run.report.resolve()}")
    return len(exams), unreadable
