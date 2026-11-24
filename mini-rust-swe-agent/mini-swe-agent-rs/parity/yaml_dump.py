"""Dump YAML files as JSON with PyYAML's safe_load, the loader the Python reference uses.

Run from mini-swe-agent-rs/ with the reference environment:

    uv run --frozen --project ../mini-swe-agent python parity/yaml_dump.py

It writes tests/goldens/yaml/<name>.json for every file in config/ and parity/yaml_cases/.
The Rust tests in tests/yaml.rs compare the Rust loader with these goldens.
"""

import json
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
SOURCES = [ROOT / "config", ROOT / "parity" / "yaml_cases"]
OUT = ROOT / "tests" / "goldens" / "yaml"


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    for source in SOURCES:
        for path in sorted(source.glob("*.yaml")):
            data = yaml.safe_load(path.read_text(encoding="utf-8"))
            target = OUT / f"{path.stem}.json"
            target.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
            print(f"{path.relative_to(ROOT)} -> {target.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
