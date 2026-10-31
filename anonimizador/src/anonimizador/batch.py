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

With `--classifier`, each copy whose re-verification is `ok` is also classified by exam
type (`classifier`): the index gains two columns and a header line, and each copy's JSON
line is kept in `<run>/classifier/`. A classifier failure never changes the exit status.
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

from anonimizador import classifier as cls
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
# A neutral name to ask git about, before any copy exists.
NEUTRAL_PROBE = "exam-" + "0" * DIGEST_LENGTH
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
    classification: cls.Classification | None = None
    classification_file: Path | None = None


@dataclass
class Totals:
    exams: int = 0
    # Exams with a file or step that could not be read or written.
    unreadable: int = 0
    # Copies in which `verify` still finds a term.
    found: int = 0


@dataclass
class Run:
    """Where one run writes: its own new directory for the copies and the logs, and the
    index beside it. Nothing that exists before the run is ever written (Codex CB-1)."""

    stamp: str
    out: Path
    copies: Path = field(init=False)
    logs: Path = field(init=False)
    classified: Path = field(init=False)
    report: Path = field(init=False)

    def __post_init__(self) -> None:
        self.copies = self.out / self.stamp
        self.logs = self.copies / "logs"
        self.classified = self.copies / "classifier"
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
        candidates = [
            p
            for p in source.iterdir()
            if not p.name.startswith(APPLEDOUBLE) and p.suffix.lower() in SUFFIXES
        ]
        # A link that leads nowhere is an original never read: an error, as in
        # `verify`, not a skipped file (Antigravity AB-6).
        if any(p.is_symlink() and not p.exists() for p in candidates):
            raise VerifyError("a link among the originals leads nowhere")
        paths = sorted(p for p in candidates if p.is_file())
    except OSError as e:
        # The message of an OSError holds the path: report its reason only.
        raise VerifyError(f"cannot list the originals ({e.strerror or type(e).__name__})") from None
    if not paths:
        raise VerifyError("no PDF, PNG, or JPEG original to anonymize")
    exams = []
    taken: set[str] = set()
    for n, path in enumerate(paths, start=1):
        neutral = neutral_name(path)
        if neutral in taken:
            # Two originals with the same bytes: each copy gets its own name.
            stem, suffix = os.path.splitext(neutral)
            neutral = f"{stem}-{n}{suffix}"
        taken.add(neutral)
        exams.append(Exam(n, path, neutral))
    return exams


def start(out: Path, now: datetime, classify: bool = False) -> Run:
    """Makes the run's new directories, refusing any place git could commit, by its
    path or where its links lead: the index and the logs name the originals (Codex
    CB-2). With `classify`, also the directory of the classifier's JSON lines."""
    run = Run(now.strftime(STAMP_FORMAT), out)
    # The copies too: a rule that ignores the index and logs by name can leave them
    # committable, and a copy is unreviewed (Antigravity AB-5).
    places = [
        run.report,
        run.logs / "console.txt",
        run.logs / f"01-{NEUTRAL_PROBE}-detections.json",
        *(run.copies / f"{NEUTRAL_PROBE}{suffix}" for suffix in (".pdf", ".png")),
        *([run.classified / f"{NEUTRAL_PROBE}.json"] if classify else []),
    ]
    for place in places:
        uncommittable(place)
    if run.report.exists() or run.report.is_symlink():
        raise VerifyError("an index of this second already exists; run again")
    out.mkdir(parents=True, exist_ok=True)
    run.copies.mkdir()
    run.logs.mkdir()
    if classify:
        run.classified.mkdir()
    return run


def uncommittable(path: Path) -> None:
    """Refuses `path` when git could commit it, by its path or where its links lead."""
    if committable(str(path)) or committable(os.path.realpath(path)):
        raise VerifyError(
            "the output directory is inside a git work tree and not ignored; "
            "the index names the originals, so keep it where git cannot commit it"
        )


def write_new(path: Path, text: str) -> None:
    """Writes a file that must not exist yet, so no existing file or link is written
    through, and only where git cannot commit it: ignore rules can name single files
    (Codex CB-20)."""
    uncommittable(path)
    with path.open("x", encoding="utf-8") as f:
        f.write(text)


def write_log(path: Path, lines: list[str]) -> None:
    write_new(path, "\n".join(lines) + "\n")


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
    # An original that cannot be read whole is an error of the run, even when its copy
    # verifies clean (Codex CB-8).
    unreadable = bool(result.errors)
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
            )
            found = detect(pages, terms, remove_dates)
            detections = run.logs / f"{exam.log_prefix}-detections.json"
            write_new(detections, json.dumps(as_json(found, pages), indent=1))
            outcome.detections, outcome.detection_counts = detections, summary(found)
            copy = run.copies / exam.neutral
            uncommittable(copy)
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
    totals.unreadable += unreadable or bool(result.errors)
    totals.found += result.failed(options.strict_patterns)
    return outcome


