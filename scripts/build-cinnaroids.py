#!/usr/bin/env python3
"""Build and validate Cinnaroids as one importable WASM component."""

import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile

from importlib.util import module_from_spec, spec_from_file_location

ROOT = Path(__file__).resolve().parents[1]


def build() -> None:
    """Compile the guest, embed import metadata and licenses, then checksum the component."""
    subprocess.run([sys.executable, str(ROOT / "scripts/generate-panel.py"), "--check"],
                   cwd=ROOT, check=True)
    spec = spec_from_file_location("packaging", ROOT / "scripts/package-release.py")
    packaging = module_from_spec(spec)
    spec.loader.exec_module(packaging)
    # Developers may opt into their own Cargo limiter; public builds need only Cargo.
    wrapper = shlex.split(os.environ.get("CINNABAR_CARGO_WRAPPER", ""))
    cargo = [*wrapper, "cargo"]
    target = ROOT / "target"
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target))
    subprocess.run([*cargo, "build", "--locked", "--release", "-p", "cinnaroids-mod",
                    "--target", "wasm32-unknown-unknown"], cwd=ROOT, env=environment, check=True)
    output = ROOT / "dist/cinnaroids.wasm"
    output.parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="cinnaroids-pack-") as temporary:
        licenses = Path(temporary) / "notices.txt"
        licenses.write_bytes(packaging.notices(ROOT))
        subprocess.run([*cargo, "run", "--locked", "-p", "cinnaroids-pack", "--",
                        str(target / "wasm32-unknown-unknown/release/cinnaroids_mod.wasm"),
                        str(output), str(licenses)], cwd=ROOT, env=environment, check=True)
    packaging.package(ROOT)
    print(f"Built {output}")


if __name__ == "__main__":
    build()
