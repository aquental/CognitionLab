"""The optional classifier step of `batch` (`../embeddings-fundamentals/spec/CLASSIFIER.md`
§11), with fake classifier binaries: scripts that print fixed JSON lines. Every input is
synthetic, and nothing is sent anywhere."""

import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path

import pytest
from conftest import requires_tools
from make_covered_name_pdf import build_pdf, covered_name_pdf

import anonimizador.batch
from anonimizador import classifier as cls
from anonimizador.batch import run_batch, start
from anonimizador.cli import EXIT_OK, EXIT_USAGE, main
from anonimizador.terms import Term
from anonimizador.verify import Options, VerifyError

NOW = datetime(2026, 10, 31, 9, 15, 0)
PROBABILITIES = {
    "vop": 0.001,
    "eco_convencional": 0.04,
    "eco_avancado": 0.93,
    "calorimetria": 0.004,
    "ergoespirometria": 0.004,
    "holter": 0.004,
    "mapa": 0.004,
    "ecg": 0.009,
    "outro": 0.004,
}


def line(estado: str, codigo: int, **extra: object) -> dict:
    """A classifier JSON line with the keys the index reads."""
    fields = {
        "estado": estado,
        "codigo_saida": codigo,
        "envio_realizado": estado in ("ok", "incerto"),
        "provider": "typesafe",
        "model": "jev-1.13.0" if estado in ("ok", "incerto") else None,
        "adapter_version": "cognitionlab-jev-http/1",
        "perguntas_versao": "classificador-documento-1",
        "mascaramento_versao": "mascaramento-5+cognitionlab-1",
        "paginas_modo": "2",
        "limiar_aplicado": 0.7,
        "rotulo": None,
        "confianca": None,
        "probabilidades": {},
        "erro_categoria": None,
        "motivo_codigo": None,
    }
    fields.update(extra)
    return fields


def ok(estado: str = "ok", confianca: float = 0.93) -> dict:
    return line(
        estado,
        0,
        tipo="eco_avancado",
        rotulo="Eco avançado",
        confianca=confianca,
        probabilidades=PROBABILITIES,
    )


def out(fields: dict) -> bytes:
    return (json.dumps(fields, ensure_ascii=False) + "\n").encode("utf-8")


def fake(directory: Path, stdout: bytes, code: int = 0, sleep: float = 0.0) -> Path:
    """A fake classifier: records its arguments without the copy's path in `argv.txt`,
    then prints `stdout` and exits with `code`."""
    record = directory / "argv.txt"
    script = directory / "fake-classifier"
    script.write_text(
        f"#!{sys.executable}\n"
        "import pathlib, sys, time\n"
        f"with pathlib.Path({str(record)!r}).open('a') as f:\n"
        "    f.write(' '.join(sys.argv[1:-1]) + '\\n')\n"
        f"time.sleep({sleep})\n"
        f"sys.stdout.buffer.write({stdout!r})\n"
        f"sys.exit({code})\n",
        encoding="utf-8",
    )
    script.chmod(0o755)
    return script


def test_each_state_becomes_its_cell():
    weights = (
        "Eco avançado 0.930 · Eco convencional 0.040 · ECG 0.009 · Calorimetria 0.004 · "
        "Ergoespirometria 0.004 · Holter 0.004 · MAPA 0.004 · Outro 0.004 · VOP 0.001"
    )
    for fields, code, cell, shown in [
        (ok(), 0, "Eco avançado (0.930)", weights),
        (ok("incerto", 0.6), 0, "uncertain: Eco avançado (0.600)", weights),
        (line("preparado", 0), 0, "not sent", "—"),
        (line("sem_conteudo", 3), 3, "no content", "—"),
        (line("bloqueado_privacidade", 6), 6, "blocked (privacy check)", "—"),
        (line("erro", 4, erro_categoria="timeout"), 4, "error: timeout", "—"),
        (line("erro", 2, motivo_codigo="arquivo_ilegivel"), 2, "error: arquivo_ilegivel", "—"),
        (line("erro", 5, motivo_codigo="ocr_indisponivel"), 5, "error: ocr_indisponivel", "—"),
        (line("erro", 1, motivo_codigo="erro_interno"), 1, "error: erro_interno", "—"),
    ]:
        result = cls.interpret(code, out(fields))
        assert (result.cell, result.weights) == (cell, shown)
        assert result.line == json.dumps(fields, ensure_ascii=False)
        assert result.state == fields["estado"]


