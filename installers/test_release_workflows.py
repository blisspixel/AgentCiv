"""Exercise the public verifier's guards without network or native binaries."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


WORKFLOW = Path(__file__).resolve().parent.parent / ".github/workflows/native-published.yml"
TOOLS = ["agentciv-archive", "agentciv-reader", "agentciv-host", "agentciv-conformance"]


def embedded_python(marker: str) -> str:
    """Keep tests attached to the shipped workflow body, not a copied validator."""
    return WORKFLOW.read_text(encoding="utf-8").split(marker + "\n", 1)[1].split("\n          PY", 1)[0].replace("\n          ", "\n").removeprefix("          ")


class PublishedReleaseGuards(unittest.TestCase):
    def test_versions_before_any_download(self) -> None:
        guard = embedded_python("          python - <<'PY'")
        cases = [
            ("v0.1.1", "", True), ("v0.1.1", "v0.1.0", True), ("v10.0.0", "v2.9.9", True),
            ("", "", False), ("0.1.1", "", False), ("v0.1.1-preview.1", "", False),
            ("v0.1.1\n", "", False), ("v0.1.1;touch bad", "", False),
            ("v0.1.1", "v0.1.1", False), ("v0.1.1", "v0.1.2", False),
            ("v2.0.0", "v10.0.0", False), ("v0.1.1", "v0.1.0\n", False),
            ("v0.1.1", "../v0.1.0", False), ("v0.1.1", "v0.1.0$(bad)", False),
            ("v" + "1" * 64 + ".1.1", "", False),
        ]
        for current, previous, valid in cases:
            with self.subTest(current=current, previous=previous):
                result = subprocess.run(
                    [sys.executable, "-c", guard],
                    env={**os.environ, "RELEASE_VERSION": current, "PREVIOUS_RELEASE_VERSION": previous},
                    capture_output=True, text=True, check=False, timeout=10,
                )
                self.assertEqual(result.returncode == 0, valid, result.stderr)

    def test_installed_activation_components_and_checksums(self) -> None:
        checker = embedded_python("          cat > downloads/check-generation.py <<'PY'")
        targets = ["x86_64-unknown-linux-gnu", "aarch64-pc-windows-msvc", "aarch64-apple-darwin"]
        cases = ["valid", "activation", "missing_optional_tool", "tampered_binary", "tampered_notices",
                 "duplicate_manifest", "malformed_manifest", "missing_manifest_hash", "missing_resource", "wrong_target"]
        for target in targets:
            for case in cases:
                with self.subTest(target=target, case=case), tempfile.TemporaryDirectory(prefix="agentciv-release-test-") as temporary:
                    root = Path(temporary)
                    prefix, manifests, output = root / "installed with spaces", root / "manifests", root / "output.json"
                    generation = prefix / "versions" / "v0.1.1"
                    generation.mkdir(parents=True)
                    manifests.mkdir()
                    prefix.joinpath("current").write_text("v0.1.1\n", encoding="utf-8")
                    generation.joinpath("components").write_text(" ".join(TOOLS) + "\n", encoding="utf-8")
                    suffix = ".exe" if "windows" in target else ""
                    rows = []
                    for tool in TOOLS:
                        payload = (tool + " fixture").encode()
                        generation.joinpath(tool + suffix).write_bytes(payload)
                        rows.append(hashlib.sha256(payload).hexdigest() + "  " + tool + "-v0.1.1-" + target + suffix)
                    for resource in ["LICENSE", "THIRD_PARTY_NOTICES.txt", "NOTICE_INVENTORY.json"]:
                        payload = (resource + " fixture").encode()
                        generation.joinpath(resource).write_bytes(payload)
                        rows.append(hashlib.sha256(payload).hexdigest() + "  " + resource)
                    manifest = manifests / "SHA256SUMS"
                    manifest.write_text("\n".join(rows) + "\n", encoding="utf-8")
                    if case == "activation":
                        prefix.joinpath("current").write_text("v0.1.0\n", encoding="utf-8")
                    if case == "missing_optional_tool":
                        generation.joinpath("components").write_text("agentciv-archive agentciv-reader\n", encoding="utf-8")
                    if case == "tampered_binary":
                        generation.joinpath("agentciv-reader" + suffix).write_bytes(b"changed")
                    if case == "tampered_notices":
                        generation.joinpath("NOTICE_INVENTORY.json").write_bytes(b"changed")
                    if case == "duplicate_manifest":
                        manifest.write_text("\n".join(rows + [rows[0]]) + "\n", encoding="utf-8")
                    if case == "malformed_manifest":
                        manifest.write_text("bad checksum\n", encoding="utf-8")
                    if case == "missing_manifest_hash":
                        manifest.write_text("\n".join(rows[1:]) + "\n", encoding="utf-8")
                    if case == "missing_resource":
                        generation.joinpath("LICENSE").unlink()
                    selected_target = "wrong-target" if case == "wrong_target" else target
                    result = subprocess.run(
                        [sys.executable, "-c", checker, str(prefix), "v0.1.1", selected_target, str(manifests), str(output)],
                        capture_output=True, text=True, check=False, timeout=10,
                    )
                    self.assertEqual(result.returncode == 0, case == "valid", result.stderr)
                    self.assertEqual(output.exists(), case == "valid")
                    if case == "valid":
                        evidence = json.loads(output.read_text(encoding="utf-8"))
                        self.assertEqual(len(evidence["checked"]), 7)
                        self.assertEqual(evidence["version"], "v0.1.1")
                        self.assertEqual(evidence["target"], target)


if __name__ == "__main__":
    unittest.main()
