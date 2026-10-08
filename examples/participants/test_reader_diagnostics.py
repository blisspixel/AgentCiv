"""Fixed reader diagnostics distinguish failures without retaining private output."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import traceback
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reader_collaboration as reader  # noqa: E402
import mock_collaboration as mock  # noqa: E402
import oracle  # noqa: E402


class ReaderDiagnosticsTests(unittest.TestCase):
    def failure(self, completed: subprocess.CompletedProcess[bytes]) -> reader.ReaderSnapshotError:
        with patch.object(subprocess, "run", return_value=completed):
            with self.assertRaises(reader.ReaderSnapshotError) as caught:
                reader.read_snapshot(Path("reader"), "private-origin", "private-token", "civ:world")
        self.assertNotIn("private", str(caught.exception))
        return caught.exception

    def test_native_fixed_failure_is_retained_without_output(self) -> None:
        completed = subprocess.CompletedProcess(["reader"], 1, b"private-stdout",
            b'{"outcome":"failed","code":"deadline_exceeded"}\n')
        failure = self.failure(completed)
        self.assertEqual((failure.operation, failure.code, str(failure)),
            ("reader_process", "deadline_exceeded", "reader failed"))
        self.assertIsInstance(failure, ValueError)

    def test_untrusted_error_envelopes_are_never_copied(self) -> None:
        variants = [b"private-trace", b"\xff", b"{}", b"[]", b"null",
            b'{"outcome":"failed","code":"private-token"}',
            b'{"outcome":"failed","code":"transport_failed","trace":"private-token"}',
            b'{"outcome":"success","code":"transport_failed"}',
            b'{"outcome":"failed","code":true}',
            b'{"outcome":"failed","code":"transport_failed","code":"http_failed"}',
            b'{"outcome":"failed","code":"transport_failed"}\nprivate-trace',
            b'{"outcome":"failed","code":"transport_failed"}' + b" " * 256]
        for stderr in variants:
            with self.subTest(stderr=stderr):
                failure = self.failure(subprocess.CompletedProcess(["reader"], 1, b"private-stdout", stderr))
                self.assertEqual((failure.operation, failure.code), ("reader_process", "process_failed"))

    def test_output_limit_does_not_report_a_stderr_code_from_success(self) -> None:
        failure = self.failure(subprocess.CompletedProcess(["reader"], 0, b"x" * 1_048_577,
            b'{"outcome":"failed","code":"transport_failed"}'))
        self.assertEqual(failure.code, "output_limit")

    def test_decode_failures_are_distinct_from_native_process_failures(self) -> None:
        failure = self.failure(subprocess.CompletedProcess(["reader"], 0, b"private-invalid-json", b"private-stderr"))
        self.assertEqual((failure.operation, failure.code), ("reader_decode", "invalid_output"))

    def test_credential_reflection_has_a_fixed_diagnostic(self) -> None:
        failure = self.failure(subprocess.CompletedProcess(["reader"], 0,
            b'{"private":"private-token"}', b""))
        self.assertEqual((failure.operation, failure.code), ("reader_decode", "credential_reflected"))

    def test_snapshot_validation_has_a_fixed_diagnostic(self) -> None:
        for payload, code in [({}, "invalid_output"),
            ({"snapshot": {"world": "civ:other", "records": []}, "report": {}}, "world_changed")]:
            with self.subTest(code=code):
                failure = self.failure(subprocess.CompletedProcess(["reader"], 0, json.dumps(payload).encode(), b""))
                self.assertEqual((failure.operation, failure.code), ("reader_validation", code))

    def test_timeout_and_launch_errors_do_not_retain_command_or_trace(self) -> None:
        errors = [subprocess.TimeoutExpired(["private-command", "private-config"], 40,
                     output=b"private-stdout", stderr=b"private-stderr"), OSError("private-trace")]
        for error in errors:
            with self.subTest(error=type(error).__name__), patch.object(subprocess, "run", side_effect=error):
                with self.assertRaises(reader.ReaderSnapshotError) as caught:
                    reader.read_snapshot(Path("reader"), "private-origin", "private-token", "civ:world")
                failure = caught.exception
                self.assertEqual(failure.operation, "reader_process")
                self.assertEqual(failure.code, "process_timeout" if isinstance(error, subprocess.TimeoutExpired)
                    else "operation_io_failed")
                self.assertNotIn("private", "".join(traceback.format_exception(failure)))

    def test_configuration_failure_is_distinct(self) -> None:
        with patch.object(mock, "save", side_effect=OSError("private-path")):
            with self.assertRaises(reader.ReaderSnapshotError) as caught:
                reader.read_snapshot(Path("reader"), "private-origin", "private-token", "civ:world")
        self.assertEqual((caught.exception.operation, caught.exception.code),
            ("reader_configuration", "operation_io_failed"))
        self.assertNotIn("private", "".join(traceback.format_exception(caught.exception)))

    def test_export_failure_is_distinct_and_does_not_leak_its_path(self) -> None:
        events = oracle.validate_events(oracle.load_json(reader.ROOT / "examples/inheritance/history.json"))
        world = str(events[0]["world"])
        payload = {"snapshot": {"world": world, "records": [json.dumps(event) for event in events]},
            "report": {"pages": 1, "events": len(events), "reached_end": True,
                       "scope": "current_caller_view", "copying_permission": "not_granted"}}
        actual_save = mock.save

        def fail_export(path: Path, value: object) -> None:
            if path.name == "private-export.json":
                raise OSError("private-export-path")
            actual_save(path, value)

        with tempfile.TemporaryDirectory() as temporary, patch.object(mock, "save", side_effect=fail_export), \
            patch.object(subprocess, "run", return_value=subprocess.CompletedProcess(
                ["reader"], 0, json.dumps(payload).encode(), b"")):
            with self.assertRaises(reader.ReaderSnapshotError) as caught:
                reader.read_snapshot(Path("reader"), "private-origin", "private-token", world,
                    originals_path=Path(temporary) / "private-export.json")
        self.assertEqual((caught.exception.operation, caught.exception.code), ("reader_export", "operation_io_failed"))
        self.assertNotIn("private", "".join(traceback.format_exception(caught.exception)))

    def test_run_retains_outer_call_site_and_fixed_native_operation(self) -> None:
        failure = reader.ReaderSnapshotError("reader_process", "deadline_exceeded", "private-native-output")
        with tempfile.TemporaryDirectory() as temporary, \
            patch.object(reader, "source_identity", return_value={"fixture_identity": True}), \
            patch.object(reader, "read_snapshot", side_effect=failure) as fetch:
            output = Path(temporary) / "run"
            # The patched reader never executes this hashable stand-in binary.
            report = reader.run(output, condition="repair", reader_binary=Path(__file__))
            retained = (output / "report.json").read_text(encoding="utf-8")
        fetch.assert_called_once()
        self.assertEqual(report["outcome"], "failed")
        self.assertEqual(report["failure_stage"], "study_evidence")
        self.assertEqual(report["failure_operation"], "study_original_capture")
        self.assertEqual(report["reader_failure_operation"], "reader_process")
        self.assertEqual(report["failure_code"], "deadline_exceeded")
        self.assertTrue(report["prior_records_retained"])
        self.assertFalse(report["useful_result"])
        self.assertFalse(report["qualified_improvement"])
        self.assertNotIn("private-native-output", retained)
        self.assertEqual(json.loads(retained), report)


if __name__ == "__main__":
    unittest.main()
