"""`anonimizador batch`: run the anonymization steps over every original in a directory
and write a Markdown index of the run.

Each original gets a neutral output name, `exam-<digest>.<suffix>`, from a digest of
its bytes. The console names originals by position and carries counts only, like
`verify`. The index, `<YYYYMMDD-HHMMSS>-anonimizer.md`, links every original by its
name, so it and the run's logs are written only where git cannot commit them.

For each original the run:
1. verifies it, for the counts before anonymization;
2. reads its pages with positions (`locate`) and detects identifiers (`detect`);
3. writes the redacted copy (`redact`) and the detections, as boxes and kinds, as JSON;
4. verifies the copy with `verify`, a separate code path (R8).

A copy in which `verify` still finds a term goes back to review: the run exits 1.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import TextIO
from urllib.parse import quote

from anonimizador.channels import ToolError
from anonimizador.detect import as_json, detect, summary
from anonimizador.locate import Box, read_pages
from anonimizador.redact import redact
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
NOT_WRITTEN = "not written"
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
    detections: Path | None = None
    detection_counts: dict[str, int] = field(default_factory=dict)
    anonymized: Path | None = None
    anonymized_status: str = NOT_WRITTEN
    anonymized_log: Path | None = None


@dataclass
class Totals:
    exams: int = 0
    # Exams with a file or step that could not be read or written.
    unreadable: int = 0
    # Copies in which `verify` still finds a term.
    found: int = 0


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
    # An image's copy is a PNG whatever its original's format (see `redact`).
    suffix = ".pdf" if path.suffix.lower() == ".pdf" else ".png"
    return f"exam-{digest.hexdigest()[:DIGEST_LENGTH]}{suffix}"


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


def verify_one(
    position: int, path: Path, name: str, terms: list[Term], options: Options, log: Path
) -> tuple[FileResult, list[str]]:
    """`verify` of one file, its lines saved to `log`."""
    [result] = verify([CorpusFile(str(path), name)], terms, options)
    lines = render(result, options)
    write_log(log, lines)
    return result, lines


def labelled(lines: list[str], position: int, step: str) -> list[str]:
    """`verify` lines of a one-file run, labelled with the exam's position and the step."""
    return [line.replace("[1]", f"[{position}] {step}:", 1) for line in lines]


def boxes_by_page(detections: list) -> dict[int, list[Box]]:
    boxes: dict[int, list[Box]] = {}
    for d in detections:
        boxes.setdefault(d.page, []).append(d.box)
    return boxes


def anonymize(
    exam: Exam,
    terms: list[Term],
    options: Options,
    remove_dates: bool,
    run: Run,
    console: Console,
    totals: Totals,
) -> Outcome:
    outcome = Outcome(exam)
    n = exam.position
    log = run.logs / f"{exam.log_prefix}-verify-original.txt"
    result, lines = verify_one(n, exam.original, exam.original.name, terms, options, log)
    for line in labelled(lines, n, "original"):
        console.print(line)
    outcome.original_status, outcome.original_log = result.status(options.strict_patterns), log
    try:
        with (
            tempfile.TemporaryDirectory() as tmp,
            ThreadPoolExecutor(max_workers=os.cpu_count() or 2) as pool,
        ):
            pages = read_pages(
                exam.original,
                Path(tmp),
                options.lang,
                options.dpi,
                options.timeout,
                pool,
                options.use_ocr,
            )
            found = detect(pages, terms, remove_dates)
            detections = run.logs / f"{exam.log_prefix}-detections.json"
            detections.write_text(json.dumps(as_json(found, pages), indent=1), encoding="utf-8")
            outcome.detections, outcome.detection_counts = detections, summary(found)
            copy = run.out / exam.neutral
            redact(pages, boxes_by_page(found), copy, options.dpi)
            outcome.anonymized = copy
    except (ToolError, OSError) as e:
        # Neither message holds a path (see `channels.run`); an OSError's reason only.
        reason = str(e) if isinstance(e, ToolError) else e.strerror or type(e).__name__
        console.print(f"[{n}] anonymized: ERROR")
        console.print(f"    not written: {reason}")
        outcome.anonymized_status = "ERROR"
        totals.unreadable += 1
        return outcome
    counts = " ".join(f"{k}={v}" for k, v in outcome.detection_counts.items()) or "none"
    unaligned = sum(p.unaligned for p in pages)
    console.print(f"[{n}] detections: {counts} on {len(pages)} page(s)")
    if unaligned:
        console.print(f"    text layer not placed on {unaligned} page(s): OCR boxes only")
    log = run.logs / f"{exam.log_prefix}-verify-anonymized.txt"
    result, lines = verify_one(n, copy, exam.neutral, terms, options, log)
    for line in labelled(lines, n, "anonymized"):
        console.print(line)
    outcome.anonymized_status, outcome.anonymized_log = result.status(options.strict_patterns), log
    totals.unreadable += bool(result.errors)
    totals.found += result.failed(options.strict_patterns)
    return outcome


