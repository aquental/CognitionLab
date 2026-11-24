"""Typed config dumps of the shipped YAML files (spec/migration-plan.md §3.10 item 5).

Run from mini-swe-agent-rs/ with the reference environment and no provider variables set:

    env -u LITELLM_MODEL_REGISTRY_PATH -u MSWEA_COST_TRACKING MSWEA_SILENT_STARTUP=1 \
      MSWEA_GLOBAL_CONFIG_DIR=$(mktemp -d) uv run --frozen --project ../mini-swe-agent \
      python parity/config_dumps.py

For each file in config/, it writes tests/goldens/config/<name>.json with the pydantic
`model_dump(mode="json")` of the agent, environment and text-based model sections. The model
name is not in the files, so it is set to "test-model", as the CLI's `-m` would.
"""

import json
import sys
from pathlib import Path

import yaml

from minisweagent.agents.default import AgentConfig
from minisweagent.environments.local import LocalEnvironmentConfig
from minisweagent.models.litellm_textbased_model import LitellmTextbasedModelConfig

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "goldens" / "config"


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    for path in sorted((ROOT / "config").glob("*.yaml")):
        config = yaml.safe_load(path.read_text(encoding="utf-8"))
        dump = {
            "agent": AgentConfig(**config.get("agent", {})).model_dump(mode="json"),
            "environment": LocalEnvironmentConfig(**config.get("environment", {})).model_dump(mode="json"),
            "model": LitellmTextbasedModelConfig(**{**config.get("model", {}), "model_name": "test-model"}).model_dump(
                mode="json"
            ),
        }
        target = OUT / f"{path.stem}.json"
        target.write_text(json.dumps(dump, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"{path.relative_to(ROOT)} -> {target.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
