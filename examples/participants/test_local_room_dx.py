"""Tests for local room client diagnostics, safe next steps, and roster warnings."""

from __future__ import annotations

import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples" / "http-commons"))
import encounter_client as client  # noqa: E402
import encounter_private as private  # noqa: E402
import local_encounter as operator  # noqa: E402

JsonObject = dict[str, object]
WORLD = "civ:local-room-dx-test"
WRITER = "agent:writer"
READER = "agent:reader"


def _make_config(root: Path, *, members: list[str] | None = None) -> tuple[Path, JsonObject]:
    state = root / "private"
    private.create_directory(state)
    cache = state / "cache"
    private.create_directory(cache)
    value: JsonObject = {
        "format": client.CONFIG_FORMAT,
        "origin": "http://127.0.0.1:8787",
        "world": WORLD,
        "principal": WRITER,
        "token": "synthetic-fixture-credential-0123456789",
        "payload_bytes": 16384,
        "cache_directory": str(cache),
        "private_cache_permission": client.CACHE_PERMISSION,
    }
    if members is not None:
        value["members"] = members
    path = state / "client.json"
    private.write_new(path, json.dumps(value).encode("utf-8"))
    return path, value


class LocalRoomBudgetDiagnosticTests(unittest.TestCase):
    def test_reader_budget_codes_map_to_room_exceeds_client_budget(self) -> None:
        cases = (
            ("page_limit", "max_pages", 32),
            ("event_limit", "max_events", 2048),
            ("response_limit", "max_response_bytes", 262144),
            ("record_limit", "max_record_bytes", 16000),
            ("total_byte_limit", "max_total_bytes", 4194304),
        )
        for native_code, expected_limit, expected_value in cases:
            with self.subTest(native_code=native_code):
                stderr = json.dumps({"outcome": "failed", "code": native_code}).encode("utf-8")
                fake = SimpleNamespace(returncode=1, stdout=b"", stderr=stderr)
                with patch("subprocess.run", return_value=fake), self.assertRaises(client.BudgetExceededError) as caught:
                    client._process(Path("/bin/true"), "read", Path("reader.json"))
                error = caught.exception
                self.assertEqual(error.code, "room_exceeds_client_budget")
                self.assertEqual(error.limit, expected_limit)
                self.assertEqual(error.configured_value, expected_value)
                formatted = client.format_error(error)
                self.assertEqual(formatted["outcome"], "failed")
                self.assertEqual(formatted["code"], "room_exceeds_client_budget")
                self.assertEqual(formatted["limit"], expected_limit)
                self.assertEqual(formatted["configured_value"], expected_value)
                self.assertIn(expected_limit, str(formatted["title"]))
                self.assertIn(str(expected_value), str(formatted["title"]))
                self.assertEqual(formatted["next_step"], client.SAFE_NEXT_STEPS["room_exceeds_client_budget"])

    def test_oversized_stdout_reports_max_output_budget(self) -> None:
        oversized = b"x" * (client.MAX_OUTPUT + 1)
        for returncode in (0, 1):
            with self.subTest(returncode=returncode):
                fake = SimpleNamespace(returncode=returncode, stdout=oversized, stderr=b"")
                with patch("subprocess.run", return_value=fake), self.assertRaises(client.BudgetExceededError) as caught:
                    client._process(Path("/bin/true"), "read", Path("reader.json"))
                self.assertEqual(caught.exception.code, "room_exceeds_client_budget")
                self.assertEqual(caught.exception.limit, "max_output")
                self.assertEqual(caught.exception.configured_value, client.MAX_OUTPUT)

    def test_existing_infrastructure_codes_do_not_regress(self) -> None:
        for native_code, expected in (
            ("transport_failed", "host_unreachable"),
            ("deadline_exceeded", "host_read_timeout"),
            ("prefix_changed", "prefix_changed"),
            ("unclassified_error", "reader_failed"),
        ):
            with self.subTest(native_code=native_code):
                stderr = json.dumps({"outcome": "failed", "code": native_code}).encode("utf-8")
                fake = SimpleNamespace(returncode=1, stdout=b"", stderr=stderr)
                with patch("subprocess.run", return_value=fake), self.assertRaises(private.EncounterError) as caught:
                    client._process(Path("/bin/true"), "read", Path("reader.json"))
                self.assertEqual(caught.exception.code, expected)
                self.assertNotIsInstance(caught.exception, client.BudgetExceededError)

    def test_cli_outputs_budget_fields_and_next_step(self) -> None:
        error = client.BudgetExceededError("max_events", 2048)
        with patch.object(sys, "argv", ["local_encounter.py", "read", "--config", "client.json"]), \
                patch.object(client, "read", side_effect=error), patch("builtins.print") as printed:
            self.assertEqual(operator.main(), 1)
            payload = json.loads(printed.call_args.args[0])
            self.assertEqual(payload["outcome"], "failed")
            self.assertEqual(payload["code"], "room_exceeds_client_budget")
            self.assertEqual(payload["limit"], "max_events")
            self.assertEqual(payload["configured_value"], 2048)
            self.assertIn("next_step", payload)


