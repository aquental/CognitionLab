"""Python side of the differential parity harness (spec/migration-plan.md §3.15 item 4).

Runs parity scenarios through the reference library, never modifying it:

    python parity/run_python.py <scenarios_dir> <base_dir> <out.json> [scenario_id ...]

The Rust harness (tests/support/mod.rs) starts this script with the interpreter of the reference
environment (`uv run --frozen --project ../mini-swe-agent`) and one explicit, credential-free
environment. Each scenario runs in a fresh workspace at `<base_dir>/<id>/work`; the Rust side later
recreates the same path, so paths in outputs and configs are equal without rewriting.

Output: {"oracle": <path of the imported minisweagent package>, "results": {scenario_id: result}},
each result with the same shape the Rust harness builds. The default config is the reference
package's own `config/default.yaml`.
"""

import builtins
import copy
import json
import os
import shutil
import sys
from pathlib import Path

import yaml

import minisweagent
import minisweagent.models.test_models as test_models
from minisweagent import exceptions
from minisweagent.agents.default import DefaultAgent
from minisweagent.config import builtin_config_dir
from minisweagent.environments.local import LocalEnvironment
from minisweagent.models import GlobalModelStats
from minisweagent.utils.serialize import recursive_merge

INTERRUPTS = {
    name: getattr(exceptions, name)
    for name in ["Submitted", "LimitsExceeded", "TimeExceeded", "UserInterruption", "FormatError"]
}


def build_exception(spec: dict) -> Exception:
    """The exception a JSON `raise` spec stands for (plan §2.2, JSON-only fixtures)."""
    kind = spec["kind"]
    if kind in INTERRUPTS:
        return INTERRUPTS[kind](*spec["messages"])
    cls = getattr(builtins, kind, None)
    if not (isinstance(cls, type) and issubclass(cls, Exception)):
        cls = type(kind, (Exception,), {})
    return cls(spec.get("message", ""))


class JsonRaiseDeterministicModel(test_models.DeterministicModel):
    """`DeterministicModel` whose `raise` actions are JSON specs, turned into exceptions here.

    Python's `_process_test_actions` would `raise` the dict itself. Actions are inspected in
    order, as `_process_test_actions` does: a `/sleep` or `/warning` before the `raise` is left
    to the parent class.
    """

    def query(self, messages, **kwargs):
        index = self.current_index + 1
        if index < len(self.config.outputs):
            for action in self.config.outputs[index].get("extra", {}).get("actions", []):
                if isinstance(action, dict) and isinstance(action.get("raise"), dict):
                    self.current_index = index
                    raise build_exception(action["raise"])
                command = action.get("command", "") if isinstance(action, dict) else ""
                if command.startswith("/sleep ") or command.startswith("/warning"):
                    break
        return super().query(messages, **kwargs)


class RecordingModel:
    """Records a deep copy of the history passed to each agent-level `query`."""

    def __init__(self, inner, queries: list):
        self._inner = inner
        self._queries = queries

    def query(self, messages, **kwargs):
        self._queries.append(copy.deepcopy(messages))
        return self._inner.query(messages, **kwargs)

    def __getattr__(self, name):
        return getattr(self._inner, name)


class RecordingEnvironment:
    """Records each action passed to `execute`."""

    def __init__(self, inner, actions: list):
        self._inner = inner
        self._actions = actions

    def execute(self, action, *args, **kwargs):
        self._actions.append(copy.deepcopy(action))
        return self._inner.execute(action, *args, **kwargs)

    def __getattr__(self, name):
        return getattr(self._inner, name)


def snapshot(workspace: Path) -> dict:
    """Every entry under the workspace: directories (empty ones too), symlinks and file bytes."""
    files = {}
    for path in sorted(workspace.rglob("*")):
        rel = path.relative_to(workspace).as_posix()
        if path.is_symlink():
            files[rel] = {"symlink": os.readlink(path)}
        elif path.is_dir():
            files[rel] = {"dir": True}
        elif path.is_file():
            data = path.read_bytes()
            try:
                files[rel] = {"text": data.decode("utf-8")}
            except UnicodeDecodeError:
                files[rel] = {"hex": data.hex()}
    return files


