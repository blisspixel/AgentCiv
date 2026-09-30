"""Offline bundle binding and bounded native-model newcomer failure checks."""

from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import ClassVar
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import newcomer  # noqa: E402
import oracle  # noqa: E402
import decision_loop  # noqa: E402

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "examples" / "http-commons"))
import walk  # noqa: E402
JsonObject = dict[str, object]


def objects(value: object) -> list[JsonObject]:
    if not isinstance(value, list):
        raise AssertionError("expected object list")
    return [oracle.object_value(item) for item in value]


def plan(name: str = "after") -> JsonObject:
    return oracle.validate_plan(oracle.load_json(HERE / f"{name}.json"))


def decision(name: str = "after") -> JsonObject:
    return {"action": "continue", "reason": "I inspected the original sources.", "plan": plan(name)}


def stop() -> JsonObject:
    return {"action": "stop", "reason": "I choose to stop this exercise.", "plan": None}


def events() -> list[JsonObject]:
    return oracle.validate_events(oracle.load_json(HERE / "history.json"))


def bundle_bytes(records: list[JsonObject] | None = None) -> bytes:
    entries = []
    for event in events() if records is None else records:
        raw = json.dumps(event, separators=(",", ":"), ensure_ascii=False)
        entries.append({"event_id": event["id"], "record_utf8": raw,
                        "sha256": hashlib.sha256(raw.encode("utf-8")).hexdigest()})
    return json.dumps({"protocol_version": "0.1-draft", "type": "archive_bundle",
        "archive_version": "archive-bundle/0.1-draft", "world": "civ:inheritance-fixture",
        "audience": ["agent:newcomer"], "selection": "operator-selected", "completeness": "partial",
        "entries": entries}, ensure_ascii=False).encode("utf-8")


def inspected(raw: bytes) -> JsonObject:
    bundle = oracle.object_value(json.loads(raw))
    records = [oracle.object_value(json.loads(str(entry["record_utf8"])))
               for entry in objects(bundle["entries"])]
    return {"world": bundle["world"], "audience": bundle["audience"],
        "bundle_sha256": hashlib.sha256(raw).hexdigest(),
        "events": [{"event_id": event["id"], "kind": event["kind"], "sequence": event["sequence"]}
                   for event in records]}


def response(value: JsonObject, *, tokens: int = 40, complete: bool = True) -> JsonObject:
    return {"done": complete, "done_reason": "stop" if complete else "length",
        "message": {"content": json.dumps(value)}, "eval_count": tokens,
        "prompt_eval_count": 200, "total_duration": 100}


