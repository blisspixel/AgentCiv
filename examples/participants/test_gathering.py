"""Bounded gathering mechanics and real HTTP/provider failure regressions."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from http.server import BaseHTTPRequestHandler
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration as client  # noqa: E402
import decision_loop as loop  # noqa: E402
import gathering  # noqa: E402
import local_participant as wire  # noqa: E402
from test_local_participant import HOST  # noqa: E402
from test_loop_integration import FakeOllama, response  # noqa: E402
import walk  # noqa: E402


class GatheringDecisions(unittest.TestCase):
    def test_all_choices_and_unicode_with_separate_publication_policy(self) -> None:
        for action, text in (("message", "Un lugar tranquilo. 欢迎一起想象。"), ("quiet", ""), ("leave", "")):
            decision = {"action": action, "text": text}
            self.assertEqual(gathering.validate_decision(json.dumps(decision)), decision)
        self.assertEqual(client.object_value(client.object_value(gathering.SCHEMA["properties"])["action"])["enum"],
                         ["message", "quiet", "leave"])

    def test_invalid_json_and_decisions_fail_without_coercion(self) -> None:
        invalid: list[object] = [None, [], {}, {"action": "message", "text": "ok", "extra": True},
            {"action": "quiet", "text": "explanation"}, {"action": "leave", "text": "reason"},
            {"action": "message", "text": " "}, {"action": "message", "text": 1},
            {"action": "message", "text": "x" * 401}, {"action": "message", "text": "a\u2014b"},
            {"action": "message", "text": "\ud800"}, {"action": "unknown", "text": ""},
            {"action": ["quiet"], "text": ""}]
        for value in invalid:
            with self.subTest(value=value), self.assertRaises(loop.InvalidDecision):
                gathering.validate_decision(json.dumps(value))
        for content in ('{', '{"action":"quiet","action":"leave","text":""}', '{"text":NaN}'):
            with self.assertRaises(loop.InvalidDecision):
                gathering.validate_decision(content)

    def test_conditions_do_not_assign_personas_or_require_social_outcome(self) -> None:
        self.assertIn("no assigned deliverable", gathering.OPEN)
        self.assertIn("Completion is optional", gathering.ASSIGNED)
        self.assertIn("No social or productivity score", gathering.COMMON)
        self.assertIn("not carried across processes", gathering.COMMON)
        self.assertIn("Neither quiet nor leave submits a civic decline", gathering.COMMON)
        self.assertIn("Leaving does not recall text another participant already received", gathering.COMMON)
        self.assertIn("truth, agreement, and authority stay separate", gathering.COMMON)
        self.assertEqual(gathering.CONSIDER, "Consider:\n\nTreat others with the consideration you would wish for yourself.\n\n"
            "Try to have compassion.\n\nConsider perspectives beyond your own.\n\nTrust with discernment.")


class GatheringProvider(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="agentciv-gathering-provider-")
        self.directory = Path(self.temporary.name)
        self.server = HOST.start_server(HOST.HostConfig(
            world_id="civ:gathering-provider", title="Gathering provider test", database_path=self.directory / "world.sqlite",
            listen=("127.0.0.1", 0), visibility="members", retention_seconds=60, max_payload_bytes=16384,
            credentials=(HOST.Credential("agent:writer", "gathering-fixture-token", True, True),)))
        self.origin: str = self.server.origin
        self.provider: FakeOllama | None = None
        self.config: client.JsonObject = {"origin": self.origin, "principal": "agent:writer", "token": "gathering-fixture-token",
            "recipients": ["agent:writer"], "record_id": "message:gathering-test", "mode": "scripted",
            "condition": "open", "round": 1, "consider": False, "return_invitation": False,
            "decision_attempts": 2, "attempt_journal": str(self.directory / "attempts.json")}

    def tearDown(self) -> None:
        if self.provider is not None:
            self.provider.close()
        self.server.shutdown()
        self.server.server_close()
        self.temporary.cleanup()

    def native(self, replies: list[client.JsonObject | None]) -> None:
        self.provider = FakeOllama(replies)
        self.config.update({"mode": "ollama", "model": "fixture:8b", "seed": 42, "ollama_origin": self.provider.origin})

    def history(self) -> list[client.JsonObject]:
        return client.history(self.origin, "gathering-fixture-token")

    def test_invalid_then_native_unicode_message_preserves_attempts_and_shared_budget(self) -> None:
        self.native([response({"action": "message", "text": ""}, 150),
                     response({"action": "message", "text": "欢迎分享一个想法。"}, 100)])
        self.config.update({"private_candidates": str(self.directory / "private"), "consider": True})
        result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "recorded")
        self.assertEqual(client.object_value(result["decision"])["text"], "欢迎分享一个想法。")
        self.assertEqual(len(self.history()), 1)
        if self.provider is None:
            raise AssertionError("provider missing")
        first, second = self.provider.requests
        self.assertEqual(first["format"], gathering.SCHEMA)
        self.assertEqual(second["format"], gathering.SCHEMA)
        self.assertEqual(client.object_value(second["options"])["num_predict"], 874)
        self.assertFalse(first["truncate"])
        self.assertFalse(first["shift"])
        self.assertNotIn("gathering-fixture-token", json.dumps(first))
        messages = first["messages"]
        if not isinstance(messages, list):
            raise AssertionError("missing native messages")
        prompt = client.object_value(messages[0])["content"]
        supplied = client.decode(str(prompt).encode())
        self.assertEqual(supplied["permitted_events"], [])
        self.assertEqual(supplied["optional_invitation"], gathering.CONSIDER)
        second_messages = second["messages"]
        if not isinstance(second_messages, list):
            raise AssertionError("missing feedback messages")
        self.assertTrue(str(client.object_value(second_messages[0])["content"]).startswith(str(prompt)))
        journal = client.decode((self.directory / "attempts.json").read_bytes())
        self.assertEqual(journal["charged_output_tokens"], 250)
        self.assertEqual(client.object_value(journal["publication"])["outcome"], "verified")
        self.assertEqual(len(list((self.directory / "private").glob("*.json"))), 2)
        provider = client.object_value(result["provider"])
        self.assertEqual(provider["calls"], 2)
        self.assertEqual(client.object_value(provider["model"])["digest"], "fixture-digest")

    def test_quiet_and_leave_are_not_publication_or_civic_decline(self) -> None:
        for index, (action, expected) in enumerate((("quiet", "quiet"), ("leave", "left"))):
            self.config.update({"scripted_action": action, "attempt_journal": str(self.directory / f"action-{index}.json")})
            result = gathering.participant(self.config)
            self.assertEqual(result["outcome"], expected)
            self.assertNotIn("receipt", result)
            self.assertNotIn("record", result)
        self.assertEqual(self.history(), [])

    def test_provider_failure_never_becomes_quiet_leave_or_scripted_message(self) -> None:
        self.native([None])
        result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["failure_stage"], "provider")
        self.assertEqual(result["failure_code"], "provider_failed")
        self.assertNotIn("decision", result)
        self.assertEqual(self.history(), [])

    def test_incomplete_usage_and_invalid_attempts_fail_closed(self) -> None:
        self.native([response({"action": "quiet", "text": ""}, complete=False)])
        result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["failure_code"], "provider_incomplete")
        self.assertNotIn("decision", result)
        self.assertEqual(self.history(), [])

    def test_default_one_attempt_and_optional_invitation_off(self) -> None:
        self.native([response({"action": "message", "text": ""}), response({"action": "quiet", "text": ""})])
        self.config.pop("decision_attempts")
        result = gathering.participant(self.config)
        self.assertEqual(result["failure_code"], "attempts_exhausted")
        if self.provider is None:
            raise AssertionError("provider missing")
        self.assertEqual(len(self.provider.requests), 1)
        messages = self.provider.requests[0]["messages"]
        if not isinstance(messages, list):
            raise AssertionError("missing prompt")
        prompt = client.decode(str(client.object_value(messages[0])["content"]).encode())
        self.assertIsNone(prompt["optional_invitation"])

    def test_reflected_provider_credential_is_not_saved_in_candidate_or_public_report(self) -> None:
        self.native([response({"action": "message", "text": "gathering-fixture-token"})])
        self.config["private_candidates"] = str(self.directory / "private")
        result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertNotIn("gathering-fixture-token", json.dumps(result))
        self.assertEqual(list((self.directory / "private").glob("*.json")), [])
        self.assertEqual(self.history(), [])

    def test_escaped_provider_credential_is_rejected_before_candidate_or_publication(self) -> None:
        reply = response({"action": "message", "text": "gathering-fixture-token"})
        message = client.object_value(reply["message"])
        message["content"] = str(message["content"]).replace("gathering-fixture-token", "\\u0067athering-fixture-token")
        reply["message"] = message
        self.native([reply])
        self.config["private_candidates"] = str(self.directory / "private")
        result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["failure_stage"], "provider")
        self.assertNotIn("decision", result)
        self.assertEqual(list((self.directory / "private").glob("*.json")), [])
        journal = client.decode((self.directory / "attempts.json").read_bytes())
        self.assertNotIn("gathering-fixture-token", json.dumps(journal))
        self.assertNotIn("gathering-fixture-token", json.dumps(result))
        self.assertEqual(client.object_value(journal["publication"])["outcome"], "not_attempted")
        self.assertEqual(self.history(), [])

    def test_malformed_model_json_still_receives_structural_feedback(self) -> None:
        malformed = response({"action": "message", "text": "first attempt"}, 100)
        malformed["message"] = {"content": '{"action":'}
        self.native([malformed, response({"action": "quiet", "text": ""}, 100)])
        result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "quiet")
        journal = client.object_value(result["decision_loop"])
        attempts = journal["attempts"]
        if not isinstance(attempts, list):
            raise AssertionError("attempts missing")
        self.assertEqual(client.object_value(attempts[0])["feedback_code"], "invalid_json")
        self.assertEqual(len(attempts), 2)
        self.assertEqual(self.history(), [])

    def test_escaped_provider_metadata_is_not_exposed_by_standalone_child(self) -> None:
        self.native([response({"action": "quiet", "text": ""})])
        if self.provider is None:
            raise AssertionError("provider missing")

        def respond(handler: BaseHTTPRequestHandler, status: int, body: client.JsonObject) -> None:
            if handler.path == "/api/tags":
                body = {"models": [{"name": "fixture:8b", "digest": "fixture-digest", "note": "gathering-fixture-token"}]}
            payload = json.dumps(body).replace("gathering-fixture-token", "\\u0067athering-fixture-token").encode()
            handler.send_response(status)
            handler.send_header("Content-Type", "application/json")
            handler.send_header("Content-Length", str(len(payload)))
            handler.end_headers()
            handler.wfile.write(payload)

        config_path = self.directory / "private-config.json"
        gathering.save(config_path, self.config)
        with patch.object(self.provider.server.RequestHandlerClass, "respond", new=respond):
            completed = subprocess.run([sys.executable, gathering.__file__, "--config", str(config_path)],
                capture_output=True, check=False, timeout=30)
        result = client.decode(completed.stdout)
        self.assertEqual(completed.returncode, 1)
        self.assertEqual(result["failure_stage"], "provider")
        self.assertEqual(result["failure_code"], "stage_failed")
        self.assertNotIn("provider", result)
        self.assertNotIn("gathering-fixture-token", json.dumps(result))
        self.assertFalse((self.directory / "attempts.json").exists())
        self.assertEqual(self.history(), [])

    def test_reflected_provider_metrics_are_not_exposed_by_standalone_child(self) -> None:
        reply = response({"action": "quiet", "text": ""})
        reply["total_duration"] = "gathering-fixture-token"
        self.native([reply])
        config_path = self.directory / "private-config.json"
        gathering.save(config_path, self.config)
        completed = subprocess.run([sys.executable, gathering.__file__, "--config", str(config_path)],
            capture_output=True, check=False, timeout=30)
        result = client.decode(completed.stdout)
        self.assertEqual(completed.returncode, 1)
        self.assertEqual(result["failure_code"], "provider_failed")
        self.assertEqual(client.object_value(result["provider"])["metrics"], {})
        self.assertNotIn("gathering-fixture-token", json.dumps(result))
        journal = client.decode((self.directory / "attempts.json").read_bytes())
        self.assertNotIn("gathering-fixture-token", json.dumps(journal))
        self.assertEqual(self.history(), [])

    def test_final_provider_guard_rejects_reflection_even_with_unprotected_adapter(self) -> None:
        self.provider = FakeOllama([response({"action": "quiet", "text": ""})],
            metadata={"note": "gathering-fixture-token"})
        self.config.update({"mode": "ollama", "model": "fixture:8b", "seed": 42, "ollama_origin": self.provider.origin})
        original = client.OllamaDecision

        def unprotected(origin: str, model: str, seed: int, trace: Path | None,
                        *, token: str | None = None) -> client.OllamaDecision:
            # Failure fixture for the final export guard; production passes the credential to the adapter.
            return original(origin, model, seed, trace)

        with patch.object(client, "OllamaDecision", side_effect=unprotected):
            result = gathering.participant(self.config)
        self.assertEqual(result["failure_code"], "credential_reflected")
        self.assertNotIn("provider", result)
        self.assertNotIn("gathering-fixture-token", json.dumps(result))
        self.assertEqual(self.history(), [])

    def test_ambiguous_receipt_fails_preserves_act_without_publication_retry(self) -> None:
        original = wire.exchange

        def changed(method: str, url: str, *, token: str | None = None, body: bytes | None = None) -> tuple[int, bytes]:
            status, payload = original(method, url, token=token, body=body)
            if method == "POST":
                receipt = client.decode(payload)
                receipt["event_id"] = "event:unreadable"
                payload = json.dumps(receipt).encode()
            return status, payload

        with patch.object(wire, "exchange", side_effect=changed):
            result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["failure_stage"], "publication")
        self.assertEqual(len(self.history()), 1)
        self.assertEqual(client.object_value(client.object_value(result["decision_loop"])["publication"])["outcome"], "verification_pending")

    def test_host_reflection_wrong_receipt_and_changed_source_are_rejected(self) -> None:
        original = wire.exchange
        for index, kind in enumerate(("token", "escaped_token", "world", "body")):
            self.config.update({"record_id": f"message:bad-{index}", "attempt_journal": str(self.directory / f"bad-{index}.json")})

            def changed(method: str, url: str, *, token: str | None = None, body: bytes | None = None) -> tuple[int, bytes]:
                status, payload = original(method, url, token=token, body=body)
                if method == "POST":
                    receipt = client.decode(payload)
                    if kind in ("token", "escaped_token"):
                        receipt["extra"] = "gathering-fixture-token"
                    if kind == "world":
                        receipt["world"] = "civ:other"
                    rendered = json.dumps(receipt)
                    if kind == "escaped_token":
                        rendered = rendered.replace("gathering-fixture-token", "\\u0067athering-fixture-token")
                    return status, rendered.encode()
                if kind == "body" and "/events" in url:
                    page = client.decode(payload)
                    events = page.get("events")
                    if isinstance(events, list) and events:
                        event = client.object_value(events[-1])
                        stored = client.object_value(client.object_value(event["body"])["message"])
                        stored["body"] = {"text": "changed"}
                        event["body"] = {"message": stored}
                        events[-1] = event
                    return status, json.dumps(page).encode()
                return status, payload

            with patch.object(wire, "exchange", side_effect=changed):
                result = gathering.participant(self.config)
            self.assertEqual(result["outcome"], "failed")
            self.assertNotIn("gathering-fixture-token", json.dumps(result))
            journal = client.decode((self.directory / f"bad-{index}.json").read_bytes())
            self.assertNotIn("gathering-fixture-token", json.dumps(journal))

    def test_integral_numeric_receipt_is_correlated_without_changing_message(self) -> None:
        original = wire.exchange

        def numeric(method: str, url: str, *, token: str | None = None, body: bytes | None = None) -> tuple[int, bytes]:
            status, payload = original(method, url, token=token, body=body)
            if method == "POST":
                receipt = client.decode(payload)
                receipt["sequence"] = float(str(receipt["sequence"]))
                return status, json.dumps(receipt).encode()
            return status, payload

        with patch.object(wire, "exchange", side_effect=numeric):
            result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "recorded")
        self.assertEqual(client.object_value(result["receipt"])["sequence"], 1.0)
        self.assertEqual(len(self.history()), 1)

    def test_configuration_and_history_bounds_precede_provider(self) -> None:
        for key, value in (("condition", "bad"), ("round", True), ("consider", 1), ("return_invitation", None)):
            result = gathering.participant({**self.config, key: value})
            self.assertEqual(result["outcome"], "failed")
            self.assertEqual(result["failure_stage"], "configuration")
        with patch.object(client, "history", return_value=[{"id": "event:long", "body": "x" * 7000}]):
            result = gathering.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertNotIn("decision", result)
        self.assertEqual(self.history(), [])


class GatheringRuns(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="agentciv-gathering-run-")
        self.directory = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_relative_output_from_other_working_directory_keeps_journals_with_report(self) -> None:
        name = "relative-" + self.directory.name
        previous = Path.cwd()
        checkout_path = gathering.ROOT / name
        self.assertFalse(checkout_path.exists())
        try:
            os.chdir(self.directory)
            report = gathering.run(Path(name), newcomer=False)
        finally:
            os.chdir(previous)
        output = self.directory / name
        self.assertTrue(report["mechanics_verified"], report)
        self.assertTrue((output / "report.json").is_file())
        self.assertEqual(len(list(output.glob("attempts-*.json"))), 4)
        self.assertFalse(checkout_path.exists())

    def test_scripted_actual_http_recurring_newcomer_restart_and_leaving(self) -> None:
        output = self.directory / "scripted"
        report = gathering.run(output, rounds=3, private_traces=True)
        self.assertEqual(report["outcome"], "completed")
        self.assertTrue(report["mechanics_verified"])
        self.assertTrue(report["restart_history_equal"])
        self.assertTrue(report["old_credentials_rejected"])
        self.assertFalse(report["source_changed_during_run"])
        observations = report["observations"]
        if not isinstance(observations, list):
            raise AssertionError("observations missing")
        self.assertEqual(len(observations), 7)
        self.assertEqual([client.object_value(value)["outcome"] for value in observations],
                         ["recorded", "quiet", "recorded", "left", "recorded", "recorded", "recorded"])
        self.assertEqual(client.object_value(observations[-1])["principal"], gathering.NEWCOMER)
        self.assertEqual(report["departed_from_schedule"], [gathering.RESIDENTS[1]])
        history = json.loads((output / "history.json").read_text(encoding="utf-8"))
        self.assertEqual(len(history), 5)
        self.assertTrue(all(event["kind"] == "message.recorded" for event in history))
        visible = client.object_value(observations[-1])["visible_event_ids"]
        if not isinstance(visible, list):
            raise AssertionError("visible IDs missing")
        self.assertEqual(len(visible), 4)
        with self.assertRaises(FileExistsError):
            gathering.run(output)

    def test_choices_configuration_rejects_a_newcomer(self) -> None:
        report = gathering.run(self.directory / "choices-newcomer", choices=True)
        self.assertEqual(report["outcome"], "failed")
        self.assertFalse(report["mechanics_verified"])
        self.assertEqual(report["failure_stage"], "configuration")
        self.assertIsNone(report.get("choice_record"))

    def test_choices_cli_rejects_a_newcomer(self) -> None:
        with patch.object(sys, "argv", ["gathering", "--choices", "--output", str(self.directory / "cli-out")]):
            with self.assertRaises(SystemExit) as raised:
                gathering.main()
        self.assertEqual(raised.exception.code, 2)

    def test_choices_history_preserves_proposals_decline_and_return(self) -> None:
        output = self.directory / "choices"
        report = gathering.run(output, choices=True, newcomer=False)
        self.assertEqual(report["outcome"], "completed", report)
        self.assertTrue(report["mechanics_verified"])
        self.assertTrue(report["restart_history_equal"])
        observations = report["observations"]
        if not isinstance(observations, list):
            raise AssertionError("observations missing")
        self.assertEqual([client.object_value(value)["outcome"] for value in observations],
                         ["recorded", "recorded", "left", "recorded", "recorded"])
        self.assertEqual([client.object_value(value)["principal"] for value in observations],
                         [gathering.RESIDENTS[0], gathering.RESIDENTS[1], gathering.RESIDENTS[0],
                          gathering.RESIDENTS[1], gathering.RESIDENTS[0]])
        history = json.loads((output / "history.json").read_text(encoding="utf-8"))
        authored = [(event["actor"], event["body"]["message"]["body"]["text"]) for event in history]
        self.assertEqual(authored, [
            (gathering.RESIDENTS[0], "I propose a walk."),
            (gathering.RESIDENTS[1], "I propose a song."),
            (gathering.RESIDENTS[1], "I decline the walk."),
            (gathering.RESIDENTS[0], "I am back. Both proposals are still open."),
        ])
        before = json.loads((output / "before-restart.json").read_text(encoding="utf-8"))
        self.assertEqual([(event["actor"], event["body"]["message"]["body"]["text"]) for event in before], authored[:2])
        record = client.object_value(report["choice_record"])
        self.assertIsNone(record["scores"])
        self.assertIsNone(client.object_value(record["decline"])["reputation"])
        self.assertFalse(record["local_leave_in_shared_history"])
        self.assertIn("who submitted", str(record["establishes"]))
        self.assertIn("free will", str(record["does_not_establish"]))
        self.assertIn("true", str(record["does_not_establish"]))
        self.assertIsNone(client.object_value(report["controls"])["scores"])
        self.assertEqual(report["departed_from_schedule"], [])

    def test_return_invitation_is_explicit_and_does_not_ignore_leave(self) -> None:
        def child(arguments: list[str], **kwargs: object) -> subprocess.CompletedProcess[bytes]:
            if "--config" not in arguments or str(Path(gathering.__file__)) not in arguments:
                raise AssertionError("unexpected subprocess")
            config = client.decode(Path(arguments[-1]).read_bytes())
            if config["principal"] == gathering.RESIDENTS[0] and config["round"] == 1:
                config["scripted_action"] = "leave"
            result = gathering.participant(config)
            return subprocess.CompletedProcess(arguments, 0 if result["outcome"] != "failed" else 1, json.dumps(result).encode(), b"")

        # Source identity also uses subprocess.run; keep it outside this narrow child fixture.
        with patch.object(gathering, "source_identity", return_value={"fixture": "stable"}), patch.object(subprocess, "run", side_effect=child):
            report = gathering.run(self.directory / "return", returning=True)
        self.assertEqual(report["outcome"], "completed")
        observations = report["observations"]
        if not isinstance(observations, list):
            raise AssertionError("missing observations")
        a = [client.object_value(value) for value in observations if client.object_value(value).get("principal") == gathering.RESIDENTS[0]]
        self.assertEqual(len(a), 2)
        self.assertEqual(a[0]["outcome"], "left")
        self.assertTrue(a[1]["return_invitation"])
        self.assertEqual(a[1]["round"], 2)
        self.assertNotIn(gathering.RESIDENTS[0], str(report["departed_from_schedule"]))

    def test_failed_child_retains_completed_observation_and_host_history(self) -> None:
        original = subprocess.run
        count = 0

        def child(arguments: list[str], **kwargs: object) -> subprocess.CompletedProcess[bytes]:
            nonlocal count
            if "--config" in arguments and str(Path(gathering.__file__)) in arguments:
                count += 1
                if count == 2:
                    return subprocess.CompletedProcess(arguments, 1, json.dumps({"outcome": "failed", "failure_stage": "provider",
                        "failure_code": "provider_failed"}).encode(), b"private arbitrary exception text")
            return original(arguments, **kwargs)  # type: ignore[call-overload, no-any-return]

        with patch.object(subprocess, "run", side_effect=child):
            report = gathering.run(self.directory / "failed")
        self.assertEqual(report["outcome"], "failed")
        self.assertFalse(report["mechanics_verified"])
        observations = report["observations"]
        self.assertIsInstance(observations, list)
        self.assertEqual(len(observations) if isinstance(observations, list) else 0, 2)
        self.assertEqual(len(json.loads((self.directory / "failed" / "history.json").read_text())), 1)
        self.assertNotIn("private arbitrary", json.dumps(report))

    def test_killed_child_and_nonzero_success_claim_fail_with_preserved_history(self) -> None:
        original = subprocess.run
        for index, failure in enumerate(("timeout", "nonzero")):
            count = 0

            def child(arguments: list[str], **kwargs: object) -> subprocess.CompletedProcess[bytes]:
                nonlocal count
                if "--config" in arguments and str(Path(gathering.__file__)) in arguments:
                    count += 1
                    if count == 2:
                        if failure == "timeout":
                            raise subprocess.TimeoutExpired(arguments, 180, output=b"private partial output")
                        return subprocess.CompletedProcess(arguments, 1, b'{"outcome":"quiet"}', b"private error")
                return original(arguments, **kwargs)  # type: ignore[call-overload, no-any-return]

            output = self.directory / f"process-{index}"
            with patch.object(subprocess, "run", side_effect=child):
                report = gathering.run(output)
            self.assertEqual(report["outcome"], "failed")
            self.assertFalse(report["mechanics_verified"])
            observations = report["observations"]
            if not isinstance(observations, list):
                raise AssertionError("missing attempts")
            self.assertEqual(len(observations), 2)
            self.assertEqual(client.object_value(observations[-1])["failure_stage"], "process")
            self.assertEqual(len(json.loads((output / "history.json").read_text())), 1)
            self.assertNotIn("private partial output", json.dumps(observations))
            self.assertNotIn("private error", json.dumps(observations))

    def test_all_quiet_is_valid_mechanics_without_a_social_success_claim(self) -> None:
        def child(arguments: list[str], **kwargs: object) -> subprocess.CompletedProcess[bytes]:
            config = client.decode(Path(arguments[-1]).read_bytes())
            config["scripted_action"] = "quiet"
            result = gathering.participant(config)
            return subprocess.CompletedProcess(arguments, 0, json.dumps(result).encode(), b"")

        with patch.object(gathering, "source_identity", return_value={"fixture": "stable"}), patch.object(subprocess, "run", side_effect=child):
            report = gathering.run(self.directory / "quiet", newcomer=False)
        self.assertTrue(report["mechanics_verified"])
        self.assertEqual(report["recorded_messages"], 0)
        self.assertNotIn("social_success", report)
        self.assertIsNone(client.object_value(report["controls"])["scores"])

    def test_scripted_rust_http_has_same_bounded_mechanics(self) -> None:
        report = gathering.run(self.directory / "rust", host="rust", newcomer=False)
        self.assertEqual(report["outcome"], "completed", report)
        self.assertTrue(report["restart_history_equal"])
        self.assertTrue(report["old_credentials_rejected"])
        self.assertTrue(report["prior_records_retained"])
        self.assertEqual(report["recorded_messages"], 2)

    def test_late_change_to_post_restart_record_vetoes_mechanics(self) -> None:
        original = client.history
        final_reads = 0

        def history(origin: str, token: str, *, expected_world: str | None = None) -> list[client.JsonObject]:
            nonlocal final_reads
            events = original(origin, token, expected_world=expected_world)
            if len(events) == 3:
                final_reads += 1
                if final_reads == 2:
                    message = client.object_value(client.object_value(events[1]["body"])["message"])
                    message["body"] = {"text": "changed after restart"}
                    events[1]["body"] = {"message": message}
            return events

        output = self.directory / "changed"
        with patch.object(client, "history", side_effect=history):
            report = gathering.run(output)
        self.assertEqual(report["outcome"], "failed")
        self.assertFalse(report["mechanics_verified"])
        retained = json.loads((output / "history.json").read_text(encoding="utf-8"))
        self.assertEqual(len(retained), 3)
        self.assertNotIn("changed after restart", json.dumps(retained))

    def test_late_cleanup_failure_and_source_drift_veto_mechanical_pass(self) -> None:
        original = walk.stop_host
        count = 0

        def stop(host: walk.RunningHost) -> None:
            nonlocal count
            count += 1
            original(host)
            if count == 2:
                raise RuntimeError("late stop failure")

        with patch.object(walk, "stop_host", side_effect=stop):
            report = gathering.run(self.directory / "late", newcomer=False)
        self.assertEqual(report["outcome"], "failed")
        self.assertFalse(report["mechanics_verified"])
        with patch.object(gathering, "source_identity", side_effect=[{"snapshot": "before"}, {"snapshot": "after"}]):
            drift = gathering.run(self.directory / "drift", newcomer=False)
        self.assertTrue(drift["source_changed_during_run"])
        self.assertFalse(drift["mechanics_verified"])
        self.assertEqual(drift["failure_code"], "source_changed_during_run")

    def test_invalid_configuration_and_failed_build_keep_condition_metadata(self) -> None:
        for index, changes in enumerate(({"rounds": True}, {"rounds": 4}, {"mode": "cloud"},
            {"mode": "ollama"}, {"host": "remote"}, {"newcomer": 1}, {"temp_root": gathering.ROOT})):
            output = self.directory / f"config-{index}"
            report = gathering.run(output, **changes)  # type: ignore[arg-type]
            self.assertEqual(report["outcome"], "failed")
            self.assertTrue((output / "report.json").is_file())
        with patch.object(walk, "rust_binary", side_effect=RuntimeError("private build output")):
            report = gathering.run(self.directory / "build", host="rust", condition="assigned")
        self.assertEqual(report["outcome"], "failed")
        self.assertEqual(client.object_value(report["controls"])["condition"], "assigned")
        self.assertNotIn("private build output", json.dumps(report))

    def test_cli_both_conditions_and_private_config_boundary(self) -> None:
        output = self.directory / "cli"
        completed = subprocess.run([sys.executable, gathering.__file__, "--output", str(output), "--no-newcomer"],
            capture_output=True, check=False, timeout=60)
        self.assertEqual(completed.returncode, 0, completed.stdout)
        summary = client.decode(completed.stdout)
        self.assertEqual(summary["outcomes"], ["completed", "completed"])
        self.assertIn("not a causal comparison", str(summary["interpretation"]))
        refused = subprocess.run([sys.executable, gathering.__file__, "--config", str(gathering.ROOT / "README.md")],
            capture_output=True, check=False, timeout=10)
        self.assertEqual(refused.returncode, 1)
        self.assertEqual(client.decode(refused.stdout)["failure_stage"], "configuration")


if __name__ == "__main__":
    unittest.main()
