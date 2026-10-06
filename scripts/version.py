#!/usr/bin/env python3
"""Keep SDK versions in sync with the workspace version in Cargo.toml.

    scripts/version.py check        fail if package.json differs from Cargo.toml
    scripts/version.py set 0.2.0    bump Cargo.toml, package.json and lockfiles

Python's version is read from Cargo.toml by maturin (dynamic version).
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARGO = ROOT / "Cargo.toml"
NODE = ROOT / "crates/cask-sdk-node"
VERSION_RE = re.compile(r'^(version\s*=\s*")([^"]+)(")', re.M)
PIN_RE = re.compile(r'(cask-sdk-core\s*=\s*\{[^}]*version\s*=\s*"=)([^"]+)(")')


def workspace_version() -> str:
    section = CARGO.read_text().split("[workspace.package]", 1)[1]
    return VERSION_RE.search(section).group(2)


def node_version() -> str:
    return json.loads((NODE / "package.json").read_text())["version"]


def check() -> int:
    expected = workspace_version()
    mismatches = []
    found = node_version()
    if found != expected:
        mismatches.append(f"package.json is {found}, Cargo.toml is {expected}")
    for manifest in sorted(ROOT.glob("crates/*/Cargo.toml")):
        for match in PIN_RE.finditer(manifest.read_text()):
            if match.group(2) != expected:
                mismatches.append(
                    f"{manifest.relative_to(ROOT)} pins cask-sdk-core ={match.group(2)}, "
                    f"Cargo.toml is {expected}"
                )
    if mismatches:
        print("\n".join(mismatches), file=sys.stderr)
        return 1
    print(f"all versions are {expected}")
    return 0


def set_version(version: str) -> int:
    if not re.fullmatch(r"\d+\.\d+\.\d+([-+][0-9A-Za-z.-]+)?", version):
        print(f"invalid version: {version}", file=sys.stderr)
        return 2
    head, sep, tail = CARGO.read_text().partition("[workspace.package]")
    tail = VERSION_RE.sub(lambda m: m.group(1) + version + m.group(3), tail, count=1)
    CARGO.write_text(head + sep + tail)
    for manifest in ROOT.glob("crates/*/Cargo.toml"):
        text = manifest.read_text()
        updated = PIN_RE.sub(lambda m: m.group(1) + version + m.group(3), text)
        if updated != text:
            manifest.write_text(updated)
    subprocess.run(["cargo", "update", "--workspace", "--offline"], cwd=ROOT, check=True)
    subprocess.run(
        ["npm", "version", version, "--no-git-tag-version", "--allow-same-version"],
        cwd=NODE,
        check=True,
    )
    return check()


if __name__ == "__main__":
    args = sys.argv[1:]
    if args == ["check"]:
        sys.exit(check())
    if len(args) == 2 and args[0] == "set":
        sys.exit(set_version(args[1]))
    print(__doc__, file=sys.stderr)
    sys.exit(2)
