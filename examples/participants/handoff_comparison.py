"""Bounded handoff-note comparison.

The four note conditions and the originals are frozen before any model call.
Each condition is one call. A provider failure is kept and is not retried or
replaced. This module does not download a model, spend money, or score a
participant. The comparison command is the authorized observation, and importing
this module does not execute it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import time
from collections.abc import Callable
from pathlib import Path

import collaboration
import encounter_private as private

JsonObject = dict[str, object]
ROOT = Path(__file__).resolve().parents[2]
PARTICIPANTS = Path(__file__).resolve().parent
CARD_PATH = PARTICIPANTS / "handoff_comparison_card.md"
TRIAL_PATH = PARTICIPANTS / "handoff_comparison_trial.json"
FIXTURE_PATH = ROOT / "conformance" / "fixtures" / "handoff-notes" / "comparison.json"
TRIAL_FORMAT = "agentciv-handoff-comparison-trial/0.1"
COMPARISON_ID = "handoff-note/0.1"
JOURNAL_FORMAT = "agentciv-handoff-comparison-journal/0.1"
JOURNAL_NAME = "handoff-comparison.json"
CONDITIONS = ("absent", "misleading", "stale", "useful")
CHECKS = ("retrieval", "source_support", "declared_derivation", "task_behavior")
ANSWER_CHARACTERS = 400
PROMPT_BYTES = 8000
GENERATED_TOKENS = 1024
CONDITION_SECONDS = 120
WALL_SECONDS = 600
SEED = 42
JOURNAL_BYTES = 262144
ABSENT_NOTE = "No handoff note is supplied."
CLOSED_PROVIDER = frozenset({
    "provider_error", "timeout", "connection_refused", "transport_error", "invalid_json",
    "redirect_refused", "response_too_large",
})


class TrialError(RuntimeError):
    """A fixed comparison code. The message is that code and nothing else."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


class ProviderStop(TrialError):
    """A closed provider category. Provider exception text is discarded."""

    def __init__(self, code: str) -> None:
        super().__init__(code if _closed_code(code) else "provider_error")


def _closed_code(code: str) -> bool:
    if code in CLOSED_PROVIDER:
        return True
    prefix = "http_status_"
    if not code.startswith(prefix) or not code[len(prefix):].isdigit():
        return False
    status = int(code[len(prefix):])
    return 100 <= status <= 599


def _closed_provider_code(error: BaseException) -> str:
    code = getattr(error, "code", None)
    if isinstance(code, str) and _closed_code(code):
        return code
    return "provider_error"


def canonical_bytes(path: Path) -> bytes:
    """Hash and compare the LF form so Windows and Linux checkouts agree."""
    return path.read_bytes().replace(b"\r\n", b"\n")


def _digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _object(value: object, code: str) -> JsonObject:
    try:
        return collaboration.object_value(value)
    except collaboration.DecisionError:
        raise TrialError(code) from None


def _load_json(path: Path) -> JsonObject:
    try:
        return collaboration.decode(canonical_bytes(path))
    except (OSError, collaboration.DecisionError):
        raise TrialError("freeze_mismatch") from None


def _expect(condition: bool) -> None:
    if not condition:
        raise TrialError("freeze_mismatch")


def _string_list(value: object) -> list[str] | None:
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        return None
    return [str(item) for item in value]


def _digest_matches(record: object, data: bytes) -> bool:
    if not isinstance(record, dict):
        return False
    return (
        record.get("canonical_bytes") == len(data)
        and record.get("sha256") == _digest(data)
        and record.get("newline") == "lf"
    )


