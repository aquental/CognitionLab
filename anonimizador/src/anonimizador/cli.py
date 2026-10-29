"""Command-line interface.

Exit status: 0 nothing found, 1 something found, 2 usage error or missing tool.
"""

from __future__ import annotations

import argparse
import sys

from anonimizador import __version__
from anonimizador.terms import Term, TermsError, load_terms
from anonimizador.verify import Options, VerifyError, collect, missing_tools, render, verify

EXIT_OK, EXIT_FOUND, EXIT_USAGE = 0, 1, 2


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(
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
    v.add_argument(
        "--terms",
        help="file of identifiers to look for, one per line (keep it outside the repository)",
    )
    v.add_argument("--lang", default="por+eng", help="tesseract languages")
    v.add_argument("--dpi", type=int, default=300, help="page render resolution")
    v.add_argument("--no-ocr", action="store_true", help="skip rendering and OCR")
    v.add_argument("--show-names", action="store_true", help="print file paths")
    v.add_argument("--strict-patterns", action="store_true", help="fail on identifier patterns too")
    return root


def run_verify(args: argparse.Namespace) -> int:
    options = Options(
        lang=args.lang,
        dpi=args.dpi,
        use_ocr=not args.no_ocr,
        show_names=args.show_names,
        strict_patterns=args.strict_patterns,
    )
    try:
        missing = missing_tools(options.use_ocr)
        if missing:
            raise VerifyError("missing tools: " + ", ".join(missing))
        terms: list[Term] = load_terms(args.terms) if args.terms else []
        files = collect(args.paths)
    except (VerifyError, TermsError, OSError) as e:
        print(f"anonimizador verify: {e}", file=sys.stderr)
        return EXIT_USAGE

    mode = "no OCR" if not options.use_ocr else f"OCR at {options.dpi} dpi ({options.lang})"
    print(f"anonimizador verify: {len(files)} file(s), {len(terms)} term(s), {mode}")
    found = False
    for result in verify(files, terms, options):
        found = found or result.failed(options.strict_patterns)
        for line in render(result, options):
            print(line, flush=True)
    return EXIT_FOUND if found else EXIT_OK


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.command == "verify":
        return run_verify(args)
    return EXIT_USAGE
