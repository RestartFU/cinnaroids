"""Exercise the installer with local release fixtures and no system installation."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile


INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"


@unittest.skipUnless(os.name == "posix", "The installer runs on Linux and macOS")
class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.home = self.root / "home with spaces"
        self.home.mkdir()
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.commands = self.root / "commands"
        self.commands.mkdir()
        self.environment = dict(os.environ, HOME=str(self.home), XDG_DATA_HOME=str(self.home / "data"),
            TMPDIR=str(self.root), PATH=str(self.commands), TEST_ASSETS=str(self.assets),
            TEST_SYSTEM="Linux", TEST_ARCH="x86_64", TEST_ROSETTA="0")
        for command in ["sh", "grep", "awk", "mktemp", "mkdir", "cp", "mv", "chmod", "rm", "tar", "gzip", "unzip"]:
            executable = shutil.which(command)
            if not executable:
                self.skipTest(f"Missing fixture command: {command}")
            (self.commands / command).symlink_to(executable)
        self.write_command("uname", '#!/bin/sh\ncase "$1" in -s) printf "%s\\n" "$TEST_SYSTEM";; -m) printf "%s\\n" "$TEST_ARCH";; esac\n')
        self.write_command("sysctl", '#!/bin/sh\nprintf "%s\\n" "$TEST_ROSETTA"\n')
        self.write_command("curl", f'''#!{sys.executable}
import os, pathlib, shutil, sys
arguments = sys.argv[1:]
if '--write-out' in arguments:
    print('https://github.com/RestartFU/cinnaroids/releases/tag/v2.6.2', end='')
else:
    name = arguments[-1].rsplit('/', 1)[-1]
    shutil.copy2(pathlib.Path(os.environ['TEST_ASSETS']) / name, arguments[arguments.index('--output') + 1])
''')
        self.write_command("sha256sum", f'''#!{sys.executable}
import hashlib, pathlib, sys
print(hashlib.sha256(pathlib.Path(sys.argv[-1]).read_bytes()).hexdigest(), sys.argv[-1])
''')
        self.write_command("ditto", '#!/bin/sh\ncp -R "$1" "$2"\n')
        self.write_command("codesign", '#!/bin/sh\nexit "${TEST_CODESIGN_STATUS:-0}"\n')

    def write_command(self, name, contents):
        path = self.commands / name
        path.write_text(contents)
        path.chmod(0o755)

    def archive(self, platform="linux", arch="x86_64", version="2.6.2"):
        payload = self.root / f"Cinnaroids-{version}-{platform}-{arch}"
        payload.mkdir()
        for notice in ["LICENSE.txt", "THIRD_PARTY_NOTICES.txt", "README.md"]:
            (payload / notice).write_text(notice)
        (payload / "licenses").mkdir()
        (payload / "licenses/example.txt").write_text("License")
        binary = payload / ("cinnaroids" if platform == "linux" else "Cinnaroids.app/Contents/MacOS/cinnaroids")
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text("#!/bin/sh\nprintf 'installed\\n'\n")
        binary.chmod(0o755)
        if platform == "linux":
            archive = self.assets / (payload.name + ".tar.gz")
            with tarfile.open(archive, "w:gz") as output:
                output.add(payload, arcname=payload.name)
        else:
            archive = self.assets / (payload.name + ".zip")
            with zipfile.ZipFile(archive, "w") as output:
                for path in payload.rglob("*"):
                    output.write(path, path.relative_to(self.root))
        archive.with_name(archive.name + ".sha256").write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n")
        return archive

    def run_installer(self, *arguments):
        return subprocess.run(["/bin/sh", str(INSTALLER), *arguments], env=self.environment,
            text=True, capture_output=True)

    def test_linux_latest_installs_executable_and_notices_without_touching_preferences(self):
        self.archive()
        preferences = self.home / "data/Cinnaroids/settings.json"
        preferences.parent.mkdir(parents=True)
        preferences.write_text('{"dark_mode": false}')
        for _ in range(2):
            result = self.run_installer()
            self.assertEqual(result.returncode, 0, result.stderr)
        executable = self.home / ".local/bin/cinnaroids"
        self.assertTrue(os.access(executable, os.X_OK))
        self.assertTrue((self.home / "data/Cinnaroids/launcher/licenses/example.txt").exists())
        self.assertEqual(preferences.read_text(), '{"dark_mode": false}')
        self.assertIn("Cinnaroids 2.6.2 installed", result.stdout)

    def test_corrupt_archive_is_rejected_before_replacing_existing_installation(self):
        archive = self.archive()
        archive.write_bytes(archive.read_bytes() + b"corrupt")
        executable = self.home / ".local/bin/cinnaroids"
        executable.parent.mkdir(parents=True)
        executable.write_text("existing installation")
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Checksum mismatch", result.stderr)
        self.assertEqual(executable.read_text(), "existing installation")

    def test_explicit_version_and_shasum_fallback(self):
        self.archive(version="2.6.1")
        (self.commands / "sha256sum").rename(self.commands / "shasum")
        result = self.run_installer("--version", "2.6.1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Cinnaroids 2.6.1 installed", result.stdout)

    def test_macos_architectures_and_rosetta_replace_entire_app(self):
        for arch in ["arm64", "x86_64"]:
            self.archive(platform="macos", arch=arch)
        for arch, rosetta, expected in [("arm64", "0", "arm64"), ("x86_64", "0", "x86_64"), ("x86_64", "1", "arm64")]:
            self.environment.update(TEST_SYSTEM="Darwin", TEST_ARCH=arch, TEST_ROSETTA=rosetta)
            old_file = self.home / "Applications/Cinnaroids.app/stale.txt"
            old_file.parent.mkdir(parents=True, exist_ok=True)
            old_file.write_text("Old bundle file")
            result = self.run_installer()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(f"macos {expected}", result.stdout)
            self.assertFalse(old_file.exists())
            self.assertTrue((old_file.parent / "Contents/MacOS/cinnaroids").exists())

    def test_invalid_macos_signature_preserves_existing_app(self):
        self.archive(platform="macos", arch="arm64")
        self.environment.update(TEST_SYSTEM="Darwin", TEST_ARCH="arm64", TEST_CODESIGN_STATUS="1")
        old = self.home / "Applications/Cinnaroids.app/existing.txt"
        old.parent.mkdir(parents=True)
        old.write_text("Existing app")
        self.assertNotEqual(self.run_installer().returncode, 0)
        self.assertEqual(old.read_text(), "Existing app")

    def test_unsupported_architecture_and_invalid_version_fail(self):
        self.environment["TEST_ARCH"] = "aarch64"
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported platform", result.stderr)
        self.environment["TEST_ARCH"] = "x86_64"
        for invalid in ["../../invalid", ""]:
            result = self.run_installer("--version", invalid)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Release version must look", result.stderr)


if __name__ == "__main__":
    unittest.main()