def link(target: Path, base: Path, text: str) -> str:
    """A Markdown link to `target`, relative to the directory `base`."""
    href = quote(os.path.relpath(target, base).replace(os.sep, "/"))
    label = text.replace("\\", "\\\\").replace("|", "\\|").replace("[", "\\[").replace("]", "\\]")
    return f"[{label}]({href})"


def index(
    run: Run, outcomes: list[Outcome], header: list[str], console_log: Path, remove_dates: bool
) -> str:
    base = run.report.parent
    rows = []
    for o in outcomes:
        original = link(o.exam.original, base, o.exam.original.name)
        anonymized = link(o.anonymized, base, o.exam.neutral) if o.anonymized else NOT_WRITTEN
        verified = link(o.original_log, base, o.original_status) if o.original_log else "—"
        counts = ", ".join(f"{k} {v}" for k, v in o.detection_counts.items()) or "none"
        found = link(o.detections, base, counts) if o.detections else "—"
        after = (
            link(o.anonymized_log, base, o.anonymized_status)
            if o.anonymized_log
            else o.anonymized_status
        )
        rows.append(
            f"| {o.exam.position} | {original} | {verified} | {found} | {anonymized} | {after} |"
        )
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
            "| # | Original | Verify original | Detections (boxes) | Anonymized | "
            "Verify anonymized |",
            "|---:|---|---|---|---|---|",
            *rows,
            "",
            "A verify log holds counts per term (by its line in the terms file) and per place, "
            "never a value. `FOUND` on an original is expected: it is the count before "
            "anonymization. `FOUND` on an anonymized copy sends it back to review. The "
            "detections file holds each box with its page, kind, and detector, never its text.",
            "",
            "## Before using an anonymized copy",
            "",
            "- **Each copy is a scanned PDF (or a PNG)**: every page was rasterized, boxes were "
            "painted over the detections, and the file was rebuilt from the images alone, so no "
            "text layer, covered text, or metadata of the original is left.",
            "- **Dates** were "
            + ("removed" if remove_dates else "kept")
            + "; the date of birth is removed with its label either way.",
            "- **A person reviews every copy.** No detector is complete: handwriting, stamps, "
            "poor scans, and unusual names defeat OCR and name recognition, and rare "
            "combinations of diagnosis, date, and institution can identify a person.",
            "- Only an explicit human approval records the review in the manifest "
            "(`docs/SPEC.md` R10).",
            "",
        ]
    )


def run_batch(
    source: Path,
    out: Path,
    terms: list[Term],
    options: Options,
    now: datetime | None = None,
    remove_dates: bool = True,
) -> Totals:
    """Runs every step over the originals in `source` and writes the copies and the index
    under `out`. Raises `VerifyError` when the run cannot start."""
    exams = originals(source)
    now = now or datetime.now()
    run = start(out, now)
    console_log = run.logs / "console.txt"
    mode = "no OCR" if not options.use_ocr else f"OCR at {options.dpi} dpi ({options.lang})"
    header = [
        f"- Started: {now.isoformat(timespec='seconds')}",
        f"- Originals: {len(exams)}; terms: {len(terms)}; {mode}; "
        f"dates {'removed' if remove_dates else 'kept'}",
    ]
    if not terms:
        header.append(f"- **{NO_TERMS}**")
    totals = Totals(len(exams))
    with console_log.open("w", encoding="utf-8") as log:
        console = Console(sys.stdout, log)
        console.print(f"anonimizador batch: {len(exams)} original(s), {len(terms)} term(s), {mode}")
        if not terms:
            console.print(f"anonimizador batch: warning: {NO_TERMS}")
        outcomes = [
            anonymize(exam, terms, options, remove_dates, run, console, totals) for exam in exams
        ]
        header.append(f"- Finished: {datetime.now().isoformat(timespec='seconds')}")
        report = index(run, outcomes, header, console_log, remove_dates)
        run.report.write_text(report, encoding="utf-8")
        console.print(
            f"anonimizador batch: {sum(o.anonymized_status == 'ok' for o in outcomes)} of "
            f"{totals.exams} copy(ies) verified ok, {totals.found} with something left, "
            f"{totals.unreadable} with an error"
        )
        console.print(f"anonimizador batch: index written to {run.report.resolve()}")
    return totals
