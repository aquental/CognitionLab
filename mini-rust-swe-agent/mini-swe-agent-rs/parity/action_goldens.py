"""Goldens for the text-action parser and observation messages (`models/utils/actions_text.py`).

Run from mini-swe-agent-rs/ with the reference environment:

    MSWEA_SILENT_STARTUP=1 MSWEA_GLOBAL_CONFIG_DIR=$(mktemp -d) \
      uv run --frozen --project ../mini-swe-agent python parity/action_goldens.py

It writes tests/goldens/actions/parse.json and observations.json. `time.time` returns fixed,
increasing values so the observation timestamps are reproducible and show one call per output.
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
ABSENT = object()

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
    # Python's `\s` includes \x1c-\x1f; Rust's does not.
    "python_space_on_fence_line": f"{FENCE}\x1c\nls\n```",
    "python_space_before_command": f"{FENCE}\n\x1fls\x1e\n```",
}


def parse_case(name, content, regex, template, finish_reason=ABSENT):
    # `None` is passed on as a value (a provider without a finish reason); ABSENT omits the key.
    kwargs = {} if finish_reason is ABSENT else {"finish_reason": finish_reason}
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
    cases = []
    for name, content in CONTENTS.items():
        cases.append(parse_case(name, content, DEFAULT_REGEX, DEFAULT_FORMAT_ERROR))
    for file in ["mini_textbased", "default"]:
        shipped = yaml.safe_load((ROOT / "config" / f"{file}.yaml").read_text(encoding="utf-8"))
        template = shipped["model"]["format_error_template"]
        for reason in [ABSENT, None, "stop", "length", "tool_calls"]:
            label = "absent" if reason is ABSENT else reason
            for name in ["zero_blocks", "two_blocks", "one_block"]:
                cases.append(parse_case(f"shipped:{file}:{name}:{label}", CONTENTS[name], DEFAULT_REGEX, template, reason))
    cases.append(parse_case("custom:optional_group_unmatched", "ACT:;", r"ACT:(\w+)?;", "{{ error }}"))
    cases.append(parse_case("custom:no_group", "run ls -la now", r"ls -la", "{{ error }}"))
    cases.append(parse_case("custom:backslash_upper_z", "x", r"(x)\Z", "{{ error }}"))
    cases.append(parse_case("custom:backslash_s_in_class", "a\x1db", r"a([\s]+)b", "{{ actions | length }}"))
    cases.append(parse_case("custom:backslash_upper_s", "a\x1db", r"a(\S)b", "{{ actions | length }}"))
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
        # An `extra` key that replaces a base key keeps the base key's position.
        "extra_override": {
            "output": "o\n", "returncode": 1, "exception_info": "", "extra": {"returncode": 7, "zz": True},
        },
    }
    template_vars = {"task": "t", "system": "Linux", "n_model_calls": 2, "model_cost": 1.0, "cwd": ""}
    observations = []
    clock = iter(TIMESTAMP + i for i in range(1000))
    with patch("minisweagent.models.utils.actions_text.time.time", side_effect=lambda: next(clock)):
        for template_name, template in [
            ("shipped", config["model"]["observation_template"]),
            ("default", LitellmTextbasedModelConfig.model_fields["observation_template"].default),
            ("vars", "{{ task }} {{ n_model_calls }} {{ output.returncode }}"),
        ]:
            for name, output in outputs.items():
                messages = format_observation_messages([output], observation_template=template, template_vars=template_vars)
                observations.append(
                    {"name": f"{template_name}:{name}", "template": template, "template_vars": template_vars,
                     "outputs": [output], "messages": messages}
                )
            messages = format_observation_messages(
                list(outputs.values()), observation_template=template, template_vars=template_vars
            )
            observations.append(
                {"name": f"{template_name}:all", "template": template, "template_vars": template_vars,
                 "outputs": list(outputs.values()), "messages": messages}
            )
    (OUT / "observations.json").write_text(json.dumps(observations, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{len(cases)} parse cases, {len(observations)} observation cases -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
