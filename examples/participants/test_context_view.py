"""Tests for context views across unequal reader budgets."""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "http-commons"))
import context_view as view
import reading_table as table
import encounter_client as client

JsonObject = dict[str, object]
FIXTURE = Path(__file__).resolve().parent / "fixtures" / "reading-table.json"


class ContextViewTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name) / "fixture.json"
        self.path.write_bytes(FIXTURE.read_bytes())
        self.raw, self.package, self.originals = table.load(self.path)

    def test_unconstrained_budget_complete_coverage(self) -> None:
        lens: JsonObject = {
            "name": "chronological",
            "author": "agent:test",
            "purpose": "unconstrained baseline",
            "version": "0.1-example",
            "byte_budget": None,
        }
        with patch.object(table, "validate"):
            result = view.inspect_view(self.path, lens, Path("reader-binary"))
        self.assertEqual(result["outcome"], "inspected")
        self.assertEqual(result["format"], view.FORMAT)
        accounting = table.obj(result["accounting"])
        self.assertEqual(accounting["coverage"], "complete")
        records = [table.obj(r) for r in table.rows(accounting["records"])]
        self.assertEqual(len(records), 7)
        self.assertTrue(all(r["status"] == "delivered_in_full" for r in records))
        events = [client._decode(original.encode("utf-8")) for original in self.originals]
        self.assertEqual([r["author"] for r in records], [event["actor"] for event in events])
        for event in events:
            self.assertIn(f"by {event['actor']}", view.record_summary(event))
        self.assertTrue(accounting["objection_present_in_observation"])
        self.assertTrue(accounting["objection_delivered_in_context"])
        self.assertEqual(result["group_state"], "not_inferred")
        self.assertEqual(result["consensus"], "not_evaluated")

    def test_tight_budget_causes_selection_coverage(self) -> None:
        # With 2500 bytes budget, earlier records fit in full while later records are linked
        lens: JsonObject = {
            "name": "chronological",
            "author": "agent:test",
            "purpose": "constrained test",
            "version": "0.1-example",
            "byte_budget": 2500,
        }
        with patch.object(table, "validate"):
            result = view.inspect_view(self.path, lens, Path("reader-binary"))
        self.assertEqual(result["outcome"], "inspected")
        accounting = table.obj(result["accounting"])
        self.assertEqual(accounting["coverage"], "selection")
        self.assertEqual(accounting["omission_reason"], "budget_constraint")
        records = [table.obj(r) for r in table.rows(accounting["records"])]
        statuses = {r["status"] for r in records}
        self.assertIn("delivered_in_full", statuses)
        self.assertIn("linked_only", statuses)
        # Even if linked or omitted, route remains intact
        for record in records:
            route = table.obj(record["route"])
            self.assertIn("event_id", route)
            self.assertIn("original_sha256", route)

    def test_lens_prioritization_passages_vs_questions(self) -> None:
        # Passage lens prioritizes source and passage notes over questions/objections
        passage_lens: JsonObject = {
            "name": "passages",
            "author": "agent:lens-maker",
            "purpose": "passage focus",
            "version": "0.1-example",
            "byte_budget": 1200,
        }
        question_lens: JsonObject = {
            "name": "open_questions",
            "author": "agent:lens-maker",
            "purpose": "question focus",
            "version": "0.1-example",
            "byte_budget": 1200,
        }
        with patch.object(table, "validate"):
            res_passages = view.inspect_view(self.path, passage_lens, Path("reader-binary"))
            res_questions = view.inspect_view(self.path, question_lens, Path("reader-binary"))

        self.assertEqual(res_passages["outcome"], "inspected")
        self.assertEqual(res_questions["outcome"], "inspected")
        # Both views are generated from the exact same observation without modifying originals
        self.assertEqual(res_passages["fixture_sha256"], res_questions["fixture_sha256"])
        # Delivered contexts differ
        self.assertNotEqual(res_passages["delivered_context"], res_questions["delivered_context"])

    def test_observation_boundary_truncation(self) -> None:
        # Sequence boundary cuts off later events without retroactively changing history
        lens: JsonObject = {
            "name": "chronological",
            "author": "agent:test",
            "purpose": "boundary test",
            "version": "0.1-example",
            "byte_budget": None,
        }
        with patch.object(table, "validate"):
            result = view.inspect_view(self.path, lens, Path("reader-binary"), boundary_sequence=2)
        self.assertEqual(result["outcome"], "inspected")
        obs = table.obj(result["observation"])
        self.assertEqual(obs["total_records"], 2)
        accounting = table.obj(result["accounting"])
        records = [table.obj(r) for r in table.rows(accounting["records"])]
        self.assertEqual(len(records), 2)
        # Later events like objection (sequence 3) are not in this observation
        self.assertFalse(accounting["objection_present_in_observation"])
        self.assertFalse(accounting["objection_delivered_in_context"])

    def test_invalid_lens_fails(self) -> None:
        lens: JsonObject = {
            "name": "unknown_lens",
            "author": "agent:test",
            "purpose": "bad",
            "version": "0.1-example",
            "byte_budget": None,
        }
        with patch.object(table, "validate"):
            result = view.inspect_view(self.path, lens, Path("reader-binary"))
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["code"], "invalid_lens")

    def test_cli_execution(self) -> None:
        lens: JsonObject = {
            "name": "chronological",
            "author": "agent:test",
            "purpose": "cli test",
            "version": "0.1-example",
            "byte_budget": None,
        }
        with patch.object(view, "inspect_view", return_value={"outcome": "inspected", "delivered_context": "ok"}):
            with patch.object(sys, "argv", ["context_view.py", "--fixture", str(self.path), "--reader-binary", "dummy"]):
                code = view.main()
                self.assertEqual(code, 0)


if __name__ == "__main__":
    unittest.main()