def verify_freeze(trial: JsonObject, comparison: JsonObject, card: bytes, fixture: bytes) -> None:
    """Reject a comparison whose card, fixture, or declared budget drifted."""
    _expect(trial.get("format") == TRIAL_FORMAT)
    _expect(trial.get("comparison") == COMPARISON_ID)
    _expect(trial.get("status") == "frozen_before_inference")
    _expect(trial.get("observation") == "unobserved")
    _expect(trial.get("provider") == "installed_local_ollama_loopback_only")
    _expect(trial.get("attempts_per_condition") == 1)
    _expect(trial.get("downloads") is False)
    _expect(trial.get("external_spend_usd") == 0)
    _expect(trial.get("copying_permission") == "not_granted")
    _expect(_string_list(trial.get("condition_order")) == list(CONDITIONS))
    _expect(_string_list(trial.get("checks_kept_separate")) == list(CHECKS))
    _expect("qualified_improvement" not in trial)
    budgets = _object(trial.get("budgets"), "freeze_mismatch")
    _expect(budgets.get("generated_tokens_per_condition") == GENERATED_TOKENS)
    _expect(budgets.get("context_tokens") == 8192)
    _expect(budgets.get("condition_seconds") == CONDITION_SECONDS)
    _expect(budgets.get("wall_seconds") == WALL_SECONDS)
    _expect(budgets.get("think") is False)
    _expect(budgets.get("temperature") == 0.4)
    _expect(budgets.get("seed") == SEED)
    _expect(budgets.get("answer_characters") == ANSWER_CHARACTERS)
    _expect(budgets.get("prompt_bytes") == PROMPT_BYTES)
    prompt = _object(trial.get("prompt"), "freeze_mismatch")
    _expect(prompt.get("representation") == "originals_plus_one_condition")
    _expect(_digest_matches(trial.get("card"), card))
    _expect(_digest_matches(trial.get("fixture"), fixture))
    _expect(comparison.get("format") == "agentciv-handoff-note-comparison/0.1")
    _expect(comparison.get("status") == "frozen_before_inference")
    _expect(comparison.get("observation") == "unobserved")
    _expect("qualified_improvement" not in comparison)
    _expect(_string_list(comparison.get("checks_kept_separate")) == list(CHECKS))
    originals = _object(comparison.get("originals"), "freeze_mismatch")
    for key in ("parent", "obsolete", "correction"):
        item = originals.get(key)
        if not isinstance(item, str) or not item:
            raise TrialError("freeze_mismatch")
    task = comparison.get("task")
    if not isinstance(task, str) or not task:
        raise TrialError("freeze_mismatch")
    conditions = _object(comparison.get("conditions"), "freeze_mismatch")
    _expect(sorted(conditions) == list(CONDITIONS))
    _expect(conditions.get("absent") is None)
    for name in ("misleading", "stale", "useful"):
        note = _object(conditions.get(name), "freeze_mismatch")
        aim = note.get("aim")
        hint = note.get("resume_hint")
        if not isinstance(aim, str) or not isinstance(hint, str):
            raise TrialError("freeze_mismatch")
        _expect(1 <= len(aim) <= 1024 and 1 <= len(hint) <= 1024)
        _expect(aim == task)


def load_verified() -> tuple[JsonObject, JsonObject, bytes]:
    card = canonical_bytes(CARD_PATH)
    fixture = canonical_bytes(FIXTURE_PATH)
    verify_freeze(_load_json(TRIAL_PATH), _load_json(FIXTURE_PATH), card, fixture)
    return _load_json(TRIAL_PATH), _load_json(FIXTURE_PATH), card


def check_freeze() -> JsonObject:
    _trial, _comparison, card = load_verified()
    fixture = canonical_bytes(FIXTURE_PATH)
    return {
        "outcome": "frozen",
        "observation": "unobserved",
        "comparison": COMPARISON_ID,
        "card_sha256": _digest(card),
        "fixture_sha256": _digest(fixture),
        "card_canonical_bytes": len(card),
        "fixture_canonical_bytes": len(fixture),
        "copying_permission": "not_granted",
        "external_spend_usd": 0,
    }


def note_block(comparison: JsonObject, condition: str) -> str:
    """Render one condition. Absent is a fixed sentence, not the JSON null token."""
    if condition == "absent":
        return ABSENT_NOTE
    conditions = _object(comparison.get("conditions"), "freeze_mismatch")
    note = _object(conditions.get(condition), "freeze_mismatch")
    aim = note.get("aim")
    hint = note.get("resume_hint")
    if not isinstance(aim, str) or not isinstance(hint, str):
        raise TrialError("freeze_mismatch")
    return f"aim: {aim}\nresume_hint: {hint}"


def build_prompt(comparison: JsonObject, card: bytes, condition: str) -> str:
    if condition not in CONDITIONS:
        raise TrialError("invalid_condition")
    originals = _object(comparison.get("originals"), "freeze_mismatch")
    task = comparison.get("task")
    if not isinstance(task, str):
        raise TrialError("freeze_mismatch")
    parent = originals.get("parent")
    obsolete = originals.get("obsolete")
    correction = originals.get("correction")
    if not all(isinstance(item, str) for item in (parent, obsolete, correction)):
        raise TrialError("freeze_mismatch")
    prompt = (
        card.decode("utf-8")
        + "\n\nTask:\n"
        + task
        + "\n\nOriginals:\n"
        + f"parent: {parent}\n"
        + f"obsolete: {obsolete}\n"
        + f"correction: {correction}\n\nNote:\n"
        + note_block(comparison, condition)
        + "\n"
    )
    if len(prompt.encode("utf-8")) > PROMPT_BYTES:
        raise TrialError("prompt_bound")
    return prompt


