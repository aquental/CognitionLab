"""Render template goldens with Jinja2, the engine the Python reference uses.

Run from mini-swe-agent-rs/ with the reference environment (importing minisweagent creates the
global config dir and loads its .env, so point it at a temporary one):

    MSWEA_SILENT_STARTUP=1 MSWEA_GLOBAL_CONFIG_DIR=$(mktemp -d) \
      uv run --frozen --project ../mini-swe-agent python parity/template_goldens.py

It writes tests/goldens/templates/cases.json: a list of {name, template, context, expected}.
Each case renders `template` with `Template(template, undefined=StrictUndefined)` (what
`default.py:66-67` and `actions_text.py` do). Cases named `config:<file>:<section>.<key>` take
their template from config/<file>.yaml; the others hold Python class defaults or formatting probes.
`tests/templates.rs` renders the same template and context in Rust and compares the bytes.
"""

import json
import sys
from pathlib import Path

import yaml
from jinja2 import StrictUndefined, Template

from minisweagent.models.litellm_model import LitellmModelConfig
from minisweagent.models.litellm_textbased_model import LitellmTextbasedModelConfig
from minisweagent.models.test_models import DeterministicModelConfig

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "goldens" / "templates" / "cases.json"

TASK = "Fix the off-by-one error in parser.py — ünïcode ✓"
UNAME = {
    "Darwin": {"system": "Darwin", "release": "25.0.0", "version": "Darwin Kernel Version 25.0.0", "machine": "arm64"},
    "Linux": {"system": "Linux", "release": "6.8.0", "version": "#1 SMP PREEMPT_DYNAMIC", "machine": "x86_64"},
}


def outputs():
    head = "".join(f"head line {i}\n" for i in range(600))
    tail = "".join(f"tail line {i}\n" for i in range(600))
    padding = 20000 - len(head) - len(tail)
    assert padding > 0, padding
    long_ascii = head + "x" * padding + tail
    # Over 10,000 code points, with different multibyte text in the first and last 5,000.
    long_multibyte = "é" * 3000 + "日本" * 1500 + "x" * 1000 + "🦀" * 3000 + "ü" * 2001
    assert len(long_multibyte) > 10000 and long_multibyte[:5000] != long_multibyte[-5000:]
    under_limit_multibyte = "é" * 4000 + "日本" * 1000 + "🦀" * 1000 + "z" * 1001
    assert len(under_limit_multibyte) < 10000 < len(under_limit_multibyte.encode())
    return {
        "short": {"output": "hello world\n", "returncode": 0, "exception_info": ""},
        "empty": {"output": "", "returncode": 0, "exception_info": ""},
        "error": {
            "output": "partial\n",
            "returncode": -1,
            "exception_info": "An error occurred while executing the command: Command 'sleep 5' timed out after 1 seconds",
        },
        "failing": {"output": "ls: cannot access 'x': No such file or directory\n", "returncode": 2, "exception_info": ""},
        "len_9999": {"output": "a" * 9999, "returncode": 0, "exception_info": ""},
        "len_10000": {"output": "b" * 10000, "returncode": 0, "exception_info": ""},
        "long_ascii": {"output": long_ascii, "returncode": 0, "exception_info": ""},
        # 8,001 code points but over 10,000 bytes: `length` must count code points.
        "under_limit_multibyte": {"output": under_limit_multibyte, "returncode": 1, "exception_info": ""},
        # 12,001 code points: `[:5000]` and `[-5000:]` must slice code points.
        "long_multibyte": {"output": long_multibyte, "returncode": 1, "exception_info": ""},
    }


def format_error_contexts():
    return {
        "zero": {"actions": [], "error": "Expected exactly 1 action, found 0."},
        "two": {"actions": ["ls", "pwd"], "error": "Expected exactly 1 action, found 2."},
        "stop": {"actions": [], "error": "Expected exactly 1 action, found 0.", "finish_reason": "stop"},
        "length": {"actions": [], "error": "Expected exactly 1 action, found 0.", "finish_reason": "length"},
        "tool_calls": {"actions": [], "error": "Expected exactly 1 action, found 0.", "finish_reason": "tool_calls"},
        # `response.choices[0].finish_reason` can be None.
        "none": {"actions": [], "error": "Expected exactly 1 action, found 0.", "finish_reason": None},
    }


def cases():
    result = []
    for file in ["default", "mini_textbased"]:
        config = yaml.safe_load((ROOT / "config" / f"{file}.yaml").read_text(encoding="utf-8"))
        agent, model = config["agent"], config["model"]
        result.append((f"config:{file}:agent.system_template", agent["system_template"], {"task": TASK}))
        for system, uname in UNAME.items():
            result.append((f"config:{file}:agent.instance_template:{system}", agent["instance_template"], {"task": TASK, **uname}))
        for name, output in outputs().items():
            result.append((f"config:{file}:model.observation_template:{name}", model["observation_template"], {"output": output}))
        for name, ctx in format_error_contexts().items():
            result.append((f"config:{file}:model.format_error_template:{name}", model["format_error_template"], ctx))
    defaults = {
        "litellm.observation_template": LitellmModelConfig.model_fields["observation_template"].default,
        "litellm.format_error_template": LitellmModelConfig.model_fields["format_error_template"].default,
        "textbased.format_error_template": LitellmTextbasedModelConfig.model_fields["format_error_template"].default,
        "deterministic.observation_template": DeterministicModelConfig.model_fields["observation_template"].default,
    }
    for name, template in defaults.items():
        if "observation" in name:
            for oname in ["short", "error", "empty"]:
                result.append((f"default:{name}:{oname}", template, {"output": outputs()[oname]}))
        else:
            for ename in ["zero", "two"]:
                result.append((f"default:{name}:{ename}", template, format_error_contexts()[ename]))
    probe = "{{ n }}|{{ t }}|{{ f }}|{{ i }}|{{ big }}|{{ neg }}|" + "|".join("{{ x%d }}" % i for i in range(8))
    floats = [1.0, 0.1, 1e20, 1e-05, 123456.789, -0.0, 1e16, 2.5e-4]
    probe_ctx = {"n": None, "t": True, "f": False, "i": 42, "big": 12345678901234567890, "neg": -7}
    probe_ctx.update({f"x{i}": v for i, v in enumerate(floats)})
    result.append(("probe:value_formatting", probe, probe_ctx))
    # The agent's counters as `get_template_vars` passes them (test_default.py:286-307).
    result.append(("probe:agent_stats", "Calls: {{n_model_calls}}, Cost: {{model_cost}}", {"n_model_calls": 2, "model_cost": 1.0}))
    result.append(("probe:float_boundaries", "{{ a }}|{{ b }}|{{ c }}|{{ d }}", {"a": 0.0, "b": 1e-4, "c": 9.9999e-5, "d": -1e-5}))
    result.append(("probe:whitespace", "a\n{%- if t -%}\n  b  \n{%- endif -%}\nc\n", {"t": True}))
    return result


def main() -> int:
    rendered = []
    for name, template, context in cases():
        expected = Template(template, undefined=StrictUndefined).render(**context)
        rendered.append({"name": name, "template": template, "context": context, "expected": expected})
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(rendered, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{len(rendered)} cases -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
