"""Frozen missing-grant pilot freeze and declaration tests.

No test here contacts Ollama, opens a network socket, or runs model inference.
"""
from __future__ import annotations

import copy
import json
import sys
import unittest
from io import StringIO
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import missing_grant_pilot as pilot  # noqa: E402

JsonObject = dict[str, object]


def capture(argv: list[str]) -> tuple[int, JsonObject]:
    buffer = StringIO()
    with patch("sys.stdout", buffer):
        code = pilot.main(argv)
    parsed: object = json.loads(buffer.getvalue())
    if not isinstance(parsed, dict):
        raise AssertionError("expected a JSON object")
    return code, {str(key): value for key, value in parsed.items()}


def load_frozen() -> tuple[JsonObject, JsonObject, bytes, bytes]:
    manifest = pilot._load_json(pilot.MANIFEST_PATH)
    fixture = pilot._load_json(pilot.FIXTURE_PATH)
    card = pilot.canonical_bytes(pilot.CARD_PATH)
    fixture_bytes = pilot.canonical_bytes(pilot.FIXTURE_PATH)
    return manifest, fixture, card, fixture_bytes


class MissingGrantFreezeTests(unittest.TestCase):
    def test_valid_frozen_manifest_passes_and_stays_unobserved(self) -> None:
        result = pilot.check_freeze()
        card = pilot.canonical_bytes(pilot.CARD_PATH)
        fixture_bytes = pilot.canonical_bytes(pilot.FIXTURE_PATH)
        self.assertEqual(result["outcome"], "frozen")
        self.assertEqual(result["observation"], "unobserved")
        self.assertEqual(result["trial"], pilot.TRIAL_ID)
        self.assertEqual(result["card_sha256"], pilot._digest(card))
        self.assertEqual(result["fixture_sha256"], pilot._digest(fixture_bytes))
        self.assertEqual(result["card_canonical_bytes"], len(card))
        self.assertEqual(result["fixture_canonical_bytes"], len(fixture_bytes))
        self.assertEqual(result["copying_permission"], "not_granted")
        self.assertEqual(result["external_spend_usd"], 0)
        self.assertNotIn(b"\r\n", card)

    def test_missing_departure_rule_fails_verify_freeze(self) -> None:
        manifest, fixture, card, fixture_bytes = load_frozen()
        mutated = copy.deepcopy(manifest)
        del mutated["departure_rule"]
        with self.assertRaises(pilot.TrialError) as caught:
            pilot.verify_freeze(mutated, fixture, card, fixture_bytes)
        self.assertEqual(caught.exception.code, "freeze_mismatch")

    def test_missing_disclosure_fails_verify_freeze(self) -> None:
        manifest, fixture, card, fixture_bytes = load_frozen()
        mutated = copy.deepcopy(manifest)
        del mutated["disclosure"]
        with self.assertRaises(pilot.TrialError) as caught:
            pilot.verify_freeze(mutated, fixture, card, fixture_bytes)
        self.assertEqual(caught.exception.code, "freeze_mismatch")

    def test_missing_disclosure_subfields_and_empty_declarations_fail(self) -> None:
        manifest, fixture, card, fixture_bytes = load_frozen()
        for field in pilot.DISCLOSURE_FIELDS:
            with self.subTest(missing_disclosure_field=field):
                mutated = copy.deepcopy(manifest)
                disclosure = mutated["disclosure"]
                self.assertIsInstance(disclosure, dict)
                assert isinstance(disclosure, dict)
                del disclosure[field]
                with self.assertRaises(pilot.TrialError) as caught:
                    pilot.verify_freeze(mutated, fixture, card, fixture_bytes)
                self.assertEqual(caught.exception.code, "freeze_mismatch")

        for field, bad_value in (
            ("purpose", "   "),
            ("stop_conditions", []),
            ("stop_conditions", ["   "]),
            ("departure_rule", ""),
            ("seeded_lines", [123]),
            ("reply_policy", ""),
        ):
            with self.subTest(field=field, bad_value=bad_value):
                mutated = copy.deepcopy(manifest)
                mutated[field] = bad_value
                with self.assertRaises(pilot.TrialError) as caught:
                    pilot.verify_freeze(mutated, fixture, card, fixture_bytes)
                self.assertEqual(caught.exception.code, "freeze_mismatch")

    def test_empty_seeded_lines_list_is_allowed_by_declarations_check(self) -> None:
        manifest, _fixture, _card, _fixture_bytes = load_frozen()
        mutated = copy.deepcopy(manifest)
        mutated["seeded_lines"] = []
        pilot.verify_study_declarations(mutated)

    def test_card_or_fixture_digest_mismatch_fails_verify_freeze(self) -> None:
        manifest, fixture, card, fixture_bytes = load_frozen()
        with self.assertRaises(pilot.TrialError) as caught_card:
            pilot.verify_freeze(manifest, fixture, card + b"\n", fixture_bytes)
        self.assertEqual(caught_card.exception.code, "freeze_mismatch")

        with self.assertRaises(pilot.TrialError) as caught_fixture:
            pilot.verify_freeze(manifest, fixture, card, fixture_bytes + b"\n")
        self.assertEqual(caught_fixture.exception.code, "freeze_mismatch")

    def test_invalid_observation_or_budget_fails_verify_freeze(self) -> None:
        manifest, fixture, card, fixture_bytes = load_frozen()
        observed = copy.deepcopy(manifest)
        observed["observation"] = "observed"
        with self.assertRaises(pilot.TrialError) as caught:
            pilot.verify_freeze(observed, fixture, card, fixture_bytes)
        self.assertEqual(caught.exception.code, "freeze_mismatch")

        bad_budget = copy.deepcopy(manifest)
        budgets = bad_budget["budgets"]
        self.assertIsInstance(budgets, dict)
        assert isinstance(budgets, dict)
        budgets["steps"] = True
        with self.assertRaises(pilot.TrialError) as caught_budget:
            pilot.verify_freeze(bad_budget, fixture, card, fixture_bytes)
        self.assertEqual(caught_budget.exception.code, "freeze_mismatch")

    def test_peer_invitation_cannot_be_a_grant(self) -> None:
        manifest, fixture, card, fixture_bytes = load_frozen()
        mutated_fixture = copy.deepcopy(fixture)
        peer = mutated_fixture["peer_invitation"]
        self.assertIsInstance(peer, dict)
        assert isinstance(peer, dict)
        peer["is_grant"] = True
        with self.assertRaises(pilot.TrialError) as caught:
            pilot.verify_freeze(manifest, mutated_fixture, card, fixture_bytes)
        self.assertEqual(caught.exception.code, "freeze_mismatch")


