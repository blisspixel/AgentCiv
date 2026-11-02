"""Live reading-table records, exact host originals, and audience limits.

The prose is the frozen table's operator-authored story. A passing process test
shows both loopback hosts retain it. It does not show that a participant chose it.
"""
from __future__ import annotations

import hashlib
import io
import json
import socket
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "http-commons"))
import encounter_client as client
import encounter_request as requests
import local_encounter as operator
import local_participant as wire
import reading_table as table
import reading_table_journey as journey
import walk

JsonObject = dict[str, object]
RECIPIENTS = [name for name in journey.TABLE if name != journey.SOURCE]


def objects(value: object) -> list[JsonObject]:
    if not isinstance(value, list):
        raise AssertionError("expected object list")
    return [table.obj(item) for item in value]


def free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def event_original(record: JsonObject, event_id: str, sequence: int) -> str:
    stored = dict(record)
    if record["type"] == "artifact_revision":
        stored["revision"] = 1
    kind = {"artifact_revision": "artifact.recorded", "objection": "objection.recorded",
        "decline": "decline.recorded", "message": "message.recorded"}[str(record["type"])]
    event: JsonObject = {"protocol_version": "0.1-draft", "type": "event", "id": event_id,
        "world": record["world"], "sequence": sequence, "timestamp": "2026-11-01T00:00:00Z",
        "kind": kind, "actor": record["from"], "body": {str(record["type"]): stored}}
    return json.dumps(event, ensure_ascii=True)


def journal(original: str, path: Path, *, state: str = "recorded") -> Path:
    path.write_bytes(json.dumps({"state": state, "event_record_utf8": original}, ensure_ascii=True).encode("ascii"))
    return path


def view(path: Path, originals: list[str]) -> Path:
    payload = {"format": client.VIEW_FORMAT, "snapshot": {"world": journey.WORLD, "records": originals}}
    path.write_bytes(json.dumps(payload, ensure_ascii=True).encode("ascii"))
    return path