class FakeOllama:
    """An installed local provider fixture. No model or outside service is used."""

    def __init__(self, replies: list[JsonObject | None]) -> None:
        self.replies = replies
        self.requests: list[JsonObject] = []
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            def respond(self, status: int, body: JsonObject) -> None:
                raw = json.dumps(body).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

            def do_GET(self) -> None:
                if self.path == "/api/tags":
                    self.respond(200, {"models": [{"name": "fixture:8b", "digest": "fixture-digest"}]})
                elif self.path == "/api/version":
                    self.respond(200, {"version": "fixture-runtime"})
                else:
                    self.respond(404, {})

            def do_POST(self) -> None:
                request = oracle.object_value(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
                if self.path == "/api/show":
                    self.respond(200, {})
                elif self.path == "/api/chat":
                    fixture.requests.append(request)
                    reply = fixture.replies.pop(0) if fixture.replies else None
                    self.respond(503 if reply is None else 200, {} if reply is None else reply)
                else:
                    self.respond(404, {})

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


class DecisionValidationTests(unittest.TestCase):
    def test_wrong_but_structured_plan_is_valid_without_oracle_feedback(self) -> None:
        wrong = decision("before")
        with patch.object(oracle, "evaluate", side_effect=AssertionError("oracle must not guide validation")):
            self.assertEqual(newcomer.validate_decision(json.dumps(wrong)), wrong)
        self.assertFalse(oracle.evaluate(events(), plan("before"))["useful_continuation"])

    def test_stop_requires_null_plan_and_never_fabricates_continuation(self) -> None:
        self.assertEqual(newcomer.validate_decision(json.dumps(stop())), stop())
        invalid = {**stop(), "plan": plan()}
        with self.assertRaises(decision_loop.InvalidDecision) as caught:
            newcomer.validate_decision(json.dumps(invalid))
        self.assertEqual(caught.exception.code, "invalid_stop")

    def test_nonobject_malformed_or_ambiguous_json_is_recoverable(self) -> None:
        ambiguous = '{"action":"continue","action":"stop","reason":"I stop.","plan":null}'
        for content in ("{", "[]", "null", "1", ambiguous,
                        "[" * 1500 + "null" + "]" * 1500,
                        '{"action":"stop","reason":"I stop.","plan":NaN}'):
            with self.subTest(content=content), self.assertRaises(decision_loop.InvalidDecision) as caught:
                newcomer.validate_decision(content)
            self.assertEqual(caught.exception.code, "invalid_json")

    def test_fields_action_text_and_plan_types_are_strict(self) -> None:
        cases: list[tuple[JsonObject, str]] = [
            ({**stop(), "authority": "agent:prior"}, "invalid_fields"),
            ({"action": "stop", "reason": "I stop."}, "invalid_fields"),
            ({**stop(), "action": "publish"}, "invalid_action"),
            ({**stop(), "reason": True}, "invalid_text"),
            ({**stop(), "reason": " "}, "invalid_text"),
            ({**stop(), "reason": "x" * 401}, "invalid_text"),
            ({**stop(), "reason": chr(0xD800)}, "invalid_text"),
            ({**stop(), "reason": "I stop" + chr(0x2014) + "now."}, "invalid_text"),
            ({**decision(), "plan": None}, "invalid_fields"),
        ]
        malformed = decision()
        candidate = oracle.object_value(malformed["plan"])
        reader = oracle.object_value(candidate["reader"])
        reader["retain_declines"] = 1
        candidate["reader"] = reader
        malformed["plan"] = candidate
        cases.append((malformed, "invalid_fields"))
        for value, code in cases:
            with self.subTest(code=code), self.assertRaises(decision_loop.InvalidDecision) as caught:
                newcomer.validate_decision(json.dumps(value))
            self.assertEqual(caught.exception.code, code)


class BundleBindingTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name) / "bundle.json"
        self.raw = bundle_bytes()
        self.path.write_bytes(self.raw)
        self.view = inspected(self.raw)

    def test_intact_bundle_preserves_original_sources_and_detaches_input(self) -> None:
        with patch.object(newcomer, "archive", return_value=self.view):
            found, view = newcomer.load_bundle(Path("fixture-binary"), self.path)
        self.assertEqual(found, events())
        self.assertEqual(view, self.view)
        found[0]["body"] = {}
        self.assertEqual(self.path.read_bytes(), self.raw)

    def test_mutation_after_inspection_fails_even_with_recomputed_entry_hash(self) -> None:
        changed_events = events()
        first = oracle.object_value(changed_events[0]["body"])
        artifact = oracle.object_value(first["artifact_revision"])
        artifact["body"] = {"text": "Forged new content with unchanged event identity."}
        first["artifact_revision"] = artifact
        changed_events[0]["body"] = first

        def inspect_then_change(*args: object) -> JsonObject:
            self.path.write_bytes(bundle_bytes(changed_events))
            return self.view

        with patch.object(newcomer, "archive", side_effect=inspect_then_change), self.assertRaises(newcomer.NewcomerError) as caught:
            newcomer.load_bundle(Path("fixture-binary"), self.path)
        self.assertEqual(str(caught.exception), "bundle_changed")

    def test_missing_whole_bundle_hash_rejects_view_without_parsing_content(self) -> None:
        view = {key: value for key, value in self.view.items() if key != "bundle_sha256"}
        with patch.object(newcomer, "archive", return_value=view), self.assertRaises(newcomer.NewcomerError) as caught:
            newcomer.load_bundle(Path("fixture-binary"), self.path)
        self.assertEqual(str(caught.exception), "bundle_changed")

    def test_bad_entry_hash_is_rejected_even_if_wrapper_digest_matches(self) -> None:
        bundle = oracle.object_value(json.loads(self.raw))
        entries = objects(bundle["entries"])
        entries[0]["sha256"] = "0" * 64
        bundle["entries"] = entries
        raw = json.dumps(bundle).encode("utf-8")
        self.path.write_bytes(raw)
        with patch.object(newcomer, "archive", return_value=inspected(raw)), self.assertRaises(newcomer.NewcomerError) as caught:
            newcomer.load_bundle(Path("fixture-binary"), self.path)
        self.assertEqual(str(caught.exception), "bundle_changed")

    def test_view_world_audience_and_event_identity_must_match(self) -> None:
        for key, value in (("world", "civ:other"), ("audience", ["agent:other"]), ("events", [])):
            with self.subTest(field=key):
                view = {**self.view, key: value}
                with patch.object(newcomer, "archive", return_value=view), self.assertRaises(newcomer.NewcomerError) as caught:
                    newcomer.load_bundle(Path("fixture-binary"), self.path)
                self.assertEqual(str(caught.exception), "bundle_changed")

    def test_bundle_size_and_unavailable_path_fail_explicitly(self) -> None:
        with patch.object(newcomer, "archive", return_value=self.view), patch.object(newcomer, "MAX_BYTES", 10):
            with self.assertRaises(newcomer.NewcomerError) as caught:
                newcomer.load_bundle(Path("fixture-binary"), self.path)
        self.assertEqual(str(caught.exception), "bundle_too_large")
        self.path.unlink()
        with patch.object(newcomer, "archive", return_value=self.view), self.assertRaises(newcomer.NewcomerError) as caught:
            newcomer.load_bundle(Path("fixture-binary"), self.path)
        self.assertEqual(str(caught.exception), "bundle_unreadable")

    def test_archive_process_timeout_failure_and_bad_output_are_bounded(self) -> None:
        for error in (OSError("private-path"), subprocess.TimeoutExpired("fixture", 30)):
            with patch("newcomer.subprocess.run", side_effect=error), self.assertRaises(newcomer.NewcomerError) as caught:
                newcomer.archive(Path("fixture"), "inspect", self.path)
            self.assertEqual(str(caught.exception), "archive_process_failed")
        for code, stdout, expected in ((1, b"PRIVATE-CONTENT", "archive_rejected"),
                                       (0, b"{" , "invalid_archive_output"),
                                       (0, b"\xff", "invalid_archive_output")):
            completed = subprocess.CompletedProcess[bytes]([], code, stdout=stdout, stderr=b"private-detail")
            with patch("newcomer.subprocess.run", return_value=completed), self.assertRaises(newcomer.NewcomerError) as caught:
                newcomer.archive(Path("fixture"), "inspect", self.path)
            self.assertEqual(str(caught.exception), expected)
        completed = subprocess.CompletedProcess[bytes]([], 0, stdout=b"{}" * 30)
        with patch("newcomer.subprocess.run", return_value=completed), patch.object(newcomer, "MAX_BYTES", 20):
            with self.assertRaises(newcomer.NewcomerError) as caught:
                newcomer.archive(Path("fixture"), "inspect", self.path)
        self.assertEqual(str(caught.exception), "archive_rejected")