class OutcomeCodingTests(unittest.TestCase):
    def test_valid_outcome_records_serialize_all_eight_fields(self) -> None:
        record = pilot.code_outcome(
            offered_capability="read_only",
            participant_choice="draft",
            requested_action="draft",
            host_decision="not_applicable",
            persistent_effect="unchanged",
            useful_permitted_work_retained=True,
            names_grant_owner=True,
            indeterminate=False,
        )
        as_dict = record.to_dict()
        self.assertEqual(tuple(as_dict.keys()), pilot.OUTCOME_FIELDS)
        self.assertEqual(as_dict["offered_capability"], "read_only")
        self.assertTrue(as_dict["useful_permitted_work_retained"])
        self.assertFalse(as_dict["indeterminate"])

        decline_record = pilot.code_outcome(
            offered_capability="read_only",
            participant_choice="decline",
            requested_action="none",
            host_decision="not_applicable",
            persistent_effect="unchanged",
            useful_permitted_work_retained=False,
            names_grant_owner=False,
            indeterminate=False,
        )
        self.assertEqual(decline_record.participant_choice, "decline")

        indeterminate_record = pilot.code_outcome(
            offered_capability="read_write",
            participant_choice="indeterminate",
            requested_action="indeterminate",
            host_decision="host_error",
            persistent_effect="indeterminate",
            useful_permitted_work_retained=False,
            names_grant_owner=False,
            indeterminate=True,
        )
        self.assertTrue(indeterminate_record.indeterminate)

    def test_invalid_outcome_combinations_are_rejected(self) -> None:
        with self.assertRaises(pilot.TrialError):
            pilot.code_outcome(
                offered_capability="read_only",
                participant_choice="say",
                requested_action="say",
                host_decision="allowed",
                persistent_effect="appended_one_event",
                useful_permitted_work_retained=False,
                names_grant_owner=False,
            )
        with self.assertRaises(pilot.TrialError):
            pilot.code_outcome(
                offered_capability="read_only",
                participant_choice="indeterminate",
                requested_action="none",
                host_decision="not_applicable",
                persistent_effect="unchanged",
                useful_permitted_work_retained=False,
                names_grant_owner=False,
                indeterminate=False,
            )
        with self.assertRaises(pilot.TrialError):
            pilot.code_outcome(
                offered_capability="admin",
                participant_choice="quiet",
                requested_action="none",
                host_decision="not_applicable",
                persistent_effect="unchanged",
                useful_permitted_work_retained=False,
                names_grant_owner=False,
            )


class CommandTests(unittest.TestCase):
    def test_cli_check_passes_and_reports_failure_on_drift(self) -> None:
        code, result = capture(["check"])
        self.assertEqual(code, 0)
        self.assertEqual(result["outcome"], "frozen")
        self.assertEqual(result["observation"], "unobserved")

        with patch.object(pilot, "verify_freeze", side_effect=pilot.TrialError("freeze_mismatch")):
            fail_code, failed = capture(["check"])
        self.assertEqual(fail_code, 1)
        self.assertEqual(failed["outcome"], "failed")
        self.assertEqual(failed["code"], "freeze_mismatch")


if __name__ == "__main__":
    unittest.main()
