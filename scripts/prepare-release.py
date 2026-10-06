#!/usr/bin/env python3
"""Resolve the release workflow's exact source commit and optional release tag.

Environment: EVENT_NAME=push|workflow_dispatch, DEFAULT_BRANCH (main),
BUMP=patch|minor|major, optional RELEASE_TAG for a tag push, and the
standard GITHUB_REF, GITHUB_SHA, GITHUB_REPOSITORY, GITHUB_OUTPUT, GH_TOKEN.
Dispatch must check out the default branch with full history and tags. Only a
dispatch writes a release commit/tag; GitHub's token does not trigger another
tag workflow. Outputs: channel, tag, version, ref, commit. ref is a commit SHA.
"""

import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
TAG = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


class ReleaseError(Exception):
    pass


def command(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True)
    if check and result.returncode:
        raise ReleaseError(f"{' '.join(args[:3])} failed: {result.stderr.strip()}")
    return result


def git(*args: str, check: bool = True) -> str:
    return command("git", *args, check=check).stdout.strip()


def version(kind: str = "current", dry_run: bool = False) -> str:
    args = [sys.executable, str(ROOT / "scripts/bump-version.py"), kind]
    if dry_run:
        args.append("--dry-run")
    return command(*args).stdout.strip()


def release_exists(repository: str, tag: str) -> bool:
    # An authentication/network error must not be interpreted as a missing release.
    result = command("gh", "api", "--include", f"repos/{repository}/releases/tags/{tag}", check=False)
    if result.returncode == 0:
        return True
    status = re.search(r"(?m)^HTTP/\S+\s+(\d{3})\b", result.stdout)
    if (status and status[1] == "404") or "(HTTP 404)" in result.stderr:
        return False
    raise ReleaseError(f"cannot verify whether release {tag} exists: {result.stderr.strip()}")


def tag_state(tag: str) -> tuple[str | None, str | None]:
    local = command("git", "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}", check=False)
    local_commit = local.stdout.strip() if local.returncode == 0 else None
    remote = git("ls-remote", "--tags", "origin", f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}")
    refs = dict(line.split()[::-1] for line in remote.splitlines())
    remote_commit = refs.get(f"refs/tags/{tag}^{{}}", refs.get(f"refs/tags/{tag}"))
    return local_commit, remote_commit


def configure_identity() -> None:
    git("config", "user.name", "github-actions[bot]")
    git("config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")


def dispatch(default_branch: str, bump: str) -> tuple[str, str]:
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ReleaseError("GITHUB_REPOSITORY must identify the release repository")
    if git("symbolic-ref", "--quiet", "--short", "HEAD") != default_branch:
        raise ReleaseError("manual releases must check out the repository's default branch")
    current = version()
    head = git("rev-parse", "HEAD")
    current_tag = f"v{current}"
    states = tag_state(current_tag)
    recover = head in states and not release_exists(repository, current_tag)
    if recover:
        target = current
        tag = current_tag
        if release_exists(repository, tag):
            raise ReleaseError(f"release {tag} already exists")
        if any(commit is not None and commit != head for commit in states):
            raise ReleaseError(f"tag {tag} is on another commit")
        remote_exists = states[1] is not None
        if states[0] is None and not remote_exists:
            configure_identity()
            git("tag", "-a", tag, "-m", f"Cinnaroids {tag}")
    else:
        target = version(bump, dry_run=True)
        tag = f"v{target}"
        if release_exists(repository, tag):
            raise ReleaseError(f"release {tag} already exists")
        if any(commit is not None for commit in tag_state(tag)):
            raise ReleaseError(f"tag {tag} already exists")
        configure_identity()
        if version(bump) != target:
            raise ReleaseError("version changed while preparing the release")
        git("add", "--", "Cargo.toml", "Cargo.lock", "mods/cinnaroids/Cargo.toml", "mods/cinnaroids/Cargo.lock", "app.rc")
        git("commit", "-m", f"chore: release {tag}")
        head = git("rev-parse", "HEAD")
        git("tag", "-a", tag, "-m", f"Cinnaroids {tag}")
        remote_exists = False
    refs = [f"HEAD:refs/heads/{default_branch}"]
    if not remote_exists:
        refs.append(f"refs/tags/{tag}:refs/tags/{tag}")
    git("push", "--atomic", "origin", *refs)
    return tag, target


def prepare() -> dict[str, str]:
    event = os.environ.get("EVENT_NAME", os.environ.get("GITHUB_EVENT_NAME", ""))
    default_branch = os.environ.get("DEFAULT_BRANCH", "main")
    git("check-ref-format", f"refs/heads/{default_branch}")
    if git("status", "--porcelain"):
        raise ReleaseError("release preparation requires a clean checkout")
    if event == "workflow_dispatch":
        bump = os.environ.get("BUMP", "patch")
        if bump not in {"patch", "minor", "major"}:
            raise ReleaseError("BUMP must be patch, minor, or major")
        tag, source_version = dispatch(default_branch, bump)
        channel = "stable"
    elif event == "push":
        ref = os.environ.get("GITHUB_REF", "")
        source_version = version()
        if ref.startswith("refs/tags/"):
            tag = os.environ.get("RELEASE_TAG", ref.removeprefix("refs/tags/"))
            if not TAG.fullmatch(tag) or ref != f"refs/tags/{tag}":
                raise ReleaseError("release tag must look like v1.2.3 and match the pushed ref")
            if tag.removeprefix("v") != source_version:
                raise ReleaseError(f"tag {tag} does not match source version {source_version}")
            if git("rev-parse", f"refs/tags/{tag}^{{commit}}") != git("rev-parse", "HEAD"):
                raise ReleaseError(f"tag {tag} is on another commit")
            channel = "stable"
        else:
            raise ReleaseError("only v* tag pushes can publish releases")
        event_sha = os.environ.get("GITHUB_SHA", "")
        if event_sha:
            if not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", event_sha):
                raise ReleaseError("GITHUB_SHA is not a commit hash")
            if git("rev-parse", f"{event_sha}^{{commit}}") != git("rev-parse", "HEAD"):
                raise ReleaseError("checkout does not match the pushed commit")
    else:
        raise ReleaseError("EVENT_NAME must be push or workflow_dispatch")
    commit = git("rev-parse", "HEAD")
    return {"channel": channel, "tag": tag, "version": source_version, "ref": commit, "commit": commit}


def main() -> int:
    try:
        output = os.environ.get("GITHUB_OUTPUT")
        if not output:
            raise ReleaseError("GITHUB_OUTPUT must be set before release preparation")
        values = prepare()
        with open(output, "a") as destination:
            for name, value in values.items():
                destination.write(f"{name}={value}\n")
        print(f"Prepared {values['channel']} {values['tag']} at {values['commit']}")
        return 0
    except (OSError, ReleaseError) as error:
        print(f"release preparation: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
