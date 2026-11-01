"""Anonymizes the owner's exams from Cloudflare R2 in batches (local only).

For each batch it:
1. lists the bucket of originals (R2 `sample-exams`) and picks exams not yet in the ledger;
2. downloads them under neutral local names, `src-<sha256 of the key>.<suffix>`, into
   a git-ignored directory, so not even the local index names a patient;
3. runs `anonimizador batch` on them;
4. uploads to the bucket `anonymized` only the copies whose re-verification is `ok`;
5. records each exam in a local ledger (key hashes and neutral names only) and deletes
   the downloaded originals.

The index and the logs stay local: they link the originals. Nothing printed names an
exam: the `aws` CLI's output, which names objects, is captured and never shown.

    uv run python scripts/anonymize-r2.py --count            # eligible, done, pending
    uv run python scripts/anonymize-r2.py --limit 50 [--terms FILE]

Credentials: R2_API_KEY (an account API token) and R2_ACCOUNT_ID, from the environment
or `../embeddings-fundamentals/.env`; the S3 key pair is derived at each run, as
`fetch-sample-exams.sh` does.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.request
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT.parent / "embeddings-fundamentals"
WORK = CRATE / "tests" / "sample-exams"
OUT = WORK / "anonymized"
BATCHES = WORK / "r2-batches"
LEDGER = OUT / "r2-ledger.jsonl"
SOURCE_BUCKET = os.environ.get("R2_SOURCE_BUCKET", "sample-exams")
TARGET_BUCKET = os.environ.get("R2_TARGET_BUCKET", "anonymized")
SUFFIXES = (".pdf", ".png", ".jpg", ".jpeg")


def say(message: str) -> None:
    print(f"anonymize-r2: {message}", flush=True)


def fail(message: str) -> None:
    say(message)
    sys.exit(1)


def setting(name: str) -> str | None:
    if os.environ.get(name):
        return os.environ[name]
    dotenv = CRATE / ".env"
    if dotenv.is_file():
        for line in dotenv.read_text(encoding="utf-8").splitlines():
            if line.startswith(f"{name}="):
                return line.split("=", 1)[1].strip().strip("\"'")
    return None


def aws_environment() -> tuple[dict[str, str], list[str]]:
    """The environment and endpoint arguments for `aws`, with the S3 pair derived from the
    token. Never printed."""
    token, account = setting("R2_API_KEY"), setting("R2_ACCOUNT_ID")
    if not token or not account:
        fail("set R2_API_KEY and R2_ACCOUNT_ID (environment or embeddings-fundamentals/.env)")
    url = f"https://api.cloudflare.com/client/v4/accounts/{account}/tokens/verify"
    request = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            result = json.load(response).get("result") or {}
    except Exception:
        result = {}
    if result.get("status") != "active" or not result.get("id"):
        fail("R2_API_KEY did not verify as an active token of R2_ACCOUNT_ID")
    env = dict(os.environ)
    env.pop("AWS_PROFILE", None)
    env.pop("AWS_SESSION_TOKEN", None)
    env.update(
        AWS_ACCESS_KEY_ID=result["id"],
        AWS_SECRET_ACCESS_KEY=hashlib.sha256(token.encode()).hexdigest(),
        AWS_DEFAULT_REGION="auto",
        AWS_REQUEST_CHECKSUM_CALCULATION="when_required",
        AWS_RESPONSE_CHECKSUM_VALIDATION="when_required",
    )
    return env, ["--endpoint-url", f"https://{account}.r2.cloudflarestorage.com"]


class Aws:
    def __init__(self) -> None:
        self.env, self.endpoint = aws_environment()

    def run(self, *args: str) -> subprocess.CompletedProcess[str]:
        """Runs `aws`; its output is returned, never printed: it names objects."""
        return subprocess.run(
            ["aws", *self.endpoint, *args],
            env=self.env,
            capture_output=True,
            text=True,
            stdin=subprocess.DEVNULL,
        )

    def keys(self) -> list[tuple[str, int]]:
        done = self.run("s3api", "list-objects-v2", "--bucket", SOURCE_BUCKET, "--output", "json")
        if done.returncode != 0:
            fail(f"listing the originals failed (exit {done.returncode}; output withheld)")
        contents = json.loads(done.stdout or "{}").get("Contents", [])
        return [(o["Key"], int(o.get("Size", 0))) for o in contents]


def key_id(key: str) -> str:
    return hashlib.sha256(key.encode("utf-8")).hexdigest()[:16]


def ledger() -> dict[str, dict]:
    if not LEDGER.is_file():
        return {}
    entries = (json.loads(line) for line in LEDGER.read_text(encoding="utf-8").splitlines())
    return {e["key_id"]: e for e in entries}


def record(entry: dict) -> None:
    with LEDGER.open("a", encoding="utf-8") as f:
        f.write(json.dumps(entry) + "\n")


def ignored(path: Path) -> bool:
    probe = path / "probe.pdf"
    return subprocess.run(["git", "check-ignore", "-q", str(probe)], cwd=ROOT).returncode == 0


def eligible(keys: list[tuple[str, int]]) -> list[tuple[str, str]]:
    """(key id, key) of every exam-like object, in key-id order (never name order)."""
    found = [
        (key_id(k), k)
        for k, size in keys
        if size > 0 and not k.endswith("/") and k.lower().endswith(SUFFIXES)
    ]
    return sorted(found)


def index_rows(index: Path) -> dict[str, tuple[str, str]]:
    """Local original name -> (copy name or "not written", re-verification status)."""
    rows: dict[str, tuple[str, str]] = {}
    for line in index.read_text(encoding="utf-8").splitlines():
        if not line.startswith("| ") or not line[2:3].isdigit():
            continue
        cells = [c.strip() for c in line.strip().strip("|").split(" | ")]
        text = [c[1 : c.index("](")] if c.startswith("[") and "](" in c else c for c in cells]
        rows[text[1]] = (text[4], text[5])
    return rows


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--count", action="store_true", help="counts only; nothing downloaded")
    parser.add_argument("--limit", type=int, default=50, help="exams in this batch (default 50)")
    parser.add_argument("--terms", help="terms file for anonimizador batch (outside the repo)")
    args = parser.parse_args()

    for place in (OUT, BATCHES):
        place.mkdir(parents=True, exist_ok=True)
        if not ignored(place):
            fail("a work directory is not git-ignored; refusing to write exams there")
    aws = Aws()
    exams = eligible(aws.keys())
    done = ledger()
    pending = [(kid, key) for kid, key in exams if kid not in done]
    say(f"{len(exams)} exam(s) in the bucket, {len(done)} done, {len(pending)} pending")
    if args.count or not pending:
        return

    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    batch = BATCHES / stamp
    batch.mkdir()
    started = time.monotonic()
    local: dict[str, str] = {}  # local name -> key id
    failed_downloads = 0
    for kid, key in pending[: args.limit]:
        name = f"src-{kid}{Path(key).suffix.lower()}"
        got = aws.run(
            "s3api", "get-object", "--bucket", SOURCE_BUCKET, "--key", key, str(batch / name)
        )
        if got.returncode != 0:
            failed_downloads += 1
            continue
        local[name] = kid
    say(
        f"downloaded {len(local)} exam(s) ({failed_downloads} failed) "
        f"in {time.monotonic() - started:.0f} s"
    )
    if not local:
        shutil.rmtree(batch)
        fail("nothing to anonymize")

    before = set(OUT.glob("*-anonimizer.md"))
    command = ["uv", "run", "anonimizador", "batch", "--out", str(OUT), str(batch)]
    if args.terms:
        command[4:4] = ["--terms", args.terms]
    console = OUT / f"r2-batch-{stamp}-console.txt"
    with console.open("x", encoding="utf-8") as log:
        code = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT).returncode
    indexes = set(OUT.glob("*-anonimizer.md")) - before
    if len(indexes) != 1:
        fail(
            f"anonimizador batch wrote no index (exit {code}); "
            "the originals stay in the batch directory"
        )
    index = indexes.pop()
    run_dir = OUT / index.name.removesuffix("-anonimizer.md")
    rows = index_rows(index)
    say(f"anonimizador batch: exit {code} after {time.monotonic() - started:.0f} s")

    counts: dict[str, int] = {}
    uploaded = 0
    for name, kid in local.items():
        copy, status = rows.get(name, ("not written", "missing"))
        counts[status] = counts.get(status, 0) + 1
        sent = False
        if status == "ok":
            put = aws.run(
                "s3",
                "cp",
                str(run_dir / copy),
                f"s3://{TARGET_BUCKET}/{copy}",
                "--only-show-errors",
            )
            sent = put.returncode == 0
            uploaded += sent
        record(
            {"key_id": kid, "copy": copy, "status": status, "uploaded": sent, "run": run_dir.name}
        )
    shutil.rmtree(batch)
    summary = ", ".join(f"{k} {v}" for k, v in sorted(counts.items()))
    say(f"verify of the copies: {summary}; uploaded to {TARGET_BUCKET}: {uploaded}")
    say(
        f"batch done in {time.monotonic() - started:.0f} s; "
        f"{len(pending) - len(local)} exam(s) still pending"
    )


if __name__ == "__main__":
    main()
