"""Build one synthetic reading table from host originals, without choosing a winner.

Each command writes authored bytes for ``local_encounter.py submit``. It does not
contact a host, mint a replacement id, retry, or treat silence as a decision.
Selectors come from the exact event string a recorded journal already saved.
The prose below is operator-authored fixture text, shared with the frozen table.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from dataclasses import dataclass
from pathlib import Path

import collaboration as civic
import encounter_client as client

JsonObject = dict[str, object]
WORLD = "civ:live-reading-table"
SOURCE = "agent:table-source"
PASSAGES = "agent:table-passages"
QUESTIONS = "agent:table-questions"
VISITOR = "agent:table-visitor"
ADOPT_PASSAGES = "agent:table-adopter-passages"
ADOPT_QUESTIONS = "agent:table-adopter-questions"
READER = "agent:table-reader"
NEWCOMER = "agent:table-newcomer"
TABLE = (SOURCE, PASSAGES, QUESTIONS, VISITOR, ADOPT_PASSAGES, ADOPT_QUESTIONS, READER)
ARTIFACT_ORIGINAL = "artifact:table-original"
ARTIFACT_TABLE = "artifact:table-reading"
PRIVATE_SENTINEL = "table-private-sentinel-not-for-the-reader"
SCOPE = "My next reading only; no group decision or obligation."
PASSAGE_STATEMENT = (
    "For my next notes I choose passage organization. I do not claim a decision for the table."
)
QUESTION_STATEMENT = (
    "I choose open questions for my own next reading. I have not answered them or accepted anyone else's duty."
)
STORY = (
    ("circle", "A visitor draws a circle around the table."),
    ("gap", "Another visitor leaves a gap in the circle and sets down an empty cup."),
    ("return", "When they return, no one has filled the gap. A note says: keep a place for an answer that has not arrived."),
)
PASSAGE_NOTES = (
    ("circle", "The boundary is drawn by one visitor; the source does not say the others agreed."),
    ("gap", "The empty cup could welcome a visitor, but that is my interpretation, not a source fact."),
    ("return", "The note asks for patience. Who wrote it remains unspecified."),
)
OPEN_QUESTIONS = (
    ("Who can change the circle without speaking for the other visitors?", ("circle", "gap")),
    ("Does keeping the gap open welcome someone or postpone their answer?", ("gap", "return")),
    ("Could returning visitors reject the note and still keep the original story?", ("return", "circle")),
)
OFFER_LIMITS = "No deliverable, agreement, enrollment or outside access. Quiet and unfinished work are allowed."
OFFER_COPYING = (
    "Operator permits redistribution and modification of these original synthetic fixture records; "
    "this claim authenticates no other material."
)
MAX_INPUT = 4 * 1024 * 1024


class JourneyError(ValueError):
    """Fixed code only. The message never carries a path, credential, or record body."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class Observed:
    """One exact host event string and the identity fields taken from it."""

    event_id: str
    sequence: int
    kind: str
    original_utf8: str
    sha256: str
    author: str
    artifact_id: str | None = None
    revision: int | None = None

    def selector(self) -> JsonObject:
        if self.artifact_id is None or self.revision is None:
            raise JourneyError("not_an_artifact")
        return {"event_id": self.event_id, "from": self.author, "artifact_id": self.artifact_id,
            "revision": self.revision, "original_sha256": self.sha256}

    def derivation(self) -> JsonObject:
        selected = self.selector()
        return {"from": selected["from"], "artifact_id": selected["artifact_id"], "revision": selected["revision"]}

    def source_ref(self) -> JsonObject:
        selected = self.selector()
        return {key: selected[key] for key in ("event_id", "from", "artifact_id", "revision")}


def _identifier(value: object, maximum: int = 256) -> str:
    if not isinstance(value, str) or not 1 <= len(value.encode("utf-8")) <= maximum:
        raise JourneyError("invalid_identifier")
    if any(ord(char) < 32 or ord(char) == 127 for char in value):
        raise JourneyError("invalid_identifier")
    return value


def _prose(value: str, maximum: int = 1024) -> str:
    if not 1 <= len(value.encode("utf-8")) <= maximum or any(ord(char) < 32 or ord(char) == 127 for char in value):
        raise JourneyError("invalid_prose")
    return value


def _audience(recipients: list[str]) -> list[str]:
    chosen: list[str] = []
    for item in recipients:
        name = _identifier(item)
        if name in chosen:
            raise JourneyError("invalid_audience")
        chosen.append(name)
    if not chosen:
        raise JourneyError("invalid_audience")
    return chosen


