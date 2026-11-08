"""Frozen handoff comparison. No test here contacts Ollama or a live host."""
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from collections.abc import Callable
from io import StringIO
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration  # noqa: E402
import decision_loop  # noqa: E402
import encounter_private as private  # noqa: E402
import handoff_comparison as handoff  # noqa: E402

JsonObject = dict[str, object]
TOKEN = "synthetic-fixture-credential-0123456789"
LABELS = {"matches_parent", "matches_obsolete", "matches_correction", "other", "withheld"}


class Clock:
    def __init__(self, values: list[float]) -> None:
        self.values = values
        self.index = 0

    def __call__(self) -> float:
        value = self.values[min(self.index, len(self.values) - 1)]
        self.index += 1
        return value


def private_state(test: unittest.TestCase) -> Path:
    temporary = tempfile.TemporaryDirectory()
    test.addCleanup(temporary.cleanup)
    # Resolve first. macOS temp roots include /var, which is a symlink, and the
    # private-path check rejects every link on the ancestor chain.
    path = Path(temporary.name).resolve() / "comparison"
    private.create_directory(path)
    return path


def capture(argv: list[str]) -> tuple[int, JsonObject]:
    buffer = StringIO()
    with patch("sys.stdout", buffer):
        code = handoff.main(argv)
    parsed: object = json.loads(buffer.getvalue())
    if not isinstance(parsed, dict):
        raise AssertionError("expected a JSON object")
    return code, {str(key): value for key, value in parsed.items()}


def loaded() -> tuple[JsonObject, str, str, str]:
    _trial, comparison, _card = handoff.load_verified()
    found = comparison.get("originals")
    if not isinstance(found, dict):
        raise AssertionError("originals")
    parent = found.get("parent")
    obsolete = found.get("obsolete")
    correction = found.get("correction")
    if not all(isinstance(item, str) and item for item in (parent, obsolete, correction)):
        raise AssertionError("original strings")
    assert isinstance(parent, str) and isinstance(obsolete, str) and isinstance(correction, str)
    return comparison, parent, obsolete, correction


def hints() -> dict[str, str]:
    comparison, _parent, _obsolete, _correction = loaded()
    found = comparison.get("conditions")
    if not isinstance(found, dict):
        raise AssertionError("conditions")
    result: dict[str, str] = {}
    for name in ("misleading", "stale", "useful"):
        item = found.get(name)
        if not isinstance(item, dict):
            raise AssertionError(name)
        hint = item.get("resume_hint")
        if not isinstance(hint, str):
            raise AssertionError(name)
        result[name] = hint
    return result


def condition_from_prompt(prompt: str) -> str:
    note = prompt.split("Note:\n", 1)[1]
    if note.startswith(handoff.ABSENT_NOTE):
        return "absent"
    found = hints()
    if found["misleading"] in note:
        return "misleading"
    if found["stale"] in note:
        return "stale"
    if found["useful"] in note:
        return "useful"
    raise AssertionError("unrecognized note")


def exact_reply(prompt: str, support: str | None = None) -> str:
    _comparison, parent, obsolete, correction = loaded()
    condition = condition_from_prompt(prompt)
    return json.dumps({
        "declared_derivation": correction,
        "retrieval": parent,
        "source_support": obsolete if support is None else support,
        "task_behavior": condition + " kept separate",
    })


def rows(public: JsonObject) -> list[JsonObject]:
    found = public.get("conditions")
    if not isinstance(found, list):
        raise AssertionError("conditions")
    result: list[JsonObject] = []
    for item in found:
        if not isinstance(item, dict):
            raise AssertionError("row")
        result.append({str(key): value for key, value in item.items()})
    return result


def journal_object(raw: bytes) -> JsonObject:
    parsed: object = json.loads(raw)
    if not isinstance(parsed, dict):
        raise AssertionError("journal")
    return {str(key): value for key, value in parsed.items()}


def run_with(
    test: unittest.TestCase,
    complete: Callable[[str], str],
    token: str | None = None,
    clock: Callable[[], float] | None = None,
) -> tuple[Path, JsonObject, bytes]:
    state = private_state(test)
    public = handoff.run_comparison(state, complete, token=token, clock=clock)
    raw = private.read_bounded(state / handoff.JOURNAL_NAME, handoff.JOURNAL_BYTES)
    return state, public, raw


