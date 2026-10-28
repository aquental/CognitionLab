#!/usr/bin/env python3
"""Writes tests/features/<tag>.feature from the Gherkin blocks of spec/PRD.md.

Only scenarios and Examples blocks whose effective milestone tag (@mN) is at most the
milestone in config/acceptance.toml are kept, so each phase adds exactly its own
scenarios. Kept lines are copied verbatim; nothing is re-typed by hand.
Usage: scripts/sync-features.py            (run from the crate directory)
"""
import re
import pathlib

root = pathlib.Path(__file__).resolve().parent.parent
milestone = int(re.search(r"milestone\s*=\s*(\d+)", (root / "config/acceptance.toml").read_text()).group(1))
prd = (root / "spec/PRD.md").read_text()
blocks = re.findall(r"```gherkin\n(.*?)```", prd, re.S)
out_dir = root / "tests/features"


def tag_milestone(tags):
    found = [int(m) for m in re.findall(r"@m(\d+)", tags)]
    return found[0] if found else None


for block in blocks:
    lines = block.split("\n")
    feature_tags = lines[0] if lines[0].startswith("@") else ""
    feature_m = tag_milestone(feature_tags)
    name = re.search(r"@(us\d+|rel\d+)", feature_tags).group(1)
    # Split into header (until first scenario-level item) and scenario chunks.
    out, i, keep_any = [], 0, False
    pending_tags = []
    header_done = False
    scenario_m = None
    keep_scenario = True
    examples_keep = True
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if stripped.startswith("@"):
            pending_tags.append(line)
            i += 1
            continue
        if stripped.startswith(("Scenario:", "Scenario Outline:")):
            header_done = True
            m = tag_milestone(" ".join(pending_tags))
            scenario_m = m if m is not None else feature_m
            # An outline without its own tag is kept if any Examples block is kept.
            if stripped.startswith("Scenario Outline:") and m is None:
                rest = "\n".join(lines[i:])
                nxt = re.search(r"\n  (?:@[^\n]*\n  )?Scenario", rest)
                body = rest[: nxt.start()] if nxt else rest
                ex_ms = [int(x) for x in re.findall(r"@m(\d+)\s*\n\s*Examples", body)]
                keep_scenario = any(x <= milestone for x in ex_ms) or (not ex_ms and (feature_m or 99) <= milestone)
            else:
                keep_scenario = scenario_m is not None and scenario_m <= milestone
            if keep_scenario:
                keep_any = True
                out.extend(pending_tags)
                out.append(line)
            pending_tags = []
            examples_keep = True
            i += 1
            continue
        if stripped.startswith("Examples"):
            m = tag_milestone(" ".join(pending_tags))
            ex_m = m if m is not None else scenario_m
            examples_keep = keep_scenario and ex_m is not None and ex_m <= milestone
            if examples_keep:
                out.extend(pending_tags)
                out.append(line)
            pending_tags = []
            i += 1
            continue
        if pending_tags and not header_done:
            out.extend(pending_tags)
            pending_tags = []
        if not header_done:
            out.append(line)
        elif keep_scenario and examples_keep:
            out.append(line)
        elif keep_scenario and stripped == "":
            out.append(line)
        i += 1
    target = out_dir / f"{name}.feature"
    if keep_any:
        text = re.sub(r"\n{3,}", "\n\n", "\n".join(out)).rstrip() + "\n"
        target.write_text(text)
        print(f"wrote {target.name}")
    elif target.exists():
        target.unlink()