def test_anything_but_one_valid_line_matching_its_exit_code_is_a_failure():
    probabilities = dict(PROBABILITIES)
    del probabilities["vop"]
    for code, stdout in [
        (0, b""),
        (0, b"not json\n"),
        (0, out(ok()) + out(ok())),
        (0, b"[1, 2]\n"),
        (0, b"\xff\xfe\n"),
        (3, out(ok())),  # the exit code does not match the line
        (7, out(line("erro", 7, motivo_codigo="x"))),  # an unknown exit code
        (4, out(line("ok", 4))),  # a state with another state's code
        (5, out(line("erro", 2, motivo_codigo="arquivo_ilegivel"))),  # valid code, not the line's
        (0, out(line("desconhecido", 0))),
        (0, out(ok() | {"codigo_saida": False})),
        (0, out(ok() | {"probabilidades": probabilities})),
        (0, out(ok() | {"probabilidades": PROBABILITIES | {"vop": "0.001"}})),
        (0, out(ok() | {"probabilidades": PROBABILITIES | {"extra": 0.0}})),
        (0, out(ok() | {"rotulo": "Raio X"})),
        (0, out(ok() | {"confianca": True})),
        (4, out(line("erro", 4, erro_categoria="x | y"))),
        (2, out(line("erro", 2, motivo_codigo=None))),
    ]:
        result = cls.interpret(code, stdout)
        assert result.cell == cls.FAILED
        assert result.line is None and result.fields is None


def test_weights_break_ties_in_the_classifiers_order():
    even = dict.fromkeys(cls.LABELS, 1 / 9)
    assert cls.weights(even) == " · ".join(f"{label} 0.111" for label in cls.LABELS.values())


def test_the_deadline_is_testodocs_step_ceiling():
    assert cls.deadline({}) == 55
    assert cls.deadline({"RLCD_TIMEOUT_S": "10"}) == 35
    for invalid in ["x", "0", "-1", "inf", "nan", ""]:
        assert cls.deadline({"RLCD_TIMEOUT_S": invalid}) == 55


def test_the_child_gets_enviar_only_with_remote(tmp_path):
    binary = fake(tmp_path, out(line("preparado", 0)))
    copy = tmp_path / "exam-000000000000.pdf"
    copy.write_bytes(b"synthetic")
    assert cls.classify(binary, copy, remote=False).cell == "not sent"
    assert cls.classify(binary, copy, remote=True).cell == "not sent"
    recorded = (tmp_path / "argv.txt").read_text(encoding="utf-8").splitlines()
    assert recorded == ["classify --json", "classify --json --enviar"]


def test_a_child_that_hangs_cannot_start_or_crashes_is_a_failure(tmp_path, monkeypatch):
    copy = tmp_path / "exam-000000000000.pdf"
    copy.write_bytes(b"synthetic")
    monkeypatch.setattr(cls, "deadline", lambda env: 0.5)
    slow = fake(tmp_path, out(ok()), sleep=5)
    assert cls.classify(slow, copy, remote=False).cell == cls.FAILED
    assert cls.classify(tmp_path / "missing", copy, remote=False).cell == cls.FAILED
    not_executable = tmp_path / "plain.txt"
    not_executable.write_text("x", encoding="utf-8")
    assert cls.classify(not_executable, copy, remote=False).cell == cls.FAILED


def test_the_header_names_the_classifier_never_its_path():
    first = cls.interpret(0, out(ok()))
    text = cls.header(first, remote=True)
    assert text == (
        "- Classifier: provider typesafe, model jev-1.13.0, adapter cognitionlab-jev-http/1, "
        "questions classificador-documento-1, masking mascaramento-5+cognitionlab-1, "
        "threshold 0.7, pages 2; sending requested (--classify-remote)"
    )
    prepared = cls.header(cls.interpret(0, out(line("preparado", 0))), remote=False)
    assert "model none" in prepared and prepared.endswith("sending not requested")
    assert cls.header(None, remote=False) == "- Classifier: no result; sending not requested"


def test_the_classifier_directory_must_be_one_git_cannot_commit(tmp_path):
    # The JSON lines sit beside the copies: rules that ignore the index, the logs, and
    # the copies but not `classifier/` would leave them committable.
    repo = tmp_path / "repo"
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    rules = "*-anonimizer.md\nlogs/\nexam-*.pdf\nexam-*.png\n"
    (repo / ".gitignore").write_text(rules, encoding="utf-8")
    with pytest.raises(VerifyError, match="git cannot commit"):
        start(repo / "out", NOW, classify=True)
    assert not (repo / "out").exists()
    run = start(repo / "out", NOW, classify=False)
    assert not run.classified.exists()