class HandoffComparisonTests(unittest.TestCase):
    def test_check_recomputes_the_freeze_and_stays_unobserved(self) -> None:
        result = handoff.check_freeze()
        card = handoff.canonical_bytes(handoff.CARD_PATH)
        fixture = handoff.canonical_bytes(handoff.FIXTURE_PATH)
        self.assertEqual(result["outcome"], "frozen")
        self.assertEqual(result["observation"], "unobserved")
        self.assertEqual(result["card_sha256"], handoff._digest(card))
        self.assertEqual(result["fixture_sha256"], handoff._digest(fixture))
        self.assertEqual(result["card_canonical_bytes"], len(card))
        self.assertEqual(result["fixture_canonical_bytes"], len(fixture))
        self.assertEqual(result["external_spend_usd"], 0)
        self.assertEqual(result["copying_permission"], "not_granted")
        self.assertNotIn(b"\r\n", card)
        code, body = capture(["check"])
        self.assertEqual(code, 0)
        self.assertEqual(body["outcome"], "frozen")
        self.assertEqual(body["observation"], "unobserved")
        self.assertNotIn("qualified_improvement", json.dumps(body))

    def test_prompts_keep_one_original_set_and_separate_notes(self) -> None:
        _trial, comparison, card = handoff.load_verified()
        _comparison, parent, obsolete, correction = loaded()
        notes: list[str] = []
        for condition in handoff.CONDITIONS:
            prompt = handoff.build_prompt(comparison, card, condition)
            self.assertIn(f"parent: {parent}", prompt)
            self.assertIn(f"obsolete: {obsolete}", prompt)
            self.assertIn(f"correction: {correction}", prompt)
            self.assertIn("Task:\n", prompt)
            notes.append(prompt.split("Note:\n", 1)[1])
            self.assertEqual(condition_from_prompt(prompt), condition)
        self.assertEqual(len(set(notes)), 4)
        self.assertEqual(notes[0].strip(), handoff.ABSENT_NOTE)
        self.assertNotIn("resume_hint", notes[0])
        self.assertNotEqual(notes[0].strip(), "null")
        self.assertNotEqual(notes[1], notes[2])
        self.assertNotEqual(notes[2], notes[3])
        with self.assertRaises(handoff.TrialError) as caught:
            handoff.build_prompt(comparison, card, "forked")
        self.assertEqual(caught.exception.code, "invalid_condition")

    def test_changed_freeze_is_rejected_before_a_journal(self) -> None:
        trial, comparison, card = handoff.load_verified()
        fixture = handoff.canonical_bytes(handoff.FIXTURE_PATH)
        with self.assertRaises(handoff.TrialError) as caught:
            handoff.verify_freeze(trial, comparison, card + b"\n", fixture)
        self.assertEqual(caught.exception.code, "freeze_mismatch")
        for field, value in (
            ("attempts_per_condition", 2),
            ("observation", "observed"),
            ("qualified_improvement", False),
        ):
            changed = json.loads(json.dumps(trial))
            self.assertIsInstance(changed, dict)
            changed[field] = value
            with self.assertRaises(handoff.TrialError) as caught:
                handoff.verify_freeze(changed, comparison, card, fixture)
            self.assertEqual(caught.exception.code, "freeze_mismatch")
        present = json.loads(json.dumps(comparison))
        self.assertIsInstance(present, dict)
        conditions = present["conditions"]
        self.assertIsInstance(conditions, dict)
        conditions["absent"] = "present"
        with self.assertRaises(handoff.TrialError) as caught:
            handoff.verify_freeze(trial, present, card, fixture)
        self.assertEqual(caught.exception.code, "freeze_mismatch")
        state = private_state(self)
        with patch.object(handoff, "PROMPT_BYTES", 10):
            with self.assertRaises(handoff.TrialError) as caught:
                handoff.run_comparison(state, lambda prompt: exact_reply(prompt))
        self.assertEqual(caught.exception.code, "freeze_mismatch")
        self.assertFalse((state / handoff.JOURNAL_NAME).exists())

    def test_exact_labels_stay_in_the_private_journal(self) -> None:
        _comparison, parent, obsolete, correction = loaded()
        calls = {"count": 0}

        def complete(prompt: str) -> str:
            calls["count"] += 1
            return exact_reply(prompt)

        state, public, raw = run_with(self, complete)
        self.assertEqual(calls["count"], 4)
        self.assertEqual(public["outcome"], "finished")
        self.assertEqual(public["terminal"], "completed")
        self.assertEqual(public["provider_calls"], 4)
        self.assertEqual(public["model_words"], "withheld")
        rendered = json.dumps(public)
        self.assertNotIn(parent, rendered)
        self.assertNotIn(obsolete, rendered)
        self.assertNotIn(correction, rendered)
        self.assertNotIn("qualified_improvement", rendered)
        self.assertNotIn("kept separate", rendered)
        found = rows(public)
        self.assertEqual([row.get("condition") for row in found], list(handoff.CONDITIONS))
        journal = journal_object(raw)
        answers = journal.get("answers")
        self.assertIsInstance(answers, dict)
        assert isinstance(answers, dict)
        for row in found:
            self.assertEqual(row.get("outcome"), "recorded")
            labels = row.get("labels")
            self.assertIsInstance(labels, dict)
            assert isinstance(labels, dict)
            self.assertEqual(labels.get("retrieval"), "matches_parent")
            self.assertEqual(labels.get("source_support"), "matches_obsolete")
            self.assertEqual(labels.get("declared_derivation"), "matches_correction")
            self.assertEqual(labels.get("task_behavior"), "other")
            self.assertTrue(set(str(value) for value in labels.values()) <= LABELS)
            condition = row.get("condition")
            self.assertIsInstance(condition, str)
            stored = answers.get(condition)
            self.assertIsInstance(stored, dict)
            assert isinstance(stored, dict)
            self.assertEqual(stored.get("task_behavior"), f"{condition} kept separate")
        reported = handoff.report(state)
        self.assertEqual(reported["outcome"], "finished")
        self.assertNotIn(parent, json.dumps(reported))
        self.assertNotIn("qualified_improvement", raw.decode("utf-8"))
        with self.assertRaises(handoff.TrialError) as caught:
            handoff.run_comparison(state, complete)
        self.assertEqual(caught.exception.code, "comparison_already_begun")
        self.assertEqual(calls["count"], 4)

    def test_provider_failures_keep_closed_codes_and_continue(self) -> None:
        def complete(prompt: str) -> str:
            condition = condition_from_prompt(prompt)
            if condition == "misleading":
                raise collaboration.ModelError("hidden provider sentence", "provider_error")
            if condition == "stale":
                raise collaboration.ModelError("second hidden sentence", "http_status_429")
            if condition == "useful":
                raise handoff.ProviderStop("timeout")
            return exact_reply(prompt)

        _state, public, raw = run_with(self, complete)
        self.assertEqual(public["provider_calls"], 4)
        self.assertEqual(public["terminal"], "completed")
        by_condition = {row.get("condition"): row for row in rows(public)}
        self.assertEqual(by_condition["absent"].get("outcome"), "recorded")
        self.assertEqual(by_condition["misleading"].get("code"), "provider_error")
        self.assertEqual(by_condition["stale"].get("code"), "http_status_429")
        self.assertEqual(by_condition["useful"].get("code"), "timeout")
        for name in ("misleading", "stale", "useful"):
            self.assertEqual(by_condition[name].get("outcome"), "provider_failed")
            self.assertNotIn("labels", by_condition[name])
        text = raw.decode("utf-8")
        rendered = json.dumps(public)
        self.assertNotIn("hidden provider sentence", text)
        self.assertNotIn("second hidden sentence", text)
        self.assertNotIn("hidden provider sentence", rendered)
        answers = journal_object(raw).get("answers")
        self.assertIsInstance(answers, dict)
        assert isinstance(answers, dict)
        self.assertEqual(set(answers), {"absent"})

    def test_invalid_replies_store_no_answers(self) -> None:
        _comparison, parent, obsolete, correction = loaded()
        valid = {
            "declared_derivation": correction,
            "retrieval": parent,
            "source_support": obsolete,
            "task_behavior": "kept separate",
        }
        self.assertIsNotNone(handoff.parse_reply(json.dumps(valid)))
        self.assertIsNone(handoff.parse_reply("```json\n" + json.dumps(valid) + "\n```"))
        extra = dict(valid)
        extra["score"] = "no"
        self.assertIsNone(handoff.parse_reply(json.dumps(extra)))
        blank = dict(valid)
        blank["retrieval"] = " "
        self.assertIsNone(handoff.parse_reply(json.dumps(blank)))
        long = dict(valid)
        long["retrieval"] = "x" * 401
        self.assertIsNone(handoff.parse_reply(json.dumps(long)))
        numbered: dict[str, object] = dict(valid)
        numbered["retrieval"] = 1
        self.assertIsNone(handoff.parse_reply(json.dumps(numbered)))
        self.assertIsNone(handoff.parse_reply('{"retrieval":"a","retrieval":"b"}'))
        self.assertIsNone(handoff.parse_reply("[]"))
        self.assertEqual(handoff.classify_answer(parent, loaded()[0]), "matches_parent")
        self.assertEqual(handoff.classify_answer(obsolete, loaded()[0]), "matches_obsolete")
        self.assertEqual(handoff.classify_answer(correction, loaded()[0]), "matches_correction")
        self.assertEqual(handoff.classify_answer("kept separate", loaded()[0]), "other")

        def complete(prompt: str) -> str:
            del prompt
            return "```json\n{}\n```"

        _state, public, raw = run_with(self, complete)
        self.assertEqual(public["provider_calls"], 4)
        self.assertTrue(all(row.get("outcome") == "invalid_reply" for row in rows(public)))
        self.assertEqual(journal_object(raw).get("answers"), {})
        self.assertNotIn("kept separate", json.dumps(public))

    def test_credential_in_an_answer_is_withheld_and_not_stored(self) -> None:
        def complete(prompt: str) -> str:
            condition = condition_from_prompt(prompt)
            support = TOKEN if condition == "useful" else None
            return exact_reply(prompt, support)

        _state, public, raw = run_with(self, complete, token=TOKEN)
        useful = next(row for row in rows(public) if row.get("condition") == "useful")
        labels = useful.get("labels")
        self.assertIsInstance(labels, dict)
        assert isinstance(labels, dict)
        self.assertEqual(labels.get("source_support"), "withheld")
        self.assertEqual(labels.get("retrieval"), "matches_parent")
        text = raw.decode("utf-8")
        self.assertNotIn(TOKEN, text)
        self.assertNotIn(TOKEN, json.dumps(public))
        answers = journal_object(raw).get("answers")
        self.assertIsInstance(answers, dict)
        assert isinstance(answers, dict)
        stored = answers.get("useful")
        self.assertIsInstance(stored, dict)
        assert isinstance(stored, dict)
        self.assertNotIn("source_support", stored)
        self.assertIn("retrieval", stored)

    def test_credential_in_the_prompt_is_not_sent(self) -> None:
        calls = {"count": 0}
        real = handoff.build_prompt

        def wrapped(comparison: JsonObject, card: bytes, condition: str) -> str:
            prompt = real(comparison, card, condition)
            if condition == "misleading":
                return prompt + TOKEN
            return prompt

        def complete(prompt: str) -> str:
            calls["count"] += 1
            self.assertNotIn(TOKEN, prompt)
            return exact_reply(prompt)

        state = private_state(self)
        with patch.object(handoff, "build_prompt", wrapped):
            public = handoff.run_comparison(state, complete, token=TOKEN)
        raw = private.read_bounded(state / handoff.JOURNAL_NAME, handoff.JOURNAL_BYTES)
        self.assertEqual(calls["count"], 1)
        self.assertEqual(public["terminal"], "credential_reflected")
        self.assertEqual(public["provider_calls"], 1)
        by_condition = {row.get("condition"): row for row in rows(public)}
        self.assertEqual(by_condition["absent"].get("outcome"), "recorded")
        self.assertEqual(by_condition["misleading"].get("code"), "credential_reflected")
        self.assertEqual(by_condition["stale"].get("outcome"), "not_run")
        self.assertEqual(by_condition["stale"].get("code"), "not_sent")
        self.assertEqual(by_condition["useful"].get("code"), "not_sent")
        self.assertNotIn(TOKEN, raw.decode("utf-8"))

    def test_wall_time_leaves_the_rest_unrun(self) -> None:
        calls = {"count": 0}

        def complete(prompt: str) -> str:
            calls["count"] += 1
            return exact_reply(prompt)

        _state, public, _raw = run_with(self, complete, clock=Clock([0.0, 0.0, float(handoff.WALL_SECONDS)]))
        self.assertEqual(calls["count"], 1)
        self.assertEqual(public["terminal"], "wall_time_exhausted")
        self.assertEqual(public["provider_calls"], 1)
        by_condition = {row.get("condition"): row for row in rows(public)}
        self.assertEqual(by_condition["absent"].get("outcome"), "recorded")
        for name in ("misleading", "stale", "useful"):
            self.assertEqual(by_condition[name].get("outcome"), "not_run")
            self.assertEqual(by_condition[name].get("code"), "wall_time_exhausted")

    def test_prompt_bound_continues_and_other_freeze_errors_do_not(self) -> None:
        real = handoff.build_prompt

        def bounded(comparison: JsonObject, card: bytes, condition: str) -> str:
            if condition == "stale":
                raise handoff.TrialError("prompt_bound")
            return real(comparison, card, condition)

        state = private_state(self)
        with patch.object(handoff, "build_prompt", bounded):
            public = handoff.run_comparison(state, exact_reply)
        by_condition = {row.get("condition"): row for row in rows(public)}
        self.assertEqual(by_condition["stale"].get("outcome"), "prompt_bound")
        self.assertEqual(public["provider_calls"], 3)
        self.assertEqual(public["terminal"], "completed")
        self.assertEqual(by_condition["useful"].get("outcome"), "recorded")

        def broken(comparison: JsonObject, card: bytes, condition: str) -> str:
            del comparison, card
            if condition == "absent":
                raise handoff.TrialError("freeze_mismatch")
            return ""

        state = private_state(self)
        with patch.object(handoff, "build_prompt", broken):
            with self.assertRaises(handoff.TrialError) as caught:
                handoff.run_comparison(state, exact_reply)
        self.assertEqual(caught.exception.code, "freeze_mismatch")
        self.assertTrue((state / handoff.JOURNAL_NAME).exists())
        with self.assertRaises(handoff.TrialError) as caught:
            handoff.run_comparison(state, exact_reply)
        self.assertEqual(caught.exception.code, "comparison_already_begun")

    def test_an_existing_journal_is_not_replaced(self) -> None:
        state = private_state(self)
        path = state / handoff.JOURNAL_NAME
        private.write_new(path, b"original\n")
        with self.assertRaises(handoff.TrialError) as caught:
            handoff.run_comparison(state, exact_reply)
        self.assertEqual(caught.exception.code, "comparison_already_begun")
        self.assertEqual(private.read_bounded(path, handoff.JOURNAL_BYTES), b"original\n")

    def test_report_rejects_a_missing_or_invalid_journal(self) -> None:
        state = private_state(self)
        code, body = capture(["report", "--state", str(state)])
        self.assertEqual(code, 1)
        self.assertEqual(body["code"], "comparison_not_begun")
        private.write_new(state / handoff.JOURNAL_NAME, b"[]\n")
        code, body = capture(["report", "--state", str(state)])
        self.assertEqual(body["code"], "invalid_journal")
        root = tempfile.TemporaryDirectory()
        self.addCleanup(root.cleanup)
        ordinary = Path(root.name).resolve() / "not-a-directory"
        ordinary.write_bytes(b"x")
        code, body = capture(["report", "--state", str(ordinary)])
        self.assertEqual(code, 1)
        self.assertEqual(body["code"], "private_directory_invalid")
        finished = {
            "answers": {},
            "comparison": handoff.COMPARISON_ID,
            "conditions": [],
            "format": handoff.JOURNAL_FORMAT,
            "provider_calls": True,
            "status": "finished",
            "terminal": 1,
        }
        other = private_state(self)
        private.write_new(other / handoff.JOURNAL_NAME, json.dumps(finished).encode("utf-8"))
        code, body = capture(["report", "--state", str(other)])
        self.assertEqual(body["code"], "invalid_journal")
        self.assertEqual(handoff.ProviderStop("timeout").code, "timeout")
        self.assertEqual(handoff.ProviderStop("nope").code, "provider_error")
        self.assertEqual(handoff._closed_provider_code(RuntimeError("x")), "provider_error")

        class Tagged(Exception):
            def __init__(self, code: str) -> None:
                super().__init__("hidden")
                self.code = code

        self.assertEqual(handoff._closed_provider_code(Tagged("not_a_code")), "provider_error")
        self.assertEqual(handoff._closed_provider_code(Tagged("http_status_99")), "provider_error")
        self.assertEqual(handoff._closed_provider_code(Tagged("http_status_200")), "http_status_200")
        with self.assertRaises(handoff.TrialError):
            handoff._calls({"provider_calls": True})
        with self.assertRaises(handoff.TrialError):
            handoff._answers({"answers": []})

    def test_constructor_failure_writes_no_journal(self) -> None:
        state = private_state(self)

        def refuse(origin: str, model: str, *, token: str | None = None) -> Callable[[str], str]:
            del origin, model, token
            raise collaboration.ModelError("do not keep this sentence", "connection_refused")

        with patch.object(handoff, "live_complete", refuse):
            with self.assertRaises(handoff.ProviderStop) as caught:
                handoff.run_live(state, "installed-local", "http://127.0.0.1:11434")
        self.assertEqual(caught.exception.code, "connection_refused")
        self.assertNotIn("sentence", str(caught.exception))
        self.assertFalse((state / handoff.JOURNAL_NAME).exists())

        def boom() -> JsonObject:
            raise collaboration.ModelError("hidden check", "timeout")

        with patch.object(handoff, "check_freeze", boom):
            code, body = capture(["check"])
        self.assertEqual(code, 1)
        self.assertEqual(body["code"], "timeout")
        self.assertNotIn("hidden", json.dumps(body))

    def test_live_complete_uses_the_patched_decision_and_the_cli(self) -> None:
        seen: list[str] = []

        class Installed:
            def __init__(
                self,
                origin: str,
                model: str,
                seed: int,
                trace: Path | None,
                *,
                timeout: float,
                max_tokens: int,
                token: str | None,
                think: bool,
            ) -> None:
                if origin != "http://127.0.0.1:11434" or model != "installed-local":
                    raise AssertionError("loopback decision")
                if seed != handoff.SEED or trace is not None or think is not False or token is not None:
                    raise AssertionError("frozen budget")
                if timeout != handoff.CONDITION_SECONDS or max_tokens != handoff.GENERATED_TOKENS:
                    raise AssertionError("frozen budget")
                seen.append("init")

            def complete(
                self,
                prompt: str,
                events: list[JsonObject],
                *,
                output_tokens: int,
                timeout: float,
                schema: JsonObject,
            ) -> decision_loop.Reply:
                del events, timeout
                self.assert_schema(schema, output_tokens)
                seen.append(prompt)
                return decision_loop.Reply(content=exact_reply(prompt), metrics={}, complete=True)

            @staticmethod
            def assert_schema(schema: JsonObject, output_tokens: int) -> None:
                if schema.get("additionalProperties") is not False or output_tokens != handoff.GENERATED_TOKENS:
                    raise AssertionError("schema")
                required = schema.get("required")
                if not isinstance(required, list) or set(required) != set(handoff.CHECKS):
                    raise AssertionError("schema")

        class Fails:
            def __init__(
                self,
                origin: str,
                model: str,
                seed: int,
                trace: Path | None,
                *,
                timeout: float,
                max_tokens: int,
                token: str | None,
                think: bool,
            ) -> None:
                del origin, model, seed, trace, timeout, max_tokens, token, think

            def complete(
                self,
                prompt: str,
                events: list[JsonObject],
                *,
                output_tokens: int,
                timeout: float,
                schema: JsonObject,
            ) -> decision_loop.Reply:
                del prompt, events, output_tokens, timeout, schema
                raise collaboration.ModelError("hidden completion", "timeout")

        state = private_state(self)
        with patch.object(collaboration, "OllamaDecision", Fails):
            complete = handoff.live_complete("http://127.0.0.1:11434", "installed-local")
            with self.assertRaises(handoff.ProviderStop) as caught:
                complete("prompt")
        self.assertEqual(caught.exception.code, "timeout")
        self.assertNotIn("hidden", str(caught.exception))
        with patch.object(collaboration, "OllamaDecision", Installed):
            code, public = capture([
                "run", "--state", str(state), "--model", "installed-local",
            ])
        self.assertEqual(code, 0)
        self.assertEqual(seen[0], "init")
        self.assertEqual(len(seen), 5)
        self.assertEqual(public["outcome"], "finished")
        self.assertEqual(public["provider_calls"], 4)
        _comparison, parent, _obsolete, _correction = loaded()
        self.assertNotIn(parent, json.dumps(public))
        code, reported = capture(["report", "--state", str(state)])
        self.assertEqual(code, 0)
        self.assertEqual(reported["model_words"], "withheld")
        self.assertNotIn(parent, json.dumps(reported))
        code, again = capture(["run", "--state", str(state), "--model", "installed-local"])
        self.assertEqual(code, 1)
        self.assertEqual(again["code"], "comparison_already_begun")
