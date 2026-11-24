"""Goldens for the text-action parser and observation messages (`models/utils/actions_text.py`).

Run from mini-swe-agent-rs/ with the reference environment:

    MSWEA_SILENT_STARTUP=1 MSWEA_GLOBAL_CONFIG_DIR=$(mktemp -d) \
      uv run --frozen --project ../mini-swe-agent python parity/action_goldens.py

It writes tests/goldens/actions/parse.json and observations.json. `time.time` is fixed so the
observation timestamps are reproducible.
"""

import json
import sys
from pathlib import Path
from unittest.mock import patch

import yaml

from minisweagent.exceptions import FormatError
from minisweagent.models.litellm_textbased_model import LitellmTextbasedModelConfig
from minisweagent.models.utils.actions_text import format_observation_messages, parse_regex_actions

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "goldens" / "actions"
FENCE = "```mswea_bash_command"
DEFAULT_REGEX = LitellmTextbasedModelConfig.model_fields["action_regex"].default
DEFAULT_FORMAT_ERROR = LitellmTextbasedModelConfig.model_fields["format_error_template"].default
TIMESTAMP = 1700000000.25

CONTENTS = {
    "one_block": f"THOUGHT: list files.\n\n{FENCE}\nls -la\n```\n",
    "zero_blocks": "I will just think about it.",
    "two_blocks": f"{FENCE}\nls\n```\nand\n{FENCE}\npwd\n```",
    "adjacent_fences": f"{FENCE}\na\n```\n{FENCE}\nb\n```",
    "empty_block": f"{FENCE}\n\n```",
    "blank_command_line": f"{FENCE}\n   \n```",
    "multi_line_block": f"{FENCE}\ncd src &&\n  make test\n```",
    "heredoc_with_fence": f"{FENCE}\ncat <<'EOF' > a.md\n```\nEOF\n```",
    "plain_bash_fence": "```bash\nls\n```",
    "crlf_block": f"{FENCE}\r\nls -la\r\n```",
    "surrounding_whitespace": f"{FENCE}   \n\t  echo hi  \t\n```",
    "unicode": f"Olá ✓\n{FENCE}\necho 'ünïcode 🦀'\n```",
    "empty_content": "",
    "fence_without_newline": f"{FENCE} ls```",
}


def parse_case(name, content, regex, template, finish_reason=None):
    kwargs = {} if finish_reason is None else {"finish_reason": finish_reason}
    try:
        actions = parse_regex_actions(
            content, action_regex=regex, format_error_template=template, template_kwargs=kwargs or None
        )
        result = {"actions": actions}
    except FormatError as e:
        result = {"format_error": list(e.messages)}
    return {
        "name": name,
        "content": content,
        "action_regex": regex,
        "format_error_template": template,
        "template_kwargs": kwargs,
        "result": result,
    }


def main() -> int:
    config = yaml.safe_load((ROOT / "config" / "mini_textbased.yaml").read_text(encoding="utf-8"))
    shipped_format_error = config["model"]["format_error_template"]
    cases = []
    for name, content in CONTENTS.items():
        cases.append(parse_case(name, content, DEFAULT_REGEX, DEFAULT_FORMAT_ERROR))
    for reason in [None, "stop", "length", "tool_calls"]:
        for name in ["zero_blocks", "two_blocks", "one_block"]:
            cases.append(parse_case(f"shipped:{name}:{reason}", CONTENTS[name], DEFAULT_REGEX, shipped_format_error, reason))
    cases.append(parse_case("custom:optional_group_unmatched", "ACT:;", r"ACT:(\w+)?;", "{{ error }}"))
    cases.append(parse_case("custom:no_group", "run ls -la now", r"ls -la", "{{ error }}"))
    (OUT / "parse.json").write_text(json.dumps(cases, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")

    outputs = {
        "success": {"output": "hello\n", "returncode": 0, "exception_info": ""},
        "timeout": {
            "output": "partial\n",
            "returncode": -1,
            "exception_info": "An error occurred while executing the command: Command 'sleep 5' timed out after 1 seconds",
            "extra": {"exception_type": "TimeoutExpired", "exception": "Command 'sleep 5' timed out after 1 seconds"},
        },
        "long": {"output": "z" * 12000, "returncode": 0, "exception_info": ""},
    }
    template_vars = {"task": "t", "system": "Linux", "n_model_calls": 2, "model_cost": 1.0, "cwd": ""}
    observations = []
    with patch("minisweagent.models.utils.actions_text.time.time", return_value=TIMESTAMP):
        for template_name, template in [
            ("shipped", config["model"]["observation_template"]),
            ("default", LitellmTextbasedModelConfig.model_fields["observation_template"].default),
        ]:
            for name, output in outputs.items():
                messages = format_observation_messages([output], observation_template=template, template_vars=template_vars)
                observations.append(
                    {"name": f"{template_name}:{name}", "template": template, "template_vars": template_vars,
                     "outputs": [output], "timestamp": TIMESTAMP, "messages": messages}
                )
            messages = format_observation_messages(
                list(outputs.values()), observation_template=template, template_vars=template_vars
            )
            observations.append(
                {"name": f"{template_name}:all", "template": template, "template_vars": template_vars,
                 "outputs": list(outputs.values()), "timestamp": TIMESTAMP, "messages": messages}
            )
    (OUT / "observations.json").write_text(json.dumps(observations, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{len(cases)} parse cases, {len(observations)} observation cases -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
