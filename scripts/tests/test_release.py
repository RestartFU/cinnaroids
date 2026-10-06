"""Release integration checks run against disposable local Git repositories."""

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import zipfile


ROOT = Path(__file__).resolve().parents[2]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


version = load("bump-version")
prepare = load("prepare-release")
packaging = load("package-release")


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "checkout"
        self.root.mkdir()
        for name in ["Cargo.toml", "Cargo.lock", "mods/cinnaroids/Cargo.toml", "mods/cinnaroids/Cargo.lock", "mods/cinnaroids/pack/Cargo.toml", "app.rc", "scripts/bump-version.py"]:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, destination)
        self.remote = Path(self.temporary.name) / "remote.git"
        self.git("init", "--initial-branch=main")
        self.git("config", "user.name", "Release test")
        self.git("config", "user.email", "test@example.com")
        self.git("add", ".")
        self.git("commit", "-m", "Initial source")
        subprocess.run(["git", "init", "--bare", str(self.remote)], check=True, capture_output=True)
        self.git("remote", "add", "origin", str(self.remote))
        self.git("push", "origin", "main")
        self.environ = patch.dict(os.environ, {
            "GITHUB_REPOSITORY": "test/cinnaroids", "EVENT_NAME": "workflow_dispatch",
            "DEFAULT_BRANCH": "main", "BUMP": "patch", "GITHUB_SHA": "",
        })
        self.environ.start()
        self.addCleanup(self.environ.stop)
        self.root_patch = patch.object(prepare, "ROOT", self.root)
        self.root_patch.start()
        self.addCleanup(self.root_patch.stop)

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True, text=True).stdout.strip()

    def test_bump_levels_keep_module_locks_and_resources_in_sync(self):
        self.assertEqual(version.bump(self.root, "patch", True), "2.6.1")
        self.assertEqual(version.bump(self.root, "minor", True), "2.7.0")
        self.assertEqual(version.bump(self.root, "major", True), "3.0.0")
        self.assertEqual(version.bump(self.root, "current"), "2.6.0")
        version.bump(self.root, "minor")
        self.assertEqual(version.bump(self.root, "current"), "2.7.0")
        self.assertIn("FILEVERSION 2,7,0,0", (self.root / "app.rc").read_text())
        self.assertEqual(tomllib.loads((self.root / "mods/cinnaroids/pack/Cargo.toml").read_text())["package"]["version"], "2.0.0")

    def test_mismatched_module_prevents_partial_version_updates(self):
        path = self.root / "mods/cinnaroids/Cargo.toml"
        path.write_text(path.read_text().replace('version = "2.6.0"', 'version = "2.5.0"'))
        before = (self.root / "Cargo.toml").read_bytes()
        with self.assertRaises(ValueError):
            version.bump(self.root, "patch")
        self.assertEqual((self.root / "Cargo.toml").read_bytes(), before)

    def test_dispatch_pushes_matching_commit_and_recovers_without_second_bump(self):
        with patch.object(prepare, "release_exists", return_value=False):
            result = prepare.prepare()
            self.assertEqual(result["tag"], "v2.6.1")
            self.assertEqual(result["commit"], self.git("rev-parse", "HEAD"))
            self.assertEqual(result["commit"], self.git("rev-parse", "v2.6.1^{commit}"))
            self.assertIn(result["commit"], self.git("ls-remote", "origin", "refs/heads/main"))
            self.assertEqual(prepare.prepare(), result)
        with patch.object(prepare, "release_exists", side_effect=lambda repo, tag: tag == "v2.6.1"):
            self.assertEqual(prepare.prepare()["tag"], "v2.6.2")

    def test_current_is_not_a_manual_release_option(self):
        os.environ["BUMP"] = "current"
        with self.assertRaisesRegex(prepare.ReleaseError, "BUMP must be patch"):
            prepare.prepare()

    def test_tag_push_requires_matching_source_and_commit(self):
        self.git("tag", "v2.6.0")
        os.environ.update(EVENT_NAME="push", GITHUB_REF="refs/tags/v2.6.0")
        self.assertEqual(prepare.prepare()["version"], "2.6.0")
        self.git("tag", "v2.7.0")
        os.environ["GITHUB_REF"] = "refs/tags/v2.7.0"
        with self.assertRaisesRegex(prepare.ReleaseError, "does not match source"):
            prepare.prepare()

    def test_existing_tag_on_another_commit_is_rejected(self):
        self.git("tag", "v2.6.1")
        with patch.object(prepare, "release_exists", return_value=False):
            with self.assertRaisesRegex(prepare.ReleaseError, "already exists"):
                prepare.prepare()
        self.assertEqual(version.bump(self.root, "current"), "2.6.0")

    def test_api_failure_is_not_treated_as_missing_release(self):
        response = subprocess.CompletedProcess([], 1, "HTTP/2.0 403 Forbidden\n", "forbidden")
        with patch.object(prepare, "command", return_value=response):
            with self.assertRaisesRegex(prepare.ReleaseError, "cannot verify"):
                prepare.release_exists("test/repo", "v1.0.0")
        response.stdout = "HTTP/2.0 404 Not Found\n"
        with patch.object(prepare, "command", return_value=response):
            self.assertFalse(prepare.release_exists("test/repo", "v1.0.0"))

    def test_archives_include_executable_notices_and_valid_checksum(self):
        import hashlib
        for name in ["LICENSE.txt", "THIRD_PARTY_NOTICES.txt", "README.md"]:
            (self.root / name).write_text(name)
        (self.root / "licenses").mkdir()
        (self.root / "licenses/example.txt").write_text("Example license")
        for platform, target in [("linux", "x86_64-unknown-linux-gnu"), ("windows", "x86_64-pc-windows-msvc")]:
            binary = self.root / "target" / target / "release" / ("cinnaroids.exe" if platform == "windows" else "cinnaroids")
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"test executable")
            archive = packaging.package(self.root, "2.6.0", platform, "x86_64", target)
            checksum = archive.with_name(archive.name + ".sha256").read_text().split()[0]
            self.assertEqual(checksum, hashlib.sha256(archive.read_bytes()).hexdigest())
            if platform == "linux":
                with tarfile.open(archive) as bundle:
                    files = bundle.getnames()
                    executable = next(item for item in bundle.getmembers() if item.name.endswith("/cinnaroids"))
                    self.assertEqual(executable.mode & 0o777, 0o755)
            else:
                with zipfile.ZipFile(archive) as bundle:
                    files = bundle.namelist()
            for name in ["LICENSE.txt", "THIRD_PARTY_NOTICES.txt", "licenses/example.txt"]:
                self.assertTrue(any(path.endswith("/" + name) for path in files))

    def test_macos_bundle_has_matching_version_and_executable(self):
        import plistlib
        for name in ["LICENSE.txt", "THIRD_PARTY_NOTICES.txt", "README.md"]:
            (self.root / name).write_text(name)
        (self.root / "licenses").mkdir()
        binary = self.root / "target/aarch64-apple-darwin/release/cinnaroids"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"test executable")
        with patch.object(packaging.subprocess, "run") as signing:
            archive = packaging.package(self.root, "2.6.0", "macos", "arm64", "aarch64-apple-darwin")
            self.assertEqual(signing.call_count, 2)
        with zipfile.ZipFile(archive) as bundle:
            plist = next(path for path in bundle.namelist() if path.endswith("/Contents/Info.plist"))
            info = plistlib.loads(bundle.read(plist))
            self.assertEqual(info["CFBundleShortVersionString"], "2.6.0")
            self.assertEqual(info["CFBundleExecutable"], "cinnaroids")
            binary = next(path for path in bundle.namelist() if path.endswith("/Contents/MacOS/cinnaroids"))
            self.assertEqual(bundle.getinfo(binary).external_attr >> 16 & 0o777, 0o755)


if __name__ == "__main__":
    unittest.main()
