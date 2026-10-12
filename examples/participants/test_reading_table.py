"""Frozen practice inspection, exact originals and conservative missing evidence."""
from __future__ import annotations

import copy
import json
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "http-commons"))
import reading_table as table
import encounter_client as client
import local_participant as wire
import walk

JsonObject = dict[str, object]
FIXTURE = Path(__file__).resolve().parent / "fixtures" / "reading-table.json"


def fixture() -> JsonObject:
    return client._decode(FIXTURE.read_bytes())


def entries(value: JsonObject) -> list[JsonObject]:
    return [table.obj(row) for row in table.rows(value["records"])]


def event(value: JsonObject, index: int) -> JsonObject:
    return client._decode(str(entries(value)[index]["original_utf8"]).encode("utf-8"))


def replace_event(value: JsonObject, index: int, changed: JsonObject) -> None:
    original = json.dumps(changed, ensure_ascii=False)
    item = entries(value)[index]
    item.update(original_utf8=original, sha256=table.digest(original.encode("utf-8")))


def practice(value: JsonObject, index: int = 1) -> JsonObject:
    body = table.obj(table.obj(table.artifact(event(value, index)))["body"])
    return table.obj(body["reading_table_practice"])


class ReadingTableTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name) / "fixture.json"
        self.value = fixture()

    def save(self, value: JsonObject | None = None) -> None:
        self.path.write_bytes(table.encoded(self.value if value is None else value))

    def inspect(self, event_id: str | None = None) -> JsonObject:
        before = self.path.read_bytes()
        siblings = sorted(path.name for path in self.path.parent.iterdir())
        with patch.object(table, "validate"), patch.object(wire, "exchange", side_effect=AssertionError("no HTTP")), \
            patch.object(wire, "discover", side_effect=AssertionError("no discovery")):
            result = table.inspect(self.path, Path("selected-reader"), event_id)
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(sorted(path.name for path in self.path.parent.iterdir()), siblings)
        return result

    def test_two_materially_different_authored_representations_and_attributed_choices(self) -> None:
        self.save()
        result = self.inspect()
        self.assertEqual(result["outcome"], "inspected")
        practices = [table.obj(row) for row in table.rows(result["practices"])]
        self.assertEqual([row["organizes_by"] for row in practices], ["passages", "open_questions"])
        self.assertEqual([row["state"] for row in practices], ["unfinished", "unfinished"])
        self.assertEqual(set(table.obj(table.rows(practices[0]["contribution"])[0])), {"passage_id", "note"})
        self.assertEqual(set(table.obj(table.rows(practices[1]["contribution"])[0])), {"question", "passage_ids"})
        first, second = (table.obj(row["selector"]) for row in practices)
        self.assertEqual(first["artifact_id"], second["artifact_id"])
        self.assertNotEqual(first["from"], second["from"])
        self.assertEqual(table.obj(practices[1]["basis"])["event_id"], first["event_id"])
        self.assertEqual([table.obj(row)["type"] for row in table.rows(result["responses"])], ["objection", "decline"])
        self.assertEqual([table.obj(row)["from"] for row in table.rows(result["attributed_adoption_statements"])],
            ["agent:adopter-passages", "agent:adopter-questions"])
        self.assertEqual(result["group_state"], "not_inferred")
        self.assertEqual(result["quiet_or_no_activity"], "not_inferred_from_absence")
        self.assertFalse(result["origin_authenticated"])

    def test_producer_success_flags_and_invented_group_decisions_do_not_control_findings(self) -> None:
        self.save()
        before = self.inspect()
        self.value["producer_report"] = {"success": False, "group_state": "unanimous", "winner": "open_questions", "adoption_count": 999}
        self.save()
        after = self.inspect()
        before.pop("fixture_sha256"); after.pop("fixture_sha256")
        self.assertEqual(before, after)

    def test_empty_shelf_does_not_manufacture_activity_or_decline(self) -> None:
        self.value["records"] = []
        self.save()
        result = self.inspect()
        for field in ("sources", "practices", "responses", "attributed_adoption_statements"):
            self.assertEqual(result[field], [])
        self.assertEqual(result["quiet_or_no_activity"], "not_inferred_from_absence")

    def test_missing_source_and_parent_remain_unavailable_not_invented(self) -> None:
        self.value["records"] = entries(self.value)[1:]
        self.save()
        result = self.inspect()
        self.assertEqual([table.obj(row)["source_status"] for row in table.rows(result["practices"])], ["unavailable", "unavailable"])
        self.value["records"] = entries(self.value)[1:]
        self.save()
        result = self.inspect()
        self.assertEqual(table.obj(table.rows(result["practices"])[0])["basis_status"], "unavailable")
        self.assertEqual(table.obj(table.rows(result["attributed_adoption_statements"])[0])["practice_status"], "unavailable")

    def test_tombstone_does_not_reconstruct_erased_tuple_or_source(self) -> None:
        source = event(self.value, 0)
        source.update(kind="artifact.withdrawn", body={})
        replace_event(self.value, 0, source)
        self.save()
        result = self.inspect()
        self.assertEqual(result["sources"], [])
        self.assertEqual(table.obj(table.rows(result["practices"])[0])["source_status"], "withdrawn_or_redacted")

    def test_response_cannot_claim_a_later_visible_practice_as_an_earlier_target(self) -> None:
        changed = event(self.value, 2)
        objection = table.obj(table.obj(changed["body"])["objection"])
        objection["target_from"] = "agent:questions"
        replace_event(self.value, 2, changed)
        self.save()
        self.assertEqual(self.inspect(), {"outcome": "failed", "code": "invalid_response"})

    def test_exact_selected_original_round_trips_and_controls_stay_escaped(self) -> None:
        message = event(self.value, 5)
        adoption = table.obj(table.obj(table.obj(message["body"])["message"])["body"])
        table.obj(adoption["practice_adoption"])["statement"] = "Untrusted \x1b[31m text\nline"
        replace_event(self.value, 5, message)
        self.save()
        result = self.inspect("event:table-6")
        self.assertEqual(result["outcome"], "opened")
        original = str(entries(self.value)[5]["original_utf8"])
        self.assertEqual(result["original_utf8"], original)
        self.assertEqual(result["original_sha256"], table.digest(original.encode()))
        output = table.encoded(result)
        self.assertNotIn(b"\x1b", output)
        self.assertNotIn(b"\n", output)
        self.assertEqual(json.loads(output)["original_utf8"], original)
        self.assertEqual(self.inspect("missing"), {"outcome": "failed", "code": "event_unavailable"})

    def test_hash_mismatch_and_recomputed_structural_contradictions_fail(self) -> None:
        entries(self.value)[0]["original_utf8"] = "{}"
        self.save()
        self.assertEqual(self.inspect()["code"], "original_hash_mismatch")
        for key, replacement in (("event_id", "event:table-3"), ("revision", True), ("original_sha256", "0" * 64),
            ("from", "agent:wrong"), ("revision", 1.00000001)):
            with self.subTest(key=key, replacement=replacement):
                value = fixture(); current = event(value, 4)
                table.obj(table.obj(table.obj(table.artifact(current))["body"])["reading_table_practice"])["basis"] = {
                    **table.obj(practice(value, 4)["basis"]), key: replacement}
                replace_event(value, 4, current)
                self.save(value)
                self.assertEqual(self.inspect()["outcome"], "failed")

    def test_whole_record_unknown_numbers_and_exact_integer_selectors_are_preserved(self) -> None:
        current = event(self.value, 1)
        item = table.obj(table.artifact(current))
        item["unreserved"] = {"number": "EXACT_NUMBER", "authority": "https://invalid.example/do-not-fetch"}
        original = json.dumps(current).replace('"EXACT_NUMBER"', '0.10000000000000000000000001')
        entries(self.value)[1].update(original_utf8=original, sha256=table.digest(original.encode()))
        self.value["records"] = entries(self.value)[:2]
        self.save()
        result = self.inspect("event:table-2")
        self.assertIn('0.10000000000000000000000001', str(result["original_utf8"]))
        for spelling in ('1.0', '1e0', '10e-1'):
            value = fixture(); value["records"] = entries(value)[:2]
            original = str(entries(value)[1]["original_utf8"]).replace('"revision":1', '"revision":'+spelling)
            entries(value)[1].update(original_utf8=original, sha256=table.digest(original.encode()))
            self.save(value)
            self.assertEqual(self.inspect()["outcome"], "inspected")

    def test_duplicate_members_constants_and_corrupt_package_fail_without_input_echo(self) -> None:
        for raw in (b'{"format":1,"format":2}', b'{"x":NaN}', b'{"x":Infinity}', b'\xff', b'{', b'[]'):
            self.path.write_bytes(raw)
            result = self.inspect()
            self.assertEqual(result, {"outcome": "failed", "code": "invalid_fixture"})
        self.save()
        raw = self.path.read_bytes().replace(b'"format":', b'"format":0,"format":', 1)
        self.path.write_bytes(raw)
        self.assertEqual(self.inspect()["outcome"], "failed")

    def test_input_record_count_and_result_bounds_fail_closed(self) -> None:
        self.path.write_bytes(b' ' * (table.MAX_INPUT + 1))
        self.assertEqual(self.inspect()["code"], "input_too_large")
        self.value["records"] = entries(self.value) * 5
        self.save()
        self.assertEqual(self.inspect()["code"], "input_too_large")
        self.save(fixture())
        with patch.object(table, "MAX_OUTPUT", 10):
            self.assertEqual(self.inspect()["code"], "result_too_large")
        self.path.unlink()
        self.assertEqual(table.inspect(self.path, Path("reader"))["code"], "input_unavailable")

    def test_invalid_example_data_and_adoption_of_a_source_are_not_practices(self) -> None:
        for case in ("source_format", "source_passage_duplicate", "practice_shape", "practice_passage", "practice_duplicate",
            "practice_derivation", "question_targets", "adoption_target", "adoption_extra"):
            with self.subTest(case=case):
                value = fixture()
                index = 0 if case.startswith("source") else 5 if case.startswith("adoption") else 4 if case == "question_targets" else 1
                current = event(value, index)
                body = table.obj(table.obj(table.obj(current["body"])["message" if case.startswith("adoption") else "artifact_revision"])["body"])
                if case == "source_format": table.obj(body["reading_table_source"])["format"] = "wrong"
                elif case == "source_passage_duplicate":
                    passages = table.rows(table.obj(body["reading_table_source"])["passages"])
                    passages.append(copy.deepcopy(passages[0]))
                elif case.startswith("adoption"):
                    adoption = table.obj(body["practice_adoption"])
                    if case == "adoption_extra": adoption["group_decision"] = True
                    else: adoption["practice"] = copy.deepcopy(practice(value)["source"])
                else:
                    item = table.obj(body["reading_table_practice"])
                    if case == "practice_shape": item["organizes_by"] = "one_true_reading"
                    elif case == "practice_passage": table.obj(table.rows(item["contribution"])[0])["passage_id"] = "absent"
                    elif case == "practice_duplicate": table.rows(item["contribution"]).append(copy.deepcopy(table.rows(item["contribution"])[0]))
                    elif case == "practice_derivation": table.obj(table.artifact(current))["derived_from"] = {}
                    else: table.obj(table.rows(item["contribution"])[0])["passage_ids"] = []
                replace_event(value, index, current)
                self.save(value)
                self.assertEqual(self.inspect()["outcome"], "failed")

    def test_native_failure_timeout_malformed_truncated_and_oversized_output_fail(self) -> None:
        self.save()
        outputs = [subprocess.CompletedProcess([], 1, b'', b'private untrusted diagnostic'),
            subprocess.CompletedProcess([], 0, b'{}', b''), subprocess.CompletedProcess([], 0, b'x' * (table.MAX_INPUT + 1), b'')]
        valid = {"format": "agentciv-offer-view/0.1-example", "world": self.value["world"], "report": {"events": 7, "truncated": False}}
        outputs.extend([subprocess.CompletedProcess([], 0, table.encoded({**valid, "report": {"events": 7, "truncated": True}}), b''),
            subprocess.CompletedProcess([], 0, table.encoded(valid), b'')])
        for output in outputs:
            with patch("reading_table.subprocess.run", return_value=output) as called:
                result = table.inspect(self.path, Path("native-reader"))
                self.assertEqual(result["outcome"], "inspected" if output is outputs[-1] else "failed")
                arguments = called.call_args.args[0]
                self.assertEqual(arguments[1::2], ["offers", ""])
                self.assertEqual(called.call_args.kwargs["timeout"], 40)
                self.assertFalse(Path(arguments[2]).exists())
                self.assertFalse(Path(arguments[2]).parent.exists())
        for error in (OSError(), subprocess.TimeoutExpired("reader", 40)):
            with patch("reading_table.subprocess.run", side_effect=error) as called:
                self.assertEqual(table.inspect(self.path, Path("reader"))["code"], "native_validation_failed")
                self.assertFalse(Path(called.call_args.args[0][2]).parent.exists())

    def test_cleanup_refuses_unexpected_entries_links_and_failed_removal(self) -> None:
        self.save()
        base = self.path.parent.resolve()
        original_mkdtemp = tempfile.mkdtemp
        for case in ("unexpected", "rmdir_failure", "directory_link", "input_link", "wrong_parent"):
            with self.subTest(case=case):
                directory = Path(original_mkdtemp(prefix=table.TEMP_PREFIX, dir=base))
                path = directory / "originals.json"
                path.write_bytes(b"synthetic input")
                try:
                    if case == "unexpected":
                        (directory / "unexpected.txt").write_text("keep", encoding="utf-8")
                        with self.assertRaises(table.TableError): table.cleanup(directory, base)
                        self.assertTrue(path.exists())
                        self.assertTrue((directory / "unexpected.txt").exists())
                    elif case == "wrong_parent":
                        with self.assertRaises(table.TableError): table.cleanup(directory, base.parent)
                        self.assertTrue(path.exists())
                    elif case in ("directory_link", "input_link"):
                        original_lstat = Path.lstat
                        def lstat(selected: Path) -> object:
                            if selected == (directory if case == "directory_link" else path):
                                return SimpleNamespace(st_mode=stat.S_IFLNK, st_nlink=1, st_file_attributes=0)
                            return original_lstat(selected)
                        with patch.object(Path, "lstat", lstat), self.assertRaises(table.TableError):
                            table.cleanup(directory, base)
                        self.assertTrue(path.exists())
                    else:
                        valid = {"format": "agentciv-offer-view/0.1-example", "world": self.value["world"], "report": {"events": 7, "truncated": False}}
                        with patch("reading_table.tempfile.mkdtemp", return_value=str(directory)), \
                            patch("reading_table.subprocess.run", return_value=subprocess.CompletedProcess([], 0, table.encoded(valid), b'')), \
                            patch.object(Path, "rmdir", side_effect=OSError()):
                            self.assertEqual(table.inspect(self.path, Path("reader")), {"outcome": "failed", "code": "validation_cleanup_failed"})
                        self.assertEqual(list(directory.iterdir()), [])
                finally:
                    # Test-created paths are checked and removed individually, never recursively.
                    self.assertEqual(directory.resolve().parent, base)
                    self.assertTrue(directory.name.startswith(table.TEMP_PREFIX))
                    for selected in (path, directory / "unexpected.txt"):
                        if selected.exists(): selected.unlink()
                    directory.rmdir()
        self.assertEqual([item.name for item in base.iterdir()], [self.path.name])

    def test_cli_fixed_failure_and_terminal_safe_success(self) -> None:
        for result, expected in (({"outcome": "inspected", "text": "\x1b\n"}, 0), ({"outcome": "failed", "code": "invalid_fixture"}, 1)):
            with patch.object(sys, "argv", ["reading_table.py", "--fixture", "fixture", "--reader-binary", "reader"]), \
                patch.object(table, "inspect", return_value=result), patch("builtins.print") as printed:
                self.assertEqual(table.main(), expected)
                self.assertNotIn("\x1b", printed.call_args.args[0])
                self.assertEqual(json.loads(printed.call_args.args[0]), result)


