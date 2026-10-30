"""Command-line interface.

Exit status of `verify`: 0 nothing found, 1 something found, 2 usage error, missing
tool, or a channel that could not be read (a failed or timed-out tool).

Exit status of `batch`: 0 every copy was written and verifies clean, 1 a copy still holds
something `verify` finds, 2 the run could not start or a file or step failed.
Identifiers found in an original are expected and do not fail the run.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections.abc import Callable
from pathlib import Path
from typing import NoReturn

from anonimizador import __version__
from anonimizador.channels import ToolError, missing_languages
from anonimizador.terms import Term, TermsError, load_terms
from anonimizador.verify import Options, VerifyError, collect, missing_tools, render, verify

EXIT_OK, EXIT_FOUND, EXIT_USAGE = 0, 1, 2


def bounded_int(maximum: int) -> Callable[[str], int]:
    """An argument that must be a whole number from 1 to `maximum`: argparse reports
    anything else as a usage error (exit 2), never as a traceback (Grok G0-9, G0-31; a
    huge timeout overflowed in `subprocess`)."""

    def parse(value: str) -> int:
        try:
            number = int(value)
        except ValueError:
            number = 0
        if not 1 <= number <= maximum:
            raise argparse.ArgumentTypeError(f"must be a whole number from 1 to {maximum}")
        return number

    return parse


# One day per tool run; 2400 dpi is beyond any scan.
MAX_TIMEOUT, MAX_DPI = 86_400, 2_400
# A value quoted in an argparse message (`invalid choice: 'x'`).
QUOTED = re.compile(r"'(?:[^'\\]|\\.)*'|\"(?:[^\"\\]|\\.)*\"")
# Messages that end with the arguments themselves.
ARGUMENT_LISTS = ("unrecognized arguments", "ambiguous option")


class Parser(argparse.ArgumentParser):
    """A parser whose usage errors never print an argument: a rejected argument can be a
    path or a term (Codex C0-1)."""

    def error(self, message: str) -> NoReturn:
        for prefix in ARGUMENT_LISTS:
            if message.startswith(prefix):
                message = prefix
        message = QUOTED.sub("(value not shown)", message)
        self.print_usage(sys.stderr)
        self.exit(EXIT_USAGE, f"{self.prog}: error: {message}\n")


def parser() -> argparse.ArgumentParser:
    root = Parser(
        prog="anonimizador",
        description="Local tools to anonymize medical PDFs and images (counts only, never values).",
    )
    root.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    commands = root.add_subparsers(dest="command", required=True)

    v = commands.add_parser(
        "verify",
        help="count identifiers left in corpus files",
        description="Search files for the identifiers listed in a terms file, in the file "
        "name, metadata, text layer, OCR of pages and embedded images, and raw bytes. "
        "Prints counts only.",
    )
    v.add_argument("paths", nargs="+", help="files or directories to check")
    add_reading_options(v)
    v.add_argument("--show-names", action="store_true", help="print file paths")

    b = commands.add_parser(
        "batch",
        help="anonymize every original in a directory and index the run",
        description="Run the anonymization steps over every PDF, PNG, and JPEG directly in "
        "SOURCE, write the outputs under neutral names to --out, and write a Markdown index "
        "(<YYYYMMDD-HHMMSS>-anonimizer.md) that links each original, its anonymized copy, and "
        "the console output. The index names the originals: --out must be a place git cannot "
        "commit. Each copy is rasterized, with black boxes over the identifiers found (terms, "
        "patterns, field labels), then verified. The console shows positions and counts only.",
    )
    b.add_argument("source", help="directory of original exams (not walked recursively)")
    b.add_argument("--out", required=True, help="output directory (git-ignored or outside a repo)")
    b.add_argument(
        "--keep-dates",
        action="store_true",
        help="keep dates other than the date of birth (default: remove every date)",
    )
    add_reading_options(b)
    return root


def add_reading_options(command: argparse.ArgumentParser) -> None:
    """The options of every command that reads files the way `verify` does."""
    command.add_argument(
        "--terms",
        help="file of identifiers to look for, one per line (keep it outside the repository)",
    )
    command.add_argument("--lang", default="por+eng", help="tesseract languages")
    command.add_argument(
        "--dpi", type=bounded_int(MAX_DPI), default=300, help="page render resolution"
    )
    command.add_argument("--no-ocr", action="store_true", help="skip rendering and OCR")
    command.add_argument(
        "--timeout",
        type=bounded_int(MAX_TIMEOUT),
        default=600,
        help="seconds each tool run may take before its channel is reported unreadable",
    )
    command.add_argument(
        "--strict-patterns", action="store_true", help="fail on identifier patterns too"
    )


def reading_options(args: argparse.Namespace) -> Options:
    return Options(
        lang=args.lang,
        dpi=args.dpi,
        use_ocr=not args.no_ocr,
        show_names=getattr(args, "show_names", False),
        strict_patterns=args.strict_patterns,
        timeout=args.timeout,
    )


def prepare(args: argparse.Namespace, options: Options) -> list[Term]:
    """Checks the tools and OCR languages, and loads the terms."""
    missing = missing_tools(options.use_ocr)
    if missing:
        raise VerifyError("missing tools: " + ", ".join(missing))
    if options.use_ocr:
        no_data = missing_languages(options.lang, options.timeout)
        if no_data:
            # Named by position: a mistyped --lang can be anything (Codex C0-1).
            positions = ", ".join(str(n) for n in no_data)
            raise VerifyError(f"tesseract has no data for --lang language(s) {positions}")
    return load_terms(args.terms) if args.terms else []


def run_verify(args: argparse.Namespace) -> int:
    options = reading_options(args)
    try:
        terms = prepare(args, options)
        files = collect(args.paths)
    except (VerifyError, TermsError, ToolError) as e:
        print(f"anonimizador verify: {e}", file=sys.stderr)
        return EXIT_USAGE
    except OSError as e:
        # The message of an OSError holds the path: report its reason only.
        print(f"anonimizador verify: {e.strerror or type(e).__name__}", file=sys.stderr)
        return EXIT_USAGE

    mode = "no OCR" if not options.use_ocr else f"OCR at {options.dpi} dpi ({options.lang})"
    print(f"anonimizador verify: {len(files)} file(s), {len(terms)} term(s), {mode}")
    found = unreadable = False
    for result in verify(files, terms, options):
        found = found or result.failed(options.strict_patterns)
        unreadable = unreadable or bool(result.errors)
        for line in render(result, options):
            print(line, flush=True)
    # An unread channel means the files cannot be vouched for, whatever was found.
    if unreadable:
        return EXIT_USAGE
    return EXIT_FOUND if found else EXIT_OK


def run_batch_command(args: argparse.Namespace) -> int:
    # Imported here: the redaction needs Pillow, the verifier only the standard library.
    from anonimizador.batch import run_batch

    options = reading_options(args)
    try:
        terms = prepare(args, options)
        totals = run_batch(
            Path(args.source), Path(args.out), terms, options, remove_dates=not args.keep_dates
        )
    except (VerifyError, TermsError, ToolError) as e:
        print(f"anonimizador batch: {e}", file=sys.stderr)
        return EXIT_USAGE
    except OSError as e:
        # The message of an OSError holds the path: report its reason only.
        print(f"anonimizador batch: {e.strerror or type(e).__name__}", file=sys.stderr)
        return EXIT_USAGE
    if totals.unreadable:
        return EXIT_USAGE
    return EXIT_FOUND if totals.found else EXIT_OK


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.command == "verify":
        return run_verify(args)
    if args.command == "batch":
        return run_batch_command(args)
    return EXIT_USAGE
