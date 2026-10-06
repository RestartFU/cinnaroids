#!/usr/bin/env python3
"""Read or bump the launcher and module release version without updating dependencies."""

import argparse
from pathlib import Path
import re
import sys
import tomllib


SEMVER = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


def replace_version(text: str, current: str, target: str) -> str:
    updated, count = re.subn(
        r'(?m)^(version\s*=\s*")' + re.escape(current) + r'("\s*)$',
        lambda match: match[1] + target + match[2], text,
    )
    if count != 1:
        raise ValueError("expected exactly one package version assignment")
    return updated


def bump(root: Path, kind: str, dry_run: bool = False) -> str:
    current = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    match = SEMVER.fullmatch(current)
    if not match:
        raise ValueError("package.version must look like 1.2.3")
    parts = list(map(int, match.groups()))
    if kind != "current":
        index = {"major": 0, "minor": 1, "patch": 2}[kind]
        parts[index] += 1
        parts[index + 1:] = [0] * (2 - index)
    target = ".".join(map(str, parts))
    if any(part > 65535 for part in parts):
        raise ValueError("Windows resource version components must fit in 16 bits")
    updates = {}
    for folder, name in [(Path("."), "cinnaroids"), (Path("mods/cinnaroids"), "cinnaroids-mod")]:
        manifest = root / folder / "Cargo.toml"
        text = manifest.read_text()
        package = tomllib.loads(text)["package"]
        if package["name"] != name or package["version"] != current:
            raise ValueError(f"{manifest} is out of sync with the launcher version")
        tables = re.split(r"(?m)(?=^\[)", text)
        for index, table in enumerate(tables):
            if table.startswith("[package]\n"):
                tables[index] = replace_version(table, current, target)
        updates[manifest] = "".join(tables)
        lock = root / folder / "Cargo.lock"
        blocks = re.split(r"(?m)(?=^\[\[package\]\]\s*$)", lock.read_text())
        found = 0
        for index, block in enumerate(blocks):
            if not block.startswith("[[package]]"):
                continue
            package = tomllib.loads(block)["package"][0]
            if package["name"] == name and "source" not in package:
                if package["version"] != current:
                    raise ValueError(f"{lock} version mismatch for {name}")
                blocks[index] = replace_version(block, current, target)
                found += 1
        if found != 1:
            raise ValueError(f"{lock} must contain one local {name} package")
        updates[lock] = "".join(blocks)
    resource = root / "app.rc"
    text = resource.read_text()
    for field in ["FILEVERSION", "PRODUCTVERSION"]:
        text, count = re.subn(r"(?m)^" + field + r" \d+,\d+,\d+,0$", field + " " + target.replace(".", ",") + ",0", text)
        if count != 1:
            raise ValueError(f"app.rc must contain one {field}")
    for field in ["FileVersion", "ProductVersion"]:
        text, count = re.subn(
            r'(VALUE "' + field + r'", ")\d+\.\d+\.\d+(\\0")',
            lambda match: match[1] + target + match[2], text,
        )
        if count != 1:
            raise ValueError(f"app.rc must contain one {field}")
    if kind == "current" and text != resource.read_text():
        raise ValueError("app.rc version is out of sync with Cargo.toml")
    updates[resource] = text
    # Validate all files before changing any of them.
    if kind != "current" and not dry_run:
        for path, text in updates.items():
            path.write_text(text)
    return target


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=["current", "patch", "minor", "major"])
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        print(bump(args.root, args.kind, args.dry_run))
        return 0
    except (OSError, ValueError, KeyError) as error:
        print(f"release version: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