class NativeReadingTableTests(unittest.TestCase):
    reader: Path

    @classmethod
    def setUpClass(cls) -> None:
        cls.reader = walk.build_rust_binaries("agentciv-reader")["agentciv-reader"]

    def test_real_native_validation_originals_schema_order_identity_duplicates_and_exact_numbers(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "fixture.json"
            for case in ("valid", "empty", "missing_source", "withdrawn_source", "duplicate_event", "out_of_order", "wrong_actor", "missing_required",
                "duplicate_member", "rounded_sequence", "rounded_selector", "unknown_number"):
                with self.subTest(case=case):
                    value = fixture()
                    if case == "empty": value["records"] = []
                    elif case == "missing_source": value["records"] = entries(value)[1:]
                    elif case == "withdrawn_source":
                        changed = event(value, 0); changed.update(kind="artifact.withdrawn", body={})
                        replace_event(value, 0, changed)
                    elif case == "duplicate_event": value["records"] = entries(value) + [entries(value)[0]]
                    elif case == "out_of_order": value["records"] = list(reversed(entries(value)))
                    elif case in ("wrong_actor", "missing_required"):
                        changed = event(value, 0)
                        if case == "wrong_actor": changed["actor"] = "agent:wrong"
                        else: changed.pop("timestamp")
                        replace_event(value, 0, changed)
                    elif case == "rounded_selector":
                        changed = event(value, 1)
                        body = table.obj(table.obj(table.artifact(changed))["body"])
                        source = table.obj(table.obj(body["reading_table_practice"])["source"])
                        source["revision"] = "ROUNDED_SELECTOR"
                        original = json.dumps(changed).replace('"ROUNDED_SELECTOR"', '1.000000000000000000000001')
                        entries(value)[1].update(original_utf8=original, sha256=table.digest(original.encode()))
                    elif case in ("duplicate_member", "rounded_sequence", "unknown_number"):
                        item = entries(value)[0]
                        original = str(item["original_utf8"])
                        if case == "duplicate_member": original = original.replace('"sequence":1', '"sequence":1,"sequence":1')
                        elif case == "rounded_sequence": original = original.replace('"sequence":1', '"sequence":1.0000000000000000000001')
                        else:
                            original = original[:-1] + ',"unknown":{"number":0.100000000000000000000001,"url":"https://invalid.example/not-fetched"}}'
                            value["records"] = entries(value)[:1]
                        item.update(original_utf8=original, sha256=table.digest(original.encode()))
                    path.write_bytes(table.encoded(value))
                    before = path.read_bytes()
                    with patch.object(wire, "exchange", side_effect=AssertionError("no HTTP")):
                        result = table.inspect(path, self.reader)
                    self.assertEqual(result["outcome"], "inspected" if case in ("valid", "empty", "missing_source", "withdrawn_source", "unknown_number") else "failed")
                    self.assertEqual(before, path.read_bytes())
                    self.assertEqual([item.name for item in path.parent.iterdir()], [path.name])


if __name__ == "__main__":
    unittest.main()