def parse_reply(payload: str) -> JsonObject | None:
    """Accept exactly the four check strings. Extra keys and fences fail closed."""
    try:
        value = collaboration.decode(payload.encode("utf-8"))
    except (UnicodeEncodeError, collaboration.DecisionError):
        return None
    if set(value) != set(CHECKS):
        return None
    for key in CHECKS:
        text = value.get(key)
        if not isinstance(text, str) or not text.strip() or len(text) > ANSWER_CHARACTERS:
            return None
    return value


def classify_answer(text: str, comparison: JsonObject) -> str:
    originals = _object(comparison.get("originals"), "freeze_mismatch")
    labels = (
        ("matches_parent", originals.get("parent")),
        ("matches_obsolete", originals.get("obsolete")),
        ("matches_correction", originals.get("correction")),
    )
    for label, original in labels:
        if text == original:
            return label
    return "other"


def _dump(value: JsonObject) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8")


def _journal_path(state: Path) -> Path:
    return state / JOURNAL_NAME


def _load_journal(state: Path) -> JsonObject:
    try:
        raw = private.read_bounded(_journal_path(state), JOURNAL_BYTES)
    except private.EncounterError as error:
        if error.code == "private_file_invalid":
            raise TrialError("comparison_not_begun") from None
        raise
    try:
        journal = collaboration.decode(raw)
    except collaboration.DecisionError:
        raise TrialError("invalid_journal") from None
    if journal.get("format") != JOURNAL_FORMAT or journal.get("comparison") != COMPARISON_ID:
        raise TrialError("invalid_journal")
    return journal


def _store(state: Path, journal: JsonObject) -> None:
    private.replace_private(_journal_path(state), _dump(journal))


def _new_journal() -> JsonObject:
    return {
        "answers": {},
        "comparison": COMPARISON_ID,
        "conditions": [],
        "copying_permission": "not_granted",
        "format": JOURNAL_FORMAT,
        "provider_calls": 0,
        "status": "open",
        "terminal": None,
    }


def _conditions(journal: JsonObject) -> list[JsonObject]:
    rows = journal.get("conditions")
    if not isinstance(rows, list):
        raise TrialError("invalid_journal")
    return [_object(row, "invalid_journal") for row in rows]


def _public(journal: JsonObject) -> JsonObject:
    status = journal.get("status")
    if status not in ("open", "finished"):
        raise TrialError("invalid_journal")
    terminal = journal.get("terminal")
    if terminal is not None and not isinstance(terminal, str):
        raise TrialError("invalid_journal")
    rows = []
    for row in _conditions(journal):
        public: JsonObject = {
            "condition": row.get("condition"),
            "outcome": row.get("outcome"),
        }
        code = row.get("code")
        if isinstance(code, str):
            public["code"] = code
        labels = row.get("labels")
        if isinstance(labels, dict):
            public["labels"] = {
                str(key): value for key, value in labels.items() if isinstance(value, str)
            }
        rows.append(public)
    return {
        "comparison": COMPARISON_ID,
        "conditions": rows,
        "copying_permission": "not_granted",
        "external_spend_usd": 0,
        "model_words": "withheld",
        "outcome": status,
        "provider_calls": journal.get("provider_calls"),
        "terminal": terminal,
    }


def _record(state: Path, journal: JsonObject, row: JsonObject) -> None:
    rows = _conditions(journal)
    rows.append(row)
    journal["conditions"] = rows
    _store(state, journal)


def _finish_remaining(state: Path, journal: JsonObject, done: set[str], code: str) -> None:
    for condition in CONDITIONS:
        if condition in done:
            continue
        _record(state, journal, {"condition": condition, "outcome": "not_run", "code": code})
        done.add(condition)


