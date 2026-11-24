"""Python side of the raw-response differential (spec/migration-plan.md §3.15 item 4, phase P8).

Runs the reference's real `LitellmTextbasedModel` (with the reference's `mini_textbased.yaml`
model section) on canned provider responses, with `litellm.completion` patched, and prints one
projection per case: the outcome, the parsed actions or format-error message, the cost, the
content kept in `extra.response`, the messages sent to the provider, and the global stats. The Rust side (`tests/text_parity.rs`) builds the same projection.

    python parity/text_model_python.py parity/text_model_cases.json <out.json>
"""

import json
import sys
import tempfile
from pathlib import Path
from unittest.mock import patch

import litellm
import yaml

import minisweagent.models.litellm_model as litellm_model
from minisweagent.config import builtin_config_dir
from minisweagent.exceptions import FormatError
from minisweagent.models import GlobalModelStats
from minisweagent.models.litellm_textbased_model import LitellmTextbasedModel


def project(case: dict, registry: Path, section: dict, messages: list) -> dict:
    stats = GlobalModelStats()
    litellm_model.GLOBAL_MODEL_STATS = stats
    kwargs = {**section, "model_name": case["model_name"], "litellm_model_registry": str(registry)}
    kwargs["cost_tracking"] = case.get("cost_tracking", "default")
    model = LitellmTextbasedModel(**kwargs)
    response = litellm.ModelResponse(**case["response"])
    with patch("litellm.completion", return_value=response) as completion:
        try:
            message = model.query(messages)
            result = {
                "kind": "ok",
                "role": message["role"],
                "content": message["content"],
                "actions": message["extra"]["actions"],
                "cost": message["extra"]["cost"],
                "extra_keys": list(message["extra"]),
                "response_content": message["extra"]["response"]["choices"][0]["message"]["content"],
            }
        except FormatError as e:
            head = e.messages[0]
            result = {
                "kind": "format_error",
                "role": head["role"],
                "content": head["content"],
                "interrupt_type": head["extra"]["interrupt_type"],
                "n_actions": head["extra"]["n_actions"],
                "model_response": head["extra"]["model_response"],
                "cost": head["extra"]["cost"],
                "extra_keys": list(head["extra"]),
                "response_content": head["extra"]["response"]["choices"][0]["message"]["content"],
            }
        except Exception as e:  # a fatal model error
            result = {"kind": "error", "type": type(e).__name__}
    result["request_messages"] = completion.call_args.kwargs["messages"]
    result["stats"] = {"calls": stats.n_calls, "cost": stats.cost}
    return result


def main() -> int:
    doc = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    section = yaml.safe_load((builtin_config_dir / "mini_textbased.yaml").read_text(encoding="utf-8"))["model"]
    with tempfile.TemporaryDirectory() as tmp:
        registry = Path(tmp) / "registry.json"
        registry.write_text(json.dumps(doc["registry"]), encoding="utf-8")
        results = {case["id"]: project(case, registry, section, doc["messages"]) for case in doc["cases"]}
    Path(sys.argv[2]).write_text(json.dumps(results, indent=1, ensure_ascii=False), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
