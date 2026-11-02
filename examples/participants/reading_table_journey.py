"""Build one synthetic reading table from host originals, without choosing a winner.

Each command writes authored bytes for ``local_encounter.py submit``. It does not
contact a host, mint a replacement id, retry, or treat silence as a decision.
Selectors come from a recorded journal whose request hash, receipt, world, and
exact event string agree. The prose below is operator-authored fixture text,
shared with the frozen table. Export accepts only those synthetic event shapes.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import NoReturn

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
OFFERED_SCOPE = "Read this synthetic source and, if you choose, keep your own interpretation."
PASSAGE_AIM = "Keep attributed notes close to exact passages."
PASSAGE_HINT = "These notes are unfinished; no one is obliged to complete them."
PASSAGE_PURPOSE = "Keep separate notes beside each passage instead of seeking a single interpretation."
QUESTION_AIM = "Keep cross-passage questions unresolved and separately attributed."
QUESTION_HINT = "A reader may answer, change organization, or leave this unfinished."
QUESTION_PURPOSE = "Organize a separate reading by unresolved questions, including questions crossing passages."
OBJECTION_TEXT = (
    "Passage order can hide a question spanning the gap and return. "
    "I object to treating this organization as the only shared reading."
)
DECLINE_TEXT = (
    "I will not finish the passage notes or a joint interpretation. The invitation may remain for another visitor."
)
MALFORMED_TEXT = "A newer revision that is not a usable offer."
MALFORMED_TITLE = "Passage notes\n"
MALFORMED_PURPOSE = "This newer revision is intentionally unusable as an offer."
MALFORMED_SCOPE = "None."
MALFORMED_LIMITS = "Malformed example."
MAX_INPUT = 4 * 1024 * 1024
MAX_RECORD = 16000
RECORDED_JOURNAL_KEYS = {"format", "world", "principal", "record_id", "record_type", "request_utf8",
    "request_sha256", "endpoint_kind", "state", "copying_permission", "receipt", "event_record_utf8"}
EVENT_KEYS = {"protocol_version", "type", "id", "world", "sequence", "timestamp", "kind", "actor", "body"}
_TIMESTAMP = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z")
_SHA256 = re.compile(r"[0-9a-f]{64}")


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


def _whole(value: object, minimum: int = 1) -> int:
    if type(value) is not int:
        raise JourneyError("not_synthetic_material")
    parsed = client._integer(value, minimum)
    if parsed is None:
        raise JourneyError("not_synthetic_material")
    return parsed


def _refuse_journal() -> NoReturn:
    raise JourneyError("journal_not_correlated")


def _correlated(journal: JsonObject, original: str, world: str) -> None:
    """A recorded label is not enough. The request, receipt, and event must agree."""
    request_text = journal.get("request_utf8")
    digest = journal.get("request_sha256")
    if set(journal) != RECORDED_JOURNAL_KEYS or journal.get("format") != client.JOURNAL_FORMAT:
        _refuse_journal()
    if journal.get("world") != world or journal.get("copying_permission") != "not_granted":
        _refuse_journal()
    if not isinstance(request_text, str) or not isinstance(digest, str) or _SHA256.fullmatch(digest) is None:
        _refuse_journal()
    request_raw = request_text.encode("utf-8")
    if len(request_raw) > MAX_RECORD or len(original.encode("utf-8")) > MAX_RECORD:
        _refuse_journal()
    if hashlib.sha256(request_raw).hexdigest() != digest:
        _refuse_journal()
    request = client._decode(request_raw)
    event = client._decode(original.encode("utf-8"))
    record_type = request.get("type")
    if record_type not in ("message", "artifact_revision", "objection", "decline", "withdrawal"):
        _refuse_journal()
    capability = "messages.submit" if record_type == "message" else "collaboration.submit"
    if journal.get("record_id") != request.get("id") or journal.get("record_type") != record_type:
        _refuse_journal()
    if journal.get("endpoint_kind") != capability or journal.get("principal") != request.get("from"):
        _refuse_journal()
    if request.get("world") != world or event.get("world") != world or event.get("actor") != request.get("from"):
        _refuse_journal()
    timestamp = event.get("timestamp")
    if (event.get("protocol_version") != "0.1-draft" or event.get("type") != "event"
        or not isinstance(timestamp, str) or _TIMESTAMP.fullmatch(timestamp) is None):
        _refuse_journal()
    receipt = client._object(journal.get("receipt"))
    receipt_keys = {"protocol_version", "type", "status", "world", "record_id", "event_id", "sequence"}
    if record_type == "artifact_revision":
        receipt_keys |= {"artifact_id", "revision"}
    if set(receipt) != receipt_keys or receipt.get("protocol_version") != "0.1-draft" or receipt.get("type") != "receipt":
        _refuse_journal()
    if receipt.get("status") != "recorded" or receipt.get("world") != world or receipt.get("record_id") != request.get("id"):
        _refuse_journal()
    if receipt.get("event_id") != event.get("id") or type(receipt.get("sequence")) is not int or receipt.get("sequence") != event.get("sequence"):
        _refuse_journal()
    if record_type == "artifact_revision" and "revision" in request:
        _refuse_journal()
    if record_type == "artifact_revision":
        if (receipt.get("artifact_id") != request.get("artifact_id") or type(receipt.get("revision")) is not int
            or not client._matches_authored(request, event, receipt.get("revision"))):
            _refuse_journal()
    elif record_type == "withdrawal":
        if event.get("kind") != "artifact.withdrawn" or event.get("body") != {}:
            _refuse_journal()
    elif not client._matches_authored(request, event):
        _refuse_journal()


def observe_journal(path: Path, world: str) -> Observed:
    """Use a recorded room journal only when its receipt still matches the saved event."""
    expected = _identifier(world)
    try:
        journal = client._decode(_read_capped(path))
    except (civic.DecisionError, ValueError, TypeError, UnicodeError):
        raise JourneyError("event_original_invalid") from None
    if journal.get("state") != "recorded":
        raise JourneyError("journal_not_recorded")
    original = journal.get("event_record_utf8")
    if not isinstance(original, str) or not original:
        raise JourneyError("event_original_missing")
    try:
        _correlated(journal, original, expected)
    except JourneyError:
        raise
    except (civic.DecisionError, ValueError, TypeError, UnicodeError, KeyError):
        raise JourneyError("journal_not_correlated") from None
    return observe_original(original)


def _envelope(kind: str, world: str, author: str, recipients: list[str], submission_id: str) -> JsonObject:
    record: JsonObject = {"protocol_version": "0.1-draft", "type": kind, "id": _identifier(submission_id),
        "world": _identifier(world), "from": _identifier(author), "to": _audience(recipients)}
    return record


def _offer(title: str, purpose: str, refs: list[Observed]) -> JsonObject:
    return {"format": "agentciv-activity-offer/0.1-example", "title": _prose(title), "purpose": _prose(purpose),
        "offered_scope": OFFERED_SCOPE, "limitations": OFFER_LIMITS, "copying_conditions": OFFER_COPYING,
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
        "continuity_note": {"aim": PASSAGE_AIM, "resume_hint": PASSAGE_HINT},
        "body": {"activity_offer": _offer("Passage notes", PASSAGE_PURPOSE, [source]),
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
        "continuity_note": {"aim": QUESTION_AIM, "resume_hint": QUESTION_HINT},
        "body": {"activity_offer": _offer("Open questions", QUESTION_PURPOSE, [source, passages]),
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
        "body": {"text": MALFORMED_TEXT,
            "activity_offer": {"format": "agentciv-activity-offer/0.1-example", "title": MALFORMED_TITLE,
                "purpose": MALFORMED_PURPOSE, "offered_scope": MALFORMED_SCOPE, "limitations": MALFORMED_LIMITS,
                "copying_conditions": OFFER_COPYING, "source_refs": []}}})
    return record


def withdrawal_record(world: str, author: str, submission_id: str, target: Observed) -> JsonObject:
    if target.kind != "artifact.recorded" or author != target.author:
        raise JourneyError("withdrawal_author_mismatch")
    return {"protocol_version": "0.1-draft", "type": "withdrawal", "id": _identifier(submission_id),
        "world": _identifier(world), "from": _identifier(author), "target_from": target.author,
        "artifact_id": target.artifact_id, "revision": target.revision}


def _exact(value: object, keys: set[str]) -> JsonObject:
    item = client._object(value)
    if set(item) != keys:
        raise JourneyError("not_synthetic_material")
    return item


def _names(value: object) -> None:
    if not isinstance(value, list) or not value:
        raise JourneyError("not_synthetic_material")
    chosen: list[str] = []
    for item in value:
        name = _identifier(item)
        if name in chosen:
            raise JourneyError("not_synthetic_material")
        chosen.append(name)


def _citation(value: object, *, event_id: bool) -> None:
    keys = {"from", "artifact_id", "revision"} | ({"event_id"} if event_id else set())
    item = _exact(value, keys)
    if event_id:
        _identifier(item.get("event_id"))
    _identifier(item.get("from"))
    _identifier(item.get("artifact_id"))
    _whole(item.get("revision"))


def _selector(value: object) -> None:
    item = _exact(value, {"event_id", "from", "artifact_id", "revision", "original_sha256"})
    _identifier(item.get("event_id"))
    _identifier(item.get("from"))
    _identifier(item.get("artifact_id"))
    _whole(item.get("revision"))
    hashed = item.get("original_sha256")
    if not isinstance(hashed, str) or _SHA256.fullmatch(hashed) is None:
        raise JourneyError("not_synthetic_material")


def _contains_sentinel(value: object) -> bool:
    if isinstance(value, str):
        return PRIVATE_SENTINEL in value
    if isinstance(value, dict):
        return any(_contains_sentinel(key) or _contains_sentinel(item) for key, item in value.items())
    if isinstance(value, list):
        return any(_contains_sentinel(item) for item in value)
    return False


def _activity_offer(value: object, *, title: str, purpose: str, scope: str, limitations: str, refs: int) -> None:
    offer = _exact(value, {"format", "title", "purpose", "offered_scope", "limitations", "copying_conditions", "source_refs"})
    chosen = offer.get("source_refs")
    if (offer.get("format") != "agentciv-activity-offer/0.1-example" or offer.get("title") != title
        or offer.get("purpose") != purpose or offer.get("offered_scope") != scope
        or offer.get("limitations") != limitations or offer.get("copying_conditions") != OFFER_COPYING
        or not isinstance(chosen, list) or len(chosen) != refs):
        raise JourneyError("not_synthetic_material")
    for item in chosen:
        _citation(item, event_id=True)


def _authored_common(record: JsonObject, world: str, author: str, record_type: str) -> JsonObject:
    if (record.get("protocol_version") != "0.1-draft" or record.get("type") != record_type
        or record.get("world") != world or record.get("from") != author):
        raise JourneyError("not_synthetic_material")
    _identifier(record.get("id"))
    _names(record.get("to"))
    body = record.get("body")
    if not isinstance(body, dict):
        raise JourneyError("not_synthetic_material")
    return body


def _synthetic_event(event: JsonObject, world: str) -> None:
    shell = _exact(event, EVENT_KEYS)
    timestamp = shell.get("timestamp")
    author = shell.get("actor")
    if (shell.get("protocol_version") != "0.1-draft" or shell.get("type") != "event" or shell.get("world") != world
        or not isinstance(timestamp, str) or _TIMESTAMP.fullmatch(timestamp) is None):
        raise JourneyError("not_synthetic_material")
    author_name = _identifier(author)
    _identifier(shell.get("id"))
    _whole(shell.get("sequence"))
    kind = shell.get("kind")
    body = shell.get("body")
    if kind == "artifact.withdrawn":
        if body != {}:
            raise JourneyError("not_synthetic_material")
        return
    if kind == "artifact.recorded":
        _synthetic_artifact(body, world, author_name)
        return
    if kind == "objection.recorded":
        _synthetic_response(body, "objection", world, author_name, OBJECTION_TEXT)
        return
    if kind == "decline.recorded":
        _synthetic_response(body, "decline", world, author_name, DECLINE_TEXT)
        return
    if kind == "message.recorded":
        _synthetic_adoption(body, world, author_name)
        return
    raise JourneyError("not_synthetic_material")


def _synthetic_artifact(body: object, world: str, author: str) -> None:
    if not isinstance(body, dict) or set(body) != {"artifact_revision"}:
        raise JourneyError("not_synthetic_material")
    record = client._object(body.get("artifact_revision"))
    keys = set(record)
    material = record.get("body")
    if not isinstance(material, dict):
        raise JourneyError("not_synthetic_material")
    base = {"protocol_version", "type", "id", "world", "from", "to", "artifact_id", "media_type", "body", "revision"}
    if record.get("media_type") != "application/json":
        raise JourneyError("not_synthetic_material")
    _identifier(record.get("artifact_id"))
    _whole(record.get("revision"))
    inner = _authored_common(record, world, author, "artifact_revision")
    if keys == base and set(material) == {"reading_table_source"}:
        source = _exact(inner.get("reading_table_source"), {"format", "copying_condition", "passages"})
        expected = [{"id": passage_id, "text": text} for passage_id, text in STORY]
        if (source.get("format") != "agentciv-reading-table-source/0.1-example"
            or source.get("copying_condition") != "operator_authorized_synthetic_fixture_export_only"
            or not client._json_equal(source.get("passages"), expected)):
            raise JourneyError("not_synthetic_material")
        return
    if keys == base | {"derived_from", "continuity_note"} and set(material) == {"activity_offer", "reading_table_practice"}:
        _synthetic_practice(record, inner)
        return
    if keys == base and set(material) == {"text", "activity_offer"}:
        if inner.get("text") != MALFORMED_TEXT:
            raise JourneyError("not_synthetic_material")
        _activity_offer(inner.get("activity_offer"), title=MALFORMED_TITLE, purpose=MALFORMED_PURPOSE,
            scope=MALFORMED_SCOPE, limitations=MALFORMED_LIMITS, refs=0)
        return
    raise JourneyError("not_synthetic_material")


def _synthetic_practice(record: JsonObject, body: JsonObject) -> None:
    _citation(record.get("derived_from"), event_id=False)
    continuity = _exact(record.get("continuity_note"), {"aim", "resume_hint"})
    practice = _exact(body.get("reading_table_practice"), {"format", "organizes_by", "source", "basis", "state", "contribution"})
    if practice.get("format") != "agentciv-reading-table-practice/0.1-example" or practice.get("state") != "unfinished":
        raise JourneyError("not_synthetic_material")
    _selector(practice.get("source"))
    _selector(practice.get("basis"))
    organizes_by = practice.get("organizes_by")
    if organizes_by == "passages":
        contribution: list[JsonObject] = [{"passage_id": passage_id, "note": note} for passage_id, note in PASSAGE_NOTES]
        if (not client._json_equal(practice.get("contribution"), contribution) or continuity.get("aim") != PASSAGE_AIM
            or continuity.get("resume_hint") != PASSAGE_HINT):
            raise JourneyError("not_synthetic_material")
        _activity_offer(body.get("activity_offer"), title="Passage notes", purpose=PASSAGE_PURPOSE,
            scope=OFFERED_SCOPE, limitations=OFFER_LIMITS, refs=1)
        return
    if organizes_by == "open_questions":
        contribution = [{"question": question, "passage_ids": list(passage_ids)} for question, passage_ids in OPEN_QUESTIONS]
        if (not client._json_equal(practice.get("contribution"), contribution) or continuity.get("aim") != QUESTION_AIM
            or continuity.get("resume_hint") != QUESTION_HINT):
            raise JourneyError("not_synthetic_material")
        _activity_offer(body.get("activity_offer"), title="Open questions", purpose=QUESTION_PURPOSE,
            scope=OFFERED_SCOPE, limitations=OFFER_LIMITS, refs=2)
        return
    raise JourneyError("not_synthetic_material")


def _synthetic_response(body: object, key: str, world: str, author: str, text: str) -> None:
    if not isinstance(body, dict) or set(body) != {key}:
        raise JourneyError("not_synthetic_material")
    record = _exact(body.get(key), {"protocol_version", "type", "id", "world", "from", "to",
        "target_from", "artifact_id", "revision", "body"})
    inner = _authored_common(record, world, author, key)
    if set(inner) != {"text"} or inner.get("text") != text:
        raise JourneyError("not_synthetic_material")
    _identifier(record.get("target_from"))
    _identifier(record.get("artifact_id"))
    _whole(record.get("revision"))


def _synthetic_adoption(body: object, world: str, author: str) -> None:
    if not isinstance(body, dict) or set(body) != {"message"}:
        raise JourneyError("not_synthetic_material")
    record = _exact(body.get("message"), {"protocol_version", "type", "id", "world", "from", "to", "body"})
    inner = _authored_common(record, world, author, "message")
    if set(inner) != {"practice_adoption"}:
        raise JourneyError("not_synthetic_material")
    adoption = _exact(inner.get("practice_adoption"), {"format", "practice", "scope", "statement"})
    if (adoption.get("format") != "agentciv-reading-table-adoption/0.1-example" or adoption.get("scope") != SCOPE
        or adoption.get("statement") not in (PASSAGE_STATEMENT, QUESTION_STATEMENT)):
        raise JourneyError("not_synthetic_material")
    _selector(adoption.get("practice"))


def _require_synthetic(event: JsonObject, world: str) -> None:
    try:
        if _contains_sentinel(event):
            raise JourneyError("not_synthetic_material")
        _synthetic_event(event, world)
    except (JourneyError, civic.DecisionError, ValueError, TypeError, UnicodeError, KeyError) as error:
        raise JourneyError("not_synthetic_material") from error


def package_bytes(view_path: Path, *, export_synthetic: bool) -> bytes:
    """Wrap exact saved event strings. Host read permission is not this export."""
    if export_synthetic is not True:
        raise JourneyError("export_not_affirmed")
    try:
        raw = _read_capped(view_path)
        view = client._decode(raw)
        snapshot = client._object(view.get("snapshot"))
        rows = snapshot.get("records")
        world = snapshot.get("world")
        if (view.get("format") != client.VIEW_FORMAT or not isinstance(world, str)
            or not isinstance(rows, list) or not rows or len(rows) > 32):
            raise JourneyError("invalid_view")
        originals: list[str] = []
        for row in rows:
            if not isinstance(row, str) or not row:
                raise JourneyError("invalid_view")
            originals.append(row)
    except JourneyError:
        raise
    except (civic.DecisionError, ValueError, TypeError, UnicodeError):
        raise JourneyError("invalid_view") from None
    if PRIVATE_SENTINEL.encode("ascii") in raw or _contains_sentinel(world):
        raise JourneyError("not_synthetic_material")
    try:
        checked_world = _identifier(world)
        for row in originals:
            _require_synthetic(client._decode(row.encode("utf-8")), checked_world)
    except JourneyError as error:
        if error.code != "not_synthetic_material":
            raise JourneyError("not_synthetic_material") from error
        raise
    except (civic.DecisionError, ValueError, TypeError, UnicodeError, KeyError) as error:
        raise JourneyError("not_synthetic_material") from error
    payload: JsonObject = {"format": "agentciv-reading-table/0.1-example", "world": checked_world,
        "copying_condition": "operator_authorized_synthetic_fixture_export_only",
        "records": [{"original_utf8": row, "sha256": hashlib.sha256(row.encode("utf-8")).hexdigest()} for row in originals],
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
            record = passage_record(args.world, args.author, args.recipients, args.id,
                observe_journal(args.source_journal, args.world), args.artifact_id)
        elif args.command == "questions":
            record = question_record(args.world, args.author, args.recipients, args.id,
                observe_journal(args.source_journal, args.world), observe_journal(args.basis_journal, args.world), args.artifact_id)
        elif args.command in ("objection", "decline"):
            text = OBJECTION_TEXT if args.command == "objection" else DECLINE_TEXT
            record = response_record(args.command, args.world, args.author, args.recipients, args.id,
                observe_journal(args.target_journal, args.world), text)
        elif args.command == "adoption":
            record = adoption_record(args.world, args.author, args.recipients, args.id,
                observe_journal(args.practice_journal, args.world), args.statement, args.scope)
        elif args.command == "note":
            record = note_record(args.world, args.author, args.id, args.text)
        elif args.command == "malformed-offer":
            record = malformed_offer_record(args.world, args.author, args.recipients, args.id, args.artifact_id)
        elif args.command == "withdrawal":
            record = withdrawal_record(args.world, args.author, args.id, observe_journal(args.target_journal, args.world))
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