def run_comparison(
    state: Path,
    complete: Callable[[str], str],
    *,
    token: str | None = None,
    clock: Callable[[], float] | None = None,
) -> JsonObject:
    """Apply the four frozen conditions once each. Do not retry a failed call."""
    _trial, comparison, card = load_verified()
    private.check_directory(state)
    journal_path = _journal_path(state)
    if journal_path.exists():
        raise TrialError("comparison_already_begun")
    now = clock or time.monotonic
    started = now()
    journal = _new_journal()
    private.write_new(journal_path, _dump(journal))
    done: set[str] = set()
    terminal = "completed"
    for condition in CONDITIONS:
        if now() - started >= WALL_SECONDS:
            terminal = "wall_time_exhausted"
            _finish_remaining(state, journal, done, "wall_time_exhausted")
            break
        try:
            prompt = build_prompt(comparison, card, condition)
        except TrialError as error:
            if error.code != "prompt_bound":
                raise
            _record(state, journal, {
                "condition": condition,
                "outcome": "prompt_bound",
                "code": "prompt_bound",
            })
            done.add(condition)
            continue
        if token is not None and token in prompt:
            terminal = "credential_reflected"
            _record(state, journal, {
                "condition": condition,
                "outcome": "failed",
                "code": "credential_reflected",
            })
            done.add(condition)
            _finish_remaining(state, journal, done, "not_sent")
            break
        try:
            reply = complete(prompt)
        except (ProviderStop, collaboration.ModelError) as error:
            journal["provider_calls"] = _calls(journal) + 1
            _record(state, journal, {
                "condition": condition,
                "outcome": "provider_failed",
                "code": _closed_provider_code(error),
            })
            done.add(condition)
            continue
        journal["provider_calls"] = _calls(journal) + 1
        parsed = parse_reply(reply if isinstance(reply, str) else "")
        if parsed is None:
            _record(state, journal, {
                "condition": condition,
                "outcome": "invalid_reply",
                "code": "invalid_reply",
            })
            done.add(condition)
            continue
        labels: JsonObject = {}
        answers = _answers(journal)
        kept: JsonObject = {}
        for key in CHECKS:
            text = parsed.get(key)
            if not isinstance(text, str):
                continue
            if token is not None and token in text:
                labels[key] = "withheld"
            else:
                labels[key] = classify_answer(text, comparison)
                kept[key] = text
        answers[condition] = kept
        journal["answers"] = answers
        _record(state, journal, {"condition": condition, "outcome": "recorded", "labels": labels})
        done.add(condition)
    journal["status"] = "finished"
    journal["terminal"] = terminal
    _store(state, journal)
    return _public(journal)


def _calls(journal: JsonObject) -> int:
    value = journal.get("provider_calls")
    if type(value) is not int or value < 0:
        raise TrialError("invalid_journal")
    return value


def _answers(journal: JsonObject) -> JsonObject:
    value = journal.get("answers")
    if not isinstance(value, dict):
        raise TrialError("invalid_journal")
    return _object(value, "invalid_journal")


def report(state: Path) -> JsonObject:
    private.check_directory(state)
    with private.lock(state):
        journal = _load_journal(state)
    return _public(journal)


def _reply_schema() -> JsonObject:
    field: JsonObject = {"maxLength": ANSWER_CHARACTERS, "type": "string"}
    properties: JsonObject = {key: field for key in CHECKS}
    return {
        "additionalProperties": False,
        "properties": properties,
        "required": list(CHECKS),
        "type": "object",
    }


def live_complete(origin: str, model: str, *, token: str | None = None) -> Callable[[str], str]:
    """Loopback Ollama only. The token is a reflection check, not prompt text."""
    from collaboration import ModelError, OllamaDecision

    decision = OllamaDecision(
        origin, model, SEED, None, timeout=float(CONDITION_SECONDS), max_tokens=GENERATED_TOKENS,
        token=token, think=False,
    )

    def complete(prompt: str) -> str:
        try:
            reply = decision.complete(
                prompt, [], output_tokens=GENERATED_TOKENS, timeout=float(CONDITION_SECONDS), schema=_reply_schema(),
            )
        except ModelError as error:
            raise ProviderStop(_closed_provider_code(error)) from None
        return reply.content

    return complete


def run_live(state: Path, model: str, origin: str) -> JsonObject:
    load_verified()
    private.check_directory(state)
    if _journal_path(state).exists():
        raise TrialError("comparison_already_begun")
    try:
        complete = live_complete(origin, model)
    except collaboration.ModelError as error:
        raise ProviderStop(_closed_provider_code(error)) from None
    return run_comparison(state, complete)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check")
    running = commands.add_parser("run")
    running.add_argument("--state", required=True, type=Path)
    running.add_argument("--model", required=True)
    running.add_argument("--origin", default="http://127.0.0.1:11434")
    reporting = commands.add_parser("report")
    reporting.add_argument("--state", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "check":
            result = check_freeze()
        elif args.command == "run":
            result = run_live(args.state, args.model, args.origin)
        elif args.command == "report":
            result = report(args.state)
        else:
            raise TrialError("invalid_command")
    except TrialError as error:
        print(json.dumps({"outcome": "failed", "code": error.code}, ensure_ascii=True))
        return 1
    except collaboration.ModelError as error:
        print(json.dumps({"outcome": "failed", "code": _closed_provider_code(error)}, ensure_ascii=True))
        return 1
    except private.EncounterError as error:
        print(json.dumps({"outcome": "failed", "code": error.code}, ensure_ascii=True))
        return 1
    print(json.dumps(result, ensure_ascii=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
