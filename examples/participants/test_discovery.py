"""Actual HTTP discovery, return, and denial without cached-view fallback."""

from __future__ import annotations

import json
import copy
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import discovery as discovery  # noqa: E402
import collaboration as client  # noqa: E402
import mock_collaboration as mock  # noqa: E402
import local_participant as wire  # noqa: E402
import reader_collaboration as reader  # noqa: E402
import walk as walk  # noqa: E402


class DiscoveryTests(unittest.TestCase):
    reader: Path
    host: Path

    @classmethod
    def setUpClass(cls) -> None:
        binaries = walk.build_rust_binaries("agentciv-reader", "agentciv-host")
        cls.reader = binaries["agentciv-reader"]
        cls.host = binaries["agentciv-host"]

    def test_pagination_withdrawal_restart_and_access_on_both_hosts(self) -> None:
        for host, visibility in (("python", "addressed"), ("rust", "addressed"),
                                 ("python", "members"), ("python", "sender_only"),
                                 ("rust", "members"), ("rust", "sender_only")):
            with self.subTest(host=host, visibility=visibility), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "run"
                report = discovery.run(output, host=host, visibility=visibility,
                    reader_binary=self.reader, host_binary=self.host)
                self.assertEqual(report["outcome"], "passed")
                self.assertTrue(report["old_credential_rejected"])
                self.assertTrue(report["source_unchanged"])
                self.assertEqual(len(str(client.object_value(report["binaries_sha256"])["reader"])), 64)
                first = client.object_value(report["first"])
                returned = client.object_value(report["return"])
                # Withdrawal replaces an old event rather than adding a tail entry.
                first_report = client.object_value(first["report"])
                return_report = client.object_value(returned["report"])
                for field in ("pages", "events", "reached_end", "scope"):
                    self.assertEqual(first_report[field], return_report[field])
                first_records = client.decode((output / "first-originals.json").read_bytes())["records"]
                return_records = client.decode((output / "return-originals.json").read_bytes())["records"]
                self.assertIsInstance(first_records, list)
                self.assertIsInstance(return_records, list)
                assert isinstance(first_records, list) and isinstance(return_records, list)
                original_events = [client.decode(str(record).encode("utf-8")) for record in first_records]
                returned_events = [client.decode(str(record).encode("utf-8")) for record in return_records]
                self.assertEqual([(event["id"], event["sequence"]) for event in original_events],
                    [(event["id"], event["sequence"]) for event in returned_events])
                if visibility != "sender_only":
                    offers = first["offers"]
                    assert isinstance(offers, list)
                    selected = next(client.object_value(offer) for offer in offers
                        if client.object_value(offer)["from"] == discovery.GUIDE)
                    sequence = selected["sequence"]
                    assert isinstance(sequence, int)
                    self.assertGreater(sequence, 100)
                    sources = selected["sources"]
                    assert isinstance(sources, list)
                    self.assertEqual(len(sources), 3)
                    self.assertEqual(sum(before != after for before, after in zip(first_records, return_records, strict=True)), 1)
                self.assertEqual(set(path.name for path in output.iterdir()), {
                    "first-originals.json", "first-view.json", "return-originals.json", "return-view.json",
                    "restricted-originals.json", "restricted-view.json", "report.json"})
                self.assertNotIn("token", (output / "report.json").read_text())

    def test_invalid_configuration_and_missing_inspection_fail(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            for host, visibility in (("other", "members"), ("python", "other")):
                with self.assertRaisesRegex(ValueError, "invalid configuration"):
                    discovery.run(path, host=host, visibility=visibility)
            with self.assertRaisesRegex(ValueError, "invalid visibility"):
                discovery.configure(path / "config.json", {}, "other")
            with self.assertRaises(FileExistsError):
                discovery.run(path, reader_binary=self.reader, host_binary=self.host)
        cases: list[discovery.JsonObject] = [{}, {"offers": []}, {"offers": [1]}]
        for first in cases:
            with self.subTest(first=first), self.assertRaises(ValueError):
                discovery.verify(first, first, "addressed")
        with self.assertRaisesRegex(ValueError, "sender_only_disclosed_offer"):
            discovery.verify({"offers": [{}]}, {"offers": []}, "sender_only")

    def test_projection_process_errors_do_not_reuse_cached_view_or_print_stderr(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            def snapshot(*args: object, **kwargs: object) -> tuple[list[discovery.JsonObject], discovery.JsonObject]:
                mock.save(path / "case-originals.json", {"world": discovery.WORLD, "records": [],
                    "copying_condition": "synthetic"})
                return [], {}
            for result in (subprocess.CompletedProcess([], 1, b"", b"secret-private-trace"),
                           subprocess.CompletedProcess([], 0, b"{}", b""),
                           subprocess.CompletedProcess([], 0, b"x" * 1_048_577, b""),
                           subprocess.CompletedProcess([], 0, json.dumps({"format": "agentciv-offer-view/0.1-example",
                               "world": discovery.WORLD, "untrusted": "private-token"}).encode(), b"")):
                with patch.object(reader, "read_snapshot", side_effect=snapshot), patch.object(subprocess, "run", return_value=result):
                    with self.assertRaises(ValueError) as error:
                        discovery.view(self.reader, "http://127.0.0.1:1", "private-token", path, "case")
                    self.assertNotIn("secret", str(error.exception))
                    self.assertFalse((path / "case-view.json").exists())
            for failure in (OSError("private-path"), subprocess.TimeoutExpired("private-command", 40)):
                with patch.object(reader, "read_snapshot", side_effect=snapshot), patch.object(subprocess, "run", side_effect=failure):
                    with self.assertRaisesRegex(ValueError, "offer_projection_process_failed"):
                        discovery.view(self.reader, "http://127.0.0.1:1", "private-token", path, "case")
            mock.save(path / "case-view.json", {"old_private_copy": "preserved"})
            with patch.object(reader, "read_snapshot") as requested:
                with self.assertRaisesRegex(ValueError, "offer_view_already_exists"):
                    discovery.view(self.reader, "http://127.0.0.1:1", "private-token", path, "case")
                requested.assert_not_called()
            self.assertEqual(client.decode((path / "case-view.json").read_bytes()), {"old_private_copy": "preserved"})

    def test_publication_rejects_wrong_origin_world_receipt_and_reflected_credentials(self) -> None:
        origin = "http://127.0.0.1:1"
        descriptor: discovery.JsonObject = {"id": discovery.WORLD, "endpoints": {"collaborate": origin + "/collaborate"}}
        receipt: discovery.JsonObject = {"record_id": "submission:test", "status": "recorded", "world": discovery.WORLD,
            "event_id": "event:test", "sequence": 1}
        with patch.object(wire, "discover", return_value=descriptor), patch.object(wire, "exchange",
            return_value=(200, json.dumps(receipt).encode())):
            self.assertEqual(discovery.publish(origin, "private-token", {"id": "submission:test"}), receipt)
        for field, value in (("world", "other"), ("record_id", "other"), ("status", "delivered"),
                             ("event_id", ""), ("sequence", True), ("untrusted", "private-token")):
            invalid = {**receipt, field: value}
            with patch.object(wire, "discover", return_value=descriptor), patch.object(wire, "exchange",
                return_value=(200, json.dumps(invalid).encode())):
                with self.assertRaisesRegex(ValueError, "invalid receipt"):
                    discovery.publish(origin, "private-token", {"id": "submission:test"})
        cases: list[discovery.JsonObject] = [{**descriptor, "id": "wrong-world"},
            {**descriptor, "endpoints": {"collaborate": "https://outside.invalid/private"}},
            {**descriptor, "endpoints": {"collaborate": False}}]
        for descriptor in cases:
            with patch.object(wire, "discover", return_value=descriptor), patch.object(wire, "exchange") as called:
                with self.assertRaisesRegex(ValueError, "invalid discovery"):
                    discovery.publish(origin, "private-token", {"id": "submission:test"})
                called.assert_not_called()

    def test_inspection_rejects_misidentified_sources_and_unrelated_objection(self) -> None:
        original: discovery.JsonObject = {"id": "event:source", "world": discovery.WORLD, "kind": "artifact.recorded",
            "body": {"artifact_revision": {"from": discovery.SOURCE, "artifact_id": "artifact:notes", "revision": 2}}}
        source: discovery.JsonObject = {"from": discovery.SOURCE, "revision": 2, "event_id": "event:source",
            "status": "available", "record_utf8": json.dumps(original)}
        objection: discovery.JsonObject = {"kind": "objection.recorded", "body": {"objection": {
            "target_from": discovery.GUIDE, "artifact_id": "artifact:parent"}}}
        selected: discovery.JsonObject = {"from": discovery.GUIDE, "sources": [source],
            "related_records": [{"record_utf8": json.dumps(objection)}]}
        first: discovery.JsonObject = {"offers": [selected], "report": {"pages": 2}}
        returned = copy.deepcopy(first)
        returned_selected = client.object_value(returned["offers"][0]) if isinstance(returned["offers"], list) else {}
        returned_sources = returned_selected["sources"]
        assert isinstance(returned_sources, list)
        returned_sources[0] = {**source, "status": "withdrawn", "record_utf8": json.dumps({**original,
            "kind": "artifact.withdrawn", "body": {}})}
        # client.object_value returns a copy, so store the modified nested view explicitly.
        returned["offers"] = [{**returned_selected, "sources": returned_sources}]
        discovery.verify(first, returned, "addressed")
        source_cases: list[tuple[discovery.JsonObject, str]] = [({"status": "unavailable"}, "source_freshness_failed"),
            ({"record_utf8": None}, "source_original_missing"),
            ({"record_utf8": json.dumps({**original, "id": "event:wrong"})}, "source_identity_failed"),
            ({"record_utf8": json.dumps({**original, "kind": "message.recorded"})}, "source_identity_failed")]
        for changes, message in source_cases:
            invalid = {**first, "offers": [{**selected, "sources": [{**source, **changes}]}]}
            with self.assertRaisesRegex(ValueError, message):
                discovery.verify(invalid, returned, "addressed")
        offer_cases: list[tuple[discovery.JsonObject, str]] = [({"from": "agent:other"}, "offered_scope_missing"),
            ({"sources": None}, "inspection_evidence_missing"), ({"related_records": []}, "objection_missing")]
        for changes, message in offer_cases:
            with self.assertRaisesRegex(ValueError, message):
                discovery.verify({**first, "offers": [{**selected, **changes}]}, returned, "addressed")
        with self.assertRaisesRegex(ValueError, "withdrawn_source_disclosed"):
            discovery.verify(first, {**returned, "offers": [{**selected, "sources": [{**source, "status": "withdrawn"}]}]}, "addressed")
        with self.assertRaisesRegex(ValueError, "restricted_material_disclosed"):
            discovery.verify({**first, "untrusted": "Restricted fixture sentinel"}, returned, "addressed")

    def test_cli_reports_fixed_failure(self) -> None:
        for failure in (ValueError("private"), client.DecisionError("private"), walk.WalkFailure("private")):
            with patch.object(sys, "argv", ["discovery.py", "--output", "unused"]), patch.object(discovery, "run", side_effect=failure), patch("builtins.print") as printed:
                self.assertEqual(discovery.main(), 1)
                self.assertEqual(json.loads(printed.call_args.args[0]), {"outcome": "failed", "code": "discovery_experiment_failed"})
        with patch.object(sys, "argv", ["discovery.py", "--output", "unused"]), patch.object(discovery, "run", return_value={"outcome": "passed"}), patch("builtins.print") as printed:
            self.assertEqual(discovery.main(), 0)
            self.assertEqual(json.loads(printed.call_args.args[0])["outcome"], "passed")


if __name__ == "__main__":
    unittest.main()