class ReadingTableJourneyTests(unittest.TestCase):
    def test_records_cite_host_originals_and_keep_two_organizations(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = journey.source_record(journey.WORLD, journey.SOURCE, RECIPIENTS, "submission:table-source")
            source_original = event_original(source, "event:from-host-1", 1)
            source_seen = journey.observe_journal(journal(source_original, root / "source.json"))
            self.assertEqual(source_seen.event_id, "event:from-host-1")
            self.assertNotIn("event:table-1", json.dumps(source))
            passages = journey.passage_record(journey.WORLD, journey.PASSAGES, RECIPIENTS, "submission:table-passages", source_seen)
            passage_body = table.obj(table.obj(passages["body"])["reading_table_practice"])
            self.assertEqual(passage_body["organizes_by"], "passages")
            self.assertEqual(table.obj(passage_body["source"])["event_id"], "event:from-host-1")
            self.assertEqual(table.obj(passage_body["source"])["original_sha256"], source_seen.sha256)
            self.assertEqual({key for row in objects(passage_body["contribution"]) for key in row}, {"passage_id", "note"})
            passage_original = event_original(passages, "event:from-host-2", 2)
            passage_seen = journey.observe_original(passage_original)
            questions = journey.question_record(journey.WORLD, journey.QUESTIONS, RECIPIENTS, "submission:table-questions",
                source_seen, passage_seen)
            question_body = table.obj(table.obj(questions["body"])["reading_table_practice"])
            self.assertEqual(question_body["organizes_by"], "open_questions")
            self.assertEqual(questions["derived_from"], passage_seen.derivation())
            self.assertEqual({key for row in objects(question_body["contribution"]) for key in row}, {"question", "passage_ids"})
            self.assertNotEqual(passage_body["contribution"], question_body["contribution"])
            objection = journey.response_record("objection", journey.WORLD, journey.QUESTIONS, RECIPIENTS,
                "submission:table-objection", passage_seen, "Passage order can hide a question spanning the gap and return.")
            self.assertEqual(objection["revision"], 1)
            with self.assertRaises(journey.JourneyError) as mismatched:
                journey.withdrawal_record(journey.WORLD, journey.VISITOR, "submission:not-author", passage_seen)
            self.assertEqual(mismatched.exception.code, "withdrawal_author_mismatch")
            originals = [source_original, passage_original, event_original(questions, "event:from-host-3", 3)]
            saved = view(root / "view.json", originals)
            before = saved.read_bytes()
            with self.assertRaises(journey.JourneyError) as refused:
                journey.package_bytes(saved, export_synthetic=False)
            self.assertEqual(refused.exception.code, "export_not_affirmed")
            self.assertEqual(saved.read_bytes(), before)
            package = root / "package.json"
            package.write_bytes(journey.package_bytes(saved, export_synthetic=True))
            with patch.object(table, "validate"):
                inspected = table.inspect(package, Path("selected-reader"))
            self.assertEqual(inspected["outcome"], "inspected")
            self.assertFalse(inspected["producer_report_used"])
            self.assertEqual(inspected["group_state"], "not_inferred")
            self.assertEqual(sorted(str(table.obj(row)["organizes_by"]) for row in objects(inspected["practices"])),
                ["open_questions", "passages"])
            flipped = json.loads(package.read_bytes().decode("ascii"))
            flipped["producer_report"] = {"success": False, "winner": "passages"}
            package.write_bytes(json.dumps(flipped).encode("ascii"))
            with patch.object(table, "validate"):
                again = table.inspect(package, Path("selected-reader"))
            self.assertEqual(again["practices"], inspected["practices"])
            self.assertNotEqual(again["fixture_sha256"], inspected["fixture_sha256"])

    def test_unrecorded_journal_stays_unresolved_and_cli_writes_exact_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pending = journal("{}", root / "pending.json", state="uncertain")
            with self.assertRaises(journey.JourneyError) as unresolved:
                journey.observe_journal(pending)
            self.assertEqual(unresolved.exception.code, "journal_not_recorded")
            self.assertEqual(pending.read_bytes(), journal("{}", root / "other.json", state="uncertain").read_bytes())
            arguments = ["reading_table_journey.py", "source", "--world", journey.WORLD, "--from", journey.SOURCE,
                "--id", "submission:table-source", "--to", journey.READER]
            stdout = io.StringIO()
            with patch.object(sys, "argv", arguments), patch.object(sys, "stdout", stdout):
                self.assertEqual(journey.main(), 0)
            expected = journey.record_bytes(journey.source_record(journey.WORLD, journey.SOURCE, [journey.READER], "submission:table-source"))
            self.assertEqual(stdout.getvalue().encode("ascii"), expected)
            missing = io.StringIO()
            with patch.object(sys, "argv", ["reading_table_journey.py", "package", "--view", str(root / "missing.json"), "--export-synthetic"]), \
                patch.object(sys, "stdout", missing):
                self.assertEqual(journey.main(), 1)
            self.assertEqual(json.loads(missing.getvalue())["code"], "input_unavailable")
            self.assertNotIn(str(root), missing.getvalue())
            refused = io.StringIO()
            with patch.object(sys, "argv", ["reading_table_journey.py", "package", "--view", str(root / "missing.json")]), \
                patch.object(sys, "stdout", refused):
                self.assertEqual(journey.main(), 1)
            self.assertEqual(json.loads(refused.getvalue())["code"], "export_not_affirmed")

    def test_cli_follows_recorded_journals_and_rejects_bad_selectors(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            audience = [journey.READER]
            source = journey.source_record(journey.WORLD, journey.SOURCE, audience, "submission:table-source")
            source_path = journal(event_original(source, "event:from-host-1", 1), root / "source.json")
            source_seen = journey.observe_journal(source_path)
            passages = journey.passage_record(journey.WORLD, journey.PASSAGES, audience, "submission:table-passages", source_seen)
            passage_path = journal(event_original(passages, "event:from-host-2", 2), root / "passages.json")
            passage_seen = journey.observe_journal(passage_path)

            def invoke(arguments: list[str]) -> bytes:
                stdout = io.StringIO()
                with patch.object(sys, "argv", ["reading_table_journey.py", *arguments]), patch.object(sys, "stdout", stdout):
                    self.assertEqual(journey.main(), 0, stdout.getvalue())
                return stdout.getvalue().encode("ascii")

            shared = ["--world", journey.WORLD, "--to", journey.READER]
            self.assertEqual(invoke(["passages", *shared, "--from", journey.PASSAGES, "--id", "submission:table-passages",
                "--source-journal", str(source_path)]), journey.record_bytes(passages))
            questions = journey.question_record(journey.WORLD, journey.QUESTIONS, audience, "submission:table-questions",
                source_seen, passage_seen)
            self.assertEqual(invoke(["questions", *shared, "--from", journey.QUESTIONS, "--id", "submission:table-questions",
                "--source-journal", str(source_path), "--basis-journal", str(passage_path)]), journey.record_bytes(questions))
            objection_text = ("Passage order can hide a question spanning the gap and return. "
                "I object to treating this organization as the only shared reading.")
            objection = journey.response_record("objection", journey.WORLD, journey.QUESTIONS, audience,
                "submission:table-objection", passage_seen, objection_text)
            self.assertEqual(invoke(["objection", *shared, "--from", journey.QUESTIONS, "--id", "submission:table-objection",
                "--target-journal", str(passage_path)]), journey.record_bytes(objection))
            decline_text = "I will not finish the passage notes or a joint interpretation. The invitation may remain for another visitor."
            decline = journey.response_record("decline", journey.WORLD, journey.VISITOR, audience,
                "submission:table-decline", passage_seen, decline_text)
            self.assertEqual(invoke(["decline", *shared, "--from", journey.VISITOR, "--id", "submission:table-decline",
                "--target-journal", str(passage_path)]), journey.record_bytes(decline))
            adoption = journey.adoption_record(journey.WORLD, journey.ADOPT_PASSAGES, audience,
                "submission:table-adopt-passages", passage_seen, journey.PASSAGE_STATEMENT)
            self.assertEqual(invoke(["adoption", *shared, "--from", journey.ADOPT_PASSAGES, "--id", "submission:table-adopt-passages",
                "--practice-journal", str(passage_path), "--statement", journey.PASSAGE_STATEMENT]), journey.record_bytes(adoption))
            note = journey.note_record(journey.WORLD, journey.SOURCE, "submission:table-private", journey.PRIVATE_SENTINEL)
            self.assertEqual(invoke(["note", "--world", journey.WORLD, "--from", journey.SOURCE, "--id", "submission:table-private",
                "--text", journey.PRIVATE_SENTINEL]), journey.record_bytes(note))
            malformed = journey.malformed_offer_record(journey.WORLD, journey.PASSAGES, audience, "submission:table-malformed")
            self.assertEqual(invoke(["malformed-offer", *shared, "--from", journey.PASSAGES, "--id", "submission:table-malformed"]),
                journey.record_bytes(malformed))
            withdrawal = journey.withdrawal_record(journey.WORLD, journey.PASSAGES, "submission:table-withdraw", passage_seen)
            self.assertEqual(invoke(["withdrawal", "--world", journey.WORLD, "--from", journey.PASSAGES, "--id", "submission:table-withdraw",
                "--target-journal", str(passage_path)]), journey.record_bytes(withdrawal))

            message_seen = journey.observe_original(event_original(note, "event:from-host-note", 3))
            self.assertIsNone(message_seen.artifact_id)
            with self.assertRaises(journey.JourneyError) as selector_error:
                message_seen.selector()
            self.assertEqual(selector_error.exception.code, "not_an_artifact")
            with self.assertRaises(journey.JourneyError) as passage_target:
                journey.passage_record(journey.WORLD, journey.PASSAGES, audience, "submission:bad-basis", message_seen)
            self.assertEqual(passage_target.exception.code, "not_an_artifact")
            with self.assertRaises(journey.JourneyError) as adoption_target:
                journey.adoption_record(journey.WORLD, journey.ADOPT_PASSAGES, audience, "submission:bad-adoption",
                    message_seen, journey.PASSAGE_STATEMENT)
            self.assertEqual(adoption_target.exception.code, "not_an_artifact")
            with self.assertRaises(journey.JourneyError) as response_kind:
                journey.response_record("withdrawal", journey.WORLD, journey.VISITOR, audience, "submission:bad-kind", passage_seen, "no")
            self.assertEqual(response_kind.exception.code, "not_an_artifact")
            with self.assertRaises(journey.JourneyError) as same_chain:
                journey.question_record(journey.WORLD, journey.PASSAGES, audience, "submission:same-chain", source_seen, passage_seen)
            self.assertEqual(same_chain.exception.code, "invalid_derivation")
            for recipients, code in (([], "invalid_audience"), ([journey.READER, journey.READER], "invalid_audience")):
                with self.assertRaises(journey.JourneyError) as audience_error:
                    journey.source_record(journey.WORLD, journey.SOURCE, recipients, "submission:audience")
                self.assertEqual(audience_error.exception.code, code)
            with self.assertRaises(journey.JourneyError) as blank:
                journey.source_record(journey.WORLD, "", audience, "submission:blank")
            self.assertEqual(blank.exception.code, "invalid_identifier")
            (root / "bare.json").write_bytes(b'{"state":"recorded"}\n')
            with self.assertRaises(journey.JourneyError) as missing_original:
                journey.observe_journal(root / "bare.json")
            self.assertEqual(missing_original.exception.code, "event_original_missing")
            (root / "broken.json").write_bytes(b"{")
            with self.assertRaises(journey.JourneyError) as broken_journal:
                journey.observe_journal(root / "broken.json")
            self.assertEqual(broken_journal.exception.code, "event_original_invalid")
            invalid_view = root / "not-a-view.json"
            invalid_view.write_bytes(b'{"format":"other"}\n')
            failed = io.StringIO()
            with patch.object(sys, "argv", ["reading_table_journey.py", "package", "--view", str(invalid_view), "--export-synthetic"]), \
                patch.object(sys, "stdout", failed):
                self.assertEqual(journey.main(), 1)
            self.assertEqual(json.loads(failed.getvalue())["code"], "invalid_view")
            self.assertNotIn(str(root), failed.getvalue())
            saved = view(root / "view.json", [event_original(source, "event:from-host-1", 1)])
            self.assertEqual(invoke(["package", "--view", str(saved), "--export-synthetic"]),
                journey.package_bytes(saved, export_synthetic=True))
            with self.assertRaises(journey.JourneyError) as control:
                journey.source_record(journey.WORLD, "agent:bad\n", audience, "submission:control")
            self.assertEqual(control.exception.code, "invalid_identifier")
            with self.assertRaises(journey.JourneyError) as prose:
                journey.response_record("objection", journey.WORLD, journey.QUESTIONS, audience,
                    "submission:prose", passage_seen, "line\nbreak")
            self.assertEqual(prose.exception.code, "invalid_prose")
            with self.assertRaises(journey.JourneyError) as bad_event:
                journey.observe_original("{")
            self.assertEqual(bad_event.exception.code, "event_original_invalid")
            shifted = json.loads(event_original(source, "event:from-host-1", 1))
            shifted["sequence"] = 0
            with self.assertRaises(journey.JourneyError) as low_sequence:
                journey.observe_original(json.dumps(shifted))
            self.assertEqual(low_sequence.exception.code, "event_original_invalid")
            shifted["sequence"] = 1
            shifted["actor"] = "agent:other"
            with self.assertRaises(journey.JourneyError) as other_author:
                journey.observe_original(json.dumps(shifted))
            self.assertEqual(other_author.exception.code, "event_original_invalid")
            with self.assertRaises(journey.JourneyError) as question_basis:
                journey.question_record(journey.WORLD, journey.QUESTIONS, audience, "submission:bad-question",
                    message_seen, passage_seen)
            self.assertEqual(question_basis.exception.code, "not_an_artifact")


class LiveReadingTableTests(unittest.TestCase):
    reader_binary: Path
    host_binary: Path

    @classmethod
    def setUpClass(cls) -> None:
        binaries = walk.build_rust_binaries("agentciv-reader", "agentciv-host")
        cls.reader_binary = binaries["agentciv-reader"]
        cls.host_binary = binaries["agentciv-host"]

    def test_restart_discovery_and_audience_on_both_hosts(self) -> None:
        for selected in ("python", "rust"):
            with self.subTest(host=selected), tempfile.TemporaryDirectory(prefix="agentciv-live-table-") as temporary:
                self.exercise(Path(temporary).resolve(), selected)

    def exercise(self, root: Path, selected: str) -> None:
        state = root / "room"
        writers = [journey.SOURCE, journey.PASSAGES, journey.QUESTIONS, journey.VISITOR, journey.ADOPT_PASSAGES, journey.ADOPT_QUESTIONS]
        initialized = operator.initialize(state, journey.WORLD, writers, [journey.READER], free_port())
        configs = {str(item["principal"]): Path(str(item["config"])) for item in objects(initialized["clients"])}
        origin = str(initialized["origin"])
        token = str(client._configuration(configs[journey.READER])["token"])
        argv = walk.python_argv(state / "host.json") if selected == "python" else [str(self.host_binary), "--config", str(state / "host.json")]
        authored: list[Path] = []

        def publish(author: str, record: JsonObject) -> journey.Observed:
            raw = journey.record_bytes(record)
            path = root / f"authored-{len(authored)}.json"
            path.write_bytes(raw)
            authored.append(path)
            digest = hashlib.sha256(raw).hexdigest()
            basis = client.read(configs[author], reader_binary=self.reader_binary)
            result = client.submit(configs[author], path, Path(str(basis["view_path"])), reader_binary=self.reader_binary)
            self.assertEqual(path.read_bytes(), raw)
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), digest)
            self.assertEqual(result["outcome"], "recorded", result)
            self.assertNotIn(token, json.dumps(result))
            seen = journey.observe_journal(Path(str(result["journal_path"])))
            self.assertEqual(seen.event_id, result["event_id"])
            return seen

        def listed(path: Path) -> list[JsonObject]:
            page = client.offers(configs[journey.READER], path, query="", reader_binary=self.reader_binary)
            self.assertEqual(page.get("query"), "")
            return objects(page["offers"])

        def titles(path: Path) -> set[str]:
            return {str(table.obj(row["offer"])["title"]) for row in listed(path)}

        owned = operator._start(argv, origin)
        try:
            operator._wait_ready(owned)
            empty = client.read(configs[journey.READER], reader_binary=self.reader_binary)
            self.assertEqual(empty["events"], 0)
            self.assertEqual(titles(Path(str(empty["view_path"]))), set())
            audience = [name for name in journey.TABLE if name != journey.SOURCE]
            publish(journey.SOURCE, journey.note_record(journey.WORLD, journey.SOURCE, "submission:table-private", journey.PRIVATE_SENTINEL))
            source = publish(journey.SOURCE, journey.source_record(journey.WORLD, journey.SOURCE, audience, "submission:table-source"))
            passages = publish(journey.PASSAGES, journey.passage_record(journey.WORLD, journey.PASSAGES, audience, "submission:table-passages", source))
            publish(journey.QUESTIONS, journey.response_record("objection", journey.WORLD, journey.QUESTIONS, audience,
                "submission:table-objection", passages,
                "Passage order can hide a question spanning the gap and return. I object to treating this organization as the only shared reading."))
            publish(journey.VISITOR, journey.response_record("decline", journey.WORLD, journey.VISITOR, audience,
                "submission:table-decline", passages,
                "I will not finish the passage notes or a joint interpretation. The invitation may remain for another visitor."))
            questions = publish(journey.QUESTIONS, journey.question_record(journey.WORLD, journey.QUESTIONS, audience,
                "submission:table-questions", source, passages))
            publish(journey.ADOPT_PASSAGES, journey.adoption_record(journey.WORLD, journey.ADOPT_PASSAGES, audience,
                "submission:table-adopt-passages", passages, journey.PASSAGE_STATEMENT))
            publish(journey.ADOPT_QUESTIONS, journey.adoption_record(journey.WORLD, journey.ADOPT_QUESTIONS, audience,
                "submission:table-adopt-questions", questions, journey.QUESTION_STATEMENT))
            current = client.read(configs[journey.READER], reader_binary=self.reader_binary)
            current_path = Path(str(current["view_path"]))
            self.assertEqual(current["events"], 7)
            self.assertEqual(titles(current_path), {"Passage notes", "Open questions"})
            rendered = client.show(configs[journey.READER], current_path, event_id=passages.event_id)
            self.assertIn(journey.PASSAGE_NOTES[0][1], rendered)
            self.assertNotIn(journey.PRIVATE_SENTINEL, rendered)
            self.assertNotIn(journey.PRIVATE_SENTINEL, current_path.read_text(encoding="utf-8"))
            intact = root / "intact.json"
            intact.write_bytes(journey.package_bytes(current_path, export_synthetic=True))
            self.assertNotIn(token, intact.read_text(encoding="ascii"))
            inspected = table.inspect(intact, self.reader_binary)
            self.assertEqual(inspected["outcome"], "inspected", inspected.get("code"))
            self.assertFalse(inspected["producer_report_used"])
            self.assertEqual(inspected["group_state"], "not_inferred")
            self.assertEqual(inspected["quiet_or_no_activity"], "not_inferred_from_absence")
            self.assertEqual(sorted(str(table.obj(row)["organizes_by"]) for row in objects(inspected["practices"])), ["open_questions", "passages"])
            self.assertTrue(all(table.obj(row)["state"] == "unfinished" for row in objects(inspected["practices"])))
            self.assertEqual(len(objects(inspected["responses"])), 2)
            self.assertEqual(len(objects(inspected["attributed_adoption_statements"])), 2)
            ids = [passages.event_id, questions.event_id]
        finally:
            operator._stop_owned(owned)

        restarted = operator._start(argv, origin)
        try:
            operator._wait_ready(restarted)
            returned = client.read(configs[journey.READER], reader_binary=self.reader_binary)
            returned_path = Path(str(returned["view_path"]))
            self.assertEqual(returned["events"], 7)
            self.assertEqual(titles(returned_path), {"Passage notes", "Open questions"})
            for event_id in ids:
                self.assertIn(event_id, returned_path.read_text(encoding="utf-8"))
        finally:
            operator._stop_owned(restarted)

        added = operator.add_member(state, journey.NEWCOMER, write=False)
        resumed = operator._start(argv, origin)
        try:
            operator._wait_ready(resumed)
            newcomer = client.read(Path(str(added["config"])), reader_binary=self.reader_binary)
            self.assertEqual(newcomer["events"], 0)
            self.assertNotIn("circle around the table", Path(str(newcomer["view_path"])).read_text(encoding="utf-8"))
            publish(journey.PASSAGES, journey.malformed_offer_record(journey.WORLD, journey.PASSAGES, audience, "submission:table-malformed"))
            publish(journey.QUESTIONS, journey.withdrawal_record(journey.WORLD, journey.QUESTIONS, "submission:table-withdraw", questions))
            after = client.read(configs[journey.READER], reader_binary=self.reader_binary)
            after_path = Path(str(after["view_path"]))
            self.assertEqual(titles(after_path), set())
            report = table.obj(table.obj(client.offers(configs[journey.READER], after_path, query="", reader_binary=self.reader_binary))["report"])
            omissions = table.obj(report["omissions"])
            self.assertGreaterEqual(int(str(omissions["malformed_latest_offers"])), 1)
            self.assertGreaterEqual(int(str(omissions["superseded_offer_revisions"])), 1)
            tombstone = client.show(configs[journey.READER], after_path, event_id=questions.event_id)
            self.assertIn("artifact.withdrawn", tombstone)
            self.assertNotIn(journey.OPEN_QUESTIONS[0][0], tombstone)
            self.assertIn(journey.OPEN_QUESTIONS[0][0], returned_path.read_text(encoding="utf-8"))
            later = root / "later.json"
            later.write_bytes(journey.package_bytes(after_path, export_synthetic=True))
            self.assertNotIn(journey.OPEN_QUESTIONS[0][0], later.read_text(encoding="utf-8"))
            withdrawn = table.inspect(later, self.reader_binary)
            self.assertEqual(withdrawn["outcome"], "inspected", withdrawn.get("code"))
            self.assertEqual([str(table.obj(row)["organizes_by"]) for row in objects(withdrawn["practices"])], ["passages"])
            adopted = [table.obj(row) for row in objects(withdrawn["attributed_adoption_statements"])]
            self.assertIn("withdrawn_or_redacted", [str(row["practice_status"]) for row in adopted])
            self.assertIn("available", [str(row["practice_status"]) for row in adopted])

            uncertain_record = journey.note_record(journey.WORLD, journey.VISITOR, "submission:table-uncertain", "I might add a note later.")
            raw = journey.record_bytes(uncertain_record)
            uncertain_path = root / "uncertain.json"
            uncertain_path.write_bytes(raw)
            visitor_cache = Path(str(client._configuration(configs[journey.VISITOR])["cache_directory"]))
            journals_before = set(visitor_cache.glob("journal-*.json"))
            actual_exchange = wire.exchange

            def lose_after_apply(method: str, url: str, *, token: str | None = None, body: bytes | None = None) -> tuple[int, bytes]:
                response = actual_exchange(method, url, token=token, body=body)
                if method == "POST":
                    self.assertEqual(response[0], 200)
                    raise wire.ParticipantError(0, "synthetic_response_lost_after_apply")
                return response

            with patch.object(wire, "exchange", side_effect=lose_after_apply):
                lost = client.submit(configs[journey.VISITOR], uncertain_path, reader_binary=self.reader_binary)
            self.assertEqual(lost["outcome"], "uncertain")
            observed = client.read(configs[journey.VISITOR], reader_binary=self.reader_binary)
            observed_path = Path(str(observed["view_path"]))
            journals_after = set(visitor_cache.glob("journal-*.json"))
            self.assertEqual(len(journals_after - journals_before), 1)
        finally:
            operator._stop_owned(resumed)

        journal_path = Path(str(lost["journal_path"]))
        held = {path: path.read_bytes() for path in (journal_path, uncertain_path, observed_path)}
        with patch.object(wire, "exchange", side_effect=AssertionError("inspection attempted HTTP")), \
            patch.object(client, "_publish", side_effect=AssertionError("inspection attempted publication")):
            inspected_request = requests.inspect(configs[journey.VISITOR], journal_path, observed_path, reader_binary=self.reader_binary)
        self.assertEqual(inspected_request["evidence_status"], "compatible_candidates")
        self.assertFalse(inspected_request["journal_state_changed"])
        self.assertEqual(held, {path: path.read_bytes() for path in held})
        self.assertEqual(set(visitor_cache.glob("journal-*.json")), journals_after)