def reset_dirs(base_env: dict) -> None:
    """Fresh HOME, TMPDIR and global config dir for each scenario (the Rust side does the same)."""
    for name in ["HOME", "TMPDIR", "MSWEA_GLOBAL_CONFIG_DIR"]:
        directory = Path(base_env[name])
        if directory.exists():
            shutil.rmtree(directory)
        directory.mkdir(parents=True)


def fresh_workspace(base: Path, scenario: dict) -> Path:
    workspace = base / scenario["id"] / "work"
    if workspace.exists():
        shutil.rmtree(workspace)
    workspace.mkdir(parents=True)
    for name, text in scenario.get("files", {}).items():
        target = workspace / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")
    return workspace


def set_environment(base_env: dict, scenario: dict) -> None:
    os.environ.clear()
    os.environ.update(base_env)
    os.environ.update(scenario.get("host_env", {}))


def run_agent(scenario: dict, workspace: Path, defaults: dict) -> dict:
    config = recursive_merge(defaults, scenario.get("config", {}))
    # The trajectory file sits next to the workspace, at the same path on both sides.
    output_path = workspace.parent / "traj.json" if scenario.get("output_path") else None
    if output_path is not None:
        config["agent"]["output_path"] = str(output_path)
    stats = GlobalModelStats()
    test_models.GLOBAL_MODEL_STATS = stats
    model = JsonRaiseDeterministicModel(outputs=scenario["outputs"], **config.get("model", {}))
    env = LocalEnvironment(**{**config.get("environment", {}), "cwd": str(workspace)})
    queries, actions = [], []
    agent = DefaultAgent(RecordingModel(model, queries), RecordingEnvironment(env, actions), **config["agent"])
    returned, error = None, None
    try:
        returned = agent.run(scenario["task"])
    except Exception as e:  # the fatal path re-raises after the exit message
        error = type(e).__name__
    trajectory = agent.save(None, *scenario.get("save_extra", []))
    result = {
        "trajectory": trajectory,
        "queries": queries,
        "actions": actions,
        "state": {
            "returned": returned,
            "error": error,
            "n_calls": agent.n_calls,
            "cost": agent.cost,
            "global_cost": stats.cost,
            "global_calls": stats.n_calls,
            "remaining_outputs": len(model.config.outputs) - (model.current_index + 1),
        },
        "artifacts": snapshot(workspace),
    }
    if output_path is not None:
        result["saved"] = json.loads(output_path.read_text(encoding="utf-8")) if output_path.exists() else None
        output_path.unlink(missing_ok=True)
    return result


def run_environment(scenario: dict, workspace: Path) -> dict:
    env = LocalEnvironment(**{"cwd": str(workspace), **scenario.get("config", {})})
    results = []
    for command in scenario["commands"]:
        try:
            results.append({"output": env.execute({"command": command})})
        except exceptions.Submitted as e:
            results.append({"submitted": e.messages[0]})
    return {"results": results, "artifacts": snapshot(workspace)}


def main() -> int:
    scenarios_dir, base, out = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    wanted = set(sys.argv[4:])
    defaults = yaml.safe_load((builtin_config_dir / "default.yaml").read_text(encoding="utf-8"))
    base_env = dict(os.environ)
    results = {}
    for path in sorted(scenarios_dir.glob("*.json")):
        scenario = json.loads(path.read_text(encoding="utf-8"))
        if wanted and scenario["id"] not in wanted:
            continue
        reset_dirs(base_env)
        set_environment(base_env, scenario)
        workspace = fresh_workspace(base, scenario)
        if scenario["kind"] == "agent":
            results[scenario["id"]] = run_agent(scenario, workspace, defaults)
        else:
            results[scenario["id"]] = run_environment(scenario, workspace)
        shutil.rmtree(workspace)
    set_environment(base_env, {})
    report = {"oracle": str(Path(minisweagent.__file__).resolve()), "results": results}
    out.write_text(json.dumps(report, indent=1, ensure_ascii=False), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
