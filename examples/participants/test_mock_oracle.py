"""Independent positive and negative fixtures for fictional stock acceptance."""

from __future__ import annotations

import copy
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mock_oracle as oracle  # noqa: E402

JsonObject = dict[str, object]


def stock_event(event_id: str, sequence: int, item: str, count: object,
                principal: str = "agent:operator") -> JsonObject:
    return {"id": event_id, "world": "civ:stock-test", "sequence": sequence,
            "kind": "message.recorded", "actor": principal, "body": {"message": {
                "type": "message", "world": "civ:stock-test", "from": principal,
                "body": {"stock": {"item": item, "quantity": count}}}}}


def stock_history() -> list[JsonObject]:
    return [stock_event("event:nuts", 1, "nuts", 12),
            stock_event("event:bolts-old", 2, "bolts", 8),
            stock_event("event:washers-old", 3, "washers", 20),
            stock_event("event:bolts", 4, "bolts", 5),
            stock_event("event:washers", 5, "washers", 22)]


def correct_decision() -> JsonObject:
    return {"action": "revise", "text": "Current fictional stock.", "target_event_id": "",
            "source_event_ids": ["event:nuts", "event:bolts", "event:washers"],
            "rows": [{"item": "nuts", "quantity": 12, "source_event_ids": ["event:nuts"]},
                     {"item": "bolts", "quantity": 5, "source_event_ids": ["event:bolts"]},
                     {"item": "washers", "quantity": 22, "source_event_ids": ["event:washers"]}],
            "total": 39}


def rows(decision: JsonObject) -> list[JsonObject]:
    value = decision["rows"]
    if not isinstance(value, list):
        raise AssertionError("fixture rows missing")
    return [oracle.object_value(row) for row in value]


