"""Failure-path checks for the local operator adapter's report acceptance."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

import validate
import evidence
import walk


def report(policy: str = "addressed") -> dict[str, object]:
    case_ids = sorted(validate.HIDDEN_CASES | {
        "submit.concurrent_retry", "submit.concurrent_conflict", "submit.concurrent_distinct",
        "collaborate.revision", "collaborate.objection", "collaborate.decline",
    })
    cases = [
        {"id": case_id, "status": "skipped" if policy == "members" and case_id in validate.HIDDEN_CASES else "passed",
         "required": not (policy == "members" and case_id in validate.HIDDEN_CASES)}
        for case_id in case_ids
    ]
    skipped = len(validate.HIDDEN_CASES) if policy == "members" else 0
    return {"profile": "http-commons/0.1-draft", "runner_scope": "credentialed-extended",
            "cases": cases, "summary": {"passed": len(cases) - skipped, "failed": 0, "skipped": skipped}}


class ReportTests(unittest.TestCase):
    def test_accepts_all_passes_and_only_declared_members_skips(self) -> None:
        for policy in validate.POLICIES:
            payload = report(policy)
            self.assertEqual(validate.checked_report(json.dumps(payload), 0, policy), payload)

    def test_rejects_forged_success_or_missing_evidence(self) -> None:
        for field, value in (("profile", "other"), ("runner_scope", "credentialed-smoke"),
                             ("cases", []), ("summary", {"passed": 999})):
            payload = report()
            payload[field] = value
            with self.assertRaises(validate.ValidationFailure):
                validate.checked_report(json.dumps(payload), 0, "addressed")
        for text, status in (("{}", 1), ("[]", 0), ("invalid", 0)):
            with self.assertRaises(validate.ValidationFailure):
                validate.checked_report(text, status, "addressed")

    def test_rejects_duplicate_failed_unknown_or_skipped_cases(self) -> None:
        cases = report()["cases"]
        assert isinstance(cases, list)
        variants: list[list[object]] = [
            cases + [cases[0]], cases[1:],
            [{"id": "bad", "status": "passed"}, *cases],
        ]
        for status in ("failed", "skipped", "mystery"):
            variants.append([{"id": "bad", "status": status, "required": False}, *cases])
        variants.append([{"id": 42, "status": "passed"}, *cases])
        for case_list in variants:
            payload = report()
            payload["cases"] = case_list
            # A new passed case is allowed, but only with a correct summary.
            with self.assertRaises(validate.ValidationFailure):
                validate.checked_report(json.dumps(payload), 0, "addressed")
        with self.assertRaises(validate.ValidationFailure):
            validate.checked_report(json.dumps(report("members")), 0, "sender_only")

    def test_fresh_configuration_has_separate_random_credentials(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path, environment = validate.configure(Path(temporary), "addressed")
            config = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(config["visibility"], "addressed")
            credentials = config["credentials"]
            tokens = [item["token"] for item in credentials]
            self.assertEqual(len(set(tokens)), 3)
            self.assertFalse(credentials[2]["write"])
            self.assertEqual(environment["AGENTCIV_CONFORMANCE_TOKEN"], tokens[0])
            with self.assertRaises(validate.ValidationFailure):
                validate.configure(Path(temporary), "public")

    def test_build_failure_is_reported(self) -> None:
        with patch("validate.subprocess.run") as command:
            command.return_value.returncode = 1
            with self.assertRaises(validate.ValidationFailure):
                validate.binaries()

    def test_build_uses_reported_external_executables_not_stale_repository_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / "external-build" / "custom-target" / "debug"
            folder.mkdir(parents=True)
            paths = {name: folder / name for name in ("agentciv-host", "agentciv-conformance")}
            for path in paths.values():
                path.touch()
            messages: list[dict[str, object]] = [
                {"reason": "compiler-message"}, {"reason": "build-finished", "success": True},
                {"reason": "compiler-artifact", "target": {"kind": ["lib"], "name": "agentciv-host"},
                 "executable": None},
            ]
            for name, path in paths.items():
                messages.append({"reason": "compiler-artifact", "target": {"kind": ["bin"], "name": name},
                                 "executable": str(path)})
            completed = subprocess.CompletedProcess(["cargo"], 0, "\n".join(json.dumps(m) for m in messages), "")
            with patch("walk.subprocess.run", return_value=completed) as command:
                self.assertEqual(validate.binaries(), (paths["agentciv-host"], paths["agentciv-conformance"]))
                self.assertIn("--message-format=json", command.call_args.args[0])
                self.assertEqual(walk.rust_binary(), paths["agentciv-host"])

    def test_build_rejects_missing_ambiguous_or_malformed_artifact_reports(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "host"
            binary.touch()
            artifact = {"reason": "compiler-artifact", "target": {"kind": ["bin"], "name": "agentciv-host"},
                        "executable": str(binary)}
            for payload in ("not JSON", "[]", "{}", json.dumps(artifact) + "\n" + json.dumps(artifact),
                            json.dumps({**artifact, "executable": str(binary.with_name("absent"))}),
                            json.dumps({"reason": "compiler-artifact", "target": None, "executable": str(binary)})):
                with self.subTest(payload=payload):
                    completed = subprocess.CompletedProcess(["cargo"], 0, payload, "")
                    with patch("walk.subprocess.run", return_value=completed), self.assertRaises(walk.WalkFailure):
                        walk.build_rust_binaries("agentciv-host")

    def test_lifecycle_requires_every_case_and_matching_summary(self) -> None:
        for phase, inventory in validate.LIFECYCLE_CASES.items():
            payload: dict[str, object] = {
                "profile": "http-commons/0.1-draft", "runner_scope": f"lifecycle-{phase}",
                "cases": [{"id": case_id, "status": "passed"} for case_id in sorted(inventory)],
                "summary": {"passed": len(inventory), "failed": 0, "skipped": 0},
            }
            self.assertEqual(validate.checked_lifecycle(json.dumps(payload), 0, phase), payload)
            for field, value in (("cases", []), ("summary", {}), ("runner_scope", "credentialed-extended")):
                broken = dict(payload)
                broken[field] = value
                with self.assertRaises(validate.ValidationFailure):
                    validate.checked_lifecycle(json.dumps(broken), 0, phase)
            with self.assertRaises(validate.ValidationFailure):
                validate.checked_lifecycle(json.dumps(payload), 1, phase)
        with self.assertRaises(validate.ValidationFailure):
            validate.checked_lifecycle("not JSON", 0, "verify")

    def test_source_identity_includes_new_code_and_discloses_dirty_state(self) -> None:
        identity = evidence.source_identity()
        hashes = identity["source_sha256"]
        self.assertIsInstance(hashes, dict)
        assert isinstance(hashes, dict)
        self.assertIn("examples/http-commons/validate.py", hashes)
        self.assertNotIn("script.py", hashes)
        self.assertEqual(len(str(hashes["examples/http-commons/validate.py"])), 64)
        self.assertIsInstance(identity["source_working_tree_dirty"], bool)
        with patch("evidence.git", return_value=""):
            with self.assertRaises(RuntimeError):
                evidence.source_identity()


if __name__ == "__main__":
    unittest.main()
