#!/usr/bin/env python3
"""Package the validated single-file component and its SHA-256 checksum."""

import hashlib
from pathlib import Path
import tomllib


def notices(root: Path) -> bytes:
    """Collect unchanged licenses for every locked dependency into embedded notices."""
    texts = [(root / name).read_text() for name in ["LICENSE.txt", "THIRD_PARTY_NOTICES.txt"]]
    for package in tomllib.loads((root / "Cargo.lock").read_text())["package"]:
        if "source" not in package:
            continue
        directory = root / "licenses" / f"{package['name']}-{package['version']}"
        files = sorted(path for path in directory.rglob("*") if path.is_file())
        if not files:
            raise ValueError(f"missing dependency licenses: {directory.name}")
        for path in files:
            texts.append(f"\n--- {path.relative_to(root)} ---\n" + path.read_text())
    return "\n\n".join(texts).encode("utf-8")


def package(root: Path) -> Path:
    """Require a built component and checksum its exact release bytes."""
    artifact = root / "dist" / "cinnaroids.wasm"
    # Build writes directly to dist. Keep the name stable so updates preserve settings.
    if artifact.read_bytes()[:8] != b"\0asm\x0d\0\x01\0":
        raise ValueError("release artifact must be a WebAssembly component")
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    artifact.with_name(artifact.name + ".sha256").write_text(f"{digest}  {artifact.name}\n")
    return artifact


if __name__ == "__main__":
    print(package(Path(__file__).resolve().parents[1]))