def _read_capped(path: Path) -> bytes:
    try:
        if path.is_symlink() or not path.is_file():
            raise JourneyError("input_unavailable")
        with path.open("rb") as stream:
            raw = stream.read(MAX_INPUT + 1)
    except OSError:
        raise JourneyError("input_unavailable") from None
    if len(raw) > MAX_INPUT:
        raise JourneyError("input_too_large")
    return raw


def record_bytes(record: JsonObject) -> bytes:
    """Stable UTF-8 JSON with one trailing newline and no byte-order mark."""
    return json.dumps(record, ensure_ascii=True, indent=2, allow_nan=False).encode("ascii") + b"\n"


def observe_original(original: str) -> Observed:
    try:
        raw = original.encode("utf-8")
        event = client._decode(raw)
        kind = _identifier(event.get("kind"))
        author = _identifier(event.get("actor"))
        sequence = client._integer(event.get("sequence"), 1)
        event_id = _identifier(event.get("id"))
        if sequence is None:
            raise JourneyError("event_original_invalid")
        artifact_id: str | None = None
        revision: int | None = None
        if kind == "artifact.recorded":
            record = client._object(client._object(event.get("body")).get("artifact_revision"))
            artifact_id = _identifier(record.get("artifact_id"))
            revision = client._integer(record.get("revision"), 1)
            if revision is None or record.get("from") != author:
                raise JourneyError("event_original_invalid")
        return Observed(event_id, sequence, kind, original, hashlib.sha256(raw).hexdigest(), author, artifact_id, revision)
    except JourneyError:
        raise
    except (civic.DecisionError, ValueError, TypeError, UnicodeError, KeyError):
        raise JourneyError("event_original_invalid") from None


def observe_journal(path: Path) -> Observed:
    """Use only a journal the room marked recorded. Any other state stays unresolved."""
    try:
        journal = client._decode(_read_capped(path))
    except (civic.DecisionError, ValueError, TypeError, UnicodeError):
        raise JourneyError("event_original_invalid") from None
    if journal.get("state") != "recorded":
        raise JourneyError("journal_not_recorded")
    original = journal.get("event_record_utf8")
    if not isinstance(original, str) or not original:
        raise JourneyError("event_original_missing")
    return observe_original(original)


def _envelope(kind: str, world: str, author: str, recipients: list[str], submission_id: str) -> JsonObject:
    record: JsonObject = {"protocol_version": "0.1-draft", "type": kind, "id": _identifier(submission_id),
        "world": _identifier(world), "from": _identifier(author), "to": _audience(recipients)}
    return record


def _offer(title: str, purpose: str, refs: list[Observed]) -> JsonObject:
    return {"format": "agentciv-activity-offer/0.1-example", "title": _prose(title), "purpose": _prose(purpose),
        "offered_scope": "Read this synthetic source and, if you choose, keep your own interpretation.",
        "limitations": OFFER_LIMITS, "copying_conditions": OFFER_COPYING,
        "source_refs": [item.source_ref() for item in refs]}


def _source_body() -> JsonObject:
    return {"reading_table_source": {"format": "agentciv-reading-table-source/0.1-example",
        "copying_condition": "operator_authorized_synthetic_fixture_export_only",
        "passages": [{"id": passage_id, "text": text} for passage_id, text in STORY]}}


def source_record(world: str, author: str, recipients: list[str], submission_id: str,
                  artifact_id: str = ARTIFACT_ORIGINAL) -> JsonObject:
    record = _envelope("artifact_revision", world, author, recipients, submission_id)
    record.update({"artifact_id": _identifier(artifact_id), "media_type": "application/json", "body": _source_body()})
    return record


def passage_record(world: str, author: str, recipients: list[str], submission_id: str, source: Observed,
                   artifact_id: str = ARTIFACT_TABLE) -> JsonObject:
    if source.kind != "artifact.recorded":
        raise JourneyError("not_an_artifact")
    record = _envelope("artifact_revision", world, author, recipients, submission_id)
    record.update({"artifact_id": _identifier(artifact_id), "media_type": "application/json",
        "derived_from": source.derivation(),
        "continuity_note": {"aim": "Keep attributed notes close to exact passages.",
            "resume_hint": "These notes are unfinished; no one is obliged to complete them."},
        "body": {"activity_offer": _offer("Passage notes",
            "Keep separate notes beside each passage instead of seeking a single interpretation.", [source]),
            "reading_table_practice": {"format": "agentciv-reading-table-practice/0.1-example",
                "organizes_by": "passages", "source": source.selector(), "basis": source.selector(),
                "state": "unfinished",
                "contribution": [{"passage_id": passage_id, "note": note} for passage_id, note in PASSAGE_NOTES]}}})
    return record


