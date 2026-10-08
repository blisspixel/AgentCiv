"""Real native dispatch, both HTTP hosts, and private response failure paths."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration as client  # noqa: E402
import durable_gathering as durable  # noqa: E402
import mock_collaboration as mock  # noqa: E402
import reader_collaboration as reader  # noqa: E402
import walk  # noqa: E402


class DurableGatheringTests(unittest.TestCase):
    runtime: Path
    host: Path

    @classmethod
    def setUpClass(cls) -> None:
        binaries = walk.build_rust_binaries("agentciv-runtime", "agentciv-host")
        cls.runtime, cls.host = binaries["agentciv-runtime"], binaries["agentciv-host"]

    def test_actual_stopping_replacement_and_explicit_return_on_both_hosts(self) -> None:
        for host in ("python", "rust"):
            with self.subTest(host=host), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "evidence"
                report = durable.run(output, host=host, runtime_binary=self.runtime, host_binary=self.host)
                self.assertEqual(report["outcome"], "passed")
                self.assertTrue(report["source_unchanged"])
                self.assertTrue(report["old_world_credential_rejected"])
                observations = report["observations"]
                assert isinstance(observations, list)
                rows = {str(client.object_value(row)["label"]): client.object_value(client.object_value(row)["result"])
                    for row in observations}
                for label in ("stopped_invitation", "repeated_stopped_invitation", "replacement_keeps_stop",
                              "world_restart_keeps_stop", "old_coordinator", "replacement_unready", "history_unavailable"):
                    self.assertIsNone(rows[label]["child"])
                    self.assertEqual(client.object_value(rows[label]["dispatch"])["status"], "not_launched")
                self.assertEqual(client.object_value(rows["local_leave"]["child"])["outcome"], "left")
                self.assertEqual(rows["stop_retry"], rows["explicit_scope_stop"])
                self.assertEqual(rows["old_resume_retry"], rows["explicit_initial_resume_a"])
                self.assertEqual(set(path.name for path in output.iterdir()), {"report.json", "history.json"})
                history = client.decode((output / "history.json").read_bytes())["events"]
                assert isinstance(history, list)
                self.assertEqual(len(history), 4)
                self.assertEqual([client.object_value(event)["actor"] for event in history],
                    [durable.A, durable.B, durable.B, durable.A])
                serialized = (output / "report.json").read_text() + (output / "history.json").read_text()
                for private in ("administrator_token", "control_token", "child_config_path", "database_path"):
                    self.assertNotIn(private, serialized)

    def test_process_failure_never_copies_private_error_or_reuses_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "config.json"
            mock.save(path, {})
            for failure in (OSError("private-secret"), subprocess.TimeoutExpired("private-command", 70)):
                with patch.object(subprocess, "run", side_effect=failure):
                    with self.assertRaisesRegex(ValueError, "^runtime_process_failed$"):
                        durable.invoke(self.runtime, path, {}, ["private-secret"])
            for payload, status, code in ((b"x" * 65537, 0, "runtime_response_limit"),
                (b"{}", 0, "runtime_invalid_response"), (b"{", 0, "runtime_invalid_response"),
                (b'{"outcome":"ok"}', 1, "runtime_invalid_response"),
                (b'{"outcome":"failed","code":"private-secret"}', 1, "runtime_credential_reflected")):
                result = subprocess.CompletedProcess([], status, payload, b"private-stderr")
                with patch.object(subprocess, "run", return_value=result):
                    with self.assertRaisesRegex(ValueError, "^" + code + "$"):
                        durable.invoke(self.runtime, path, {}, ["private-secret"])
            result = subprocess.CompletedProcess([], 1, b'{"outcome":"failed","code":"state_unavailable"}', b"private-stderr")
            with patch.object(subprocess, "run", return_value=result):
                self.assertEqual(durable.invoke(self.runtime, path, {}, []),
                    {"outcome": "failed", "code": "state_unavailable"})

    def test_invalid_host_and_existing_output_fail_without_dispatch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, patch.object(subprocess, "run") as launch, \
             patch.object(reader, "source_identity", return_value={}):
            with self.assertRaisesRegex(ValueError, "invalid_configuration"):
                durable.run(Path(temporary), host="outside")
            with self.assertRaises(FileExistsError):
                durable.run(Path(temporary), runtime_binary=self.runtime, host_binary=self.host)
            launch.assert_not_called()

    def test_cli_reports_fixed_failure(self) -> None:
        with patch.object(sys, "argv", ["durable_gathering.py", "--output", "unused"]), \
             patch.object(durable, "run", side_effect=ValueError("private-secret")), patch("builtins.print") as printed:
            self.assertEqual(durable.main(), 1)
            self.assertEqual(json.loads(str(printed.call_args.args[0])),
                {"outcome": "failed", "code": "durable_gathering_failed"})


if __name__ == "__main__":
    unittest.main()