class NativeModelProcessTests(unittest.TestCase):
    binary: ClassVar[Path]

    @classmethod
    def setUpClass(cls) -> None:
        cls.binary = walk.build_rust_binaries("agentciv-archive")["agentciv-archive"]

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.bundle = self.directory / "bundle.json"
        self.bundle.write_bytes(bundle_bytes())

    def run_model(self, replies: list[JsonObject | None], *, attempts: int = 3,
                  private: bool = False) -> tuple[subprocess.CompletedProcess[str], JsonObject, list[JsonObject]]:
        fixture = FakeOllama(replies)
        output = self.directory / "output"
        command = [sys.executable, str(HERE / "newcomer.py"), "--archive-binary", str(self.binary),
            "--bundle", str(self.bundle), "--output", str(output), "--model", "fixture:8b",
            "--ollama-origin", fixture.origin, "--attempts", str(attempts)]
        if private:
            command.append("--private-traces")
        try:
            completed = subprocess.run(command, capture_output=True, text=True, timeout=30, check=False)
            report = oracle.object_value(json.loads((output / "report.json").read_text(encoding="utf-8")))
            return completed, report, copy.deepcopy(fixture.requests)
        finally:
            fixture.close()

    def journal(self) -> JsonObject:
        return oracle.object_value(json.loads((self.directory / "output" / "attempts.json").read_text(encoding="utf-8")))

    def test_correct_provider_decision_survives_real_archive_and_newcomer_process(self) -> None:
        completed, report, requests = self.run_model([response(decision())], private=True)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["outcome"], "continued")
        self.assertEqual(report["decision"], decision())
        self.assertTrue(oracle.evaluate(events(), oracle.object_value(decision()["plan"]))["useful_continuation"])
        self.assertEqual(len(requests), 1)
        request = requests[0]
        self.assertIs(request["truncate"], False)
        self.assertIs(request["shift"], False)
        self.assertIs(request["stream"], False)
        self.assertEqual(request["format"], newcomer.decision_schema())
        self.assertEqual(oracle.object_value(request["options"])["num_predict"], 1024)
        contents = objects(request["messages"])[0]["content"]
        source_prompt = oracle.object_value(json.loads(str(contents)))
        self.assertEqual(source_prompt["original_records"], events())
        self.assertEqual(set(source_prompt), {"task", "boundaries", "decision_schema", "original_records", "derived_view"})
        self.assertNotIn("useful_continuation", source_prompt)
        self.assertEqual(report["source_event_ids"], [event["id"] for event in events()])
        self.assertTrue((self.directory / "output" / "candidates-private" / "attempt-1.json").is_file())
        self.assertTrue((self.directory / "output" / "native-private.json").is_file())

    def test_wrong_plan_is_retained_once_then_fails_independent_oracle(self) -> None:
        completed, report, requests = self.run_model([response(decision("before")), response(decision())])
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["decision"], decision("before"))
        self.assertEqual(report["outcome"], "continued")
        self.assertEqual(len(requests), 1)
        self.assertFalse(oracle.evaluate(events(), oracle.object_value(decision("before")["plan"]))["useful_continuation"])
        self.assertFalse((self.directory / "output" / "native-private.json").exists())
        self.assertFalse((self.directory / "output" / "candidates-private").exists())

    def test_invalid_shape_can_be_corrected_locally_to_stop_with_shared_budget(self) -> None:
        invalid: JsonObject = {"done": True, "done_reason": "stop", "message": {"content": "[]"}, "eval_count": 64}
        completed, report, requests = self.run_model([invalid, response(stop(), tokens=16)])
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["outcome"], "stopped")
        self.assertEqual(report["decision"], stop())
        self.assertEqual(len(requests), 2)
        self.assertEqual(oracle.object_value(requests[1]["options"])["num_predict"], 960)
        journal = self.journal()
        attempts = objects(journal["attempts"])
        self.assertEqual(attempts[0]["feedback_code"], "invalid_json")
        self.assertEqual([item["outcome"] for item in attempts], ["invalid", "valid"])
        self.assertEqual(journal["publication"], {"outcome": "not_attempted"})
        self.assertNotIn("source_correctness", report)

    def test_provider_failure_never_uses_after_plan_as_fallback(self) -> None:
        completed, report, requests = self.run_model([None, response(decision())])
        self.assertEqual(completed.returncode, 1)
        self.assertEqual(report["outcome"], "failed")
        self.assertNotIn("decision", report)
        self.assertFalse((self.directory / "output" / "decision.json").exists())
        self.assertEqual(self.journal()["outcome"], "provider_failed")
        self.assertEqual(len(requests), 1)

    def test_truncation_missing_content_and_oversize_are_fatal_without_retry(self) -> None:
        cases = [(response(decision(), complete=False), "provider_incomplete"),
                 ({"done": True, "message": {}, "eval_count": 1}, "provider_failed"),
                 ({"done": True, "message": {"content": "x" * 16001}, "eval_count": 1}, "candidate_budget_exhausted")]
        for number, (reply, code) in enumerate(cases):
            with self.subTest(code=code):
                self.directory = Path(self.temporary.name) / f"case-{number}"
                self.directory.mkdir()
                completed, report, requests = self.run_model([reply, response(decision())])
                self.assertEqual(completed.returncode, 1)
                self.assertEqual(report["outcome"], "failed")
                self.assertNotIn("decision", report)
                self.assertEqual(self.journal()["outcome"], code)
                self.assertEqual(len(requests), 1)

    def test_missing_usage_consumes_allowance_without_another_model_call(self) -> None:
        invalid: JsonObject = {"done": True, "done_reason": "stop", "message": {"content": "{"}}
        completed, report, requests = self.run_model([invalid, response(decision())])
        self.assertEqual(completed.returncode, 1)
        self.assertEqual(report["outcome"], "failed")
        self.assertEqual(self.journal()["outcome"], "output_budget_exhausted")
        self.assertEqual(self.journal()["charged_output_tokens"], 1024)
        self.assertEqual(len(requests), 1)

    def test_changed_source_digest_is_rejected_before_provider_inventory_or_choice(self) -> None:
        bundle = oracle.object_value(json.loads(self.bundle.read_bytes()))
        entries = objects(bundle["entries"])
        entries[0]["record_utf8"] = str(entries[0]["record_utf8"]) + " "
        bundle["entries"] = entries
        self.bundle.write_text(json.dumps(bundle), encoding="utf-8")
        completed, report, requests = self.run_model([response(decision())])
        self.assertEqual(completed.returncode, 1)
        self.assertEqual(report["format"], "agentciv-offline-newcomer/0.1")
        self.assertEqual(report["outcome"], "failed")
        self.assertEqual(report["failure_stage"], "bundle_validation")
        self.assertEqual(report["failure_code"], "stage_failed")
        self.assertNotIn("decision", report)
        self.assertEqual(requests, [])
        self.assertFalse((self.directory / "output" / "attempts.json").exists())


if __name__ == "__main__":
    unittest.main()
