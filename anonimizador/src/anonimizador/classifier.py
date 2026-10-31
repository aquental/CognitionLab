"""Optional exam-type classification of the anonymized copies (`batch --classifier`).

The classifier is the `classify` command of `../embeddings-fundamentals`
(`spec/CLASSIFIER.md` §10, §11). It runs as a child process, without a shell, on a copy
whose re-verification was `ok`, never on an original. It sends the copy's masked text to
TypeSafe's Jev only when `batch` got `--classify-remote` and the environment has
`CLASSIFIER_REMOTE_ALLOWED=1`; otherwise it only prepares the state (`not sent`).

A classifier failure never fails the anonymization: a timeout, a child that cannot start,
an output that is not exactly one valid JSON line, or an exit code that does not match
the line becomes the cell `error: classifier failed`, and the run's exit status is
unchanged.
"""

from __future__ import annotations

import json
import math
import os
import re
import subprocess
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path

# The nine exam types, in the classifier's order (`Categoria::TODAS`), with their labels.
LABELS = {
    "vop": "VOP",
    "eco_convencional": "Eco convencional",
    "eco_avancado": "Eco avançado",
    "calorimetria": "Calorimetria",
    "ergoespirometria": "Ergoespirometria",
    "holter": "Holter",
    "mapa": "MAPA",
    "ecg": "ECG",
    "outro": "Outro",
}
FAILED = "error: classifier failed"
NOT_CLASSIFIED = "—"
# The exit codes each state may have (`spec/CLASSIFIER.md` §10.3).
CODES = {
    "ok": {0},
    "incerto": {0},
    "preparado": {0},
    "sem_conteudo": {3},
    "bloqueado_privacidade": {6},
    "erro": {1, 2, 4, 5},
}
# A reason or error category is a snake_case word: anything else is not trusted into the
# index.
WORD = re.compile(r"[a-z][a-z_]*")
HEADER_FIELDS = (
    ("provider", "provider"),
    ("model", "model"),
    ("adapter_version", "adapter"),
    ("perguntas_versao", "questions"),
    ("mascaramento_versao", "masking"),
    ("limiar_aplicado", "threshold"),
    ("paginas_modo", "pages"),
)


@dataclass(frozen=True)
class Classification:
    """One copy's result: the two index cells, a short state for the console, and the
    JSON line to keep (`None` when the output was not a valid line)."""

    cell: str
    weights: str
    state: str
    line: str | None = None
    fields: Mapping[str, object] | None = None


def failed() -> Classification:
    return Classification(FAILED, NOT_CLASSIFIED, "failed")


def deadline(env: Mapping[str, str]) -> float:
    """`2 * (RLCD_TIMEOUT_S + 5) + 5` seconds, the step ceiling of TestoDoc's TD §12.2;
    `RLCD_TIMEOUT_S` defaults to 20 when missing or not a positive finite number."""
    try:
        seconds = float(env.get("RLCD_TIMEOUT_S", "20"))
    except ValueError:
        seconds = 20.0
    if not math.isfinite(seconds) or seconds <= 0:
        seconds = 20.0
    return 2 * (seconds + 5) + 5


def number(value: object) -> float | None:
    """A JSON number (never a bool, which `json` reads as an int)."""
    if isinstance(value, bool) or not isinstance(value, int | float):
        return None
    return float(value) if math.isfinite(value) else None


def weights(probabilities: object) -> str | None:
    """The nine types, highest first, ties in the classifier's order."""
    if not isinstance(probabilities, dict) or set(probabilities) != set(LABELS):
        return None
    values = {key: number(value) for key, value in probabilities.items()}
    if any(v is None for v in values.values()):
        return None
    order = list(LABELS)
    ranked = sorted(LABELS, key=lambda k: (-values[k], order.index(k)))
    return " · ".join(f"{LABELS[k]} {values[k]:.3f}" for k in ranked)


def interpret(code: int, stdout: bytes) -> Classification:
    """The child's exit code and stdout as the index shows them."""
    try:
        lines = stdout.decode("utf-8").splitlines()
    except UnicodeDecodeError:
        return failed()
    if len(lines) != 1:
        return failed()
    try:
        fields = json.loads(lines[0])
    except ValueError:
        return failed()
    if not isinstance(fields, dict):
        return failed()
    state = fields.get("estado")
    exit_code = fields.get("codigo_saida")
    if (
        not isinstance(state, str)
        or state not in CODES
        or isinstance(exit_code, bool)
        or exit_code != code
        or code not in CODES[state]
    ):
        return failed()
    keep = {"line": lines[0], "fields": fields, "state": state}
    if state in ("ok", "incerto"):
        label = fields.get("rotulo")
        confidence = number(fields.get("confianca"))
        ranked = weights(fields.get("probabilidades"))
        if label not in LABELS.values() or confidence is None or ranked is None:
            return failed()
        prefix = "uncertain: " if state == "incerto" else ""
        return Classification(f"{prefix}{label} ({confidence:.3f})", ranked, **keep)
    if state == "erro":
        reason = fields.get("erro_categoria") if code == 4 else fields.get("motivo_codigo")
        if not isinstance(reason, str) or not WORD.fullmatch(reason):
            return failed()
        return Classification(f"error: {reason}", NOT_CLASSIFIED, **keep)
    cell = {
        "preparado": "not sent",
        "sem_conteudo": "no content",
        "bloqueado_privacidade": "blocked (privacy check)",
    }[state]
    return Classification(cell, NOT_CLASSIFIED, **keep)


def classify(
    binary: Path, copy: Path, remote: bool, env: Mapping[str, str] | None = None
) -> Classification:
    """Runs `<binary> classify --json [--enviar] <copy>` and interprets its output. The
    child's stderr is discarded: it is never part of the index."""
    env = os.environ if env is None else env
    args = [str(binary), "classify", "--json", *(["--enviar"] if remote else []), str(copy)]
    try:
        done = subprocess.run(
            args,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            timeout=deadline(env),
            check=False,
        )
    except (subprocess.TimeoutExpired, OSError):
        return failed()
    return interpret(done.returncode, done.stdout)


def header(first: Classification | None, remote: bool) -> str:
    """The index line about the classifier: its identity, from the first valid JSON line
    of the run, and whether sending was asked for. Never the binary's path."""
    asked = "sending requested (--classify-remote)" if remote else "sending not requested"
    if first is None or first.fields is None:
        return f"- Classifier: no result; {asked}"
    parts = []
    for key, name in HEADER_FIELDS:
        value = first.fields.get(key)
        shown = "none" if value is None else " ".join(str(value).split())
        parts.append(f"{name} {shown}")
    return f"- Classifier: {', '.join(parts)}; {asked}"
