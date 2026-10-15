#!/usr/bin/env python3
"""Regenerates the "Rust crates" list in THIRD-PARTY-NOTICES.md from `cargo metadata`.

Usage: python3 scripts/gen-notices.py   (rewrites the section between the two markers)
"""
import collections
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
START = "<!-- crates:start -->"
END = "<!-- crates:end -->"

meta = json.loads(
    subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=ROOT, text=True)
)
local = {p["id"] for p in meta["packages"] if p["source"] is None}
groups = collections.defaultdict(list)
for p in meta["packages"]:
    if p["id"] in local:
        continue
    lic = (p.get("license") or "see crate").replace("/", " OR ")
    groups[lic].append(f'{p["name"]} {p["version"]}')

lines = []
for lic, names in sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0])):
    lines.append(f"- **{lic}** ({len(names)}): " + ", ".join(sorted(set(names))))
block = f"{START}\n" + "\n".join(lines) + f"\n{END}"

path = ROOT / "THIRD-PARTY-NOTICES.md"
text = path.read_text()
if START in text:
    head, rest = text.split(START, 1)
    _, tail = rest.split(END, 1)
    text = head + block + tail
else:
    raise SystemExit("markers missing in THIRD-PARTY-NOTICES.md")
path.write_text(text)
print(f"{sum(len(v) for v in groups.values())} crates in {len(groups)} licence groups")
