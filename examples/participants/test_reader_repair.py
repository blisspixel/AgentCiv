"""A separate defective-parent repair and a portable independent evidence check."""

from __future__ import annotations

import copy
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration as client  # noqa: E402
import mock_collaboration as mock  # noqa: E402
import reader_collaboration as reader  # noqa: E402
import reader_evidence  # noqa: E402
import walk  # noqa: E402


def objects(value: object) -> list[client.JsonObject]:
    if not isinstance(value, list):
        raise AssertionError("expected object list")
    return [client.object_value(item) for item in value]


class ReaderRepairProcessTests(unittest.TestCase):
    reader_binary: Path
    host_binary: Path

    @classmethod
    def setUpClass(cls) -> None:
        binaries = walk.build_rust_binaries("agentciv-reader", "agentciv-host")
        cls.reader_binary = binaries["agentciv-reader"]
        cls.host_binary = binaries["agentciv-host"]

    def test_defective_actual_parent_is_repaired_with_visible_sources_on_both_hosts(self) -> None:
        for host in ("python", "rust"):
            with self.subTest(host=host), tempfile.TemporaryDirectory(prefix="agentciv-reader-repair-") as temporary:
                output = Path(temporary) / "run"
                report = reader.run(output, condition="repair", host=host,
                    reader_binary=self.reader_binary, host_binary=self.host_binary)
                self.assertEqual(report.get("failure_stage"), None, report)
                self.assertEqual(report["outcome"], "completed")
                self.assertEqual(report["condition"], "repair")
                self.assertTrue(report["qualified_improvement"])
                self.assertTrue(report["qualified_reader_repair"])
                self.assertTrue(report["first_to_successor_objective_gain"])
                self.assertTrue(report["exact_peer_artifact_derivation"])
                self.assertFalse(report["model_improvement_observed"])
                first, parent, successor = objects(report["checks"])
                self.assertFalse(first["passed"])
                self.assertFalse(parent["passed"])
                self.assertTrue(client.object_value(parent["sources"])["passed"])
                self.assertFalse(parent["reuse_passed"])
                self.assertEqual(len(mock.list_strings(parent["retrieved_event_ids"])), 100)
                self.assertTrue(successor["passed"])
                self.assertEqual(len(mock.list_strings(successor["retrieved_event_ids"])), 106)
                self.assertEqual(report["peer_target_check"], parent)

                study = client.decode((output / "study-originals.json").read_bytes())
                self.assertEqual(study["copying_condition"], "operator_authorized_synthetic_fixture_export_only")
                events = [client.decode(record.encode("utf-8")) for record in mock.list_strings(study["records"])]
                artifacts = [revision for event in events if (revision := client.artifact(event)) is not None]
                notes = [artifact for artifact in artifacts if artifact["from"] == reader.GUIDE]
                self.assertEqual([note["revision"] for note in notes], [1, 2])
                self.assertEqual(notes[1]["derived_from"], {"from": reader.GUIDE,
                    "artifact_id": notes[0]["artifact_id"], "revision": 1})
                self.assertNotEqual(notes[0]["body"], notes[1]["body"])
                actual_parent = next(event for event in events if event["actor"] == reader.AUTHORS[1])
                self.assertEqual(report["peer_target_event_id"], actual_parent["id"])
                peer_artifact = client.artifact(actual_parent)
                if peer_artifact is None:
                    raise AssertionError("parent artifact missing")
                observations = objects(report["observations"])
                self.assertEqual(observations[2]["derived_from"], {
                    "from": peer_artifact["from"], "artifact_id": peer_artifact["artifact_id"],
                    "revision": peer_artifact["revision"]})
                objections = [client.object_value(client.object_value(event["body"])["objection"])
                    for event in events if event["kind"] == "objection.recorded"]
                self.assertEqual([objection["id"] for objection in objections],
                    ["submission:reader-objection-a", "submission:reader-objection-b"])
                self.assertTrue(all("106" in str(client.object_value(objection["body"])["text"])
                    for objection in objections))
                objection_b = objections[1]
                self.assertEqual((objection_b["target_from"], objection_b["artifact_id"], objection_b["revision"]),
                    (peer_artifact["from"], peer_artifact["artifact_id"], peer_artifact["revision"]))
                self.assertIn(actual_parent["id"], mock.list_strings(client.object_value(objection_b["body"])["source_event_ids"]))
                self.assertEqual(report["unresolved_objection_event_ids"],
                    [event["id"] for event in events if event["kind"] == "objection.recorded"])
                self.assertEqual(client.object_value(report["study_transport"])["reached_end"], True)
                self.assertEqual(client.object_value(report["study_revalidation_transport"])["reached_end"], True)

                # A separate directory and a stopped host suffice to inspect this package.
                copied = Path(temporary) / "copied"
                copied.mkdir()
                for name in ("report.json", "study-originals.json", "challenge-originals.json"):
                    shutil.copyfile(output / name, copied / name)
                inspection = reader_evidence.inspect_package(copied)
                self.assertTrue(inspection["passed"], inspection)
                self.assertEqual(inspection["code"], "verified")
                self.assertTrue(inspection["qualified_improvement"])
                self.assertTrue(inspection["qualified_reader_repair"])
                self.assertEqual(client.decode((output / "report.json").read_bytes()), report)
                self.assertFalse(list(output.glob("*.sqlite")))
                self.assertFalse(list(output.glob("participant-*.json")))
                self.assertNotIn("token", json.dumps(study))

    def test_older_source_change_or_incomplete_study_revalidation_prevents_grading(self) -> None:
        actual = reader.read_snapshot
        for condition in ("older_source_changed", "traversal_incomplete"):
            with self.subTest(condition=condition), tempfile.TemporaryDirectory(prefix="agentciv-study-revalidation-") as temporary:

                def changed_view(binary: Path, origin: str, token: str, world: str, *, first: bool = False,
                                 originals_path: Path | None = None) -> tuple[list[client.JsonObject], client.JsonObject]:
                    events, transport = actual(binary, origin, token, world, first=first, originals_path=originals_path)
                    if world == reader.WORLD and originals_path is None:
                        if condition == "traversal_incomplete":
                            transport["reached_end"] = False
                        else:
                            events = copy.deepcopy(events)
                            # A withdrawal can replace an old event at the same sequence.
                            events[0]["kind"] = "artifact.withdrawn"
                            events[0]["body"] = {}
                    return events, transport

                output = Path(temporary) / "run"
                with patch.object(reader, "read_snapshot", side_effect=changed_view):
                    report = reader.run(output, condition="repair", reader_binary=self.reader_binary,
                        host_binary=self.host_binary)
                self.assertEqual(report["outcome"], "failed")
                self.assertEqual(report["failure_stage"], "study_evidence")
                self.assertFalse(report["useful_result"])
                self.assertFalse(report["qualified_improvement"])
                self.assertFalse((output / "checks.json").exists())


if __name__ == "__main__":
    unittest.main()