class LocalRoomSafeNextStepTests(unittest.TestCase):
    STABLE_CODES = (
        "host_unreachable",
        "host_read_timeout",
        "reader_binary_not_found",
        "private_path_exists",
        "private_lock_unavailable",
        "room_exceeds_client_budget",
        "reader_failed",
    )

    def test_every_stable_code_has_a_safe_next_step_in_cli_and_docs(self) -> None:
        guide = (operator.ROOT / "docs" / "LOCAL_ENCOUNTER.md").read_text(encoding="utf-8")
        self.assertIn("Do not issue a new `say` id under uncertainty; preserve the request journal", guide)
        for code in self.STABLE_CODES:
            with self.subTest(code=code):
                self.assertIn(f"`{code}`", guide)
                error: private.EncounterError = (
                    client.BudgetExceededError("max_events", 2048)
                    if code == "room_exceeds_client_budget"
                    else private.EncounterError(code)
                )
                with patch.object(sys, "argv", ["local_encounter.py", "read", "--config", "client.json"]), \
                        patch.object(client, "read", side_effect=error), patch("builtins.print") as printed:
                    self.assertEqual(operator.main(), 1)
                    payload = json.loads(printed.call_args.args[0])
                    self.assertEqual(payload["outcome"], "failed")
                    self.assertEqual(payload["code"], code)
                    self.assertEqual(payload["next_step"], client.SAFE_NEXT_STEPS[code])


class SayRosterWarningTests(unittest.TestCase):
    def test_say_warns_on_unknown_recipient_when_roster_present_without_rewriting(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config_path, _ = _make_config(Path(temporary).resolve(), members=[WRITER, READER])
            captured_raw: list[bytes] = []

            def fake_publish(settings: JsonObject, raw: bytes, basis: Path | None, reader_binary: Path | None) -> JsonObject:
                captured_raw.append(raw)
                return {"outcome": "recorded", "world": WORLD, "principal": WRITER, "record_id": "message:test"}

            stderr_buf = io.StringIO()
            with patch.object(client, "_publish", side_effect=fake_publish), redirect_stderr(stderr_buf):
                result = client.say(config_path, "hello", to=["agent:readr"])

            stderr_text = stderr_buf.getvalue()
            self.assertIn("agent:readr", stderr_text)
            self.assertIn(WRITER, stderr_text)
            self.assertIn(READER, stderr_text)
            warnings = result.get("warnings")
            self.assertIsInstance(warnings, list)
            assert isinstance(warnings, list)
            self.assertEqual(len(warnings), 1)
            self.assertIn("agent:readr", str(warnings[0]))
            self.assertIn(READER, str(warnings[0]))
            sent = json.loads(captured_raw[0].decode("utf-8"))
            self.assertEqual(sent["to"], ["agent:readr"])

    def test_say_does_not_warn_for_valid_roster_or_self_addressing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config_path, _ = _make_config(Path(temporary).resolve(), members=[WRITER, READER])
            for recipients in (None, [READER], [WRITER], [WRITER, READER]):
                with self.subTest(recipients=recipients):
                    stderr_buf = io.StringIO()
                    with patch.object(client, "_publish", return_value={"outcome": "recorded"}), redirect_stderr(stderr_buf):
                        result = client.say(config_path, "hello", to=recipients)
                    self.assertEqual(stderr_buf.getvalue(), "")
                    self.assertNotIn("warnings", result)

    def test_say_does_not_invent_roster_check_for_rooms_without_members(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config_path, _ = _make_config(Path(temporary).resolve(), members=None)
            stderr_buf = io.StringIO()
            with patch.object(client, "_publish", return_value={"outcome": "recorded"}), redirect_stderr(stderr_buf):
                result = client.say(config_path, "hello", to=["agent:any-peer"])
            self.assertEqual(stderr_buf.getvalue(), "")
            self.assertNotIn("warnings", result)

    def test_say_never_echoes_bearer_token_in_roster_warning(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config_path, settings = _make_config(Path(temporary).resolve(), members=[WRITER, READER])
            token = str(settings["token"])
            stderr_buf = io.StringIO()
            with redirect_stderr(stderr_buf), self.assertRaises(private.EncounterError) as caught:
                client.say(config_path, "hello", to=[token])
            self.assertEqual(caught.exception.code, "invalid_authored_record")
            self.assertEqual(caught.exception.field, "credential")
            self.assertNotIn(token, stderr_buf.getvalue())


if __name__ == "__main__":
    unittest.main()
