"""Failure-path checks for the local operator adapter's report acceptance."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

import validate
import evidence


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
