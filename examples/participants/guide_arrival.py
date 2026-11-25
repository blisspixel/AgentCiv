"""Bounded guide-only arrival trial.

The participant card and the operator guide are frozen before any model call.
One visit may take at most six actions. Quiet finishes the visit. A recorded
say finishes the visit. An uncertain say finishes the visit. A rejected say
is not retried and does not by itself finish the visit. This module does not
download a model, spend money, or replace a provider failure with a scripted
message.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import time
import urllib.error
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

import collaboration
import encounter_client as client
import encounter_private as private
import local_participant as wire
import providers
from loopback import loopback_origin

JsonObject = dict[str, object]
ROOT = Path(__file__).resolve().parents[2]
PARTICIPANTS = Path(__file__).resolve().parent
CARD_PATH = PARTICIPANTS / "guide_arrival_card.md"
MANIFEST_PATH = PARTICIPANTS / "guide_arrival_trial.json"
STEP_MEMORY_MANIFEST_PATH = PARTICIPANTS / "guide_arrival_step_memory_trial.json"
OPENAI_COMPAT_MANIFEST_PATH = PARTICIPANTS / "guide_arrival_openai_compat_trial.json"
GUIDE_PATH = ROOT / "docs" / "LOCAL_ENCOUNTER.md"
TRIAL_FORMAT = "agentciv-guide-arrival-trial/0.1"
TRIAL_FORMAT_V02 = "agentciv-guide-arrival-trial/0.2"
TRIAL_ID = "guide-arrival/0.1"
STEP_MEMORY_TRIAL_ID = "guide-arrival-step-memory/0.1"
OPENAI_COMPAT_TRIAL_ID = "guide-arrival-openai-compat/0.1"
DEFAULT_CONDITION = "control"
JOURNAL_FORMAT = "agentciv-guide-arrival-journal/0.1"
JOURNAL_NAME = "guide-arrival.json"
ACTIONS = ("read", "show", "say", "quiet")
STEPS = 6
SAY_CHARACTERS = 400
TOOL_RESULT_BYTES = 4096
PROMPT_BYTES = 12000
GATHERING_PROMPT_BYTES = 6500
WALL_SECONDS = 900
STEP_SECONDS = 120
GENERATED_TOKENS = 1024
CONTEXT_TOKENS = 8192
SEED = 42
TEMPERATURE = 0.4
ATTEMPTS = 1
JOURNAL_BYTES = 262144
CLOSED_PROVIDER = frozenset({
    "provider_error", "timeout", "connection_refused", "transport_error", "invalid_json",
    "redirect_refused", "response_too_large",
})
OPERATOR_STEPS = (
    "checkout", "build", "init", "serve", "restart", "fresh_read_only_check",
)
DISCLOSURE_FIELDS = (
    "setting",
    "operator",
    "varied_or_synthetic",
    "observed_and_retained",
    "enforced_limits",
    "requested_only",
)


@dataclass(frozen=True)
class TrialCondition:
    name: str
    manifest_path: Path
    trial_format: str
    trial_id: str
    provider: str
    representation: str
    structured_output: str
    requires_declarations: bool


CONDITIONS: dict[str, TrialCondition] = {
    "control": TrialCondition(
        name="control",
        manifest_path=MANIFEST_PATH,
        trial_format=TRIAL_FORMAT,
        trial_id=TRIAL_ID,
        provider="installed_local_ollama_loopback_only",
        representation="card_plus_latest_tool_result",
        structured_output="ollama_json_schema",
        requires_declarations=False,
    ),
    "step_memory": TrialCondition(
        name="step_memory",
        manifest_path=STEP_MEMORY_MANIFEST_PATH,
        trial_format=TRIAL_FORMAT_V02,
        trial_id=STEP_MEMORY_TRIAL_ID,
        provider="installed_local_ollama_loopback_only",
        representation="card_plus_step_memory_and_latest_tool_result",
        structured_output="ollama_json_schema",
        requires_declarations=True,
    ),
    "openai_compat": TrialCondition(
        name="openai_compat",
        manifest_path=OPENAI_COMPAT_MANIFEST_PATH,
        trial_format=TRIAL_FORMAT_V02,
        trial_id=OPENAI_COMPAT_TRIAL_ID,
        provider="local_openai_compat_loopback_only",
        representation="card_plus_latest_tool_result",
        structured_output="openai_compat_json_schema",
        requires_declarations=True,
    ),
}
CONDITIONS_BY_TRIAL_ID: dict[str, TrialCondition] = {
    spec.trial_id: spec for spec in CONDITIONS.values()
}


class TrialError(RuntimeError):
    """A fixed trial code. The message is that code and nothing else."""

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


def resolve_condition(condition: str = DEFAULT_CONDITION) -> TrialCondition:
    spec = CONDITIONS.get(condition)
    if spec is None:
        raise TrialError("invalid_condition")
    return spec


def load_manifest(condition: str = DEFAULT_CONDITION) -> JsonObject:
    spec = resolve_condition(condition)
    try:
        return collaboration.decode(spec.manifest_path.read_bytes())
    except (OSError, collaboration.DecisionError):
        raise TrialError("freeze_mismatch") from None


def _expect(condition: bool) -> None:
    if not condition:
        raise TrialError("freeze_mismatch")


def _verify_study_declarations(manifest: JsonObject) -> None:
    for key in ("purpose", "departure_rule", "reply_policy"):
        value = manifest.get(key)
        _expect(isinstance(value, str) and bool(value.strip()))
    stop_conditions = manifest.get("stop_conditions")
    _expect(
        isinstance(stop_conditions, list)
        and bool(stop_conditions)
        and all(isinstance(item, str) and bool(item.strip()) for item in stop_conditions)
    )
    disclosure = _object(manifest.get("disclosure"), "freeze_mismatch")
    for field in DISCLOSURE_FIELDS:
        value = disclosure.get(field)
        _expect(isinstance(value, str) and bool(value.strip()))
    seeded_lines = manifest.get("seeded_lines")
    _expect(isinstance(seeded_lines, list) and all(isinstance(item, str) for item in seeded_lines))


def verify_freeze(
    manifest: JsonObject,
    card: bytes,
    guide: bytes,
    *,
    condition: str = DEFAULT_CONDITION,
) -> None:
    """Reject a trial whose card, guide, or declared budget drifted."""
    spec = resolve_condition(condition)
    _expect(manifest.get("format") == spec.trial_format)
    _expect(manifest.get("trial") == spec.trial_id)
    _expect(manifest.get("status") == "frozen_before_observation")
    _expect(manifest.get("observation") == "unobserved")
    _expect(manifest.get("provider") == spec.provider)
    _expect(manifest.get("attempts") == ATTEMPTS)
    _expect(manifest.get("external_spend_usd") == 0 and type(manifest.get("external_spend_usd")) is int)
    _expect(manifest.get("downloads") is False)
    _expect(manifest.get("actions") == list(ACTIONS))
    _expect(manifest.get("full_guide") == "docs/LOCAL_ENCOUNTER.md")
    _expect(manifest.get("card_path") == "examples/participants/guide_arrival_card.md")
    _expect(manifest.get("copying_permission") == "not_granted")
    _expect(manifest.get("operator_steps_outside_the_six") == list(OPERATOR_STEPS))
    budgets = _object(manifest.get("budgets"), "freeze_mismatch")
    _expect(budgets.get("steps") == STEPS)
    _expect(budgets.get("generated_tokens_per_step") == GENERATED_TOKENS)
    _expect(budgets.get("context_tokens") == CONTEXT_TOKENS)
    _expect(budgets.get("step_seconds") == STEP_SECONDS)
    _expect(budgets.get("wall_seconds") == WALL_SECONDS)
    _expect(budgets.get("think") is False)
    _expect(budgets.get("temperature") == TEMPERATURE)
    _expect(budgets.get("seed") == SEED)
    _expect(budgets.get("say_characters") == SAY_CHARACTERS)
    _expect(budgets.get("tool_result_bytes") == TOOL_RESULT_BYTES)
    _expect(budgets.get("prompt_bytes") == PROMPT_BYTES)
    _expect(budgets.get("gathering_prompt_bytes") == GATHERING_PROMPT_BYTES)
    prompt = _object(manifest.get("prompt"), "freeze_mismatch")
    _expect(prompt.get("representation") == spec.representation)
    _expect(prompt.get("structured_output") == spec.structured_output)
    if spec.requires_declarations:
        _verify_study_declarations(manifest)
    card_meta = _object(manifest.get("card"), "freeze_mismatch")
    guide_meta = _object(manifest.get("guide"), "freeze_mismatch")
    _expect(card_meta.get("sha256") == _digest(card) and card_meta.get("canonical_bytes") == len(card))
    _expect(guide_meta.get("sha256") == _digest(guide) and guide_meta.get("canonical_bytes") == len(guide))
    _expect(guide_meta.get("newline") == "lf")
    _expect(len(card) < GATHERING_PROMPT_BYTES < len(guide))


def check_freeze(condition: str = DEFAULT_CONDITION) -> JsonObject:
    spec = resolve_condition(condition)
    manifest = load_manifest(spec.name)
    card = canonical_bytes(CARD_PATH)
    guide = canonical_bytes(GUIDE_PATH)
    verify_freeze(manifest, card, guide, condition=spec.name)
    return {
        "outcome": "frozen",
        "observation": "unobserved",
        "trial": spec.trial_id,
        "condition": spec.name,
        "card_sha256": _digest(card),
        "guide_sha256": _digest(guide),
        "card_canonical_bytes": len(card),
        "guide_canonical_bytes": len(guide),
        "copying_permission": "not_granted",
        "external_spend_usd": 0,
    }


def _dump(value: JsonObject) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8")


def _journal_path(state: Path) -> Path:
    return state / JOURNAL_NAME


def _load_journal(state: Path, *, expected_trial: str | None = None) -> JsonObject:
    try:
        raw = private.read_bounded(_journal_path(state), JOURNAL_BYTES)
    except private.EncounterError as error:
        if error.code == "private_file_invalid":
            raise TrialError("trial_not_begun") from None
        if error.code == "private_file_too_large":
            raise TrialError("invalid_journal") from None
        raise
    try:
        journal = collaboration.decode(raw)
    except collaboration.DecisionError:
        raise TrialError("invalid_journal") from None
    trial = journal.get("trial")
    if (
        journal.get("format") != JOURNAL_FORMAT
        or not isinstance(trial, str)
        or trial not in CONDITIONS_BY_TRIAL_ID
        or (expected_trial is not None and trial != expected_trial)
    ):
        raise TrialError("invalid_journal")
    return journal


def _journal_condition(journal: JsonObject) -> TrialCondition:
    trial = journal.get("trial")
    if isinstance(trial, str) and trial in CONDITIONS_BY_TRIAL_ID:
        return CONDITIONS_BY_TRIAL_ID[trial]
    raise TrialError("invalid_journal")


def _store(state: Path, journal: JsonObject) -> None:
    private.replace_private(_journal_path(state), _dump(journal))


def _steps(journal: JsonObject) -> list[JsonObject]:
    rows = journal.get("steps")
    if not isinstance(rows, list):
        raise TrialError("invalid_journal")
    return [_object(row, "invalid_journal") for row in rows]


def _provider_calls(journal: JsonObject) -> int:
    value = journal.get("provider_calls")
    if type(value) is not int or value < 0:
        raise TrialError("invalid_journal")
    return value


def _finish(journal: JsonObject, terminal: str) -> None:
    if journal.get("status") == "finished":
        return
    journal["status"] = "finished"
    journal["terminal"] = terminal


def _new_journal(settings: JsonObject, manifest: JsonObject, spec: TrialCondition) -> JsonObject:
    card = _object(manifest.get("card"), "freeze_mismatch")
    digest = card.get("sha256")
    principal = settings.get("principal")
    if not isinstance(digest, str) or not isinstance(principal, str):
        raise TrialError("freeze_mismatch")
    return {
        "card_sha256": digest,
        "condition": spec.name,
        "copying_permission": "not_granted",
        "format": JOURNAL_FORMAT,
        "last_read_events": None,
        "latest_result": "",
        "new_events_since_last_read": None,
        "principal": principal,
        "provider_calls": 0,
        "publication": "none",
        "status": "open",
        "steps": [],
        "terminal": None,
        "trial": spec.trial_id,
        "uncertain_latched": False,
        "view_name": None,
    }


def begin(state: Path, config: Path, *, condition: str = DEFAULT_CONDITION) -> JsonObject:
    spec = resolve_condition(condition)
    manifest = load_manifest(spec.name)
    verify_freeze(manifest, canonical_bytes(CARD_PATH), canonical_bytes(GUIDE_PATH), condition=spec.name)
    private.check_directory(state)
    settings = client._configuration(config)
    with private.lock(state):
        try:
            private.write_new(_journal_path(state), _dump(_new_journal(settings, manifest, spec)))
        except private.EncounterError as error:
            if error.code == "private_path_exists":
                raise TrialError("trial_already_begun") from None
            raise
    principal = settings.get("principal")
    return {
        "outcome": "begun",
        "trial": spec.trial_id,
        "condition": spec.name,
        "principal": principal if isinstance(principal, str) else "",
    }


def _require_open(journal: JsonObject, settings: JsonObject) -> None:
    if journal.get("principal") != settings.get("principal") or not isinstance(journal.get("principal"), str):
        raise TrialError("principal_mismatch")
    status = journal.get("status")
    if status == "finished":
        raise TrialError("trial_finished")
    if status != "open":
        raise TrialError("invalid_journal")


def _action_label(action: str) -> str:
    if action in ACTIONS:
        return action
    if action.isascii() and action.isidentifier() and len(action) <= 32:
        return action
    return "invalid"


def _safe_code(value: object) -> str | None:
    if isinstance(value, str) and value.isascii() and value.replace("_", "").isalnum() and 0 < len(value) <= 64:
        return value
    return None


def _utf8_prefix(raw: bytes, limit: int) -> bytes:
    clipped = raw[:max(0, limit)]
    while clipped:
        try:
            clipped.decode("utf-8")
            return clipped
        except UnicodeError:
            clipped = clipped[:-1]
    return b""


def _cap_result(text: str, token: str) -> str:
    if token and token in text:
        return "result withheld: credential_reflected"
    raw = text.encode("utf-8")
    if len(raw) <= TOOL_RESULT_BYTES:
        return text
    marker = "\n[truncated]"
    body = _utf8_prefix(raw, TOOL_RESULT_BYTES - len(marker.encode("utf-8")))
    capped = body.decode("utf-8") + marker
    if token and token in capped:
        return "result withheld: credential_reflected"
    return capped


def _step_memory_line(steps: list[JsonObject], new_events_since_last_read: int | None = None) -> str:
    """Short fact line: step count, earlier action labels, and optional local saved-view delta."""
    step_number = len(steps) + 1
    if steps:
        labels = ", ".join(_action_label(str(step.get("action", ""))) for step in steps)
    else:
        labels = "none"
    line = f"Step {step_number} of {STEPS}. Your earlier actions: {labels}."
    if (
        steps
        and steps[-1].get("action") == "read"
        and steps[-1].get("outcome") == "read"
        and type(new_events_since_last_read) is int
        and new_events_since_last_read >= 0
    ):
        noun = "event" if new_events_since_last_read == 1 else "events"
        line += f" {new_events_since_last_read} new {noun} since last read."
    return line


def _bound_prompt(card: str, latest: str, *, step_line: str | None = None) -> str:
    """The model sees the card, optional step-memory fact line, and the latest result."""
    prefix = f"{card}\n\n{step_line}\n\nLatest result:\n" if step_line else card + "\n\nLatest result:\n"
    prompt = prefix + latest
    if len(prompt.encode("utf-8")) <= PROMPT_BYTES:
        return prompt
    return prefix + "result withheld: prompt_bound"


def _remember(step: JsonObject, text: str, token: str) -> None:
    if token not in text:
        step["text"] = text


def _perform(journal: JsonObject, settings: JsonObject, config: Path, action: str, text: str | None) -> tuple[JsonObject, str, str | None]:
    token = str(settings["token"])
    label = _action_label(action)
    step: JsonObject = {"action": label, "index": len(_steps(journal)) + 1}
    if action not in ACTIONS or (action != "say" and text is not None):
        step["outcome"] = "invalid_action"
        return step, "invalid_action", None
    if action == "quiet":
        step["outcome"] = "quiet"
        return step, "quiet", "quiet"
    if action == "show" and not isinstance(journal.get("view_name"), str):
        step["outcome"] = "show_without_read"
        return step, "show_without_read", None
    if action == "say" and journal.get("uncertain_latched") is True:
        step["outcome"] = "say_stopped_after_uncertain"
        return step, "say_stopped_after_uncertain", "uncertain"
    if action == "say" and (not isinstance(text, str) or not text.strip() or len(text) > SAY_CHARACTERS):
        step["outcome"] = "invalid_action"
        return step, "invalid_action", None
    if action == "say" and isinstance(text, str) and token in text:
        step["outcome"] = "credential_in_message"
        return step, "credential_in_message", None
    try:
        if action == "read":
            result = client.read(config)
            view_path = result.get("view_path")
            events = result.get("events")
            pages = result.get("pages")
            cache = Path(str(settings["cache_directory"]))
            selected = Path(view_path) if isinstance(view_path, str) else None
            if (selected is None or selected.parent != cache or not selected.name.startswith("view-")
                or selected.suffix != ".json" or token in selected.name
                or type(events) is not int or type(pages) is not int):
                step["outcome"] = "failed"
                step["code"] = "invalid_private_view_path"
                return step, "failed code=invalid_private_view_path", None
            journal["view_name"] = selected.name
            previous_events = journal.get("last_read_events")
            if type(previous_events) is int and events >= previous_events:
                journal["new_events_since_last_read"] = events - previous_events
            else:
                journal["new_events_since_last_read"] = None
            journal["last_read_events"] = events
            step["outcome"] = "read"
            return step, f"read events={events} pages={pages}", None
        if action == "show":
            name = journal.get("view_name")
            if not isinstance(name, str):
                step["outcome"] = "show_without_read"
                return step, "show_without_read", None
            rendered = client.show(config, Path(str(settings["cache_directory"])) / name)
            step["outcome"] = "show"
            step["truncated"] = len(rendered.encode("utf-8")) > TOOL_RESULT_BYTES
            return step, _cap_result(rendered, token), None
        result = client.say(config, text if isinstance(text, str) else "")
    except private.EncounterError as error:
        code = _safe_code(error.code) or "provider_error"
        step["outcome"] = "failed"
        step["code"] = code
        return step, f"failed code={code}", None
    outcome = result.get("outcome")
    if action == "say" and isinstance(text, str):
        _remember(step, text, token)
    if outcome == "recorded":
        journal["publication"] = "recorded"
        step["outcome"] = "recorded"
        return step, "recorded", "recorded"
    if outcome == "uncertain":
        journal["uncertain_latched"] = True
        journal["publication"] = "uncertain"
        step["outcome"] = "uncertain"
        return step, "uncertain", "uncertain"
    if outcome == "rejected":
        journal["publication"] = "rejected"
        code = _safe_code(result.get("code")) or "rejected"
        step["outcome"] = "rejected"
        step["code"] = code
        return step, f"rejected code={code}", None
    step["outcome"] = "failed"
    step["code"] = "unexpected_publication"
    return step, "failed code=unexpected_publication", None


def _public(journal: JsonObject, outcome: str, code: str | None = None) -> JsonObject:
    result: JsonObject = {
        "outcome": outcome,
        "publication": journal.get("publication"),
        "steps": len(_steps(journal)),
        "terminal": journal.get("terminal"),
        "trial": journal.get("trial", TRIAL_ID),
    }
    if code is not None:
        result["code"] = code
    return result


def act(state: Path, config: Path, action: str, text: str | None = None) -> JsonObject:
    private.check_directory(state)
    settings = client._configuration(config)
    with private.lock(state):
        journal = _load_journal(state)
        spec = _journal_condition(journal)
        check_freeze(spec.name)
        _require_open(journal, settings)
        steps = _steps(journal)
        if len(steps) >= STEPS:
            _finish(journal, "steps_exhausted")
            _store(state, journal)
            return _public(journal, "steps_exhausted")
        step, latest, terminal = _perform(journal, settings, config, action, text)
        steps.append(step)
        journal["steps"] = steps
        journal["latest_result"] = latest
        if terminal is not None:
            _finish(journal, terminal)
        code = step.get("code")
        _store(state, journal)
        return _public(journal, str(step.get("outcome")), code if isinstance(code, str) else None)


def report(state: Path) -> JsonObject:
    private.check_directory(state)
    with private.lock(state):
        journal = _load_journal(state)
    publication = journal.get("publication")
    if publication not in {"none", "recorded", "rejected", "uncertain"}:
        raise TrialError("invalid_journal")
    public_steps: list[JsonObject] = []
    for step in _steps(journal):
        item: JsonObject = {}
        for key in ("index", "action", "outcome"):
            if key in step:
                item[key] = step[key]
        code = step.get("code")
        if isinstance(code, str):
            item["code"] = code
        public_steps.append(item)
    result: JsonObject = {
        "copying_permission": "not_granted",
        "external_spend_usd": 0,
        "format": JOURNAL_FORMAT,
        "model_words": "withheld",
        "principal": journal.get("principal"),
        "provider_calls": _provider_calls(journal),
        "publication": publication,
        "status": journal.get("status"),
        "step_count": len(public_steps),
        "steps": public_steps,
        "terminal": journal.get("terminal"),
        "trial": journal.get("trial", TRIAL_ID),
    }
    provider_code = journal.get("provider_code")
    if isinstance(provider_code, str):
        result["provider_code"] = provider_code
    return result


def parse_action(reply: str) -> tuple[str, str | None] | None:
    """Accept one JSON object. Markdown, extra keys, and a shell are not actions."""
    try:
        value = collaboration.decode(reply.encode("utf-8"))
    except (UnicodeError, collaboration.DecisionError):
        return None
    if set(value) - {"action", "text"} or "action" not in value:
        return None
    action = value.get("action")
    if not isinstance(action, str) or action not in ACTIONS:
        return None
    if action == "say":
        text = value.get("text")
        if "text" not in value or not isinstance(text, str):
            return None
        return action, text
    if "text" in value:
        return None
    return action, None


def _elapsed(now: Callable[[], float], started: float) -> float:
    current = now()
    if isinstance(current, bool) or not isinstance(current, (int, float)):
        raise TrialError("invalid_clock")
    return float(current) - started


def visit(
    state: Path,
    config: Path,
    complete: Callable[[str], str],
    clock: Callable[[], float] | None = None,
    *,
    condition: str = DEFAULT_CONDITION,
) -> JsonObject:
    """One attempt. A second call finds the journal and does not call complete."""
    spec = resolve_condition(condition)
    check_freeze(spec.name)
    begin(state, config, condition=spec.name)
    settings = client._configuration(config)
    token = str(settings["token"])
    try:
        card = canonical_bytes(CARD_PATH).decode("utf-8")
    except UnicodeError:
        raise TrialError("freeze_mismatch") from None
    now = clock or time.monotonic
    started = now()
    while True:
        with private.lock(state):
            journal = _load_journal(state, expected_trial=spec.trial_id)
            if journal.get("status") != "open":
                break
            if _elapsed(now, started) >= WALL_SECONDS:
                _finish(journal, "wall_time_exhausted")
                _store(state, journal)
                break
            steps = _steps(journal)
            if len(steps) >= STEPS:
                _finish(journal, "steps_exhausted")
                _store(state, journal)
                break
            latest = journal.get("latest_result")
            latest_text = latest if isinstance(latest, str) and latest else "(none)"
            step_line: str | None = None
            if spec.representation == "card_plus_step_memory_and_latest_tool_result":
                delta = journal.get("new_events_since_last_read")
                step_line = _step_memory_line(steps, delta if type(delta) is int else None)
            prompt = _bound_prompt(card, latest_text, step_line=step_line)
            if token in prompt:
                _finish(journal, "credential_reflected")
                _store(state, journal)
                break
        try:
            reply = complete(prompt)
        except Exception as error:
            code = _closed_provider_code(error)
            with private.lock(state):
                journal = _load_journal(state, expected_trial=spec.trial_id)
                journal["provider_calls"] = _provider_calls(journal) + 1
                journal["provider_code"] = code
                _finish(journal, "provider_failed")
                _store(state, journal)
            break
        if not isinstance(reply, str):
            reply = ""
        with private.lock(state):
            journal = _load_journal(state, expected_trial=spec.trial_id)
            journal["provider_calls"] = _provider_calls(journal) + 1
            _store(state, journal)
        parsed = parse_action(reply)
        if parsed is None:
            act(state, config, "")
        else:
            action, text = parsed
            act(state, config, action, text)
    return report(state)


def _action_schema() -> JsonObject:
    action: JsonObject = {"enum": list(ACTIONS), "type": "string"}
    text: JsonObject = {"maxLength": SAY_CHARACTERS, "type": "string"}
    properties: JsonObject = {"action": action, "text": text}
    return {
        "additionalProperties": False,
        "properties": properties,
        "required": ["action"],
        "type": "object",
    }


def live_complete(origin: str, model: str, *, token: str | None = None) -> Callable[[str], str]:
    """Loopback Ollama only. The token is a reflection check, not prompt text."""
    from collaboration import ModelError, OllamaDecision

    decision = OllamaDecision(
        origin, model, SEED, None, timeout=float(STEP_SECONDS), max_tokens=GENERATED_TOKENS, token=token, think=False,
    )

    def complete(prompt: str) -> str:
        try:
            reply = decision.complete(
                prompt, [], output_tokens=GENERATED_TOKENS, timeout=float(STEP_SECONDS), schema=_action_schema(),
            )
        except ModelError as error:
            raise ProviderStop(_closed_provider_code(error)) from None
        return reply.content

    return complete


def openai_compat_complete(origin: str, model: str, *, token: str | None = None) -> Callable[[str], str]:
    """Loopback OpenAI-compatible /v1/chat/completions only."""
    try:
        checked_origin = loopback_origin(origin)
    except ValueError:
        raise TrialError("non_loopback_origin") from None
    if not isinstance(model, str) or not model.strip() or (token and token in model):
        raise TrialError("invalid_model")
    url = checked_origin + "/v1/chat/completions"

    def complete(prompt: str) -> str:
        payload: JsonObject = {
            "max_tokens": GENERATED_TOKENS,
            "messages": [{"content": prompt, "role": "user"}],
            "model": model,
            "response_format": {
                "json_schema": {
                    "name": "guide_arrival_action",
                    "schema": _action_schema(),
                },
                "type": "json_schema",
            },
            "seed": SEED,
            "stream": False,
            "temperature": TEMPERATURE,
        }
        request = urllib.request.Request(
            url,
            data=json.dumps(payload, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8"),
            method="POST",
        )
        request.add_header("Accept", "application/json")
        request.add_header("Content-Type", "application/json")
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), wire._RefuseRedirect)
        try:
            with opener.open(request, timeout=float(STEP_SECONDS)) as response:
                body = response.read(collaboration.MAX_RESPONSE_BYTES + 1)
        except urllib.error.HTTPError as error:
            error.close()
            raise ProviderStop(f"http_status_{error.code}") from None
        except TimeoutError:
            raise ProviderStop("timeout") from None
        except urllib.error.URLError as error:
            reason = error.reason
            code = (
                "timeout" if isinstance(reason, TimeoutError)
                else "connection_refused" if isinstance(reason, ConnectionRefusedError)
                else "transport_error"
            )
            raise ProviderStop(code) from None
        except wire.ParticipantError:
            raise ProviderStop("redirect_refused") from None
        if len(body) > collaboration.MAX_RESPONSE_BYTES:
            raise ProviderStop("response_too_large")
        try:
            decoded = collaboration.decode(body)
        except collaboration.DecisionError:
            raise ProviderStop("invalid_json") from None
        if token is not None and token in json.dumps(decoded, ensure_ascii=False):
            raise ProviderStop("provider_error")
        try:
            content = providers.chat_completion_text(decoded)
        except ValueError:
            raise ProviderStop("invalid_json") from None
        if token is not None and token in content:
            raise ProviderStop("provider_error")
        return content

    return complete


def run_live(
    state: Path,
    config: Path,
    model: str,
    origin: str,
    *,
    condition: str = DEFAULT_CONDITION,
) -> JsonObject:
    spec = resolve_condition(condition)
    check_freeze(spec.name)
    private.check_directory(state)
    if _journal_path(state).exists():
        raise TrialError("trial_already_begun")
    settings = client._configuration(config)
    token = settings.get("token")
    if not isinstance(token, str):
        raise private.EncounterError("invalid_client_configuration")
    try:
        if spec.provider == "local_openai_compat_loopback_only":
            complete = openai_compat_complete(origin, model, token=token)
        else:
            complete = live_complete(origin, model, token=token)
    except collaboration.ModelError as error:
        raise ProviderStop(_closed_provider_code(error)) from None
    return visit(state, config, complete, condition=spec.name)


def main(argv: list[str] | None = None) -> int:
    condition_choices = tuple(CONDITIONS)
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    checking = commands.add_parser("check")
    checking.add_argument("--condition", choices=condition_choices, default=DEFAULT_CONDITION)
    starting = commands.add_parser("begin")
    starting.add_argument("--state", required=True, type=Path)
    starting.add_argument("--config", required=True, type=Path)
    starting.add_argument("--condition", choices=condition_choices, default=DEFAULT_CONDITION)
    acting = commands.add_parser("act")
    acting.add_argument("--state", required=True, type=Path)
    acting.add_argument("--config", required=True, type=Path)
    acting.add_argument("--action", required=True)
    acting.add_argument("--text")
    running = commands.add_parser("run")
    running.add_argument("--state", required=True, type=Path)
    running.add_argument("--config", required=True, type=Path)
    running.add_argument("--model", required=True)
    running.add_argument("--origin", default="http://127.0.0.1:11434")
    running.add_argument("--condition", choices=condition_choices, default=DEFAULT_CONDITION)
    reporting = commands.add_parser("report")
    reporting.add_argument("--state", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "check":
            result = check_freeze(args.condition)
        elif args.command == "begin":
            result = begin(args.state, args.config, condition=args.condition)
        elif args.command == "act":
            result = act(args.state, args.config, args.action, args.text)
        elif args.command == "run":
            result = run_live(args.state, args.config, args.model, args.origin, condition=args.condition)
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