def classify_copy(outcome: Outcome, binary: Path, remote: bool, run: Run, console: Console) -> None:
    """Classifies the copy when its re-verification was `ok`, and keeps its JSON line.
    Nothing here raises: a failure is the cell `error: classifier failed`."""
    if outcome.anonymized is None or outcome.anonymized_status != "ok":
        return
    result = cls.classify(binary, outcome.anonymized, remote)
    if result.line is not None:
        path = run.classified / f"{Path(outcome.exam.neutral).stem}.json"
        try:
            write_new(path, result.line + "\n")
            outcome.classification_file = path
        except (OSError, VerifyError):
            result = cls.failed()
    outcome.classification = result
    console.print(f"[{outcome.exam.position}] classifier: {result.state}")


def classifier_cells(o: Outcome, base: Path) -> str:
    """The two classifier cells of a row: `—` for a copy that was not classified."""
    c = o.classification
    if c is None:
        return f" {cls.NOT_CLASSIFIED} | {cls.NOT_CLASSIFIED} |"
    cell = link(o.classification_file, base, c.cell) if o.classification_file else c.cell
    return f" {cell} | {c.weights} |"


def link(target: Path, base: Path, text: str) -> str:
    """A Markdown link to `target`, relative to the directory `base`."""
    href = quote(os.path.relpath(target, base).replace(os.sep, "/"))
    label = text.replace("\\", "\\\\").replace("|", "\\|").replace("[", "\\[").replace("]", "\\]")
    return f"[{label}]({href})"


def index(
    run: Run,
    outcomes: list[Outcome],
    header: list[str],
    console_log: Path,
    remove_dates: bool,
    classified: bool = False,
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
        row = f"| {o.exam.position} | {original} | {verified} | {found} | {anonymized} | {after} |"
        rows.append(row + (classifier_cells(o, base) if classified else ""))
    columns = " Exam type (classifier) | Weights |" if classified else ""
    rule = "---|---|" if classified else ""
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
            "Verify anonymized |" + columns,
            "|---:|---|---|---|---|---|" + rule,
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
    classifier: Path | None = None,
    classify_remote: bool = False,
) -> Totals:
    """Runs every step over the originals in `source` and writes the copies and the index
    under `out`. Raises `VerifyError` when the run cannot start. With `classifier`, the
    `embeddings-fundamentals` binary classifies every copy that verifies `ok`."""
    if not options.use_ocr:
        # The copy holds pixels only: without OCR nothing would read them, and a copy
        # would pass unread (Codex CB-4).
        raise VerifyError("batch needs OCR: a copy holds only pixels; drop --no-ocr")
    exams = originals(source)
    now = now or datetime.now()
    run = start(out, now, classify=classifier is not None)
    console_log = run.logs / "console.txt"
    mode = f"OCR at {options.dpi} dpi ({options.lang})"
    header = [
        f"- Started: {now.isoformat(timespec='seconds')}",
        f"- Originals: {len(exams)}; terms: {len(terms)}; {mode}; "
        f"dates {'removed' if remove_dates else 'kept'}",
    ]
    if not terms:
        header.append(f"- **{NO_TERMS}**")
    totals = Totals(len(exams))
    with console_log.open("x", encoding="utf-8") as log:
        console = Console(sys.stdout, log)
        console.print(f"anonimizador batch: {len(exams)} original(s), {len(terms)} term(s), {mode}")
        if not terms:
            console.print(f"anonimizador batch: warning: {NO_TERMS}")
        outcomes = []
        for exam in exams:
            outcome = anonymize(exam, terms, options, remove_dates, run, console, totals)
            if classifier is not None:
                classify_copy(outcome, classifier, classify_remote, run, console)
            outcomes.append(outcome)
        if classifier is not None:
            valid = (o.classification for o in outcomes if o.classification is not None)
            first = next((c for c in valid if c.fields is not None), None)
            header.append(cls.header(first, classify_remote))
        header.append(f"- Finished: {datetime.now().isoformat(timespec='seconds')}")
        report = index(
            run, outcomes, header, console_log, remove_dates, classified=classifier is not None
        )
        write_new(run.report, report)
        console.print(
            f"anonimizador batch: {sum(o.anonymized_status == 'ok' for o in outcomes)} of "
            f"{totals.exams} copy(ies) verified ok, {totals.found} with something left, "
            f"{totals.unreadable} with an error"
        )
        # The name only: the directories above it can name a person (Codex CB-3).
        console.print(f"anonimizador batch: index written: {run.report.name}")
    return totals