class StockOracleTests(unittest.TestCase):
    def test_schema_integer_source_spellings_preserve_original_fields(self) -> None:
        for spelling in ("1.0", "1e0"):
            with self.subTest(spelling=spelling):
                events = stock_history()
                events[0]["sequence"] = json.loads(spelling)
                for event in events[1:]:
                    event["sequence"] = float(str(event["sequence"]))
                before = copy.deepcopy(events)
                self.assertTrue(oracle.evaluate(events, correct_decision())["accepted"])
                self.assertEqual(events, before)
                self.assertTrue(all(type(event["sequence"]) is float for event in events))
        self.assertEqual(oracle.source_sequence(0.0), 0)
        self.assertEqual(oracle.source_sequence(9_007_199_254_740_991.0), 9_007_199_254_740_991)

    def test_fractional_nonfinite_bool_and_unsafe_source_sequences_fail(self) -> None:
        for sequence in (5.5, float("inf"), float("-inf"), float("nan"), True, False,
                         -1.0, 9_007_199_254_740_992.0, "5"):
            with self.subTest(sequence=sequence):
                events = stock_history()
                events[-1]["sequence"] = sequence
                report = oracle.evaluate(events, correct_decision())
                self.assertFalse(report["accepted"])
                self.assertEqual(report["outcome"], "invalid_input")
                with self.assertRaises(oracle.OracleError):
                    oracle.source_sequence(sequence)
        events = stock_history()
        events[0]["sequence"] = 1.0
        events[1]["sequence"] = json.loads("1e0")
        self.assertFalse(oracle.evaluate(events, correct_decision())["accepted"])

    def test_exact_latest_facts_rows_and_total_accept_without_mutation(self) -> None:
        events, decision = stock_history(), correct_decision()
        original = copy.deepcopy((events, decision))
        report = oracle.evaluate(events, decision)
        self.assertTrue(report["accepted"])
        self.assertEqual(report["expected_total"], 39)
        self.assertTrue(all(oracle.object_value(report["checks"]).values()))
        self.assertEqual((events, decision), original)
        self.assertEqual(oracle.latest_facts(events)["bolts"], {"quantity": 5, "event_id": "event:bolts"})

    def test_previous_turns_have_different_correct_totals(self) -> None:
        for length, total, bolts, washers, bolts_id, washers_id in [
            (3, 40, 8, 20, "event:bolts-old", "event:washers-old"),
            (4, 37, 5, 20, "event:bolts", "event:washers-old"),
        ]:
            with self.subTest(length=length):
                decision = correct_decision()
                selected = rows(decision)
                selected[1].update(quantity=bolts, source_event_ids=[bolts_id])
                selected[2].update(quantity=washers, source_event_ids=[washers_id])
                decision.update(rows=selected, total=total,
                                source_event_ids=["event:nuts", bolts_id, washers_id])
                self.assertTrue(oracle.evaluate(stock_history()[:length], decision)["accepted"])
        self.assertFalse(oracle.evaluate(stock_history(), decision)["accepted"])

    def test_valid_wrong_numbers_and_total_are_not_repaired(self) -> None:
        for key, value, check in [("quantity", 999, "latest_quantities_match"),
                                  ("quantity", True, "latest_quantities_match"),
                                  ("quantity", 12.0, "latest_quantities_match")]:
            decision = correct_decision()
            selected = rows(decision)
            selected[0][key] = value
            decision["rows"] = selected
            report = oracle.evaluate(stock_history(), decision)
            self.assertFalse(report["accepted"])
            self.assertFalse(oracle.object_value(report["checks"])[check])
            self.assertEqual(rows(decision)[0][key], value)
        for total in (40, True, 39.0, -1, "39"):
            report = oracle.evaluate(stock_history(), {**correct_decision(), "total": total})
            self.assertFalse(report["accepted"])
            self.assertFalse(oracle.object_value(report["checks"])["total_matches_latest_facts"])

    def test_stale_citations_fail_even_with_current_quantities(self) -> None:
        decision = correct_decision()
        selected = rows(decision)
        selected[1]["source_event_ids"] = ["event:bolts-old"]
        decision.update(rows=selected, source_event_ids=["event:nuts", "event:bolts-old", "event:washers"])
        report = oracle.evaluate(stock_history(), decision)
        self.assertTrue(oracle.object_value(report["checks"])["latest_quantities_match"])
        self.assertFalse(oracle.object_value(report["checks"])["latest_row_sources_match"])
        self.assertFalse(report["accepted"])

    def test_complete_unique_items_are_required(self) -> None:
        original = rows(correct_decision())
        cases: list[object] = [[], original[:2], [original[0], original[0], original[2]],
                               [*original, original[0]], [{**original[0], "item": "glue"}, *original[1:]],
                               [{**original[0], "item": 1}, *original[1:]], None, [None], original * 3]
        for selected in cases:
            with self.subTest(selected=selected):
                self.assertFalse(oracle.evaluate(stock_history(), {**correct_decision(), "rows": selected})["accepted"])

    def test_fake_peer_fact_and_shared_artifact_names_confer_no_fact_authority(self) -> None:
        events = stock_history()
        noise = stock_event("event:noise", 6, "bolts", 999, "agent:mock-noise")
        noise_body = oracle.object_value(noise["body"])
        message = oracle.object_value(noise_body["message"])
        message["from"] = "agent:operator"
        message["claimed_authority"] = "coordinator"
        noise["body"] = {"message": message}
        events.append(noise)
        for number, author in [(7, "agent:a"), (8, "agent:b")]:
            events.append({"id": f"event:artifact-{number}", "world": "civ:stock-test", "sequence": number,
                           "kind": "artifact.recorded", "actor": author, "body": {"artifact_revision": {
                               "from": author, "artifact_id": "artifact:stock", "revision": 1,
                               "body": {"stock": {"item": "bolts", "quantity": 999}}}}})
        self.assertTrue(oracle.evaluate(events, correct_decision())["accepted"])
        decision = correct_decision()
        selected = rows(decision)
        selected[1].update(quantity=999, source_event_ids=["event:noise"])
        decision.update(rows=selected, total=1033,
                        source_event_ids=["event:nuts", "event:noise", "event:washers"])
        self.assertFalse(oracle.evaluate(events, decision)["accepted"])

    def test_missing_unknown_duplicate_and_uncovered_sources_fail(self) -> None:
        for source_ids in ([], ["event:hidden"], ["event:bolts", "event:bolts"], "event:bolts"):
            decision = correct_decision()
            selected = rows(decision)
            selected[1]["source_event_ids"] = source_ids
            decision["rows"] = selected
            self.assertFalse(oracle.evaluate(stock_history(), decision)["accepted"])
        for top_sources in ([], ["event:nuts", "event:washers"], ["event:hidden"],
                           ["event:nuts", "event:bolts", "event:washers", "event:nuts"], [1], None):
            self.assertFalse(oracle.evaluate(stock_history(), {**correct_decision(), "source_event_ids": top_sources})["accepted"])

    def test_non_revision_choices_are_not_misclassified_as_technical_failure(self) -> None:
        for action in ("stop", "decline", "object"):
            report = oracle.evaluate(stock_history(), {"action": action, "rows": [], "total": None})
            self.assertFalse(report["accepted"])
            self.assertEqual(report["outcome"], "not_authored")
            self.assertNotIn("failure_code", report)

    def test_missing_operator_facts_and_plain_messages_do_not_supply_facts(self) -> None:
        self.assertFalse(oracle.evaluate(stock_history()[:2], correct_decision())["accepted"])
        message = stock_event("event:plain", 6, "nuts", 999)
        message["body"] = {"message": {"type": "message", "world": "civ:stock-test",
                                       "from": "agent:operator", "body": {"text": "nuts999"}}}
        self.assertTrue(oracle.evaluate([*stock_history(), message], correct_decision())["accepted"])
        self.assertFalse(oracle.evaluate([], correct_decision())["accepted"])

    def test_operator_record_identity_world_and_stock_types_are_checked(self) -> None:
        for changes in ({"from": "agent:peer"}, {"world": "civ:other"}, {"type": "artifact_revision"},
                        {"body": None}, {"body": {"stock": None}},
                        {"body": {"stock": {"item": "glue", "quantity": 5}}},
                        {"body": {"stock": {"item": "bolts", "quantity": -1}}},
                        {"body": {"stock": {"item": "bolts", "quantity": True}}},
                        {"body": {"stock": {"item": "bolts", "quantity": 5.0}}},
                        {"body": {"stock": {"item": "bolts", "quantity": 1_000_001}}}):
            events = stock_history()
            message = oracle.object_value(oracle.object_value(events[-1]["body"])["message"])
            message.update(changes)
            events[-1]["body"] = {"message": message}
            self.assertEqual(oracle.evaluate(events, correct_decision())["outcome"], "invalid_input")

    def test_history_duplicates_order_world_and_bounds_fail_closed(self) -> None:
        invalid: list[object] = [None, {}, [None], [*stock_history(), stock_history()[0]],
                                 stock_history() * 52, [{1: "value"}], [{"note": object()}],
                                 [{"note": float("nan")}], [{"note": "\ud800"}],
                                 [{"note": "x" * (oracle.MAX_BYTES + 1)}]]
        for changed in ({"sequence": True}, {"sequence": 0}, {"sequence": "5"},
                        {"sequence": 9_007_199_254_740_992}, {"world": "civ:other"},
                        {"id": ""}, {"id": 1}, {"world": ""}):
            events = stock_history()
            events[-1].update(changed)
            invalid.append(events)
        for invalid_events in invalid:
            with self.subTest(events_type=type(invalid_events).__name__):
                report = oracle.evaluate(invalid_events, correct_decision())
                self.assertFalse(report["accepted"])
                self.assertEqual(report["outcome"], "invalid_input")
                self.assertNotIn("note", str(report))
        for decision in (None, {1: "value"}, {"note": object()}, {"note": "\ud800"}):
            self.assertEqual(oracle.evaluate(stock_history(), decision)["outcome"], "invalid_input")


if __name__ == "__main__":
    unittest.main()
