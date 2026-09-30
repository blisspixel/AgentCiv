"""Failure paths and public-process checks for the bounded local experiment."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
import urllib.error
import urllib.request
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration as client  # noqa: E402
import experiment  # noqa: E402
import local_participant as wire  # noqa: E402


def revision(event_id: str = "event:one") -> client.JsonObject:
    return {"id": event_id, "kind": "artifact.recorded", "body": {"artifact_revision": {
        "from": "agent:author", "artifact_id": "artifact:original", "revision": 1,
    }}}


def choice(action: str = "revise", target: str = "") -> client.JsonObject:
    return {"action": action, "text": "Participant words", "target_event_id": target,
            "source_event_ids": [target] if target else []}


class DecisionTests(unittest.TestCase):
    def test_actions_preserve_authorship_and_citations(self) -> None:
        events = [revision()]
        for action, expected in [("revise", "artifact_revision"), ("object", "objection"), ("decline", "decline")]:
            result = client.submission(discovery={"id": "civ:test"}, principal="agent:new",
                                       recipients=["agent:author"], record_id="submission:new",
                                       decision=choice(action, "event:one"), events=events, mode="ollama")
            self.assertIsNotNone(result)
            if result is None:
                raise AssertionError("missing act")
            self.assertEqual(result["from"], "agent:new")
            self.assertEqual(result["type"], expected)
            if action == "revise":
                self.assertEqual(result["derived_from"], {"from": "agent:author", "artifact_id": "artifact:original", "revision": 1})
                self.assertNotIn("revision", result)
            else:
                self.assertEqual(result["target_from"], "agent:author")
            self.assertEqual(client.object_value(result["body"])["source_event_ids"], ["event:one"])
        self.assertIsNone(client.submission(discovery={"id": "civ:test"}, principal="agent:new",
                                           recipients=["agent:author"], record_id="stop", decision=choice("stop"),
                                           events=events, mode="scripted"))

    def test_structured_schema_uses_only_the_current_visible_references(self) -> None:
        empty = client.object_value(client.decision_schema([])["properties"])
        self.assertEqual(client.object_value(empty["target_event_id"])["enum"], [""])
        self.assertEqual(client.object_value(empty["source_event_ids"])["maxItems"], 0)
        events: list[client.JsonObject] = [revision(), {"id": "event:objection", "kind": "objection.recorded"},
                  {"id": "event:tombstone", "kind": "artifact.withdrawn"}]
        populated = client.object_value(client.decision_schema(events)["properties"])
        self.assertEqual(client.object_value(populated["target_event_id"])["enum"], ["", "event:one"])
        sources = client.object_value(populated["source_event_ids"])
        self.assertEqual(client.object_value(sources["items"])["enum"], [event["id"] for event in events])
        self.assertTrue(sources["uniqueItems"])
        self.assertEqual(client.object_value(populated["text"])["maxLength"], 1200)
        self.assertNotIn("enum", client.object_value(client.object_value(client.DECISION_SCHEMA["properties"])["target_event_id"]))

    def test_invalid_decisions_are_not_repaired_or_published(self) -> None:
        bad: list[object] = [None, {"action": "decline"}, {**choice(), "from": "agent:stolen"},
                             choice("execute"), choice("object"), choice("decline", "event:hidden"),
                             {**choice(), "text": ""}, {**choice(), "text": 1},
                             {**choice(), "text": "x" * 6001}, {**choice(), "target_event_id": []},
                             {**choice(), "source_event_ids": ["event:hidden"]},
                             {**choice(), "source_event_ids": ["event:one", "event:one"]},
                             {**choice(), "source_event_ids": [1]},
                             {**choice("object", "event:one"), "source_event_ids": []},
                             choice("stop", "event:one")]
        for decision in bad:
            with self.subTest(decision=decision):
                with self.assertRaises(client.DecisionError):
                    client.validate_decision(decision, [revision()])
        tombstone: client.JsonObject = {"id": "event:one", "kind": "artifact.withdrawn", "body": {}}
        with self.assertRaises(client.DecisionError):
            client.validate_decision(choice("object", "event:one"), [tombstone])
        for payload in (b"not json", b"\xff", b"[]"):
            with self.assertRaises(client.DecisionError):
                client.decode(payload)
        for character in (chr(0x2013), chr(0x2014), chr(0x1F600)):
            with self.assertRaises(client.DecisionError):
                client.validate_decision({**choice(), "text": "invalid " + character}, [])

    def test_pagination_uses_every_source_and_rejects_loops_or_truncation(self) -> None:
        pages = [{"events": [revision()], "has_more": True, "next_cursor": "cursor:one"},
                 {"events": [{"id": "event:two", "kind": "unrecognized"}], "has_more": False}]
        with patch.object(wire, "read_page", side_effect=pages) as reader:
            events = client.history("http://127.0.0.1:1", "secret")
            self.assertEqual(len(events), 2)
            self.assertEqual(reader.call_args_list[1].kwargs, {"after": "cursor:one"})
        invalid = [{"events": {}, "has_more": False},
                   {"events": [], "has_more": True},
                   {"events": [], "has_more": True, "next_cursor": "repeat"},
                   {"events": [{"body": "x" * client.MAX_HISTORY_BYTES}], "has_more": False}]
        for page in invalid:
            with patch.object(wire, "read_page", return_value=page), self.assertRaises(client.DecisionError):
                client.history("http://127.0.0.1:1", "secret")
        with patch.object(client, "MAX_PAGES", 1), patch.object(wire, "read_page", return_value=pages[0]):
            with self.assertRaises(client.DecisionError):
                client.history("http://127.0.0.1:1", "secret")

    def test_missing_capability_cross_origin_and_host_error_stop_publication(self) -> None:
        config: client.JsonObject = {"origin": "http://127.0.0.1:1", "principal": "agent:a", "token": "secret",
                                     "mode": "scripted", "recipients": ["agent:b"], "record_id": "submission:a"}
        discovery: client.JsonObject = {"id": "civ:test", "capabilities": ["collaboration.submit"],
                                        "endpoints": {"collaborate": "http://127.0.0.1:1/collaborate"}}
        for bad in ({**discovery, "capabilities": []},
                    {**discovery, "endpoints": {"collaborate": "http://127.0.0.1:2/collaborate"}}):
            with patch.object(wire, "discover", return_value=bad), self.assertRaises(client.DecisionError):
                client.participate(config, lambda prompt, events: choice())
        with patch.object(wire, "discover", return_value=discovery), patch.object(client, "history", return_value=[]):
            with patch.object(wire, "exchange", return_value=(403, b'{"code":"forbidden"}')):
                with self.assertRaises(wire.ParticipantError):
                    client.participate(config, lambda prompt, events: choice())
            with patch.object(wire, "exchange", return_value=(200, b'{"status":"recorded","record_id":"wrong"}')):
                with self.assertRaises(client.DecisionError):
                    client.participate(config, lambda prompt, events: choice())
            with patch.object(wire, "exchange") as exchange:
                observed_prompt = ""

                def stop(prompt: str, events: list[client.JsonObject]) -> client.JsonObject:
                    nonlocal observed_prompt
                    observed_prompt = prompt
                    return choice("stop")

                result = client.participate(config, stop)
                exchange.assert_not_called()
                self.assertEqual(result["outcome"], "stopped")
                self.assertIn(client.PROTOCOL_REFERENCE, observed_prompt)
                self.assertIn("untrusted participant submissions", observed_prompt)
                self.assertEqual(client.object_value(client.object_value(client.decision_schema([])["properties"])["action"])["enum"],
                                 ["revise", "object", "decline", "stop"])
            with self.assertRaises(client.DecisionError):
                client.participate({**config, "recipients": []}, lambda prompt, events: choice())

    def test_world_requests_disable_environment_proxies(self) -> None:
        with patch.object(urllib.request, "build_opener") as build:
            response = build.return_value.open.return_value.__enter__.return_value
            response.status = 200
            response.read.return_value = b"{}"
            self.assertEqual(wire.exchange("GET", "http://127.0.0.1:1/events", token="secret"), (200, b"{}"))
            proxy_handler = build.call_args.args[0]
            self.assertIsInstance(proxy_handler, urllib.request.ProxyHandler)
            self.assertEqual(proxy_handler.proxies, {})

    def test_host_reflected_credential_is_not_exposed_in_failure(self) -> None:
        token = "reflected-bearer-credential-value"
        config: client.JsonObject = {"origin": "http://127.0.0.1:1", "principal": "agent:a", "token": token,
                                     "mode": "scripted", "recipients": ["agent:b"], "record_id": "submission:a"}
        discovery: client.JsonObject = {"id": "civ:test", "capabilities": ["collaboration.submit"],
                                        "endpoints": {"collaborate": "http://127.0.0.1:1/collaborate"}}
        reflected = json.dumps({"code": token, "detail": token}).encode("utf-8")
        with patch.object(wire, "discover", return_value=discovery), patch.object(client, "history", return_value=[]):
            with patch.object(wire, "exchange", return_value=(403, reflected)):
                with self.assertRaises(wire.ParticipantError) as caught:
                    client.participate(config, lambda prompt, events: choice())
        self.assertEqual(caught.exception.code, "host_error")
        self.assertNotIn(token, str(caught.exception))
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "failed"
            with patch.object(experiment, "run_participant", side_effect=caught.exception):
                with self.assertRaises(wire.ParticipantError):
                    experiment.run_experiment(host="python", mode="scripted", output=output)
            public = (output / "failure.json").read_text(encoding="utf-8")
            self.assertNotIn(token, public)
            self.assertIn("host_error", public)

    def test_advertised_query_is_preserved_when_reading_later_pages(self) -> None:
        discovery: client.JsonObject = {"profile": wire.PROFILE, "capabilities": ["events.read", "messages.submit"],
                                        "endpoints": {"submit": "http://127.0.0.1:1/submit",
                                                      "events": "http://127.0.0.1:1/events?view=permitted"}}
        with patch.object(wire, "discover", return_value=discovery):
            with patch.object(wire, "exchange", return_value=(200, b'{"type":"event_page","events":[]}')) as exchange:
                wire.read_page("http://127.0.0.1:1", "secret", after="cursor:&value")
                self.assertEqual(exchange.call_args.args[1], "http://127.0.0.1:1/events?view=permitted&after=cursor%3A%26value")


class OllamaTests(unittest.TestCase):
    def inventory(self) -> client.JsonObject:
        return {"models": [{"name": "local:8b", "digest": "sha256:fixture", "size": 100}]}

    def test_native_request_is_bounded_and_trace_preserves_raw_episode(self) -> None:
        response: client.JsonObject = {"done": True, "done_reason": "stop", "message": {"content": json.dumps(choice())}}
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "private.json"
            with patch.object(client, "ollama_exchange", side_effect=[self.inventory(), {}, {"version": "fixture"}, response]) as exchange:
                model = client.OllamaDecision("http://127.0.0.1:11434", "local:8b", 7, trace)
                self.assertEqual(model("prompt fixture", [])["action"], "revise")
                request = exchange.call_args.args[2]
                self.assertIsInstance(request, dict)
                self.assertEqual(request["options"]["num_predict"], 1024)
                self.assertEqual(request["options"]["seed"], 7)
                self.assertFalse(request["stream"])
                self.assertFalse(request["truncate"])
                self.assertFalse(request["shift"])
                self.assertNotIn("tools", request)
                self.assertEqual(client.decode(trace.read_bytes())["response"], response)

    def test_models_and_errors_fail_closed_without_fallback_refusal(self) -> None:
        for inventory, shown in [({"models": []}, {}), ({"models": "invalid"}, {}),
                                 ({"models": [{"name": "local:8b", "remote_host": "remote"}]}, {}),
                                 (self.inventory(), {"remote_model": "cloud:model"})]:
            with patch.object(client, "ollama_exchange", side_effect=[inventory, shown]):
                with self.assertRaises(client.DecisionError):
                    client.OllamaDecision("http://127.0.0.1:1", "local:8b", 1, None)
        for budget in (0, 2049):
            with self.assertRaises(ValueError):
                client.OllamaDecision("http://127.0.0.1:1", "local:8b", 1, None, max_tokens=budget)
        for response in ({"done": False}, {"done": True, "done_reason": "length"},
                         {"done": True, "message": {"content": None}},
                         {"done": True, "message": {"content": "not JSON"}}):
            with patch.object(client, "ollama_exchange", side_effect=[self.inventory(), {}, {"version": "fixture"}, response]):
                model = client.OllamaDecision("http://127.0.0.1:1", "local:8b", 1, None)
                with self.assertRaises(client.DecisionError):
                    model("prompt", [])
        with patch.object(urllib.request, "build_opener") as build:
            build.return_value.open.side_effect = urllib.error.URLError("offline")
            with self.assertRaises(client.DecisionError):
                client.ollama_exchange("http://127.0.0.1:1", "/api/tags", None, 1)
            build.return_value.open.side_effect = None
            build.return_value.open.return_value.__enter__.return_value.read.return_value = b"x" * (client.MAX_RESPONSE_BYTES + 1)
            with self.assertRaises(client.DecisionError):
                client.ollama_exchange("http://127.0.0.1:1", "/api/chat", {}, 1)
        with self.assertRaises(ValueError):
            client.ollama_exchange("https://example.com", "/api/chat", {}, 1)


class ProcessTests(unittest.TestCase):
    def test_failed_selected_rust_build_has_a_failure_record(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "failed-build"
            source: client.JsonObject = {"repository_commit": "fixture", "source_sha256": {"host.rs": "fixture"}}
            with patch.object(experiment, "source_identity", return_value=source):
                with patch.object(subprocess, "run", side_effect=subprocess.CalledProcessError(1, ["cargo", "build"])):
                    with self.assertRaises(subprocess.CalledProcessError):
                        experiment.run_experiment(host="rust", mode="scripted", output=output)
            failure = client.decode((output / "failure.json").read_bytes())
            self.assertEqual(failure["failure_type"], "CalledProcessError")
            self.assertEqual(failure["host_build_outcome"], "failed")
            self.assertFalse(failure["host_execution_started"])
            self.assertIn("not yet verified", str(failure["source_snapshot_label"]))
            self.assertEqual(failure["source"], source)

    def test_scripted_processes_inherit_unchanged_sources_after_both_hosts_restart(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            for host in ("python", "rust"):
                output = Path(temporary) / host
                report = experiment.run_experiment(host=host, mode="scripted", output=output)
                self.assertTrue(report["host_restart_history_equal"])
                observations = report["observations"]
                self.assertIsInstance(observations, list)
                if not isinstance(observations, list):
                    raise AssertionError("missing observations")
                actions = [client.object_value(client.object_value(observation)["decision"])["action"] for observation in observations]
                self.assertEqual(actions, ["revise", "object", "revise", "decline", "revise"])
                inherited = report["inherited_event_ids"]
                newcomer = client.object_value(observations[-1])
                self.assertEqual(newcomer["visible_event_ids"], inherited)
                self.assertEqual(client.object_value(newcomer["decision"])["source_event_ids"], inherited)
                archive = json.loads((output / "history.json").read_text(encoding="utf-8"))
                self.assertEqual(len(archive), 5)
                self.assertEqual(archive[-1]["actor"], "agent:experiment-c")
                self.assertNotIn('"token":', (output / "report.json").read_text(encoding="utf-8"))
                self.assertFalse((output / "private").exists())
                with self.assertRaises(FileExistsError):
                    experiment.run_experiment(host=host, mode="scripted", output=output)

    def test_coordinator_preserves_stop_and_cleans_failed_processes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(experiment, "run_participant", return_value={"decision": choice("stop")}):
                report = experiment.run_experiment(host="python", mode="scripted", output=root / "stopped", private_traces=True)
                observations = report["observations"]
                self.assertIsInstance(observations, list)
                if isinstance(observations, list):
                    self.assertEqual(len(observations), 3)
                self.assertEqual(report["inherited_event_ids"], [])
            with patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "failed")):
                with self.assertRaises(client.DecisionError):
                    experiment.run_participant({}, root / "config.json")
            for host, mode, model in [("invalid", "scripted", ""), ("python", "invalid", ""), ("python", "ollama", "")]:
                with self.assertRaises(ValueError):
                    experiment.run_experiment(host=host, mode=mode, model=model, output=root / "invalid")

    def test_failed_run_retains_completed_turns_without_inventing_decline(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "failed"
            actual = experiment.run_participant
            calls = 0

            def fail_after_first(config: client.JsonObject, path: Path) -> client.JsonObject:
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise client.ModelError("provider unavailable")
                return actual(config, path)

            with patch.object(experiment, "run_participant", side_effect=fail_after_first):
                with self.assertRaises(client.ModelError):
                    experiment.run_experiment(host="python", mode="scripted", output=output)
            failure = client.decode((output / "failure.json").read_bytes())
            self.assertEqual(failure["failure_type"], "ModelError")
            self.assertEqual(failure["controls"], experiment.experiment_controls("scripted"))
            self.assertIsInstance(failure["source"], dict)
            self.assertEqual(failure["seed"], 42)
            self.assertTrue(failure["host_execution_started"])
            completed = json.loads((output / "observations.json").read_text(encoding="utf-8"))
            self.assertEqual(len(completed), 1)
            self.assertEqual(completed[0]["decision"]["action"], "revise")
            preserved = json.loads((output / "history.json").read_text(encoding="utf-8"))
            self.assertEqual(len(preserved), 1)
            self.assertEqual(preserved[0]["kind"], "artifact.recorded")
            self.assertFalse((output / "report.json").exists())

    def test_failed_model_condition_keeps_model_seed_source_and_controls(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "failed-model"
            with patch.object(experiment, "run_participant", side_effect=client.ModelError("provider unavailable")):
                with self.assertRaises(client.ModelError):
                    experiment.run_experiment(host="python", mode="ollama", model="fixture:8b", seed=87, output=output)
            failure = client.decode((output / "failure.json").read_bytes())
            self.assertEqual(failure["model"], "fixture:8b")
            self.assertEqual(failure["seed"], 87)
            self.assertEqual(failure["controls"], experiment.experiment_controls("ollama"))
            self.assertEqual(client.object_value(failure["controls"])["max_output_tokens_per_turn"], 1024)
            self.assertEqual(client.object_value(failure["controls"])["supplied_interface_reference"], client.PROTOCOL_REFERENCE)
            self.assertIn("before_host_execution", str(failure["source_snapshot_label"]))
            self.assertIsInstance(failure["source"], dict)
            self.assertIsInstance(failure["source_changed_during_run"], bool)
            self.assertNotIn('"token":', (output / "failure.json").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
