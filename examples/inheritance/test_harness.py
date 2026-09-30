from __future__ import annotations

import copy
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import harness
import host_source
import newcomer
import oracle
import walk


class HarnessTests(unittest.TestCase):
    def setUp(self) -> None:
        self.events = oracle.validate_events(oracle.load_json(harness.HERE / "history.json"))
        self.directory = tempfile.TemporaryDirectory(prefix="agentciv-harness-test-")
        self.addCleanup(self.directory.cleanup)
        self.output = Path(self.directory.name) / "result"
        self.binary = Path(self.directory.name) / "archive-test"
        self.binary.write_bytes(b"test binary hash only")

    def mocked_archive(self, binary: Path, operation: str, *paths: Path) -> dict[str, object]:
        del binary
        self.assertEqual(operation, "export")
        snapshot = oracle.object_value(oracle.load_json(paths[0]))
        return {"world": snapshot["world"], "records": snapshot["records"]}

    def test_scripted_composition_is_explicit_not_model_behavior(self) -> None:
        with patch.object(walk, "build_rust_binaries", return_value={"agentciv-archive": self.binary}), \
             patch.object(newcomer, "archive", side_effect=self.mocked_archive), \
             patch.object(newcomer, "load_bundle", return_value=(self.events, {"copy_integrity": "matched"})):
            report = harness.run(self.output, mode="scripted", source="fixture")
        self.assertEqual(report["outcome"], "evaluated")
        self.assertIs(report["useful_continuation"], True)
        self.assertEqual(report["decision_source"], "scripted_after_baseline")
        self.assertIs(report["model_result_is_scripted_fallback"], False)
        self.assertIs(oracle.object_value(report["before"])["useful_continuation"], False)
        self.assertEqual(json.loads((self.output / "history.json").read_text(encoding="utf-8")), self.events)

    def test_failed_model_process_preserves_failure_without_scripted_answer(self) -> None:
        class Process:
            returncode = 1

        original_identity = harness.source_identity

        def fake_process(command: list[str], **kwargs: object) -> Process:
            del kwargs
            self.assertNotIn(str(harness.HERE / "after.json"), command)
            destination = self.output / "newcomer"
            destination.mkdir()
            (destination / "report.json").write_text('{"outcome":"failed"}', encoding="utf-8")
            return Process()

        identity = original_identity()
        with patch.object(harness, "source_identity", return_value=identity), \
             patch.object(walk, "build_rust_binaries", return_value={"agentciv-archive": self.binary}), \
             patch.object(newcomer, "archive", side_effect=self.mocked_archive), \
             patch.object(newcomer, "load_bundle", return_value=(self.events, {})), \
             patch.object(subprocess, "run", side_effect=fake_process):
            report = harness.run(self.output, mode="ollama", source="fixture", model="local-test")
        self.assertEqual(report["outcome"], "failed")
        self.assertFalse((self.output / "decision.json").exists())
        self.assertIs(report["model_result_is_scripted_fallback"], False)
        self.assertEqual(report["newcomer"], {"outcome": "failed"})

    def test_source_copy_mismatch_is_failure_before_newcomer(self) -> None:
        different = copy.deepcopy(self.events)
        different.pop()
        with patch.object(walk, "build_rust_binaries", return_value={"agentciv-archive": self.binary}), \
             patch.object(newcomer, "archive", side_effect=self.mocked_archive), \
             patch.object(newcomer, "load_bundle", return_value=(different, {})):
            report = harness.run(self.output, mode="scripted", source="fixture")
        self.assertEqual(report["outcome"], "failed")
        self.assertFalse((self.output / "decision.json").exists())

    def test_baseline_mapping_keeps_author_revision_exact(self) -> None:
        changed = copy.deepcopy(self.events)
        for event in changed:
            event["id"] = "host:" + str(event["id"])
        plan = harness.baseline("after.json", changed)
        self.assertIs(oracle.evaluate(changed, plan)["useful_continuation"], True)
        self.assertIs(oracle.evaluate(changed, harness.baseline("before.json", changed))["useful_continuation"], False)

    def test_existing_output_is_never_overwritten(self) -> None:
        self.output.mkdir()
        marker = self.output / "keep.txt"
        marker.write_text("keep", encoding="utf-8")
        with self.assertRaises(FileExistsError):
            harness.run(self.output, mode="scripted", source="fixture")
        self.assertEqual(marker.read_text(encoding="utf-8"), "keep")

    def test_bundle_change_after_passing_acceptance_cannot_be_success(self) -> None:
        actual_evaluate = oracle.evaluate
        calls = 0

        def mutate_after_check(events: list[oracle.JsonObject], candidate: oracle.JsonObject) -> oracle.JsonObject:
            nonlocal calls
            result = actual_evaluate(events, candidate)
            calls += 1
            if calls == 3:
                self.assertIs(result["useful_continuation"], True)
                bundle = self.output / "bundle.json"
                bundle.write_bytes(bundle.read_bytes() + b"\n")
            return result

        identity = {"commit": "fixture", "file_sha256": {"fixture.py": "unchanged"}}
        with patch.object(harness, "source_identity", return_value=identity), \
             patch.object(walk, "build_rust_binaries", return_value={"agentciv-archive": self.binary}), \
             patch.object(newcomer, "archive", side_effect=self.mocked_archive), \
             patch.object(newcomer, "load_bundle", return_value=(self.events, {})), \
             patch.object(oracle, "evaluate", side_effect=mutate_after_check):
            report = harness.run(self.output, mode="scripted", source="fixture")
        self.assertEqual(calls, 3)
        self.assertIs(oracle.object_value(report["newcomer_check"])["useful_continuation"], True)
        self.assertEqual(report["outcome"], "failed")
        self.assertIs(report["useful_continuation"], False)
        self.assertIs(report["original_bundle_unchanged"], False)
        self.assertIs(report["source_changed_during_run"], False)
        retained = oracle.object_value(oracle.load_json(self.output / "report.json"))
        self.assertEqual(retained, report)

    def test_final_bundle_read_failure_retains_failed_report_without_exception_text(self) -> None:
        actual_read = Path.read_bytes
        reads = 0

        def lose_bundle(path: Path) -> bytes:
            nonlocal reads
            if path == self.output / "bundle.json":
                reads += 1
                if reads == 2:
                    raise OSError("private-fixture-error-text")
            return actual_read(path)

        identity = {"commit": "fixture", "file_sha256": {"fixture.py": "unchanged"}}
        with patch.object(harness, "source_identity", return_value=identity), \
             patch.object(walk, "build_rust_binaries", return_value={"agentciv-archive": self.binary}), \
             patch.object(newcomer, "archive", side_effect=self.mocked_archive), \
             patch.object(newcomer, "load_bundle", return_value=(self.events, {})), \
             patch.object(Path, "read_bytes", lose_bundle):
            report = harness.run(self.output, mode="scripted", source="fixture")
        self.assertEqual(reads, 2)
        self.assertIs(oracle.object_value(report["newcomer_check"])["useful_continuation"], True)
        self.assertEqual(report["outcome"], "failed")
        self.assertIs(report["useful_continuation"], False)
        self.assertEqual(report["failure_stage"], "independent_acceptance")
        self.assertEqual(report["failure_code"], "stage_failed")
        public = (self.output / "report.json").read_text(encoding="utf-8")
        self.assertNotIn("private-fixture-error-text", public)

    def test_host_source_error_writes_fixed_failure_and_controls(self) -> None:
        identity = {"commit": "fixture", "file_sha256": {"fixture.py": "unchanged"}}
        with patch.object(harness, "source_identity", return_value=identity), \
             patch.object(walk, "build_rust_binaries", return_value={"agentciv-archive": self.binary}), \
             patch.object(host_source, "create_history",
                          side_effect=host_source.HostSourceError("private-source-error-text")):
            report = harness.run(self.output, mode="ollama", source="host", model="fixture-local",
                                 seed=7, attempts=3, private_traces=True)
        self.assertEqual(report["outcome"], "failed")
        self.assertIs(report["useful_continuation"], False)
        self.assertEqual(report["failure_stage"], "source_history")
        self.assertEqual(report["failure_code"], "stage_failed")
        self.assertEqual(oracle.object_value(report["controls"])["model"], "fixture-local")
        self.assertEqual(oracle.object_value(report["controls"])["seed"], 7)
        self.assertEqual(oracle.object_value(report["controls"])["attempts"], 3)
        self.assertIs(oracle.object_value(report["controls"])["private_traces"], True)
        self.assertFalse((self.output / "decision.json").exists())
        public = (self.output / "report.json").read_text(encoding="utf-8")
        self.assertNotIn("private-source-error-text", public)
        self.assertEqual(oracle.load_json(self.output / "report.json"), report)

    def test_source_digest_or_commit_drift_invalidates_an_earlier_passing_check(self) -> None:
        before = {"commit": "fixture", "file_sha256": {"fixture.py": "before"}}
        changes = [
            {"commit": "fixture", "file_sha256": {"fixture.py": "after"}},
            {"commit": "different", "file_sha256": {"fixture.py": "before"}},
        ]
        for index, after in enumerate(changes):
            with self.subTest(change=after):
                destination = self.output.with_name(f"drift-{index}")
                with patch.object(harness, "source_identity", side_effect=[before, after]), \
                     patch.object(walk, "build_rust_binaries", return_value={"agentciv-archive": self.binary}), \
                     patch.object(newcomer, "archive", side_effect=self.mocked_archive), \
                     patch.object(newcomer, "load_bundle", return_value=(self.events, {})):
                    report = harness.run(destination, mode="scripted", source="fixture")
                self.assertIs(oracle.object_value(report["newcomer_check"])["useful_continuation"], True)
                self.assertEqual(report["outcome"], "failed")
                self.assertIs(report["useful_continuation"], False)
                self.assertIs(report["source_changed_during_run"], True)
                self.assertEqual(report["failure_stage"], "source_identity")
                self.assertEqual(report["failure_code"], "source_changed_during_run")
                self.assertEqual(oracle.load_json(destination / "report.json"), report)

    def test_cli_cannot_report_success_from_a_failed_report_with_a_stale_flag(self) -> None:
        with patch.object(sys, "argv", ["harness.py", "--output", "unused"]), \
             patch.object(harness, "run", return_value={"outcome": "failed", "useful_continuation": True}), \
             patch.object(sys, "stdout", io.StringIO()):
            self.assertEqual(harness.main(), 1)


if __name__ == "__main__":
    unittest.main()