def question_record(world: str, author: str, recipients: list[str], submission_id: str, source: Observed,
                    passages: Observed, artifact_id: str = ARTIFACT_TABLE) -> JsonObject:
    if source.kind != "artifact.recorded" or passages.kind != "artifact.recorded":
        raise JourneyError("not_an_artifact")
    if passages.author == author and passages.artifact_id == artifact_id:
        raise JourneyError("invalid_derivation")
    record = _envelope("artifact_revision", world, author, recipients, submission_id)
    record.update({"artifact_id": _identifier(artifact_id), "media_type": "application/json",
        "derived_from": passages.derivation(),
        "continuity_note": {"aim": "Keep cross-passage questions unresolved and separately attributed.",
            "resume_hint": "A reader may answer, change organization, or leave this unfinished."},
        "body": {"activity_offer": _offer("Open questions",
            "Organize a separate reading by unresolved questions, including questions crossing passages.",
            [source, passages]),
            "reading_table_practice": {"format": "agentciv-reading-table-practice/0.1-example",
                "organizes_by": "open_questions", "source": source.selector(), "basis": passages.selector(),
                "state": "unfinished", "contribution": [
                    {"question": question, "passage_ids": list(passage_ids)} for question, passage_ids in OPEN_QUESTIONS]}}})
    return record


def response_record(kind: str, world: str, author: str, recipients: list[str], submission_id: str,
                    target: Observed, text: str) -> JsonObject:
    if kind not in ("objection", "decline") or target.kind != "artifact.recorded":
        raise JourneyError("not_an_artifact")
    record = _envelope(kind, world, author, recipients, submission_id)
    record.update({"target_from": target.author, "artifact_id": target.artifact_id, "revision": target.revision,
        "body": {"text": _prose(text)}})
    return record


def adoption_record(world: str, author: str, recipients: list[str], submission_id: str, practice: Observed,
                    statement: str, scope: str = SCOPE) -> JsonObject:
    if practice.kind != "artifact.recorded":
        raise JourneyError("not_an_artifact")
    record = _envelope("message", world, author, recipients, submission_id)
    record["body"] = {"practice_adoption": {"format": "agentciv-reading-table-adoption/0.1-example",
        "practice": practice.selector(), "scope": _prose(scope), "statement": _prose(statement)}}
    return record


def note_record(world: str, author: str, submission_id: str, text: str) -> JsonObject:
    """Address a note only to its author. A later grant does not reveal it."""
    record = _envelope("message", world, author, [_identifier(author)], submission_id)
    record["body"] = {"text": _prose(text)}
    return record


def malformed_offer_record(world: str, author: str, recipients: list[str], submission_id: str,
                           artifact_id: str = ARTIFACT_TABLE) -> JsonObject:
    """A newer ordinary artifact whose offer title contains a control character.

    The host may accept it. The offer projection then suppresses the older offer
    instead of repairing or reviving that title.
    """
    record = _envelope("artifact_revision", world, author, recipients, submission_id)
    record.update({"artifact_id": _identifier(artifact_id), "media_type": "application/json",
        "body": {"text": "A newer revision that is not a usable offer.",
            "activity_offer": {"format": "agentciv-activity-offer/0.1-example", "title": "Passage notes\n",
                "purpose": "This newer revision is intentionally unusable as an offer.",
                "offered_scope": "None.", "limitations": "Malformed example.",
                "copying_conditions": OFFER_COPYING, "source_refs": []}}})
    return record


def withdrawal_record(world: str, author: str, submission_id: str, target: Observed) -> JsonObject:
    if target.kind != "artifact.recorded" or author != target.author:
        raise JourneyError("withdrawal_author_mismatch")
    return {"protocol_version": "0.1-draft", "type": "withdrawal", "id": _identifier(submission_id),
        "world": _identifier(world), "from": _identifier(author), "target_from": target.author,
        "artifact_id": target.artifact_id, "revision": target.revision}


