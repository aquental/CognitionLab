#!/usr/bin/env python3
"""Local anonymization verifier (TESTS.md).

Searches corpus files for the identifiers the owner lists, in every place they can
survive anonymization, and prints COUNTS ONLY: never a term, a match, or document
text. Files are named by position (`[3]`) unless --show-names is given.

Places searched, per file:

  name      the file name
  metadata  PDF information dictionary and XMP (pdfinfo), image metadata blocks
  text      the PDF text layer (pdftotext)
  ocr       what the rendered pages show (pdftoppm + tesseract), or the image itself
  images    what the images embedded in a PDF show (pdfimages + tesseract)
  raw       the file bytes and every inflatable stream (leftover incremental
            updates, uncompressed content streams, XMP packets, PNG text chunks);
            short terms (under 6 letters) are not searched here, since binary data
            is full of short letter runs

A term found in `text` or `raw` but not in `ocr` is the signature of a box drawn over
text that is still in the file: use a real redaction tool, then check again.

Besides the terms, a few identifier patterns (CPF, CNS, phone, e-mail, CEP, date,
CRM) are counted as hints. They are noisy (an exam date is a date), so they never
fail the check unless --strict-patterns is given.

    python3 scripts/anonymization-verify.py --terms ~/private/terms.txt corpus/

The terms file holds one term per line (names, CPF, record numbers, addresses...);
blank lines and lines starting with # are ignored. It is itself identifying: keep it
outside the repository. The verifier refuses a terms file that git could commit.

Exit status: 0 nothing found, 1 a term (or, with --strict-patterns, a pattern) was
found, 2 usage error or missing tool.
"""
import argparse
import concurrent.futures
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import unicodedata
import zlib

CHANNELS = ("name", "metadata", "text", "ocr", "images", "raw")
SUFFIXES = {".pdf": "pdf", ".png": "image", ".jpg": "image", ".jpeg": "image"}
PATTERNS = {
    "cpf": r"(?<!\d)\d{3}\.?\d{3}\.?\d{3}-?\d{2}(?!\d)",
    "cns": r"(?<!\d)[1-2789]\d{2} ?\d{4} ?\d{4} ?\d{4}(?!\d)",
    "phone": r"(?<!\d)\(?\d{2}\)? ?9?\d{4}[- ]?\d{4}(?!\d)",
    "email": r"[\w.+-]+@[\w-]+\.[\w.-]+",
    "cep": r"(?<!\d)\d{5}-\d{3}(?!\d)",
    "date": r"(?<!\d)\d{1,2}/\d{1,2}/\d{2,4}(?!\d)",
    "crm": r"\bcrm\b",
}
# Images smaller than this on either side are icons or masks, not readable text.
MIN_OCR_SIDE = 32


class UsageError(Exception):
    pass


def fold(text):
    """Lower case without accents: what OCR and typing variants have in common."""
    decomposed = unicodedata.normalize("NFKD", text)
    return "".join(c for c in decomposed if not unicodedata.combining(c)).casefold()


def words(text):
    return " " + " ".join(re.findall(r"[^\W_]+", fold(text))) + " "


def compact(text):
    return "".join(re.findall(r"[^\W_]+", fold(text)))


def digit_groups(text):
    """Digit runs joined across the separators numbers are written with."""
    return [re.sub(r"\D", "", m) for m in re.findall(r"\d[\d.\-/ ]*\d|\d", text)]


class Term:
    def __init__(self, raw):
        self.raw = raw
        digits = re.sub(r"\D", "", raw)
        letters = compact(raw)
        self.numeric = len(digits) >= 5 and len(digits) >= 0.8 * len(letters)
        self.digits = digits
        # Long terms are matched without spaces or punctuation, which survives OCR
        # spacing errors and line breaks; short ones need whole words.
        self.compact = letters if len(letters) >= 6 else None
        self.words = words(raw)

    def count(self, text, raw=False):
        if not text:
            return 0
        if raw and not (self.numeric or self.compact):
            return 0
        if self.numeric:
            return sum(group.count(self.digits) for group in digit_groups(text))
        if self.compact:
            return compact(text).count(self.compact)
        if self.words.strip():
            return words(text).count(self.words)
        return 0


def run(args, timeout=600):
    result = subprocess.run(args, capture_output=True, timeout=timeout, check=False)
    return result.stdout.decode("utf-8", "replace") if result.returncode == 0 else ""


def ocr(image, lang):
    return run(["tesseract", image, "-", "-l", lang], timeout=300)


