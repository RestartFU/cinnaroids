#!/usr/bin/env python3
"""Package a native launcher with the notices required for redistribution."""

import hashlib
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tarfile
import zipfile


def package(root: Path, version: str, platform: str, arch: str, target: str) -> Path:
    dist = root / "dist"
    dist.mkdir(exist_ok=True)
    staging = dist / f"Cinnaroids-{version}-{platform}-{arch}"
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir()
    executable = root / "target" / target / "release" / ("cinnaroids.exe" if platform == "windows" else "cinnaroids")
    destination = staging / ("Cinnaroids.exe" if platform == "windows" else "cinnaroids")
    if platform == "macos":
        contents = staging / "Cinnaroids.app" / "Contents"
        (contents / "MacOS").mkdir(parents=True)
        destination = contents / "MacOS" / "cinnaroids"
        with (contents / "Info.plist").open("wb") as output:
            plistlib.dump({
                "CFBundleExecutable": "cinnaroids",
                "CFBundleIdentifier": "com.restartfu.cinnaroids",
                "CFBundleName": "Cinnaroids",
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": version,
                "CFBundleVersion": version,
                "NSHighResolutionCapable": True,
                "LSMinimumSystemVersion": "11.0",
            }, output)
    shutil.copy2(executable, destination)
    if platform != "windows":
        destination.chmod(0o755)
    if platform == "macos":
        subprocess.run(["codesign", "--force", "--sign", "-", str(staging / "Cinnaroids.app")], check=True)
        subprocess.run(["codesign", "--verify", "--strict", str(staging / "Cinnaroids.app")], check=True)
    for name in ["LICENSE.txt", "THIRD_PARTY_NOTICES.txt", "README.md"]:
        shutil.copy2(root / name, staging / name)
    shutil.copytree(root / "licenses", staging / "licenses")
    # The WASM component and UI assets are embedded in the executable.
    if platform == "windows":
        archive = dist / (staging.name + ".exe")
        shutil.move(destination, archive)
        notices = dist / (staging.name + "-notices.zip")
        with zipfile.ZipFile(notices, "w", zipfile.ZIP_DEFLATED) as output:
            for path in sorted(staging.rglob("*")):
                output.write(path, path.relative_to(dist))
        checksum(notices)
    elif platform == "linux":
        archive = dist / (staging.name + ".tar.gz")
        with tarfile.open(archive, "w:gz") as output:
            output.add(staging, arcname=staging.name)
    else:
        archive = dist / (staging.name + ".zip")
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for path in sorted(staging.rglob("*")):
                output.write(path, path.relative_to(dist))
    checksum(archive)
    shutil.rmtree(staging)
    return archive


def checksum(artifact: Path) -> None:
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    artifact.with_name(artifact.name + ".sha256").write_text(f"{digest}  {artifact.name}\n")


if __name__ == "__main__":
    print(package(
        Path(__file__).resolve().parents[1],
        os.environ["RELEASE_VERSION"], os.environ["RELEASE_PLATFORM"],
        os.environ["RELEASE_ARCH"], os.environ["RELEASE_TARGET"],
    ))