def package_bytes(view_path: Path, *, export_synthetic: bool) -> bytes:
    """Wrap exact saved event strings. Host read permission is not this export."""
    if export_synthetic is not True:
        raise JourneyError("export_not_affirmed")
    try:
        view = client._decode(_read_capped(view_path))
        snapshot = client._object(view.get("snapshot"))
        rows = snapshot.get("records")
        world = snapshot.get("world")
        if (view.get("format") != client.VIEW_FORMAT or not isinstance(world, str)
            or not isinstance(rows, list) or not rows or len(rows) > 32
            or not all(isinstance(row, str) and row for row in rows)):
            raise JourneyError("invalid_view")
    except JourneyError:
        raise
    except (civic.DecisionError, ValueError, TypeError, UnicodeError):
        raise JourneyError("invalid_view") from None
    payload: JsonObject = {"format": "agentciv-reading-table/0.1-example", "world": world,
        "copying_condition": "operator_authorized_synthetic_fixture_export_only",
        "records": [{"original_utf8": row, "sha256": hashlib.sha256(str(row).encode("utf-8")).hexdigest()} for row in rows],
        "producer_report": {"success": True, "winner": "producer flags are ignored; no winner is established"}}
    return json.dumps(payload, ensure_ascii=True, indent=2, allow_nan=False).encode("ascii") + b"\n"


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    def common(name: str, help_text: str) -> argparse.ArgumentParser:
        command = commands.add_parser(name, help=help_text)
        command.add_argument("--world", required=True)
        command.add_argument("--from", dest="author", required=True)
        command.add_argument("--id", required=True)
        command.add_argument("--to", dest="recipients", action="append", required=True)
        return command

    source = common("source", "write the synthetic story artifact")
    source.add_argument("--artifact-id", default=ARTIFACT_ORIGINAL)
    passages = common("passages", "write passage notes citing one source journal")
    passages.add_argument("--source-journal", type=Path, required=True)
    passages.add_argument("--artifact-id", default=ARTIFACT_TABLE)
    questions = common("questions", "write a separate open-question chain")
    questions.add_argument("--source-journal", type=Path, required=True)
    questions.add_argument("--basis-journal", type=Path, required=True)
    questions.add_argument("--artifact-id", default=ARTIFACT_TABLE)
    for name, help_text in (("objection", "object to one recorded artifact"), ("decline", "decline one recorded artifact")):
        response = common(name, help_text)
        response.add_argument("--target-journal", type=Path, required=True)
    adoption = common("adoption", "record one author's own next-reading choice")
    adoption.add_argument("--practice-journal", type=Path, required=True)
    adoption.add_argument("--statement", required=True)
    adoption.add_argument("--scope", default=SCOPE)
    note = commands.add_parser("note", help="write a note addressed only to its author")
    note.add_argument("--world", required=True)
    note.add_argument("--from", dest="author", required=True)
    note.add_argument("--id", required=True)
    note.add_argument("--text", required=True)
    malformed = common("malformed-offer", "write a newer offer whose title contains a control character")
    malformed.add_argument("--artifact-id", default=ARTIFACT_TABLE)
    withdrawal = commands.add_parser("withdrawal", help="withdraw one artifact the same author recorded")
    withdrawal.add_argument("--world", required=True)
    withdrawal.add_argument("--from", dest="author", required=True)
    withdrawal.add_argument("--id", required=True)
    withdrawal.add_argument("--target-journal", type=Path, required=True)
    package = commands.add_parser("package", help="wrap one saved view for the offline inspector")
    package.add_argument("--view", type=Path, required=True)
    package.add_argument("--export-synthetic", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "source":
            record = source_record(args.world, args.author, args.recipients, args.id, args.artifact_id)
        elif args.command == "passages":
            record = passage_record(args.world, args.author, args.recipients, args.id, observe_journal(args.source_journal), args.artifact_id)
        elif args.command == "questions":
            record = question_record(args.world, args.author, args.recipients, args.id, observe_journal(args.source_journal),
                observe_journal(args.basis_journal), args.artifact_id)
        elif args.command in ("objection", "decline"):
            text = ("Passage order can hide a question spanning the gap and return. "
                "I object to treating this organization as the only shared reading." if args.command == "objection"
                else "I will not finish the passage notes or a joint interpretation. The invitation may remain for another visitor.")
            record = response_record(args.command, args.world, args.author, args.recipients, args.id, observe_journal(args.target_journal), text)
        elif args.command == "adoption":
            record = adoption_record(args.world, args.author, args.recipients, args.id, observe_journal(args.practice_journal), args.statement, args.scope)
        elif args.command == "note":
            record = note_record(args.world, args.author, args.id, args.text)
        elif args.command == "malformed-offer":
            record = malformed_offer_record(args.world, args.author, args.recipients, args.id, args.artifact_id)
        elif args.command == "withdrawal":
            record = withdrawal_record(args.world, args.author, args.id, observe_journal(args.target_journal))
        else:
            sys.stdout.write(package_bytes(args.view, export_synthetic=bool(args.export_synthetic)).decode("ascii"))
            return 0
        sys.stdout.write(record_bytes(record).decode("ascii"))
        return 0
    except JourneyError as error:
        sys.stdout.write(json.dumps({"outcome": "failed", "code": error.code}, ensure_ascii=True) + "\n")
        return 1
    except (civic.DecisionError, OSError, ValueError, UnicodeError, TypeError):
        sys.stdout.write(json.dumps({"outcome": "failed", "code": "invalid_input"}, ensure_ascii=True) + "\n")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
