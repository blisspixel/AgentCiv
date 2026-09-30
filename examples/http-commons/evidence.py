"""Identify the actual source files used by a local evidence run."""

from __future__ import annotations

import hashlib
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PATHS = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "SPEC.md", "PROTOCOL.md",
         "docs/COLLABORATION_PROFILE.md", "schemas", "conformance/runner",
         "reference/host", "implementations/http-commons-python", "examples/http-commons", "examples/participants")


def git(*arguments: str) -> str:
    return subprocess.run(["git", *arguments], cwd=ROOT, check=True,
                          capture_output=True, text=True).stdout.strip()


def source_identity() -> dict[str, object]:
    # New code is included before commit. Unrelated untracked user files are not read.
    names = git("ls-files", "--cached", "--others", "--exclude-standard", "--", *PATHS).splitlines()
    hashes: dict[str, str] = {}
    for name in sorted(set(names)):
        path = ROOT / name
        if path.is_file():
            hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    if not hashes:
        raise RuntimeError("source inventory was empty")
    return {"repository_commit": git("rev-parse", "HEAD"),
            "source_working_tree_dirty": bool(git("status", "--porcelain", "--", *PATHS)),
            "source_sha256": hashes}
