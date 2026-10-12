"""Private return navigation, conflicting observations and offline failure bounds."""
from __future__ import annotations

import copy
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import encounter_client as client
import encounter_private as private
import encounter_return as returns
import local_encounter as operator
import local_participant as wire
import walk
import test_local_encounter as fixtures

JsonObject = dict[str, object]


def view(events: list[JsonObject], *, later: bool = False) -> JsonObject:
    native = fixtures.native(events)
    return {"format": client.VIEW_FORMAT, "world": fixtures.WORLD, "principal": fixtures.A,
        "snapshot": native["snapshot"], "transport": native["report"], "offers": fixtures.projection(native),
        "retrieval_started": "2026-10-11T12:01:00+00:00" if later else "2026-10-11T12:00:00+00:00",
        "retrieval_finished": "2026-10-11T12:01:01+00:00" if later else "2026-10-11T12:00:01+00:00",
        "configured_budgets": client.BUDGETS, "copying_permission": "not_granted",
        "private_cache_permission": client.CACHE_PERMISSION, "freshness": "bounded_full_read_not_atomic"}


def message(sequence: int) -> JsonObject:
    return fixtures.event(fixtures.authored("message", f"message:return-{sequence}", fixtures.A), sequence)


class ReturnViewTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.config, self.settings, self.cache = fixtures.settings(Path(self.temporary.name).resolve())
        self.previous, self.current = self.cache / "view-before.json", self.cache / "view-after.json"

    def save(self, previous: JsonObject, current: JsonObject) -> None:
        private.write_new(self.previous, json.dumps(previous).encode())
        private.write_new(self.current, json.dumps(current).encode())

    def compare(self) -> JsonObject:
        with patch.object(client, "_binary", return_value=Path("native-reader")), \
            patch.object(client, "_project", return_value={}), \
            patch.object(client, "_native_read", side_effect=AssertionError("offline comparison must not read")), \
            patch.object(wire, "exchange", side_effect=AssertionError("offline comparison must not send")):
            return returns.changes(self.config, self.current, self.previous)

    def test_added_older_visible_absent_and_representation_changes_keep_originals(self) -> None:
        before = view([message(2), message(3), message(4)])
        after = view([message(1), message(2), message(4), message(5)], later=True)
        rows = fixtures.strings(fixtures.obj(after["snapshot"])["records"])
        rows[1] = rows[1].replace('"sequence": 2', '"sequence": 2.0')
        snapshot = after["snapshot"]
        assert isinstance(snapshot, dict)
        snapshot["records"] = rows
        self.save(before, after)
        digests = [hashlib.sha256(path.read_bytes()).hexdigest() for path in (self.previous, self.current)]
        result = self.compare()
        self.assertEqual([item["sequence"] for item in fixtures.objects(result["newly_visible"])], [1, 5])
        self.assertEqual(fixtures.objects(result["representation_changed"])[0]["sequence"], 2)
        self.assertEqual(fixtures.objects(result["not_in_current_observation"])[0]["sequence"], 3)
        self.assertEqual(result["unchanged_events"], 1)
        self.assertEqual(result["read_boundary_advanced"], False)
        self.assertEqual(result["origin_authenticated"], False)
        self.assertNotIn("unfinished question", json.dumps(result))
        self.assertEqual(digests, [hashlib.sha256(path.read_bytes()).hexdigest() for path in (self.previous, self.current)])
        self.assertEqual(set(path.name for path in self.cache.iterdir()), {self.previous.name, self.current.name, ".encounter.lock"})

    def test_unchanged_zero_and_integral_exponent_sequence_are_valid(self) -> None:
        before = view([message(0)])
        snapshot = before["snapshot"]
        assert isinstance(snapshot, dict)
        snapshot["records"] = [fixtures.strings(snapshot["records"])[0].replace('"sequence": 0', '"sequence": 0e0')]
        self.save(before, copy.deepcopy(before))
        result = self.compare()
        self.assertEqual(result["unchanged_events"], 1)
        self.assertEqual(result["representation_changed"], [])

    def test_conflicting_ids_and_sequences_refuse_comparison(self) -> None:
        for change in ("same_id_new_sequence", "same_sequence_new_id"):
            with self.subTest(change=change):
                before, after = view([message(1)]), view([message(2)], later=True)
                row = message(2)
                row["id" if change == "same_id_new_sequence" else "sequence"] = "event:manual-1" if change == "same_id_new_sequence" else 1
                after = view([row], later=True)
                self.save(before, after)
                with self.assertRaises(private.EncounterError) as caught:
                    self.compare()
                self.assertEqual(caught.exception.code, "conflicting_observations")
                self.previous.unlink()
                self.current.unlink()

    def test_interval_order_is_explicit_and_overlap_is_disclosed(self) -> None:
        before, after = view([]), view([], later=True)
        before["retrieval_finished"] = "2026-10-11T12:01:00.5+00:00"
        self.save(before, after)
        self.assertTrue(self.compare()["retrieval_intervals_overlap"])
        self.previous.unlink()
        self.current.unlink()
        for started, finished, code in (
            ("bad", "bad", "invalid_retrieval_interval"),
            ("2026-10-11T12:01:02Z", "2026-10-11T12:01:01Z", "invalid_retrieval_interval"),
            ("2026-10-11T12:01:00", "2026-10-11T12:01:01", "invalid_retrieval_interval"),
            ("2026-10-11T11:59:00Z", "2026-10-11T11:59:01Z", "reversed_observations"),
        ):
            with self.subTest(started=started):
                after = view([], later=True)
                after.update(retrieval_started=started, retrieval_finished=finished)
                self.save(view([]), after)
                with self.assertRaises(private.EncounterError) as caught:
                    self.compare()
                self.assertEqual(caught.exception.code, code)
                self.previous.unlink()
                self.current.unlink()

    def test_mismatch_partial_duplicate_corrupt_and_budget_failures_have_no_result(self) -> None:
        def mutate(value: JsonObject, case: str) -> None:
            if case in ("world", "principal"):
                value[case] = "different"
            elif case == "duplicate":
                value.update(view([message(1), message(1)], later=True))
            elif case in ("partial", "pages", "responses"):
                transport = value["transport"]
                assert isinstance(transport, dict)
                transport[{"partial": "reached_end", "pages": "pages", "responses": "response_bytes"}[case]] = False if case == "partial" else 9999999
            elif case == "events":
                value.update(view([message(number) for number in range(2049)], later=True))
            elif case == "record":
                value.update(view([{**message(1), "padding": "x" * 16001}], later=True))
            else:
                value["configured_budgets"] = {**client.BUDGETS, "seconds": True}
        for case in ("world", "principal", "duplicate", "partial", "pages", "responses", "events", "record", "budgets", "corrupt"):
            with self.subTest(case=case):
                after = view([message(1)], later=True)
                if case != "corrupt":
                    mutate(after, case)
                self.save(view([]), after)
                if case == "corrupt":
                    private.replace_private(self.current, b'{"format":')
                with self.assertRaises(private.EncounterError):
                    self.compare()
                self.previous.unlink()
                self.current.unlink()

    def test_reflected_escaped_credential_fails_before_native_process(self) -> None:
        record = message(1)
        record["extension"] = str(self.settings["token"])
        after = view([record], later=True)
        snapshot = after["snapshot"]
        assert isinstance(snapshot, dict)
        snapshot["records"] = [fixtures.strings(snapshot["records"])[0].replace("synthetic", "\\u0073ynthetic")]
        self.save(view([]), after)
        with patch.object(client, "_binary", return_value=Path("reader")), patch.object(client, "_project") as projected:
            with self.assertRaises(private.EncounterError) as caught:
                returns.changes(self.config, self.current, self.previous)
        self.assertEqual(caught.exception.code, "credential_reflected")
        projected.assert_called_once()  # Empty previous view only; secret-bearing current view is never passed.

    def test_output_bound_and_native_failure_do_not_emit_partial_comparison(self) -> None:
        self.save(view([]), view([message(1)], later=True))
        with patch.object(returns, "MAX_RESULT_BYTES", 1), self.assertRaises(private.EncounterError) as caught:
            self.compare()
        self.assertEqual(caught.exception.code, "return_view_too_large")
        with patch.object(client, "_binary", return_value=Path("reader")), \
            patch.object(client, "_project", side_effect=private.EncounterError("reader_failed")), self.assertRaises(private.EncounterError):
            returns.changes(self.config, self.current, self.previous)

    def test_selected_inspection_round_trips_exact_original_and_escapes_controls(self) -> None:
        source = message(1)
        source["extension"] = {"unknown": "\x1b[31m\u202e", "number": 7.5}
        saved = view([source, message(2)])
        self.save(saved, view([], later=True))
        original = fixtures.strings(fixtures.obj(saved["snapshot"])["records"])[0]
        with patch.object(wire, "exchange", side_effect=AssertionError("show is offline")):
            shown = client.show(self.config, self.previous, event_id="event:manual-1")
        quoted = shown.split("  record_utf8: ", 1)[1]
        self.assertEqual(json.loads(quoted), original)
        self.assertNotIn("\x1b", shown)
        self.assertNotIn("\u202e", shown)
        self.assertNotIn("event:manual-2", shown)
        with self.assertRaises(private.EncounterError) as caught:
            client.show(self.config, self.previous, event_id="absent")
        self.assertEqual(caught.exception.code, "event_not_in_saved_view")
        with patch.object(client, "MAX_OUTPUT", self.previous.stat().st_size), \
            patch.object(client, "_validate_view"), patch.object(client, "_originals", return_value=[original] * 2), \
            self.assertRaises(private.EncounterError):
            client.show(self.config, self.previous, event_id="event:manual-1")
        read_bounded = private.read_bounded
        with patch.object(client, "MAX_OUTPUT", 1), \
            patch.object(private, "read_bounded", side_effect=lambda path, maximum: read_bounded(path, client.MAX_CONFIG)), \
            self.assertRaises(private.EncounterError) as caught:
            client.show(self.config, self.previous, event_id="event:manual-1")
        self.assertEqual(caught.exception.code, "private_render_too_large")

    def test_cli_routes_offline_comparison_and_selected_inspection(self) -> None:
        with patch.object(sys, "argv", ["local_encounter.py", "changes", "current", "--previous", "previous", "--config", "config"]), \
            patch.object(returns, "changes", return_value={"outcome": "compared"}) as compared, patch("builtins.print"):
            self.assertEqual(operator.main(), 0)
        compared.assert_called_once_with(Path("config"), Path("current"), Path("previous"), reader_binary=None)
        with patch.object(sys, "argv", ["local_encounter.py", "show", "current", "--config", "config", "--event-id", "event:1"]), \
            patch.object(client, "show", return_value="saved") as shown, patch("builtins.print"):
            self.assertEqual(operator.main(), 0)
        shown.assert_called_once_with(Path("config"), Path("current"), event_id="event:1")