def png_size(path):
    with open(path, "rb") as f:
        head = f.read(24)
    if len(head) == 24 and head[:8] == b"\x89PNG\r\n\x1a\n":
        return struct.unpack(">II", head[16:24])
    return (0, 0)


def raw_texts(data):
    """The file bytes and every zlib stream in it, decoded both ways text is stored."""
    blobs = [data]
    for match in re.finditer(rb"stream\r?\n", data):
        start = match.end()
        end = data.find(b"endstream", start)
        if end < 0:
            continue
        try:
            blobs.append(zlib.decompressobj().decompress(data[start:end]))
        except zlib.error:
            pass
    blobs.extend(png_text_chunks(data))
    texts = []
    for blob in blobs:
        texts.append(blob.decode("utf-8", "ignore"))
        texts.append(blob.decode("latin-1"))
        if blob.count(b"\x00") > len(blob) // 4:
            texts.append(blob.decode("utf-16-be", "ignore"))
    return texts


def inflate_tail(body):
    """The zlib stream that ends a zTXt or compressed iTXt chunk, if any."""
    for start in range(len(body)):
        try:
            return zlib.decompress(body[start:])
        except zlib.error:
            continue
    return None


def png_chunks(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return
    pos = 8
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        yield kind, data[pos + 8 : pos + 8 + length]
        pos += 12 + length


def png_text_chunks(data):
    """Inflated zTXt and compressed iTXt chunks of a PNG (tEXt is already raw)."""
    out = []
    for kind, body in png_chunks(data):
        if kind in (b"zTXt", b"iTXt"):
            inflated = inflate_tail(body)
            if inflated is not None:
                out.append(inflated)
    return out


def image_metadata(data):
    """Metadata blocks of an image: PNG text and EXIF chunks; JPEG APPn and COM
    segments (EXIF, XMP, IPTC, comments)."""
    blobs = []
    for kind, body in png_chunks(data):
        if kind in (b"tEXt", b"iTXt", b"eXIf"):
            blobs.append(body)
        if kind in (b"zTXt", b"iTXt"):
            inflated = inflate_tail(body)
            if inflated is not None:
                blobs.append(inflated)
    if data[:2] == b"\xff\xd8":
        pos = 2
        while pos + 4 <= len(data) and data[pos] == 0xFF:
            marker = data[pos + 1]
            if marker == 0xDA:  # start of scan: no metadata after this
                break
            (length,) = struct.unpack(">H", data[pos + 2 : pos + 4])
            if 0xE1 <= marker <= 0xEF or marker == 0xFE:
                blobs.append(data[pos + 4 : pos + 2 + length])
            pos += 2 + length
    texts = []
    for blob in blobs:
        texts.append(blob.decode("utf-8", "ignore"))
        texts.append(blob.decode("latin-1"))
    return texts


def max_count(term, texts, raw=False):
    return max((term.count(t, raw) for t in texts), default=0)


def scan_pdf(path, tmp, lang, dpi, use_ocr, pool):
    texts = {
        "name": [os.path.basename(path)],
        "metadata": [
            run(["pdfinfo", "-enc", "UTF-8", path]),
            run(["pdfinfo", "-meta", path]),
        ],
        "text": [run(["pdftotext", "-enc", "UTF-8", "-layout", path, "-"])],
        "ocr": [],
        "images": [],
    }
    with open(path, "rb") as f:
        texts["raw"] = raw_texts(f.read())
    if use_ocr:
        pages = os.path.join(tmp, "page")
        run(["pdftoppm", "-r", str(dpi), "-png", path, pages])
        rendered = sorted(p for p in os.listdir(tmp) if p.startswith("page"))
        texts["ocr"] = list(pool.map(lambda p: ocr(os.path.join(tmp, p), lang), rendered))
        embedded = os.path.join(tmp, "img")
        run(["pdfimages", "-png", path, embedded])
        images = sorted(
            os.path.join(tmp, p)
            for p in os.listdir(tmp)
            if p.startswith("img") and min(png_size(os.path.join(tmp, p))) >= MIN_OCR_SIDE
        )
        texts["images"] = list(pool.map(lambda p: ocr(p, lang), images))
    return texts


def scan_image(path, lang, use_ocr):
    with open(path, "rb") as f:
        data = f.read()
    return {
        "name": [os.path.basename(path)],
        "metadata": image_metadata(data),
        "text": [],
        "ocr": [ocr(path, lang)] if use_ocr else [],
        "images": [],
        "raw": raw_texts(data),
    }


def collect(paths):
    files = []
    for path in paths:
        if os.path.isdir(path):
            for root, dirs, names in os.walk(path):
                dirs[:] = sorted(d for d in dirs if not d.startswith("."))
                for name in sorted(names):
                    if not name.startswith(".") and os.path.splitext(name)[1].lower() in SUFFIXES:
                        files.append(os.path.join(root, name))
        elif os.path.isfile(path):
            files.append(path)
        else:
            raise UsageError(f"not a file or directory: argument {paths.index(path) + 1}")
    if not files:
        raise UsageError("no PDF, PNG, or JPEG file to check")
    return sorted(set(files))


def load_terms(path):
    directory = os.path.dirname(os.path.abspath(path))
    inside = subprocess.run(
        ["git", "-C", directory, "rev-parse", "--is-inside-work-tree"],
        capture_output=True,
        check=False,
    )
    if inside.returncode == 0 and inside.stdout.strip() == b"true":
        ignored = subprocess.run(
            ["git", "-C", directory, "check-ignore", "-q", os.path.abspath(path)],
            check=False,
        )
        if ignored.returncode != 0:
            raise UsageError(
                "the terms file is inside a git work tree and not ignored; "
                "keep it outside the repository"
            )
    with open(path, encoding="utf-8") as f:
        lines = [line.strip() for line in f]
    terms = [Term(line) for line in lines if line and not line.startswith("#")]
    if not terms:
        raise UsageError("the terms file lists no term")
    return terms


def main():
    parser = argparse.ArgumentParser(
        description="Count identifiers left in corpus files (counts only, never values)."
    )
    parser.add_argument("paths", nargs="+", help="files or directories to check")
    parser.add_argument("--terms", help="file of identifiers to look for, one per line")
    parser.add_argument("--lang", default="por+eng", help="tesseract languages")
    parser.add_argument("--dpi", type=int, default=300, help="page render resolution")
    parser.add_argument("--no-ocr", action="store_true", help="skip rendering and OCR")
    parser.add_argument("--show-names", action="store_true", help="print file paths")
    parser.add_argument(
        "--strict-patterns", action="store_true", help="fail on identifier patterns too"
    )
    args = parser.parse_args()
    try:
        tools = ["pdftotext", "pdfinfo"]
        if not args.no_ocr:
            tools += ["pdftoppm", "pdfimages", "tesseract"]
        missing = [t for t in tools if shutil.which(t) is None]
        if missing:
            raise UsageError("missing tools: " + ", ".join(missing))
        terms = load_terms(args.terms) if args.terms else []
        files = collect(args.paths)
    except (UsageError, OSError) as e:
        print(f"anonymization-verify: {e}", file=sys.stderr)
        return 2

    mode = "no OCR" if args.no_ocr else f"OCR at {args.dpi} dpi ({args.lang})"
    print(f"anonymization-verify: {len(files)} file(s), {len(terms)} term(s), {mode}")
    found_any = False
    patterns = {k: re.compile(v) for k, v in PATTERNS.items()}
    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 2) as pool:
        for index, path in enumerate(files, start=1):
            with tempfile.TemporaryDirectory() as tmp:
                if SUFFIXES[os.path.splitext(path)[1].lower()] == "pdf":
                    texts = scan_pdf(path, tmp, args.lang, args.dpi, not args.no_ocr, pool)
                else:
                    texts = scan_image(path, args.lang, not args.no_ocr)
            label = f"[{index}] {path}" if args.show_names else f"[{index}]"
            hits = []
            for number, term in enumerate(terms, start=1):
                counts = {c: max_count(term, texts[c], raw=c == "raw") for c in CHANNELS}
                if any(counts.values()):
                    hits.append((number, counts))
            hints = {}
            for name, pattern in patterns.items():
                n = max(
                    (len(pattern.findall(fold(t))) for c in ("text", "ocr", "images", "metadata") for t in texts[c]),
                    default=0,
                )
                if n:
                    hints[name] = n
            failed = bool(hits) or (args.strict_patterns and bool(hints))
            found_any = found_any or failed
            print(f"{label} {'FOUND' if failed else 'ok'}")
            for number, counts in hits:
                where = " ".join(f"{c}={counts[c]}" for c in CHANNELS if counts[c])
                print(f"    term {number}: {where}")
                if not args.no_ocr and (counts["text"] or counts["raw"]) and not (
                    counts["ocr"] or counts["images"]
                ):
                    print(
                        f"    hint: term {number} is in the file but not visible on the page: "
                        "text under a drawn box? Redact for real (TESTS.md)."
                    )
            if hints:
                print(
                    "    possible identifiers (patterns): "
                    + " ".join(f"{k}={v}" for k, v in sorted(hints.items()))
                )
    return 1 if found_any else 0


if __name__ == "__main__":
    sys.exit(main())
