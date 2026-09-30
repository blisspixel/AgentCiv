"""Native provider and real host checks for optional participant decision feedback."""

from __future__ import annotations

import json
import sys
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration as client  # noqa: E402
import decision_loop as loop  # noqa: E402
import local_participant as wire  # noqa: E402
from test_collaboration import choice  # noqa: E402
from test_local_participant import HOST  # noqa: E402


class FakeOllama:
    """Serve installed-model metadata and queued native responses, without a model."""

    def __init__(self, replies: list[client.JsonObject | None]) -> None:
        self.replies = replies
        self.requests: list[client.JsonObject] = []
        oracle = self

        class Handler(BaseHTTPRequestHandler):
            def respond(self, status: int, body: client.JsonObject) -> None:
                payload = json.dumps(body).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def do_GET(self) -> None:
                if self.path == "/api/tags":
                    self.respond(200, {"models": [{"name": "fixture:8b", "digest": "fixture-digest"}]})
                elif self.path == "/api/version":
                    self.respond(200, {"version": "fixture-runtime"})
                else:
                    self.respond(404, {})

            def do_POST(self) -> None:
                request = client.decode(self.rfile.read(int(self.headers["Content-Length"])))
                if self.path == "/api/show":
                    self.respond(200, {})
                    return
                if self.path != "/api/chat":
                    self.respond(404, {})
                    return
                oracle.requests.append(request)
                reply = oracle.replies.pop(0) if oracle.replies else None
                self.respond(503 if reply is None else 200, {} if reply is None else reply)

            def log_message(self, fmt: str, *args: object) -> None:
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.origin = f"http://127.0.0.1:{self.server.server_address[1]}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)


def response(decision: client.JsonObject, tokens: int = 100, *, complete: bool = True) -> client.JsonObject:
    return {"done": complete, "done_reason": "stop" if complete else "length",
            "message": {"content": json.dumps(decision)}, "eval_count": tokens,
            "prompt_eval_count": 1000, "total_duration": 100}


class LoopIntegrationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.server = HOST.start_server(HOST.HostConfig(
            world_id="civ:decision-loop", title="Decision loop", database_path=self.directory / "world.sqlite",
            listen=("127.0.0.1", 0), visibility="members", retention_seconds=60, max_payload_bytes=8192,
            credentials=(HOST.Credential("agent:source", "source-fixture-token", True, True),
                         HOST.Credential("agent:writer", "writer-fixture-token", True, True))))
        self.origin: str = self.server.origin
        source_config: client.JsonObject = {
            "origin": self.origin, "principal": "agent:source", "token": "source-fixture-token",
            "recipients": ["agent:writer"], "record_id": "submission:source", "mode": "scripted",
        }
        client.participate(source_config, lambda prompt, events: choice())
        self.events = client.history(self.origin, "writer-fixture-token")
        self.source_id = str(self.events[0]["id"])
        self.config: client.JsonObject = {
            "origin": self.origin, "principal": "agent:writer", "token": "writer-fixture-token",
            "recipients": ["agent:source"], "record_id": "submission:writer", "mode": "ollama",
            "attempt_journal": str(self.directory / "attempts.json"),
            "private_candidates": str(self.directory / "candidates"), "decision_attempts": 2,
        }
        self.oracle: FakeOllama | None = None

    def tearDown(self) -> None:
        if self.oracle is not None:
            self.oracle.close()
        self.server.shutdown()
        self.server.server_close()
        self.temporary.cleanup()

    def decider(self, replies: list[client.JsonObject | None]) -> client.LoopDecision:
        self.oracle = FakeOllama(replies)
        self.config.update({"ollama_origin": self.oracle.origin, "model": "fixture:8b", "seed": 42})
        model = client.OllamaDecision(self.oracle.origin, "fixture:8b", 42, self.directory / "provider.json")
        return client.LoopDecision(self.config, model)

    def oracle_requests(self) -> list[client.JsonObject]:
        if self.oracle is None:
            raise AssertionError("provider fixture was not initialized")
        return self.oracle.requests

    def journal(self) -> client.JsonObject:
        return client.decode((self.directory / "attempts.json").read_bytes())

    def invalid_target(self) -> client.JsonObject:
        return {**choice("object", "event:invented"), "source_event_ids": [self.source_id]}

    def test_invalid_target_then_valid_uses_frozen_sources_and_publishes_once(self) -> None:
        invalid = self.invalid_target()
        valid = choice("revise", self.source_id)
        decider = self.decider([response(invalid, 250), response(valid, 120)])
        with patch.object(wire, "exchange", wraps=wire.exchange) as exchange:
            result = client.participate(self.config, decider)
        posts = [call for call in exchange.call_args_list if call.args[0] == "POST"]
        self.assertEqual(len(posts), 1)
        self.assertEqual(result["decision"], valid)
        self.assertEqual(result["visible_event_ids"], [self.source_id])
        archive = client.history(self.origin, "writer-fixture-token")
        self.assertEqual(archive[:-1], self.events)
        self.assertEqual(len(archive), 2)
        self.assertEqual(archive[-1]["actor"], "agent:writer")
        requests = self.oracle_requests()
        self.assertEqual(len(requests), 2)
        self.assertEqual(requests[0]["format"], requests[1]["format"])
        initial_message = requests[0]["messages"]
        followup_message = requests[1]["messages"]
        if not isinstance(initial_message, list) or not isinstance(followup_message, list):
            raise AssertionError("native messages are missing")
        initial = str(client.object_value(initial_message[0])["content"])
        followup = str(client.object_value(followup_message[0])["content"])
        self.assertTrue(followup.startswith(initial + "\nLocal validator feedback:\n"))
        snapshot = client.decode(initial.rsplit("\n", 1)[-1].encode("utf-8"))
        self.assertEqual(snapshot["permitted_events"], self.events)
        feedback = client.decode(followup.split("\nLocal validator feedback:\n", 1)[1].encode("utf-8"))
        self.assertEqual(feedback["code"], "invalid_target")
        self.assertEqual(client.decode(str(feedback["candidate"]).encode("utf-8")), invalid)
        journal = self.journal()
        self.assertFalse(journal["first_attempt_valid"])
        self.assertTrue(journal["eventual_valid"])
        self.assertEqual(client.object_value(journal["publication"])["outcome"], "verified")
        self.assertEqual(journal["charged_output_tokens"], 370)
        for number in (1, 2):
            self.assertTrue((self.directory / "candidates" / f"attempt-{number}.json").exists())
        self.assertTrue((self.directory / "provider.attempt-2.json").exists())

    def test_invalid_target_then_stop_publishes_nothing(self) -> None:
        decider = self.decider([response(self.invalid_target()), response(choice("stop"))])
        with patch.object(wire, "exchange", wraps=wire.exchange) as exchange:
            result = client.participate(self.config, decider)
        self.assertFalse(any(call.args[0] == "POST" for call in exchange.call_args_list))
        self.assertEqual(result["outcome"], "stopped")
        self.assertEqual(result["decision"], choice("stop"))
        self.assertIsNone(result["record"])
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        self.assertEqual(client.object_value(self.journal()["publication"])["outcome"], "stopped")

    def test_remaining_output_and_wall_time_are_shared_across_native_requests(self) -> None:
        decider = self.decider([response(self.invalid_target(), 300), response(choice("stop"), 20)])
        elapsed = 100.0
        timeouts: list[float] = []
        actual = client.ollama_exchange

        def exchange(origin: str, path: str, payload: client.JsonObject | None, timeout: float) -> client.JsonObject:
            nonlocal elapsed
            if path == "/api/chat":
                timeouts.append(timeout)
            result = actual(origin, path, payload, timeout)
            if path == "/api/chat":
                elapsed += 10
            return result

        with patch.object(time, "monotonic", side_effect=lambda: elapsed):
            with patch.object(client, "ollama_exchange", side_effect=exchange):
                client.participate(self.config, decider)
        requests = self.oracle_requests()
        self.assertEqual([client.object_value(request["options"])["num_predict"] for request in requests],
                         [1024, 724])
        self.assertEqual(timeouts, [120, 110])
        self.assertEqual(self.journal()["charged_output_tokens"], 320)

    def test_provider_failure_preserves_invalid_attempt_and_never_becomes_stop(self) -> None:
        decider = self.decider([response(self.invalid_target()), None, response(choice("stop"))])
        with patch.object(wire, "exchange", wraps=wire.exchange) as exchange:
            with self.assertRaises(client.ModelError):
                client.participate(self.config, decider)
        self.assertFalse(any(call.args[0] == "POST" for call in exchange.call_args_list))
        self.assertEqual(len(self.oracle_requests()), 2)
        journal = self.journal()
        self.assertEqual(journal["outcome"], "provider_failed")
        attempts = journal["attempts"]
        if not isinstance(attempts, list):
            raise AssertionError("attempt history is missing")
        self.assertEqual([client.object_value(attempt)["outcome"] for attempt in attempts],
                         ["invalid", "provider_failed"])
        self.assertFalse(journal["eventual_valid"])
        self.assertEqual(client.object_value(journal["publication"])["outcome"], "not_attempted")
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        public = (self.directory / "attempts.json").read_text(encoding="utf-8")
        self.assertNotIn("writer-fixture-token", public)
        self.assertNotIn("event:invented", public)

    def test_incomplete_provider_response_is_not_retried_or_published(self) -> None:
        decider = self.decider([response(choice("stop"), complete=False), response(choice("revise"))])
        with self.assertRaises(loop.LoopFailure) as caught:
            client.participate(self.config, decider)
        self.assertEqual(caught.exception.code, "provider_incomplete")
        self.assertEqual(len(self.oracle_requests()), 1)
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        self.assertFalse(self.journal()["eventual_valid"])

    def test_malformed_provider_message_remains_a_provider_failure(self) -> None:
        malformed: client.JsonObject = {"done": True, "done_reason": "stop", "message": []}
        decider = self.decider([malformed, response(choice("stop"))])
        with self.assertRaises(client.ModelError):
            client.participate(self.config, decider)
        self.assertEqual(len(self.oracle_requests()), 1)
        journal = self.journal()
        self.assertEqual(journal["outcome"], "provider_failed")
        self.assertFalse(journal["eventual_valid"])
        self.assertEqual(client.object_value(journal["publication"])["outcome"], "not_attempted")
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        trace = client.decode((self.directory / "provider.json").read_bytes())
        self.assertEqual(trace["response"], malformed)

    def test_default_valid_decision_uses_one_attempt_and_one_publication(self) -> None:
        self.config.pop("decision_attempts")
        valid = choice("revise", self.source_id)
        decider = self.decider([response(valid), response(choice("stop"))])
        result = client.participate(self.config, decider)
        self.assertEqual(result["decision"], valid)
        self.assertEqual(len(self.oracle_requests()), 1)
        journal = self.journal()
        self.assertEqual(client.object_value(journal["budget"])["attempts"], 1)
        self.assertTrue(journal["first_attempt_valid"])
        self.assertEqual(len(client.history(self.origin, "writer-fixture-token")), 2)

    def test_default_invalid_decision_cannot_use_a_second_candidate(self) -> None:
        self.config.pop("decision_attempts")
        decider = self.decider([response(self.invalid_target()), response(choice("stop"))])
        with self.assertRaises(loop.LoopFailure) as caught:
            client.participate(self.config, decider)
        self.assertEqual(caught.exception.code, "attempts_exhausted")
        self.assertEqual(len(self.oracle_requests()), 1)
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        self.assertEqual(client.object_value(self.journal()["publication"])["outcome"], "not_attempted")

    def test_accepted_post_with_lost_response_does_not_regenerate_or_republish(self) -> None:
        valid = choice("revise", self.source_id)
        decider = self.decider([response(valid), response(choice("stop"))])
        actual = wire.exchange
        posts = 0

        def lose_response(method: str, url: str, *, token: str | None = None,
                          body: bytes | None = None) -> tuple[int, bytes]:
            nonlocal posts
            result = actual(method, url, token=token, body=body)
            if method == "POST":
                posts += 1
                raise TimeoutError("response lost after the host recorded the act")
            return result

        with patch.object(wire, "exchange", side_effect=lose_response):
            with self.assertRaises(TimeoutError):
                client.participate(self.config, decider)
        self.assertEqual(posts, 1)
        self.assertEqual(len(self.oracle_requests()), 1)
        self.assertEqual(len(client.history(self.origin, "writer-fixture-token")), 2)
        publication = client.object_value(self.journal()["publication"])
        self.assertNotIn(publication["outcome"], ("verified", "stopped", "not_attempted"))
        self.assertEqual(client.object_value(publication["record"])["id"], "submission:writer")

    def test_readback_content_mismatch_does_not_claim_verified_publication(self) -> None:
        decider = self.decider([response(choice("revise", self.source_id)), response(choice("stop"))])
        actual = client.history

        def changed_readback(origin: str, token: str) -> list[client.JsonObject]:
            events = actual(origin, token)
            if len(events) > 1:
                artifact = client.object_value(client.object_value(events[-1]["body"])["artifact_revision"])
                body = client.object_value(artifact["body"])
                body["text"] = "Host returned different participant words."
                artifact["body"] = body
                events[-1]["body"] = {"artifact_revision": artifact}
            return events

        with patch.object(client, "history", side_effect=changed_readback):
            with self.assertRaises(client.DecisionError):
                client.participate(self.config, decider)
        self.assertEqual(len(self.oracle_requests()), 1)
        self.assertEqual(len(actual(self.origin, "writer-fixture-token")), 2)
        publication = client.object_value(self.journal()["publication"])
        self.assertNotIn(publication["outcome"], ("verified", "stopped", "not_attempted"))
        self.assertEqual(client.object_value(publication["record"])["id"], "submission:writer")

    def test_unicode_escaped_credential_in_receipt_never_reaches_public_journal(self) -> None:
        decider = self.decider([response(choice("revise", self.source_id))])
        actual = wire.exchange
        token = "writer-fixture-token"

        def reflected_receipt(method: str, url: str, *, token: str | None = None,
                              body: bytes | None = None) -> tuple[int, bytes]:
            status, payload = actual(method, url, token=token, body=body)
            if method == "POST":
                receipt = client.decode(payload)
                receipt["host_note"] = "writer-fixture-token"
                escaped = "".join(f"\\u{ord(character):04x}" for character in "writer-fixture-token")
                payload = json.dumps(receipt).replace("writer-fixture-token", escaped).encode("utf-8")
            return status, payload

        with patch.object(wire, "exchange", side_effect=reflected_receipt):
            with self.assertRaises(client.DecisionError):
                client.participate(self.config, decider)
        self.assertNotIn(token, (self.directory / "attempts.json").read_text(encoding="utf-8"))
        self.assertEqual(len(self.oracle_requests()), 1)
        self.assertEqual(len(client.history(self.origin, token)), 2)

    def test_invalid_config_is_rejected_before_world_or_model_calls(self) -> None:
        decider = self.decider([response(choice("stop"))])
        invalid: list[client.JsonObject] = [
            {"principal": 7}, {"token": True}, {"record_id": ["submission:bad"]},
            {"origin": {}}, {"decision_attempts": True}, {"decision_attempts": "2"},
            {"seed": "42"}, {"recipients": ["agent:source", "agent:source"]},
        ]
        for changed in invalid:
            with self.subTest(changed=changed):
                with patch.object(wire, "exchange") as exchange:
                    with self.assertRaises(client.DecisionError):
                        client.participate({**self.config, **changed}, decider)
                exchange.assert_not_called()
        self.assertEqual(self.oracle_requests(), [])
        self.assertFalse((self.directory / "attempts.json").exists())

    def test_duplicate_history_and_invalid_revision_fail_before_a_model_attempt(self) -> None:
        decider = self.decider([response(choice("stop"))])
        duplicate_pages: list[client.JsonObject] = [
            {"type": "event_page", "events": self.events, "has_more": True, "next_cursor": "cursor:next"},
            {"type": "event_page", "events": self.events, "has_more": False},
        ]
        with patch.object(wire, "read_page", side_effect=duplicate_pages):
            with self.assertRaises(client.DecisionError):
                client.participate(self.config, decider)
        for bad_revision in (True, 0, "1"):
            event = client.decode(json.dumps(self.events[0]).encode("utf-8"))
            artifact = client.object_value(client.object_value(event["body"])["artifact_revision"])
            artifact["revision"] = bad_revision
            event["body"] = {"artifact_revision": artifact}
            with self.subTest(revision=bad_revision):
                with patch.object(wire, "read_page", return_value={"events": [event], "has_more": False}):
                    with self.assertRaises(client.DecisionError):
                        client.participate(self.config, decider)
        self.assertEqual(self.oracle_requests(), [])
        self.assertFalse((self.directory / "attempts.json").exists())


if __name__ == "__main__":
    unittest.main()
