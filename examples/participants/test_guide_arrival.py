"""Frozen guide-only arrival trial. No test here contacts Ollama or a live host."""
from __future__ import annotations

import json
import sys
import tempfile
import types
import unittest
from io import StringIO
from pathlib import Path
from collections.abc import Callable
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples" / "http-commons"))
import collaboration  # noqa: E402
import encounter_private as private  # noqa: E402
import guide_arrival as guide  # noqa: E402
import test_local_encounter as fixtures  # noqa: E402

JsonObject = dict[str, object]
TOKEN = "synthetic-fixture-credential-0123456789"


class Clock:
    def __init__(self, values: list[float]) -> None:
        self.values = values
        self.index = 0

    def __call__(self) -> float:
        value = self.values[min(self.index, len(self.values) - 1)]
        self.index += 1
        return value


def room(test: unittest.TestCase) -> tuple[Path, Path, Path]:
    temporary = tempfile.TemporaryDirectory()
    test.addCleanup(temporary.cleanup)
    config, _settings, cache = fixtures.settings(Path(temporary.name).resolve())
    return cache.parent, config, cache


def capture(argv: list[str]) -> tuple[int, JsonObject]:
    buffer = StringIO()
    with patch("sys.stdout", buffer):
        code = guide.main(argv)
    parsed: object = json.loads(buffer.getvalue())
    if not isinstance(parsed, dict):
        raise AssertionError("expected a JSON object")
    return code, {str(key): value for key, value in parsed.items()}


def read_journal(state: Path) -> bytes:
    return (state / guide.JOURNAL_NAME).read_bytes()


def bound_client() -> types.ModuleType:
    """The adapter's room client. The name is an import, not a public export."""
    found = getattr(guide, "client")
    if not isinstance(found, types.ModuleType):
        raise AssertionError("room client binding")
    return found


class FreezeTests(unittest.TestCase):
    def test_check_recomputes_the_lf_digests_and_the_gathering_bound(self) -> None:
        result = guide.check_freeze()
        card = guide.canonical_bytes(guide.CARD_PATH)
        operator = guide.canonical_bytes(guide.GUIDE_PATH)
        self.assertEqual(result["outcome"], "frozen")
        self.assertEqual(result["observation"], "unobserved")
        self.assertEqual(result["card_sha256"], guide._digest(card))
        self.assertEqual(result["guide_sha256"], guide._digest(operator))
        self.assertEqual(result["card_canonical_bytes"], len(card))
        self.assertEqual(result["guide_canonical_bytes"], len(operator))
        self.assertLess(len(card), guide.GATHERING_PROMPT_BYTES)
        self.assertGreater(len(operator), guide.GATHERING_PROMPT_BYTES)
        self.assertEqual(result["external_spend_usd"], 0)
        self.assertNotIn(b"\r\n", card)

    def test_model_setup_failure_is_a_closed_code_and_writes_no_journal(self) -> None:
        state = Path("unused-state")
        config = Path("unused-config.json")

        def refuse(origin: str, model: str, *, token: str | None = None) -> Callable[[str], str]:
            del origin, model, token
            raise collaboration.ModelError("do not keep this sentence", "connection_refused")

        with patch.object(guide, "check_freeze", return_value={}), \
                patch.object(private, "check_directory"), \
                patch.object(bound_client(), "_configuration", return_value={"token": TOKEN}), \
                patch.object(Path, "exists", return_value=False), \
                patch.object(guide, "live_complete", refuse):
            with self.assertRaises(guide.ProviderStop) as caught:
                guide.run_live(state, config, "installed-local", "http://127.0.0.1:11434")
            self.assertEqual(caught.exception.code, "connection_refused")
            self.assertNotIn("sentence", str(caught.exception))
            code, failed = capture(["run", "--state", str(state), "--config", str(config), "--model", "installed-local"])
        self.assertEqual(code, 1)
        self.assertEqual(failed["code"], "connection_refused")
        self.assertNotIn("sentence", json.dumps(failed))

    def test_changed_card_short_guide_and_wrong_budget_are_rejected(self) -> None:
        manifest = guide.load_manifest()
        card = guide.canonical_bytes(guide.CARD_PATH)
        operator = guide.canonical_bytes(guide.GUIDE_PATH)
        for mutated_card, mutated_guide in ((card + b"\n", operator), (card, b"too short\n")):
            with self.subTest(card=len(mutated_card), guide=len(mutated_guide)):
                with self.assertRaises(guide.TrialError) as caught:
                    guide.verify_freeze(manifest, mutated_card, mutated_guide)
                self.assertEqual(caught.exception.code, "freeze_mismatch")
        changed = json.loads(json.dumps(manifest))
        budgets = changed["budgets"]
        self.assertIsInstance(budgets, dict)
        budgets["steps"] = 7
        with self.assertRaises(guide.TrialError) as caught:
            guide.verify_freeze(changed, card, operator)
        self.assertEqual(caught.exception.code, "freeze_mismatch")


class TrialActionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.state, self.config, self.cache = room(self)
        self.reads: list[Path] = []
        self.said: list[str] = []

    def fake_read(self, config: Path, save: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
        del config, save, reader_binary
        self.reads.append(self.cache / "view-0001.json")
        return {"outcome": "read", "events": 2, "pages": 1, "view_path": str(self.cache / "view-0001.json")}

    def fake_say(self, config: Path, text: str, to: list[str] | None = None, basis: Path | None = None,
                 *, reader_binary: Path | None = None) -> JsonObject:
        del config, to, basis, reader_binary
        self.said.append(text)
        return {"outcome": "recorded", "event_id": "event:one", "sequence": 1}

    def test_begin_is_once_and_stores_no_credential(self) -> None:
        begun = guide.begin(self.state, self.config)
        self.assertEqual(begun["outcome"], "begun")
        self.assertNotIn(TOKEN.encode(), read_journal(self.state))
        with self.assertRaises(guide.TrialError) as caught:
            guide.begin(self.state, self.config)
        self.assertEqual(caught.exception.code, "trial_already_begun")

    def test_six_reads_then_the_seventh_does_not_call_the_room(self) -> None:
        guide.begin(self.state, self.config)
        with patch.object(bound_client(), "read", self.fake_read):
            for _ in range(6):
                guide.act(self.state, self.config, "read")
            seventh = guide.act(self.state, self.config, "read")
            with self.assertRaises(guide.TrialError) as caught:
                guide.act(self.state, self.config, "read")
        self.assertEqual(len(self.reads), 6)
        self.assertEqual(seventh["outcome"], "steps_exhausted")
        self.assertEqual(seventh["steps"], 6)
        self.assertEqual(caught.exception.code, "trial_finished")

    def test_quiet_does_not_publish(self) -> None:
        guide.begin(self.state, self.config)
        with patch.object(bound_client(), "say", side_effect=AssertionError("quiet must not publish")):
            result = guide.act(self.state, self.config, "quiet")
        self.assertEqual(result["terminal"], "quiet")
        self.assertEqual(result["publication"], "none")

    def test_uncertain_say_stops_and_a_second_say_does_not_publish(self) -> None:
        guide.begin(self.state, self.config)

        def uncertain(config: Path, text: str, to: list[str] | None = None, basis: Path | None = None,
                      *, reader_binary: Path | None = None) -> JsonObject:
            del config, to, basis, reader_binary
            self.said.append(text)
            return {"outcome": "uncertain"}

        with patch.object(bound_client(), "say", uncertain):
            first = guide.act(self.state, self.config, "say", "one attempt")
            with self.assertRaises(guide.TrialError) as caught:
                guide.act(self.state, self.config, "say", "another attempt")
        self.assertEqual(first["terminal"], "uncertain")
        self.assertEqual(first["publication"], "uncertain")
        self.assertEqual(self.said, ["one attempt"])
        self.assertEqual(caught.exception.code, "trial_finished")

    def test_open_latch_refuses_another_say_without_publishing(self) -> None:
        guide.begin(self.state, self.config)
        journal = collaboration.decode(read_journal(self.state))
        journal["uncertain_latched"] = True
        private.replace_private(self.state / guide.JOURNAL_NAME, guide._dump(journal))
        with patch.object(bound_client(), "say", side_effect=AssertionError("latched say must not publish")):
            result = guide.act(self.state, self.config, "say", "still quiet")
        self.assertEqual(result["outcome"], "say_stopped_after_uncertain")
        self.assertEqual(result["terminal"], "uncertain")

    def test_credential_in_say_is_not_stored_or_published_and_quiet_can_follow(self) -> None:
        guide.begin(self.state, self.config)
        with patch.object(bound_client(), "say", side_effect=AssertionError("credential text must not publish")):
            refused = guide.act(self.state, self.config, "say", "prefix " + TOKEN)
            followed = guide.act(self.state, self.config, "quiet")
        self.assertEqual(refused["outcome"], "credential_in_message")
        self.assertEqual(followed["terminal"], "quiet")
        self.assertNotIn(TOKEN.encode(), read_journal(self.state))

    def test_show_without_read_counts_a_step(self) -> None:
        guide.begin(self.state, self.config)
        with patch.object(bound_client(), "show", side_effect=AssertionError("show needs a saved view")):
            result = guide.act(self.state, self.config, "show")
        self.assertEqual(result["outcome"], "show_without_read")
        self.assertEqual(result["steps"], 1)
        self.assertIsNone(result["terminal"])

    def test_overlong_and_blank_say_do_not_publish(self) -> None:
        guide.begin(self.state, self.config)
        with patch.object(bound_client(), "say", side_effect=AssertionError("invalid say must not publish")):
            overlong = guide.act(self.state, self.config, "say", "a" * 401)
            blank = guide.act(self.state, self.config, "say", "   ")
        self.assertEqual(overlong["outcome"], "invalid_action")
        self.assertEqual(blank["outcome"], "invalid_action")
        self.assertEqual(blank["steps"], 2)

    def test_rejected_say_stays_available_for_quiet(self) -> None:
        guide.begin(self.state, self.config)

        def rejected(config: Path, text: str, to: list[str] | None = None, basis: Path | None = None,
                     *, reader_binary: Path | None = None) -> JsonObject:
            del config, text, to, basis, reader_binary
            self.said.append("once")
            return {"outcome": "rejected", "code": "forbidden"}

        with patch.object(bound_client(), "say", rejected):
            first = guide.act(self.state, self.config, "say", "a permitted attempt")
            second = guide.act(self.state, self.config, "quiet")
        self.assertEqual(first["publication"], "rejected")
        self.assertEqual(first["code"], "forbidden")
        self.assertIsNone(first["terminal"])
        self.assertEqual(second["terminal"], "quiet")
        self.assertEqual(second["publication"], "rejected")
        self.assertEqual(self.said, ["once"])

    def test_room_failure_is_a_recorded_step(self) -> None:
        guide.begin(self.state, self.config)
        with patch.object(bound_client(), "read", side_effect=private.EncounterError("host_unreachable")):
            result = guide.act(self.state, self.config, "read")
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["code"], "host_unreachable")
        self.assertEqual(result["steps"], 1)

    def test_view_outside_the_cache_and_a_non_integer_count_are_refused(self) -> None:
        guide.begin(self.state, self.config)

        def outside(config: Path, save: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
            del config, save, reader_binary
            return {"outcome": "read", "events": 1, "pages": 1, "view_path": str(self.state / "view-outside.json")}

        with patch.object(bound_client(), "read", outside):
            refused = guide.act(self.state, self.config, "read")
        self.assertEqual(refused["code"], "invalid_private_view_path")

        def bad_count(config: Path, save: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
            del config, save, reader_binary
            return {"outcome": "read", "events": True, "pages": 1, "view_path": str(self.cache / "view-0001.json")}

        with patch.object(bound_client(), "read", bad_count):
            again = guide.act(self.state, self.config, "read")
        self.assertEqual(again["code"], "invalid_private_view_path")

    def test_show_truncates_and_withholds_a_reflected_credential(self) -> None:
        guide.begin(self.state, self.config)
        with patch.object(bound_client(), "read", self.fake_read):
            guide.act(self.state, self.config, "read")
        with patch.object(bound_client(), "show", return_value="y" * 5000):
            shown = guide.act(self.state, self.config, "show")
        stored = read_journal(self.state)
        self.assertEqual(shown["outcome"], "show")
        self.assertIn(b"[truncated]", stored)
        self.assertLess(stored.count(b"y"), 5000)
        with patch.object(bound_client(), "show", return_value="prefix " + TOKEN + " suffix"):
            withheld = guide.act(self.state, self.config, "show")
        self.assertEqual(withheld["outcome"], "show")
        self.assertNotIn(TOKEN.encode(), read_journal(self.state))

    def test_principal_mismatch_and_a_damaged_journal_do_not_call_the_room(self) -> None:
        guide.begin(self.state, self.config)
        journal = collaboration.decode(read_journal(self.state))
        journal["principal"] = "agent:someone-else"
        private.replace_private(self.state / guide.JOURNAL_NAME, guide._dump(journal))
        with patch.object(bound_client(), "read", side_effect=AssertionError("mismatch must not read")):
            with self.assertRaises(guide.TrialError) as caught:
                guide.act(self.state, self.config, "read")
        self.assertEqual(caught.exception.code, "principal_mismatch")
        private.replace_private(self.state / guide.JOURNAL_NAME, b"{}\n")
        with self.assertRaises(guide.TrialError) as caught:
            guide.act(self.state, self.config, "read")
        self.assertEqual(caught.exception.code, "invalid_journal")

    def test_act_before_begin_is_refused(self) -> None:
        with self.assertRaises(guide.TrialError) as caught:
            guide.act(self.state, self.config, "quiet")
        self.assertEqual(caught.exception.code, "trial_not_begun")


class VisitTests(unittest.TestCase):
    def setUp(self) -> None:
        self.state, self.config, self.cache = room(self)
        self.prompts: list[str] = []
        self.said: list[str] = []

    def test_read_then_quiet_shows_only_the_card_and_the_latest_result(self) -> None:
        replies = iter(('{"action":"read"}', '{"action":"quiet"}'))

        def complete(prompt: str) -> str:
            self.prompts.append(prompt)
            self.assertNotIn(TOKEN, prompt)
            self.assertNotIn("view-", prompt)
            self.assertNotIn("Prepare the tools and private state", prompt)
            return next(replies)

        def fake_read(config: Path, save: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
            del config, save, reader_binary
            return {"outcome": "read", "events": 0, "pages": 1, "view_path": str(self.cache / "view-0001.json")}

        with patch.object(bound_client(), "read", fake_read), \
            patch.object(bound_client(), "say", side_effect=AssertionError("quiet visit must not publish")):
            result = guide.visit(self.state, self.config, complete)
        card = guide.canonical_bytes(guide.CARD_PATH).decode("utf-8")
        self.assertEqual(result["terminal"], "quiet")
        self.assertEqual(result["publication"], "none")
        self.assertEqual(result["provider_calls"], 2)
        self.assertEqual(self.prompts[0].count(card), 1)
        self.assertTrue(self.prompts[0].endswith("Latest result:\n(none)"))
        latest = self.prompts[1].split("Latest result:\n", 1)[1]
        self.assertEqual(latest, "read events=0 pages=1")
        self.assertNotIn("a chosen note", json.dumps(result))

    def test_scripted_say_records_once_and_the_report_omits_the_words(self) -> None:
        def complete(prompt: str) -> str:
            self.prompts.append(prompt)
            return '{"action":"say","text":"a chosen note"}'

        def fake_say(config: Path, text: str, to: list[str] | None = None, basis: Path | None = None,
                     *, reader_binary: Path | None = None) -> JsonObject:
            del config, to, basis, reader_binary
            self.said.append(text)
            return {"outcome": "recorded", "event_id": "event:one", "sequence": 1}

        with patch.object(bound_client(), "say", fake_say):
            result = guide.visit(self.state, self.config, complete)
        self.assertEqual(len(self.prompts), 1)
        self.assertEqual(self.said, ["a chosen note"])
        self.assertEqual(result["terminal"], "recorded")
        self.assertEqual(result["publication"], "recorded")
        self.assertEqual(result["provider_calls"], 1)
        self.assertNotIn("a chosen note", json.dumps(result))
        self.assertIn(b"a chosen note", read_journal(self.state))
        self.assertEqual(result["copying_permission"], "not_granted")
        self.assertEqual(result["model_words"], "withheld")

    def test_provider_exception_is_not_replaced_and_its_text_is_dropped(self) -> None:
        def complete(prompt: str) -> str:
            del prompt
            raise RuntimeError("secret stack " + TOKEN)

        with patch.object(bound_client(), "say", side_effect=AssertionError("provider failure must not publish")):
            result = guide.visit(self.state, self.config, complete)
        self.assertEqual(result["terminal"], "provider_failed")
        self.assertEqual(result["provider_code"], "provider_error")
        self.assertEqual(result["step_count"], 0)
        self.assertEqual(result["provider_calls"], 1)
        blob = json.dumps(result).encode() + read_journal(self.state)
        self.assertNotIn(b"secret stack", blob)
        self.assertNotIn(TOKEN.encode(), blob)

    def test_closed_provider_code_is_kept_without_the_provider_sentence(self) -> None:
        def complete(prompt: str) -> str:
            del prompt
            raise collaboration.ModelError("hidden provider sentence", "timeout")

        result = guide.visit(self.state, self.config, complete)
        self.assertEqual(result["provider_code"], "timeout")
        self.assertNotIn(b"hidden provider sentence", read_journal(self.state))

    def test_malformed_shell_and_markdown_replies_are_invalid_actions(self) -> None:
        replies = iter((
            "```json\n{\"action\":\"quiet\"}\n```",
            '{"action":"read","shell":"cmd.exe","token":"no"}',
            '{"action":"quiet"}',
        ))

        def complete(prompt: str) -> str:
            del prompt
            return next(replies)

        with patch.object(bound_client(), "read", side_effect=AssertionError("extra keys must not read")), \
            patch.object(bound_client(), "say", side_effect=AssertionError("invalid reply must not publish")):
            result = guide.visit(self.state, self.config, complete)
        steps = result["steps"]
        self.assertIsInstance(steps, list)
        if not isinstance(steps, list):
            raise AssertionError("steps")
        outcomes = [item["outcome"] for item in steps if isinstance(item, dict)]
        self.assertEqual(outcomes, ["invalid_action", "invalid_action", "quiet"])
        self.assertEqual(result["terminal"], "quiet")

    def test_six_reads_stop_before_a_seventh_provider_call(self) -> None:
        calls = 0

        def complete(prompt: str) -> str:
            nonlocal calls
            del prompt
            calls += 1
            return '{"action":"read"}'

        def fake_read(config: Path, save: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
            del config, save, reader_binary
            return {"outcome": "read", "events": 1, "pages": 1, "view_path": str(self.cache / f"view-{calls:04d}.json")}

        with patch.object(bound_client(), "read", fake_read):
            result = guide.visit(self.state, self.config, complete)
        self.assertEqual(calls, 6)
        self.assertEqual(result["terminal"], "steps_exhausted")
        self.assertEqual(result["step_count"], 6)

    def test_wall_clock_stops_before_the_next_provider_call(self) -> None:
        calls = 0

        def complete(prompt: str) -> str:
            nonlocal calls
            del prompt
            calls += 1
            return '{"action":"read"}'

        def fake_read(config: Path, save: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
            del config, save, reader_binary
            return {"outcome": "read", "events": 1, "pages": 1, "view_path": str(self.cache / "view-0001.json")}

        with patch.object(bound_client(), "read", fake_read):
            jumped = guide.visit(self.state, self.config, complete, Clock([0.0, 1.0, 1000.0]))
        self.assertEqual(calls, 1)
        self.assertEqual(jumped["terminal"], "wall_time_exhausted")
        state, config, cache = room(self)

        def fail_if_called(prompt: str) -> str:
            del prompt
            raise AssertionError("exhausted clock must not call the provider")

        with patch.object(bound_client(), "read", fake_read):
            immediate = guide.visit(state, config, fail_if_called, Clock([0.0, 900.0]))
        del cache
        self.assertEqual(immediate["terminal"], "wall_time_exhausted")
        self.assertEqual(immediate["provider_calls"], 0)

    def test_second_visit_does_not_call_complete(self) -> None:
        calls = 0

        def complete(prompt: str) -> str:
            nonlocal calls
            del prompt
            calls += 1
            return '{"action":"quiet"}'

        guide.visit(self.state, self.config, complete)
        with self.assertRaises(guide.TrialError) as caught:
            guide.visit(self.state, self.config, complete)
        self.assertEqual(caught.exception.code, "trial_already_begun")
        self.assertEqual(calls, 1)

    def test_prompt_bound_and_reply_parser_and_provider_codes(self) -> None:
        prompt = guide._bound_prompt("c" * 100, "z" * 20000)
        self.assertLessEqual(len(prompt.encode("utf-8")), guide.PROMPT_BYTES)
        self.assertIn("prompt_bound", prompt)
        self.assertNotIn("z" * 100, prompt)
        self.assertIsNone(guide.parse_action('{"action":"quiet","action":"read"}'))
        self.assertIsNone(guide.parse_action('{"action":"say"}'))
        self.assertIsNone(guide.parse_action('["quiet"]'))
        self.assertEqual(guide.parse_action('{"action":"quiet"}'), ("quiet", None))
        self.assertEqual(guide._closed_provider_code(RuntimeError("secret")), "provider_error")
        self.assertEqual(guide._closed_provider_code(guide.ProviderStop("not-a-code")), "provider_error")
        self.assertEqual(guide._closed_provider_code(guide.ProviderStop("http_status_418")), "http_status_418")
        self.assertEqual(guide._closed_provider_code(collaboration.ModelError("hidden", "timeout")), "timeout")


class CommandTests(unittest.TestCase):
    def test_check_begin_act_and_report(self) -> None:
        state, config, _cache = room(self)
        code, checked = capture(["check"])
        self.assertEqual(code, 0)
        self.assertEqual(checked["outcome"], "frozen")
        self.assertEqual(checked["observation"], "unobserved")
        code, begun = capture(["begin", "--state", str(state), "--config", str(config)])
        self.assertEqual(code, 0)
        self.assertEqual(begun["outcome"], "begun")
        self.assertNotIn(TOKEN, json.dumps(begun))
        code, acted = capture(["act", "--state", str(state), "--config", str(config), "--action", "quiet"])
        self.assertEqual(code, 0)
        self.assertEqual(acted["terminal"], "quiet")
        code, reported = capture(["report", "--state", str(state)])
        self.assertEqual(code, 0)
        self.assertEqual(reported["terminal"], "quiet")
        self.assertEqual(reported["model_words"], "withheld")
        self.assertNotIn(TOKEN, json.dumps(reported))

    def test_run_uses_the_injected_provider_and_refuses_a_second_journal(self) -> None:
        state, config, _cache = room(self)

        def fake_live(origin: str, model: str, *, token: str | None = None) -> Callable[[str], str]:
            self.assertEqual(origin, "http://127.0.0.1:11434")
            self.assertEqual(model, "already-installed")
            self.assertEqual(token, TOKEN)

            def complete(prompt: str) -> str:
                self.assertNotIn(TOKEN, prompt)
                return '{"action":"quiet"}'

            return complete

        with patch.object(guide, "live_complete", fake_live):
            code, result = capture(["run", "--state", str(state), "--config", str(config), "--model", "already-installed"])
        self.assertEqual(code, 0)
        self.assertEqual(result["terminal"], "quiet")
        self.assertNotIn(TOKEN, json.dumps(result))
        with patch.object(guide, "live_complete", side_effect=AssertionError("existing journal must not call a provider")):
            code, failed = capture(["run", "--state", str(state), "--config", str(config), "--model", "already-installed"])
        self.assertEqual(code, 1)
        self.assertEqual(failed["code"], "trial_already_begun")

    def test_check_failure_exits_nonzero(self) -> None:
        with patch.object(guide, "verify_freeze", side_effect=guide.TrialError("freeze_mismatch")):
            code, failed = capture(["check"])
        self.assertEqual(code, 1)
        self.assertEqual(failed["code"], "freeze_mismatch")

    def test_live_complete_uses_the_frozen_budget_and_closed_codes(self) -> None:
        seen: dict[str, object] = {}

        class FakeDecision:
            def __init__(self, origin: str, model: str, seed: int, trace: Path | None, *,
                         timeout: float, max_tokens: int, token: str | None, think: bool) -> None:
                seen.update({"origin": origin, "model": model, "seed": seed, "trace": trace,
                             "timeout": timeout, "max_tokens": max_tokens, "token": token, "think": think})

            def complete(self, prompt: str, events: list[JsonObject], *, output_tokens: int,
                         timeout: float, schema: JsonObject) -> object:
                del prompt, events, output_tokens, timeout
                self.schema = schema
                return type("Reply", (), {"content": '{"action":"quiet"}'})()

        with patch.object(collaboration, "OllamaDecision", FakeDecision):
            complete = guide.live_complete("http://127.0.0.1:11434", "installed-local", token=TOKEN)
            self.assertEqual(complete("card"), '{"action":"quiet"}')
        self.assertEqual(seen["seed"], 42)
        self.assertEqual(seen["think"], False)
        self.assertEqual(seen["timeout"], 120)
        self.assertEqual(seen["max_tokens"], 1024)
        self.assertIsNone(seen["trace"])
        self.assertEqual(seen["token"], TOKEN)

        class FailingDecision(FakeDecision):
            def complete(self, prompt: str, events: list[JsonObject], *, output_tokens: int,
                         timeout: float, schema: JsonObject) -> object:
                del prompt, events, output_tokens, timeout, schema
                raise collaboration.ModelError("do not keep this sentence", "connection_refused")

        with patch.object(collaboration, "OllamaDecision", FailingDecision):
            complete = guide.live_complete("http://127.0.0.1:11434", "installed-local", token=TOKEN)
            with self.assertRaises(guide.ProviderStop) as caught:
                complete("card")
        self.assertEqual(caught.exception.code, "connection_refused")
        self.assertNotIn("sentence", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