class NativeReturnValidationTests(unittest.TestCase):
    reader: Path

    @classmethod
    def setUpClass(cls) -> None:
        cls.reader = walk.build_rust_binaries("agentciv-reader")["agentciv-reader"]

    def test_native_event_validation_is_required_and_numeric_originals_survive(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config, _, cache = fixtures.settings(Path(temporary).resolve())
            previous, current = cache / "view-prior.json", cache / "view-current.json"
            private.write_new(previous, json.dumps(view([])).encode())
            after = view([message(0)], later=True)
            snapshot = after["snapshot"]
            assert isinstance(snapshot, dict)
            snapshot["records"] = [fixtures.strings(snapshot["records"])[0].replace('"sequence": 0', '"sequence": 0e0')]
            private.write_new(current, json.dumps(after).encode())
            with patch.object(wire, "exchange", side_effect=AssertionError("offline only")):
                result = returns.changes(config, current, previous, reader_binary=self.reader)
            self.assertEqual(fixtures.objects(result["newly_visible"])[0]["sequence"], 0)
            current.unlink()
            for corruption in ("wrong_actor", "negative_sequence", "duplicate_member", "bad_report"):
                invalid = view([message(1)], later=True)
                snapshot = invalid["snapshot"]
                assert isinstance(snapshot, dict)
                original = fixtures.strings(snapshot["records"])[0]
                if corruption == "wrong_actor":
                    original = original.replace('"actor": "agent:encounter-a"', '"actor": "agent:other"')
                elif corruption == "negative_sequence":
                    original = original.replace('"sequence": 1', '"sequence": -1')
                elif corruption == "duplicate_member":
                    original = original.replace('"sequence": 1', '"sequence": 1, "sequence": 1')
                else:
                    transport = invalid["transport"]
                    assert isinstance(transport, dict)
                    transport["cached_events"] = 1
                snapshot["records"] = [original]
                private.write_new(current, json.dumps(invalid).encode())
                with self.subTest(corruption=corruption), self.assertRaises(private.EncounterError):
                    returns.changes(config, current, previous, reader_binary=self.reader)
                current.unlink()


if __name__ == "__main__":
    unittest.main()
