"""Independent failure-path checks for the optional local decision loop."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from typing import TextIO, cast
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import decision_loop as engine  # noqa: E402


class Clock:
    def __init__(self) -> None:
        self.now = 0.0

    def __call__(self) -> float:
        return self.now


class Replies:
    def __init__(self, *replies: engine.Reply | Exception) -> None:
        self.replies = list(replies)
        self.calls: list[tuple[str, int, float]] = []

    def __call__(self, prompt: str, output_tokens: int, timeout: float) -> engine.Reply:
        self.calls.append((prompt, output_tokens, timeout))
        reply = self.replies.pop(0)
        if isinstance(reply, Exception):
            raise reply
        return reply


def validate(content: str) -> engine.JsonObject:
    if content in {"good", "stop"}:
        return {"action": "stop" if content == "stop" else "revise"}
    raise engine.InvalidDecision("invalid_json")


def read_journal(path: Path) -> engine.JsonObject:
    value: object = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise AssertionError("journal is not an object")
    return dict(value)


class BudgetTests(unittest.TestCase):
    def test_invalid_configuration_never_creates_a_loop(self) -> None:
        for attempts in (0, 4, True, cast(int, 1.5)):
            with self.subTest(attempts=attempts), self.assertRaises(ValueError):
                engine.Budget(attempts=attempts)
        for tokens in (0, 2049, True, cast(int, 1.5)):
            with self.subTest(tokens=tokens), self.assertRaises(ValueError):
                engine.Budget(output_tokens=tokens)
        for seconds in (0.0, 151.0, True, float("nan"), float("inf")):
            with self.subTest(seconds=seconds), self.assertRaises(ValueError):
                engine.Budget(seconds=seconds)
        for code in ("invented_feedback", "private-provider-exception"):
            with self.assertRaises(ValueError):
                engine.InvalidDecision(code)

    def test_measured_usage_shares_one_budget_across_attempts(self) -> None:
        request = Replies(engine.Reply("bad", {"eval_count": 7}),
                          engine.Reply("good", {"eval_count": 11}))
        loop = engine.DecisionLoop(request, validate,
                                   budget=engine.Budget(attempts=3, output_tokens=30))
        self.assertEqual(loop.run("original context"), {"action": "revise"})
        self.assertEqual([call[1] for call in request.calls], [30, 23])
        summary = loop.summary()
        self.assertEqual(summary["charged_output_tokens"], 18)
        self.assertFalse(summary["first_attempt_valid"])
        self.assertTrue(summary["eventual_valid"])
        self.assertEqual(loop.attempts[0]["feedback_code"], "invalid_json")
        self.assertIn(engine.FEEDBACK["invalid_json"], request.calls[1][0])
        self.assertIn("untrusted data", request.calls[1][0])

    def test_missing_or_invalid_usage_cannot_buy_another_request(self) -> None:
        metrics: list[engine.JsonObject] = [
            {}, {"eval_count": None}, {"eval_count": True}, {"eval_count": -1},
            {"eval_count": 2.0}, {"eval_count": "2"},
        ]
        for usage in metrics:
            with self.subTest(metrics=usage):
                request = Replies(engine.Reply("bad", usage))
                loop = engine.DecisionLoop(request, validate,
                                           budget=engine.Budget(attempts=3, output_tokens=10))
                with self.assertRaises(engine.LoopFailure) as caught:
                    loop.run("prompt")
                self.assertEqual(caught.exception.code, "output_budget_exhausted")
                self.assertEqual(len(request.calls), 1)
                self.assertEqual(loop.tokens, 10)
                self.assertFalse(loop.attempts[0]["output_usage_measured"])

    def test_reported_generation_excess_is_a_fatal_budget_violation(self) -> None:
        for content in ("good", "bad"):
            with self.subTest(content=content):
                request = Replies(engine.Reply(content, {"eval_count": 11}))
                loop = engine.DecisionLoop(request, validate,
                                           budget=engine.Budget(attempts=3, output_tokens=10))
                with self.assertRaises(engine.LoopFailure) as caught:
                    loop.run("prompt")
                self.assertEqual(caught.exception.code, "output_budget_exceeded")
                self.assertEqual(len(request.calls), 1)
                self.assertEqual(loop.tokens, 11)
                self.assertTrue(loop.attempts[0]["output_usage_measured"])
                self.assertFalse(loop.summary()["eventual_valid"])

    def test_exact_budget_consumption_stops_without_forcing_a_decision(self) -> None:
        request = Replies(engine.Reply("bad", {"eval_count": 10}))
        loop = engine.DecisionLoop(request, validate,
                                   budget=engine.Budget(attempts=3, output_tokens=10))
        with self.assertRaises(engine.LoopFailure) as caught:
            loop.run("prompt")
        self.assertEqual(caught.exception.code, "output_budget_exhausted")
        self.assertEqual(len(request.calls), 1)
        self.assertFalse(loop.summary()["eventual_valid"])
        self.assertEqual(loop.publication, {"outcome": "not_attempted"})

    def test_attempt_exhaustion_does_not_fabricate_stop(self) -> None:
        request = Replies(*(engine.Reply("bad", {"eval_count": 1}) for _ in range(3)))
        loop = engine.DecisionLoop(request, validate,
                                   budget=engine.Budget(attempts=3, output_tokens=10))
        with self.assertRaises(engine.LoopFailure) as caught:
            loop.run("prompt")
        self.assertEqual(caught.exception.code, "attempts_exhausted")
        self.assertEqual(len(request.calls), 3)
        self.assertTrue(all(attempt["outcome"] == "invalid" for attempt in loop.attempts))
        self.assertFalse(loop.summary()["eventual_valid"])

    def test_explicit_stop_is_the_participants_valid_decision(self) -> None:
        request = Replies(engine.Reply("stop", {}))
        loop = engine.DecisionLoop(request, validate, budget=engine.Budget(attempts=3))
        self.assertEqual(loop.run("prompt"), {"action": "stop"})
        self.assertEqual(len(request.calls), 1)
        self.assertTrue(loop.summary()["first_attempt_valid"])
        self.assertEqual(loop.publication, {"outcome": "not_attempted"})


class FailureTests(unittest.TestCase):
    def test_provider_invalid_unicode_is_terminal_without_retry_or_private_trace(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            journal = root / "journal.json"
            private = root / "private"
            request = Replies(engine.Reply(chr(0xD800), {"eval_count": 1}),
                              engine.Reply("stop", {"eval_count": 1}))
            loop = engine.DecisionLoop(request, validate, budget=engine.Budget(attempts=3),
                                       journal=journal, private=private)
            with self.assertRaises(engine.LoopFailure) as caught:
                loop.run("prompt")
            self.assertEqual(caught.exception.code, "provider_invalid_encoding")
            saved = read_journal(journal)
            self.assertEqual(saved["outcome"], "provider_invalid_encoding")
            self.assertEqual(loop.attempts[0]["outcome"], "provider_invalid_encoding")
            self.assertIs(loop.attempts[0]["candidate_retained_privately"], False)
            self.assertNotIn("candidate_sha256", loop.attempts[0])
            self.assertNotIn("candidate_bytes", loop.attempts[0])
            self.assertFalse(saved["eventual_valid"])
            self.assertEqual(saved["publication"], {"outcome": "not_attempted"})
            self.assertEqual(list(private.iterdir()), [])
            self.assertEqual(len(request.calls), 1)

    def test_provider_failure_is_fatal_and_public_error_text_is_not_copied(self) -> None:
        secret = "PRIVATE-PROVIDER-CREDENTIAL"
        with tempfile.TemporaryDirectory() as temporary:
            journal = Path(temporary) / "attempts.json"
            error = RuntimeError(secret)
            request = Replies(engine.Reply("bad", {"eval_count": 2}), error)
            loop = engine.DecisionLoop(request, validate, journal=journal,
                                       budget=engine.Budget(attempts=3, output_tokens=30))
            with self.assertRaises(RuntimeError) as caught:
                loop.run("prompt")
            self.assertIs(caught.exception, error)
            self.assertEqual(len(request.calls), 2)
            self.assertEqual(loop.outcome, "provider_failed")
            self.assertEqual(loop.attempts[0]["outcome"], "invalid")
            self.assertEqual(loop.attempts[1]["outcome"], "provider_failed")
            self.assertNotIn(secret, journal.read_text(encoding="utf-8"))

    def test_only_fixed_local_validation_errors_are_recoverable(self) -> None:
        request = Replies(engine.Reply("good", {"eval_count": 2}))
        original = RuntimeError("local validator broke")

        def broken(content: str) -> engine.JsonObject:
            raise original

        loop = engine.DecisionLoop(request, broken, budget=engine.Budget(attempts=3))
        with self.assertRaises(RuntimeError) as caught:
            loop.run("prompt")
        self.assertIs(caught.exception, original)
        self.assertEqual(len(request.calls), 1)
        self.assertFalse(loop.summary()["eventual_valid"])
        self.assertEqual(loop.publication, {"outcome": "not_attempted"})

    def test_fatal_validator_error_is_retained_without_private_exception_text(self) -> None:
        secret = "PRIVATE-VALIDATOR-DETAIL"

        def broken(content: str) -> engine.JsonObject:
            raise RuntimeError(secret)

        with tempfile.TemporaryDirectory() as temporary:
            journal = Path(temporary) / "journal.json"
            request = Replies(engine.Reply("good", {"eval_count": 1}))
            loop = engine.DecisionLoop(request, broken, budget=engine.Budget(attempts=3), journal=journal)
            with self.assertRaises(RuntimeError):
                loop.run("prompt")
            saved = read_journal(journal)
            self.assertNotEqual(saved["outcome"], "running")
            self.assertNotEqual(loop.attempts[0]["outcome"], "requested")
            self.assertEqual(saved["outcome"], loop.outcome)
            self.assertNotIn(secret, journal.read_text(encoding="utf-8"))
            self.assertEqual(len(request.calls), 1)

    def test_every_feedback_code_is_local_and_preserves_participant_choice(self) -> None:
        for code, rule in engine.FEEDBACK.items():
            with self.subTest(code=code):
                request = Replies(engine.Reply("bad", {"eval_count": 1}),
                                  engine.Reply("stop", {"eval_count": 1}))

                def local(content: str) -> engine.JsonObject:
                    if content == "bad":
                        raise engine.InvalidDecision(code)
                    return validate(content)

                loop = engine.DecisionLoop(request, local, budget=engine.Budget(attempts=2))
                self.assertEqual(loop.run("context"), {"action": "stop"})
                self.assertEqual(loop.attempts[0]["feedback_code"], code)
                self.assertIn(rule, request.calls[1][0])

    def test_incomplete_or_oversized_provider_reply_is_not_retried(self) -> None:
        for reply, outcome in (
            (engine.Reply("good", {"eval_count": 1}, complete=False), "provider_incomplete"),
            (engine.Reply("x" * (engine.MAX_CANDIDATE_BYTES + 1), {"eval_count": 1}),
             "candidate_budget_exhausted"),
            (engine.Reply("\u00e9" * (engine.MAX_CANDIDATE_BYTES // 2 + 1), {"eval_count": 1}),
             "candidate_budget_exhausted"),
        ):
            with self.subTest(outcome=outcome):
                request = Replies(reply)
                loop = engine.DecisionLoop(request, validate, budget=engine.Budget(attempts=3))
                with self.assertRaises(engine.LoopFailure) as caught:
                    loop.run("prompt")
                self.assertEqual(caught.exception.code, outcome)
                self.assertEqual(len(request.calls), 1)
                self.assertEqual(loop.attempts[0]["outcome"], outcome)

    def test_initial_and_feedback_prompt_bounds_prevent_unbounded_context(self) -> None:
        request = Replies()
        loop = engine.DecisionLoop(request, validate, budget=engine.Budget())
        with self.assertRaises(engine.LoopFailure) as caught:
            loop.run("x" * (engine.MAX_PROMPT_BYTES + 1))
        self.assertEqual(caught.exception.code, "prompt_budget_exhausted")
        self.assertEqual(request.calls, [])
        request = Replies(engine.Reply("bad", {"eval_count": 1}))
        loop = engine.DecisionLoop(request, validate, budget=engine.Budget(attempts=2))
        with self.assertRaises(engine.LoopFailure) as caught:
            loop.run("x" * engine.MAX_PROMPT_BYTES)
        self.assertEqual(caught.exception.code, "prompt_budget_exhausted")
        self.assertEqual(len(request.calls), 1)

    def test_context_bound_counts_encoded_bytes_rather_than_characters(self) -> None:
        request = Replies()
        loop = engine.DecisionLoop(request, validate, budget=engine.Budget())
        with self.assertRaises(engine.LoopFailure) as caught:
            loop.run("\u00e9" * (engine.MAX_PROMPT_BYTES // 2 + 1))
        self.assertEqual(caught.exception.code, "prompt_budget_exhausted")
        self.assertEqual(request.calls, [])


class DeadlineTests(unittest.TestCase):
    def test_each_request_gets_only_the_remaining_total_time(self) -> None:
        clock = Clock()
        calls: list[float] = []

        def request(prompt: str, output_tokens: int, timeout: float) -> engine.Reply:
            calls.append(timeout)
            clock.now += 4
            return engine.Reply("bad" if len(calls) == 1 else "good", {"eval_count": 1})

        with patch("decision_loop.time.monotonic", side_effect=clock):
            loop = engine.DecisionLoop(request, validate, budget=engine.Budget(attempts=3, seconds=10))
            self.assertEqual(loop.run("prompt"), {"action": "revise"})
        self.assertEqual(calls, [10, 6])

    def test_reply_arriving_at_deadline_cannot_become_a_valid_act(self) -> None:
        clock = Clock()
        validated: list[str] = []

        def request(prompt: str, output_tokens: int, timeout: float) -> engine.Reply:
            clock.now = 10
            return engine.Reply("good", {"eval_count": 1})

        def inspect(content: str) -> engine.JsonObject:
            validated.append(content)
            return validate(content)

        with patch("decision_loop.time.monotonic", side_effect=clock):
            loop = engine.DecisionLoop(request, inspect, budget=engine.Budget(seconds=10))
            with self.assertRaises(engine.LoopFailure) as caught:
                loop.run("prompt")
        self.assertEqual(caught.exception.code, "deadline_exhausted")
        self.assertEqual(validated, [])
        self.assertEqual(loop.publication, {"outcome": "not_attempted"})
        self.assertIn("candidate_sha256", loop.attempts[0])

    def test_publication_deadline_is_checked_after_a_valid_decision(self) -> None:
        clock = Clock()
        with tempfile.TemporaryDirectory() as temporary:
            journal = Path(temporary) / "attempts.json"
            with patch("decision_loop.time.monotonic", side_effect=clock):
                loop = engine.DecisionLoop(Replies(engine.Reply("good", {})), validate,
                                           budget=engine.Budget(seconds=10), journal=journal)
                loop.run("prompt")
                clock.now = 10
                with self.assertRaises(engine.LoopFailure) as caught:
                    loop.ensure_deadline()
            self.assertEqual(caught.exception.code, "deadline_exhausted")
            self.assertEqual(read_journal(journal)["outcome"], "deadline_exhausted")
            self.assertEqual(loop.publication, {"outcome": "not_attempted"})
            self.assertTrue(loop.summary()["eventual_valid"])

    def test_deadline_api_cannot_be_used_before_start(self) -> None:
        loop = engine.DecisionLoop(Replies(), validate, budget=engine.Budget())
        with self.assertRaises(engine.LoopFailure) as caught:
            loop.ensure_deadline()
        self.assertEqual(caught.exception.code, "not_started")

    def test_validation_crossing_the_deadline_is_not_recorded_as_valid(self) -> None:
        clock = Clock()

        def slow(content: str) -> engine.JsonObject:
            clock.now = 10
            return validate(content)

        with patch("decision_loop.time.monotonic", side_effect=clock):
            loop = engine.DecisionLoop(Replies(engine.Reply("good", {"eval_count": 1})), slow,
                                       budget=engine.Budget(seconds=10))
            with self.assertRaises(engine.LoopFailure) as caught:
                loop.run("prompt")
        self.assertEqual(caught.exception.code, "deadline_exhausted")
        self.assertFalse(loop.summary()["eventual_valid"])
        self.assertEqual(loop.publication, {"outcome": "not_attempted"})

    def test_journal_delay_cannot_start_a_request_after_the_deadline(self) -> None:
        clock = Clock()
        request = Replies(engine.Reply("good", {"eval_count": 1}))
        with patch("decision_loop.time.monotonic", side_effect=clock):
            loop = engine.DecisionLoop(request, validate, budget=engine.Budget(seconds=10))
            original_save = loop.save

            def slow_save() -> None:
                original_save()
                if loop.attempts and loop.attempts[-1]["outcome"] == "requested":
                    clock.now = 10

            with patch.object(loop, "save", side_effect=slow_save):
                with self.assertRaises(engine.LoopFailure) as caught:
                    loop.run("prompt")
        self.assertEqual(caught.exception.code, "deadline_exhausted")
        self.assertEqual(request.calls, [])
        self.assertFalse(loop.summary()["eventual_valid"])


class EvidenceTests(unittest.TestCase):
    def test_journal_exists_before_a_provider_can_block(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            journal = Path(temporary) / "attempts.json"

            def request(prompt: str, output_tokens: int, timeout: float) -> engine.Reply:
                saved = read_journal(journal)
                self.assertEqual(saved["outcome"], "running")
                attempts = saved["attempts"]
                self.assertIsInstance(attempts, list)
                if not isinstance(attempts, list):
                    raise AssertionError("missing attempt list")
                self.assertEqual(attempts[0]["outcome"], "requested")
                raise TimeoutError("provider timed out")

            loop = engine.DecisionLoop(request, validate, journal=journal, budget=engine.Budget(attempts=3))
            with self.assertRaises(TimeoutError):
                loop.run("prompt")
            self.assertEqual(read_journal(journal)["outcome"], "provider_failed")
            self.assertFalse(journal.with_suffix(".json.tmp").exists())

    def test_public_summaries_hash_candidates_and_filter_arbitrary_metrics(self) -> None:
        secret = "PRIVATE-CANDIDATE-AND-PROMPT"
        metrics: engine.JsonObject = {"eval_count": 2, "prompt_eval_count": 7, "total_duration": 10,
                                      "provider_detail": secret, "authorization": secret}
        with tempfile.TemporaryDirectory() as temporary:
            journal = Path(temporary) / "public.json"
            request = Replies(engine.Reply(secret, metrics))
            loop = engine.DecisionLoop(request, validate, budget=engine.Budget(), journal=journal)
            with self.assertRaises(engine.LoopFailure):
                loop.run(secret)
            self.assertNotIn(secret, journal.read_text(encoding="utf-8"))
            attempt = loop.attempts[0]
            self.assertEqual(attempt["candidate_sha256"], hashlib.sha256(secret.encode()).hexdigest())
            self.assertEqual(attempt["prompt_sha256"], hashlib.sha256(secret.encode()).hexdigest())
            self.assertEqual(attempt["metrics"], {"eval_count": 2, "prompt_eval_count": 7, "total_duration": 10})
            self.assertFalse(loop.summary()["exact_candidates_retained_privately"])
            self.assertEqual(list(Path(temporary).iterdir()), [journal])

    def test_optional_private_traces_keep_exact_distinct_attempts(self) -> None:
        candidate = "invalid participant text with a newline\nsecond line"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            private = root / "private"
            journal = root / "public.json"
            request = Replies(engine.Reply(candidate, {"eval_count": 2}),
                              engine.Reply("good", {"eval_count": 2}))
            loop = engine.DecisionLoop(request, validate, budget=engine.Budget(attempts=2),
                                       journal=journal, private=private)
            loop.run("initial source context")
            first = read_journal(private / "attempt-1.json")
            second = read_journal(private / "attempt-2.json")
            self.assertEqual(first, {"prompt": "initial source context", "candidate": candidate})
            self.assertEqual(second["candidate"], "good")
            self.assertNotEqual(first["prompt"], second["prompt"])
            self.assertNotIn(candidate, journal.read_text(encoding="utf-8"))
            snapshot = (private / "attempt-1.json").read_bytes()
            with self.assertRaises(engine.LoopFailure):
                loop.run("rerun must not replace traces")
            self.assertEqual((private / "attempt-1.json").read_bytes(), snapshot)
            self.assertEqual(len(request.calls), 2)

    def test_oversized_reply_is_hashed_without_unbounded_private_retention(self) -> None:
        candidate = "x" * (engine.MAX_CANDIDATE_BYTES * 4)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            private = root / "private"
            journal = root / "journal.json"
            loop = engine.DecisionLoop(Replies(engine.Reply(candidate, {"eval_count": 1})), validate,
                                       budget=engine.Budget(), journal=journal, private=private)
            with self.assertRaises(engine.LoopFailure) as caught:
                loop.run("prompt")
            self.assertEqual(caught.exception.code, "candidate_budget_exhausted")
            self.assertFalse(loop.summary()["exact_candidates_retained_privately"])
            self.assertEqual(loop.attempts[0]["candidate_bytes"], len(candidate.encode()))
            self.assertEqual(loop.attempts[0]["candidate_sha256"], hashlib.sha256(candidate.encode()).hexdigest())
            self.assertNotIn(candidate, journal.read_text(encoding="utf-8"))
            for trace in private.iterdir():
                self.assertIsNone(read_journal(trace).get("candidate"))
            self.assertLess(sum(trace.stat().st_size for trace in private.iterdir()), len(candidate.encode()))

    def test_summary_is_an_independent_evidence_snapshot(self) -> None:
        loop = engine.DecisionLoop(Replies(engine.Reply("good", {"eval_count": 1})), validate,
                                   budget=engine.Budget())
        loop.run("prompt")
        record: engine.JsonObject = {"id": "submission:one", "body": {"text": "original"}}
        loop.publication_state("publication_started", record=record)
        snapshot = loop.summary()
        attempts = cast(list[engine.JsonObject], snapshot["attempts"])
        attempts[0]["outcome"] = "forged outcome"
        publication = cast(engine.JsonObject, snapshot["publication"])
        stored_record = cast(engine.JsonObject, publication["record"])
        cast(engine.JsonObject, stored_record["body"])["text"] = "forged text"
        self.assertTrue(loop.summary()["first_attempt_valid"])
        self.assertEqual(loop.summary()["publication"], {"outcome": "publication_started", "record": record})

    def test_publication_snapshots_inputs_and_keeps_envelope_on_receipt_update(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            journal = Path(temporary) / "journal.json"
            request = Replies(engine.Reply("good", {"eval_count": 1}))
            loop = engine.DecisionLoop(request, validate, budget=engine.Budget(), journal=journal)
            loop.run("prompt")
            record: engine.JsonObject = {"id": "submission:one", "body": {"text": "original"}}
            loop.publication_state("publication_started", record=record)
            cast(engine.JsonObject, record["body"])["text"] = "later local mutation"
            receipt: engine.JsonObject = {"event_id": "event:one"}
            loop.publication_state("verification_failed", receipt=receipt)
            receipt["event_id"] = "later receipt mutation"
            loop.save()
            self.assertEqual(read_journal(journal)["publication"], {
                "outcome": "verification_failed",
                "record": {"id": "submission:one", "body": {"text": "original"}},
                "receipt": {"event_id": "event:one"},
            })
            self.assertEqual(len(request.calls), 1)

    def test_existing_journal_or_private_directory_is_not_overwritten(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            journal = root / "journal.json"
            journal.write_text("existing evidence", encoding="utf-8")
            with self.assertRaises(FileExistsError):
                engine.DecisionLoop(Replies(), validate, budget=engine.Budget(), journal=journal)
            self.assertEqual(journal.read_text(encoding="utf-8"), "existing evidence")
            private = root / "private"
            private.mkdir()
            original = private / "attempt-1.json"
            original.write_text("earlier trace", encoding="utf-8")
            with self.assertRaises(FileExistsError):
                engine.DecisionLoop(Replies(), validate, budget=engine.Budget(), private=private)
            self.assertEqual(original.read_text(encoding="utf-8"), "earlier trace")

    def test_private_storage_failure_retains_received_attempt_without_exception_text(self) -> None:
        secret = "PRIVATE-STORAGE-ERROR-DETAIL"
        original_open = Path.open
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            private = root / "private"
            journal = root / "journal.json"
            request = Replies(engine.Reply("good", {"eval_count": 1}))
            loop = engine.DecisionLoop(request, validate, budget=engine.Budget(attempts=3),
                                       private=private, journal=journal)

            def open_trace(path: Path, mode: str = "r", buffering: int = -1,
                           encoding: str | None = None, errors: str | None = None,
                           newline: str | None = None) -> TextIO:
                if path.parent == private:
                    raise PermissionError(secret)
                return cast(TextIO, original_open(path, mode, buffering, encoding, errors, newline))

            with patch.object(Path, "open", new=open_trace), self.assertRaises(PermissionError):
                loop.run("prompt")
            saved = read_journal(journal)
            self.assertNotEqual(saved["outcome"], "running")
            self.assertNotEqual(loop.attempts[0]["outcome"], "requested")
            self.assertEqual(loop.attempts[0]["candidate_sha256"], hashlib.sha256(b"good").hexdigest())
            self.assertEqual(loop.attempts[0]["charged_output_tokens"], 1)
            self.assertFalse(loop.summary()["eventual_valid"])
            self.assertNotIn(secret, journal.read_text(encoding="utf-8"))
            self.assertEqual(len(request.calls), 1)

    def test_uncertain_publication_is_retained_without_another_model_request(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            journal = Path(temporary) / "journal.json"
            request = Replies(engine.Reply("good", {"eval_count": 1}))
            loop = engine.DecisionLoop(request, validate, budget=engine.Budget(attempts=3), journal=journal)
            loop.run("prompt")
            record: engine.JsonObject = {"id": "submission:original", "body": {"text": "chosen text"}}
            loop.publication_state("publication_started", record=record)
            self.assertEqual(read_journal(journal)["publication"], {"outcome": "publication_started", "record": record})
            receipt: engine.JsonObject = {"record_id": "submission:original", "event_id": "event:one"}
            loop.publication_state("verification_failed", record=record, receipt=receipt)
            self.assertEqual(read_journal(journal)["publication"],
                             {"outcome": "verification_failed", "record": record, "receipt": receipt})
            with self.assertRaises(engine.LoopFailure) as caught:
                loop.run("do not regenerate the submitted act")
            self.assertEqual(caught.exception.code, "loop_already_used")
            self.assertEqual(len(request.calls), 1)
            self.assertEqual(loop.publication["outcome"], "verification_failed")


if __name__ == "__main__":
    unittest.main()
