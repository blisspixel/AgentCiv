"""Structural, provider-fixture, and real HTTP stock-checklist regressions."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration as client  # noqa: E402
import decision_loop as loop  # noqa: E402
import local_participant as wire  # noqa: E402
import mock_collaboration as mock  # noqa: E402
import mock_oracle as oracle  # noqa: E402
import walk  # noqa: E402
from test_local_participant import HOST  # noqa: E402
from test_loop_integration import FakeOllama, response  # noqa: E402
from test_mock_oracle import correct_decision, rows, stock_history  # noqa: E402


class StockDecisionTests(unittest.TestCase):
    def test_numbers_completeness_addition_and_authority_remain_semantic(self) -> None:
        events = stock_history()
        wrong = correct_decision()
        selected = rows(wrong)
        selected[1]["quantity"] = 999
        wrong.update(rows=selected, total=1033)
        for decision in (wrong, {**correct_decision(), "rows": [], "total": 0},
                         {**correct_decision(), "total": 40}):
            self.assertEqual(mock.validate_decision(json.dumps(decision), events), decision)
            self.assertFalse(oracle.evaluate(events, decision)["accepted"])
        self.assertTrue(oracle.evaluate(events, mock.validate_decision(json.dumps(correct_decision()), events))["accepted"])

    def test_structure_bounds_unicode_duplicates_and_visible_citations(self) -> None:
        events = stock_history()
        invalid: list[object] = [None, {}, {**correct_decision(), "extra": True},
                                {**correct_decision(), "text": "x" * 401},
                                {**correct_decision(), "text": "text\u2014text"},
                                {**correct_decision(), "rows": None},
                                {**correct_decision(), "rows": rows(correct_decision()) * 3},
                                {**correct_decision(), "total": True},
                                {**correct_decision(), "total": -1},
                                {**correct_decision(), "total": mock.MAX_INTEGER + 1}]
        for changed in ({"item": "glue"}, {"quantity": True}, {"quantity": -1},
                        {"quantity": mock.MAX_INTEGER + 1}, {"source_event_ids": ["event:hidden"]},
                        {"source_event_ids": ["event:nuts", "event:nuts"]},
                        {"source_event_ids": [1]}, {"source_event_ids": "event:nuts"}, {"extra": True}):
            decision = correct_decision()
            selected = rows(decision)
            selected[0].update(changed)
            decision["rows"] = selected
            invalid.append(decision)
        invalid.append({**correct_decision(), "rows": [None]})
        invalid.append({**correct_decision(), "source_event_ids": ["event:bolts", "event:washers"]})
        for value in invalid:
            with self.subTest(value_type=type(value).__name__):
                with self.assertRaises(loop.InvalidDecision):
                    mock.validate_decision(json.dumps(value), events)
        for content in ('{"action":"stop","action":"revise"}', '{"total":NaN}', '{'):
            with self.assertRaises(loop.InvalidDecision):
                mock.validate_decision(content, events)
        for action in ("stop", "object", "decline"):
            decision = {**correct_decision(), "action": action, "rows": [], "total": None}
            if action == "stop":
                decision.update(target_event_id="", source_event_ids=[])
                self.assertEqual(mock.validate_decision(json.dumps(decision), events), decision)
            else:
                with self.assertRaises(loop.InvalidDecision):
                    mock.validate_decision(json.dumps(decision), events)
        with self.assertRaises(loop.InvalidDecision):
            mock.validate_decision(json.dumps({**correct_decision(), "action": "stop", "source_event_ids": [], "rows": []}), events)

    def test_schema_limits_references_without_installing_quantity_answers(self) -> None:
        schema = mock.decision_schema(stock_history())
        properties = client.object_value(schema["properties"])
        row = client.object_value(client.object_value(properties["rows"])["items"])
        quantity = client.object_value(client.object_value(row["properties"])["quantity"])
        self.assertEqual(quantity["type"], "integer")
        self.assertNotIn("const", quantity)
        self.assertNotIn("enum", quantity)
        total = client.object_value(properties["total"])
        self.assertNotIn("const", total)
        self.assertNotIn("expected_total", json.dumps(schema))
        self.assertEqual(client.object_value(properties["target_event_id"])["enum"], [""])


class StockProviderTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="agentciv-stock-test-")
        self.directory = Path(self.temporary.name)
        self.server = HOST.start_server(HOST.HostConfig(
            world_id="civ:stock-test", title="Fictional stock test", database_path=self.directory / "world.sqlite",
            listen=("127.0.0.1", 0), visibility="members", retention_seconds=60, max_payload_bytes=16384,
            credentials=(HOST.Credential("agent:operator", "operator-fixture-token", True, True),
                         HOST.Credential("agent:writer", "writer-fixture-token", True, True))))
        self.origin: str = self.server.origin
        latest: dict[str, str] = {}
        for index, (item, quantity) in enumerate((("nuts", 12), ("bolts", 8), ("washers", 20),
                                                 ("bolts", 5), ("washers", 22)), 1):
            record: client.JsonObject = {"protocol_version": "0.1-draft", "type": "message",
                "id": f"message:fixture-{index}", "world": "civ:stock-test", "from": "agent:operator",
                "to": ["agent:writer"], "body": {"stock": {"item": item, "quantity": quantity}}}
            receipt = wire.record_message(self.origin, "operator-fixture-token", "agent:operator", ["agent:writer"],
                text="Fictional stock.", message_id=str(record["id"]), draft=lambda **unused: record)["receipt"]
            latest[item] = str(receipt["event_id"])
        self.events = client.history(self.origin, "writer-fixture-token")
        self.valid: client.JsonObject = {"action": "revise", "text": "Current fictional stock.",
            "target_event_id": "", "source_event_ids": list(latest.values()),
            "rows": [{"item": item, "quantity": quantity, "source_event_ids": [latest[item]]}
                     for item, quantity in (("nuts", 12), ("bolts", 5), ("washers", 22))], "total": 39}
        self.config: client.JsonObject = {"origin": self.origin, "principal": "agent:writer",
            "token": "writer-fixture-token", "recipients": ["agent:operator"],
            "record_id": "submission:writer", "mode": "ollama", "model": "fixture:8b", "seed": 42,
            "decision_attempts": 2, "attempt_journal": str(self.directory / "attempts.json")}
        self.provider: FakeOllama | None = None

    def tearDown(self) -> None:
        if self.provider is not None:
            self.provider.close()
        self.server.shutdown()
        self.server.server_close()
        self.temporary.cleanup()

    def serve(self, replies: list[client.JsonObject | None]) -> None:
        self.provider = FakeOllama(replies)
        self.config["ollama_origin"] = self.provider.origin

    def test_structurally_valid_wrong_stock_publishes_once_but_fails_acceptance(self) -> None:
        wrong = {**self.valid, "total": 40}
        self.serve([response(wrong), response(self.valid)])
        result = mock.participant(self.config)
        self.assertEqual(result["outcome"], "recorded")
        self.assertFalse(client.object_value(result["semantic_check"])["accepted"])
        self.assertEqual(result["decision"], wrong)
        self.assertIsNotNone(self.provider)
        if self.provider is not None:
            self.assertEqual(len(self.provider.requests), 1)
        history = client.history(self.origin, "writer-fixture-token")
        self.assertEqual(history[:-1], self.events)
        stored = client.object_value(client.object_value(history[-1]["body"])["artifact_revision"])
        self.assertEqual(client.object_value(stored["body"])["total"], 40)
        self.assertEqual(client.object_value(client.object_value(result["decision_loop"])["publication"])["outcome"], "verified")

    def test_fixed_structural_feedback_can_recover_without_semantic_feedback(self) -> None:
        invalid = {**self.valid, "target_event_id": "event:hidden"}
        self.serve([response(invalid, 250), response(self.valid, 100)])
        result = mock.participant(self.config)
        self.assertEqual(result["outcome"], "recorded")
        self.assertTrue(client.object_value(result["semantic_check"])["accepted"])
        journal = client.object_value(result["decision_loop"])
        self.assertFalse(journal["first_attempt_valid"])
        self.assertTrue(journal["eventual_valid"])
        self.assertEqual(journal["charged_output_tokens"], 350)
        self.assertEqual(len(client.history(self.origin, "writer-fixture-token")), len(self.events) + 1)
        if self.provider is None:
            raise AssertionError("provider missing")
        self.assertEqual(len(self.provider.requests), 2)
        self.assertEqual(self.provider.requests[0]["format"], self.provider.requests[1]["format"])
        self.assertNotIn("expected_total", json.dumps(self.provider.requests))
        self.assertNotIn("latest_quantities_match", json.dumps(self.provider.requests))

    def test_equivalent_json_integer_receipts_preserve_verified_publication(self) -> None:
        self.serve([response(self.valid)])
        actual = wire.exchange

        def integer_spelling(method: str, url: str, *, token: str | None = None,
                             body: bytes | None = None) -> tuple[int, bytes]:
            status, payload = actual(method, url, token=token, body=body)
            if method == "POST" and status == 200:
                receipt = client.decode(payload)
                receipt["sequence"] = float(str(receipt["sequence"]))
                receipt["revision"] = float(str(receipt["revision"]))
                payload = json.dumps(receipt).encode("utf-8")
            return status, payload

        with patch.object(wire, "exchange", side_effect=integer_spelling):
            result = mock.participant(self.config)
        self.assertEqual(result["outcome"], "recorded")
        self.assertTrue(client.object_value(result["semantic_check"])["accepted"])
        self.assertIsInstance(client.object_value(result["receipt"])["sequence"], float)
        self.assertEqual(client.object_value(client.object_value(result["decision_loop"])["publication"])["outcome"], "verified")
        self.assertEqual(client.history(self.origin, "writer-fixture-token")[:-1], self.events)

    def test_stop_publishes_nothing_and_is_not_a_useful_checklist(self) -> None:
        self.serve([response({"action": "stop", "text": "", "target_event_id": "", "source_event_ids": [],
                              "rows": [], "total": None})])
        result = mock.participant(self.config)
        self.assertEqual(result["outcome"], "stopped")
        self.assertEqual(client.object_value(result["semantic_check"])["outcome"], "not_authored")
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        self.assertNotIn("record", result)

    def test_provider_failure_is_fixed_failure_without_scripted_fallback(self) -> None:
        self.serve([None, response(self.valid)])
        result = mock.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["failure_stage"], "provider")
        self.assertNotIn("decision", result)
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        self.assertNotIn("writer-fixture-token", json.dumps(result))
        self.assertNotIn("operator-fixture-token", json.dumps(result))
        if self.provider is not None:
            self.assertEqual(len(self.provider.requests), 1)

    def test_spanish_and_chinese_words_survive_validation_and_actual_publication(self) -> None:
        texts = ["Inventario actual. Las fuentes originales respaldan cada cantidad.",
                 "当前库存。每项数量均引用原始来源。"]
        self.serve([response({**self.valid, "text": text}) for text in texts])
        for index, text in enumerate(texts):
            with self.subTest(text=text):
                config = {**self.config, "record_id": f"submission:language-{index}",
                          "attempt_journal": str(self.directory / f"language-{index}.json")}
                result = mock.participant(config)
                self.assertEqual(result["outcome"], "recorded")
                self.assertTrue(client.object_value(result["semantic_check"])["accepted"])
                last = client.history(self.origin, "writer-fixture-token")[-1]
                record = client.object_value(client.object_value(last["body"])["artifact_revision"])
                self.assertEqual(client.object_value(record["body"])["text"], text)
                self.assertEqual(client.object_value(result["decision"])["text"], text)
        if self.provider is not None:
            self.assertEqual(len(self.provider.requests), len(texts))

    def test_chinese_model_fixture_survives_a_separate_child_with_a_legacy_pipe_encoding(self) -> None:
        text = "当前库存。请核对原始记录。"
        self.serve([response({**self.valid, "text": text})])
        config_path = self.directory / "participant.json"
        config_path.write_text(json.dumps(self.config), encoding="utf-8")
        environment = {**os.environ, "PYTHONIOENCODING": "cp1252"}
        completed = subprocess.run([sys.executable, str(Path(mock.__file__)), "--config", str(config_path)],
                                   cwd=mock.ROOT, capture_output=True, timeout=30, env=environment, check=False)
        self.assertEqual(completed.returncode, 0)
        result = client.decode(completed.stdout)
        self.assertEqual(result["outcome"], "recorded")
        self.assertEqual(client.object_value(result["decision"])["text"], text)
        last = client.history(self.origin, "writer-fixture-token")[-1]
        record = client.object_value(client.object_value(last["body"])["artifact_revision"])
        self.assertEqual(client.object_value(record["body"])["text"], text)
        self.assertNotIn(b"writer-fixture-token", completed.stdout)


class StockProcessTests(unittest.TestCase):
    def test_cleanup_failure_after_correct_checklist_cannot_retain_success(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agentciv-stock-cleanup-") as temporary:
            output = Path(temporary) / "run"
            actual = walk.stop_host
            calls = 0

            def stopped_then_failed(process: walk.RunningHost) -> None:
                nonlocal calls
                calls += 1
                actual(process)
                if calls == 2:
                    raise walk.WalkFailure("private-cleanup-error")

            with patch.object(mock, "source_identity", return_value={"fixture_identity": True}), \
                    patch.object(walk, "stop_host", side_effect=stopped_then_failed):
                report = mock.run(output)
            self.assertEqual(report["outcome"], "failed")
            self.assertFalse(report["useful_result"])
            self.assertEqual(report["failure_code"], "stage_failed")
            self.assertNotIn("private-cleanup-error", json.dumps(report))
            self.assertEqual(client.decode((output / "report.json").read_bytes()), report)

    def test_changed_prior_record_vetoes_acceptance_even_if_latest_stock_is_correct(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agentciv-stock-preservation-") as temporary:
            output = Path(temporary) / "run"
            actual = client.history

            def altered_readback(origin: str, token: str, *, expected_world: str | None = None) -> list[client.JsonObject]:
                history = actual(origin, token, expected_world=expected_world)
                if any(event.get("actor") == mock.AUTHORS[2] and event.get("kind") == "artifact.recorded"
                       for event in history):
                    history[0]["changed_context"] = "altered after newcomer publication"
                return history

            with patch.object(mock, "source_identity", return_value={"fixture_identity": True}), \
                    patch.object(client, "history", side_effect=altered_readback):
                report = mock.run(output)
            self.assertEqual(report["outcome"], "failed")
            self.assertFalse(report["useful_result"])
            self.assertFalse(report["all_prior_records_retained"])
            observed = report["observations"]
            if not isinstance(observed, list):
                raise AssertionError("completed observations missing")
            self.assertTrue(client.object_value(client.object_value(observed[-1])["semantic_check"])["accepted"])
            self.assertEqual(client.decode((output / "report.json").read_bytes()), report)

    def test_scripted_processes_restart_rotate_grants_and_update_actual_http_artifact(self) -> None:
        for host in ("python", "rust"):
            with self.subTest(host=host), tempfile.TemporaryDirectory(prefix="agentciv-stock-process-") as temporary:
                output = Path(temporary) / "run"
                with patch.object(mock, "source_identity", return_value={"fixture_identity": True}):
                    report = mock.run(output, host=host)
                self.assertEqual(report["outcome"], "completed")
                self.assertTrue(report["useful_result"])
                self.assertTrue(report["restart_history_equal"])
                self.assertTrue(report["old_author_credentials_rejected"])
                self.assertTrue(report["all_prior_records_retained"])
                self.assertTrue(report["newcomer_write_verified"])
                observations = report["observations"]
                if not isinstance(observations, list):
                    raise AssertionError("observations missing")
                self.assertEqual([client.object_value(client.object_value(turn)["decision"])["total"]
                                  for turn in observations], [40, 37, 39])
                history = json.loads((output / "history.json").read_text(encoding="utf-8"))
                artifacts = [event for event in history if event["kind"] == "artifact.recorded"]
                self.assertEqual([event["actor"] for event in artifacts], list(mock.AUTHORS))
                final_record = client.object_value(client.object_value(artifacts[-1]["body"])["artifact_revision"])
                previous_record = client.object_value(client.object_value(artifacts[-2]["body"])["artifact_revision"])
                self.assertEqual(final_record["derived_from"], {
                    "from": previous_record["from"], "artifact_id": previous_record["artifact_id"],
                    "revision": previous_record["revision"]})
                self.assertEqual(len({str(client.object_value(client.object_value(event["body"])["artifact_revision"])["artifact_id"])
                                      for event in artifacts}), 1)
                self.assertEqual(report["external_spend_usd"], 0)
                self.assertFalse(client.object_value(report["controls"])["semantic_feedback_to_model"])
                self.assertNotIn('"token":', (output / "report.json").read_text(encoding="utf-8"))
                self.assertFalse(any(output.glob("private*")))
                self.assertEqual(client.decode((output / "report.json").read_bytes()), report)
                with self.assertRaises(FileExistsError):
                    mock.run(output, host=host)


if __name__ == "__main__":
    unittest.main()