def test_classify_remote_needs_a_classifier(outside_repo, capsys):
    with pytest.raises(SystemExit) as e:
        main(["batch", "--classify-remote", "--out", str(outside_repo / "o"), str(outside_repo)])
    assert e.value.code == EXIT_USAGE
    assert "--classify-remote needs --classifier" in capsys.readouterr().err


@pytest.fixture
def originals(outside_repo: Path) -> Path:
    exams = outside_repo / "exams"
    exams.mkdir()
    (exams / "a.pdf").write_bytes(covered_name_pdf())
    (exams / "b.pdf").write_bytes(
        build_pdf(b"BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET\n", b"")
    )
    return exams


def options() -> Options:
    return Options(lang="por", dpi=150, use_ocr=True, timeout=120, strict_patterns=False)


@requires_tools
def test_batch_classifies_each_verified_copy_and_keeps_its_line(originals, outside_repo):
    binary = fake(outside_repo, out(ok()))
    out_dir = outside_repo / "out"
    totals = run_batch(originals, out_dir, [], options(), now=NOW, classifier=binary)
    assert totals.found == 0
    report = next(out_dir.glob("*-anonimizer.md")).read_text(encoding="utf-8")
    assert "| Verify anonymized | Exam type (classifier) | Weights |" in report
    assert "|---:|---|---|---|---|---|---|---|" in report
    assert "- Classifier: provider typesafe, model jev-1.13.0" in report
    assert "sending not requested" in report
    assert str(binary) not in report
    kept = sorted((out_dir / NOW.strftime("%Y%m%d-%H%M%S") / "classifier").glob("exam-*.json"))
    assert len(kept) == 2
    assert all(json.loads(p.read_text(encoding="utf-8"))["estado"] == "ok" for p in kept)
    for p in kept:
        assert f"[Eco avançado (0.930)](20261031-091500/classifier/{p.name})" in report
    assert report.count("Eco avançado 0.930 · Eco convencional 0.040") == 2


@requires_tools
def test_a_copy_that_fails_verification_is_never_classified(originals, outside_repo, monkeypatch):
    # With every detection dropped, the copy of the page that shows the name keeps it,
    # and its re-verification finds it (R8).
    monkeypatch.setattr(anonimizador.batch, "detect", lambda *args: [])
    (originals / "a.pdf").write_bytes(
        build_pdf(b"BT /F1 14 Tf 72 700 Td (Paciente Fulano Ficticio) Tj ET\n", b"")
    )
    binary = fake(outside_repo, out(ok()))
    out_dir = outside_repo / "out"
    totals = run_batch(
        originals, out_dir, [Term("Fulano Ficticio")], options(), now=NOW, classifier=binary
    )
    assert totals.found == 1
    report = next(out_dir.glob("*-anonimizer.md")).read_text(encoding="utf-8")
    rows = [r for r in report.splitlines() if r.startswith("| 1 ") or r.startswith("| 2 ")]
    found_row = next(r for r in rows if "FOUND" in r)
    assert found_row.endswith("| — | — |")
    recorded = (outside_repo / "argv.txt").read_text(encoding="utf-8").splitlines()
    assert len(recorded) == 1, "only the copy that verified ok was classified"


@requires_tools
def test_a_failing_classifier_never_fails_the_anonymization(originals, outside_repo):
    binary = fake(outside_repo, b"garbage\n", code=9)
    out_dir = outside_repo / "out"
    code = main(["batch", "--classifier", str(binary), "--out", str(out_dir), str(originals)])
    assert code == EXIT_OK
    report = next(out_dir.glob("*-anonimizer.md")).read_text(encoding="utf-8")
    assert report.count(f"| {cls.FAILED} | — |") == 2
    assert "- Classifier: no result; sending not requested" in report
    # An invalid line is never kept.
    run_dir = next(p for p in out_dir.iterdir() if p.is_dir())
    assert list((run_dir / "classifier").iterdir()) == []


@requires_tools
def test_without_a_classifier_the_index_is_unchanged(originals, outside_repo):
    out_dir = outside_repo / "out"
    run_batch(originals, out_dir, [], options(), now=NOW)
    report = next(out_dir.glob("*-anonimizer.md")).read_text(encoding="utf-8")
    assert "Exam type" not in report and "Classifier" not in report
    assert "| Verify anonymized |\n|---:|---|---|---|---|---|\n" in report
    assert not (out_dir / NOW.strftime("%Y%m%d-%H%M%S") / "classifier").exists()
