"""Release integration checks run against disposable local Git repositories."""

import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch


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
        for name in ["Cargo.toml", "Cargo.lock", "mods/cinnaroids/Cargo.toml", "mods/cinnaroids/pack/Cargo.toml", "scripts/bump-version.py"]:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, destination)
        # Keep a fixed fixture version as the repository's release version advances.
        source_version = tomllib.loads((self.root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        manifest = self.root / "Cargo.toml"
        manifest.write_text(version.replace_version(manifest.read_text(), source_version, "2.6.0"))
        lock = self.root / "Cargo.lock"
        blocks = re.split(r"(?m)(?=^\[\[package\]\]\s*$)", lock.read_text())
        for index, block in enumerate(blocks):
            if block.startswith("[[package]]"):
                package = tomllib.loads(block)["package"][0]
                if package["name"] in {"cinnaroids-mod", "cinnaroids-pack"} and "source" not in package:
                    blocks[index] = version.replace_version(block, source_version, "2.6.0")
        lock.write_text("".join(blocks))
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

    def test_bump_levels_keep_workspace_and_lock_in_sync(self):
        self.assertEqual(version.bump(self.root, "patch", True), "2.6.1")
        self.assertEqual(version.bump(self.root, "minor", True), "2.7.0")
        self.assertEqual(version.bump(self.root, "major", True), "3.0.0")
        self.assertEqual(version.bump(self.root, "current"), "2.6.0")
        version.bump(self.root, "minor")
        self.assertEqual(version.bump(self.root, "current"), "2.7.0")
        packages = tomllib.loads((self.root / "Cargo.lock").read_text())["package"]
        self.assertEqual([p["version"] for p in packages if p["name"].startswith("cinnaroids-")], ["2.7.0", "2.7.0"])

    def test_mismatched_module_prevents_partial_version_updates(self):
        path = self.root / "Cargo.lock"
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

    def test_release_is_a_component_with_valid_checksum(self):
        import hashlib
        artifact = self.root / "dist/cinnaroids.wasm"
        artifact.parent.mkdir()
        artifact.write_bytes(b"\0asm\x0d\0\x01\0")
        self.assertEqual(packaging.package(self.root), artifact)
        self.assertEqual(artifact.with_suffix(".wasm.sha256").read_text(),
                         f"{hashlib.sha256(artifact.read_bytes()).hexdigest()}  cinnaroids.wasm\n")
        self.assertEqual(sorted(p.name for p in artifact.parent.iterdir()),
                         ["cinnaroids.wasm", "cinnaroids.wasm.sha256"])

    def test_core_module_cannot_be_released_as_a_component(self):
        artifact = self.root / "dist/cinnaroids.wasm"
        artifact.parent.mkdir()
        artifact.write_bytes(b"\0asm\x01\0\0\0")
        with self.assertRaisesRegex(ValueError, "WebAssembly component"):
            packaging.package(self.root)
        self.assertFalse(artifact.with_suffix(".wasm.sha256").exists())

    def test_notices_include_every_locked_dependency_license(self):
        for name in ["LICENSE.txt", "THIRD_PARTY_NOTICES.txt"]:
            (self.root / name).write_text(name)
        font_license = self.root / "assets/fonts/Inter-OFL-1.1.txt"
        font_license.parent.mkdir(parents=True)
        font_license.write_bytes((ROOT / "assets/fonts/Inter-OFL-1.1.txt").read_bytes())
        (self.root / "Cargo.lock").write_text(
            'version = 4\n[[package]]\nname = "example"\nversion = "1.0.0"\nsource = "registry"\n')
        with self.assertRaisesRegex(ValueError, "missing dependency licenses"):
            packaging.notices(self.root)
        directory = self.root / "licenses/example-1.0.0"
        directory.mkdir(parents=True)
        (directory / "LICENSE").write_text("Example copyright and license")
        notices = packaging.notices(self.root).decode()
        self.assertIn("Example copyright and license", notices)
        self.assertIn("LICENSE.txt", notices)
        self.assertIn(font_license.read_text(), notices)


if __name__ == "__main__":
    unittest.main()
