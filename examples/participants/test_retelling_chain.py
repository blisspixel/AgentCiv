"""Test inspection of synthetic retelling ancestry, apparent consensus, and delayed corrections."""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "http-commons"))
import retelling_chain as rc
import walk

FIXTURE_PATH = Path(__file__).resolve().parent / "fixtures" / "retelling-table.json"


class RetellingChainTests(unittest.TestCase):
    reader: Path

    @classmethod
    def setUpClass(cls) -> None:
        cls.reader = walk.build_rust_binaries("agentciv-reader")["agentciv-reader"]

    def test_retelling_chain_inspection(self) -> None:
        res = rc.inspect(FIXTURE_PATH, self.reader)
        self.assertEqual(res["outcome"], "inspected")
        self.assertEqual(res["format"], rc.FORMAT)
        self.assertEqual(res["world"], "civ:frozen-retelling-table")
        self.assertEqual(res["scope"], "frozen_synthetic_records_not_an_observed_http_traversal")
        self.assertEqual(res["native_concerns_count"], 1)

        # Check ancestry analysis: 5 distinct authors cite the single root proposal
        ancestry = res["ancestry_analyses"]
        self.assertEqual(len(ancestry), 1)
        root_analysis = ancestry[0]
        self.assertEqual(
            root_analysis["root_source"],
            {
                "from": "agent:author",
                "artifact_id": "artifact:table-proposal",
                "revision": 1,
            },
        )
        self.assertEqual(root_analysis["descendant_count"], 5)
        self.assertEqual(root_analysis["distinct_author_count"], 5)
        self.assertFalse(root_analysis["independent_support_established"])

        # Check that objection and decline are recorded and attributed
        self.assertEqual(len(res["objections"]), 1)
        self.assertEqual(res["objections"][0]["from"], "agent:dissenter")
        self.assertEqual(
            res["objections"][0]["target"],
            {
                "from": "agent:author",
                "artifact_id": "artifact:table-proposal",
                "revision": 1,
            },
        )

        self.assertEqual(len(res["declines"]), 1)
        self.assertEqual(res["declines"][0]["from"], "agent:refuser")

        # Check withdrawal
        self.assertEqual(len(res["withdrawals"]), 1)
        self.assertEqual(res["withdrawals"][0]["from"], "agent:author")
        self.assertEqual(res["withdrawals"][0]["target"]["revision"], 1)

        # Check adoption
        self.assertEqual(len(res["adoptions"]), 1)
        self.assertEqual(res["adoptions"][0]["from"], "agent:adopter")
        self.assertEqual(res["adoptions"][0]["practice"]["revision"], 2)
        self.assertIn("My own reading schedule only", res["adoptions"][0]["scope"])

        # Check civic non-inference boundaries
        self.assertEqual(res["group_state"], "not_inferred")
        self.assertEqual(res["silence_or_refusal"], "not_converted_into_adoption")
        self.assertEqual(res["consensus_authority"], "none_inferred")

        # Check non-adopting summary
        non_adopting = res["non_adopting_participants"]
        self.assertIn("agent:refuser", non_adopting["declined_participants"])
        self.assertIn("agent:silent-observer", non_adopting["silent_or_general_notes"])

    def test_retelling_pruned_objections_and_consensus_detection(self) -> None:
        res = rc.inspect(FIXTURE_PATH, self.reader)
        evals = res["retelling_evaluations"]

        # Faithful retelling noted the objection
        faithful = next(e for e in evals if e["artifact"]["from"] == "agent:relayer-1")
        self.assertTrue(faithful["notes_recorded_objection"])
        self.assertFalse(faithful["pruned_recorded_objections"])

        # Pruned retelling dropped the objection
        pruned = next(e for e in evals if e["artifact"]["from"] == "agent:relayer-2")
        self.assertFalse(pruned["notes_recorded_objection"])
        self.assertTrue(pruned["pruned_recorded_objections"])

        # Consensus claiming retelling asserts consensus where none is recorded
        consensus = next(e for e in evals if e["artifact"]["from"] == "agent:relayer-3")
        self.assertTrue(consensus["claims_consensus"])
        self.assertTrue(consensus["pruned_recorded_objections"])
        claim_eval = consensus["consensus_claim_evaluation"]
        self.assertTrue(claim_eval["apparent_consensus_claimed"])
        self.assertFalse(claim_eval["supported_by_records"])

    def test_stale_basis_evaluation(self) -> None:
        res = rc.inspect(FIXTURE_PATH, self.reader)
        stale_evals = res["stale_basis_evaluations"]

        # relayer-1 derived from r1 before r2 existed
        relayer_1 = next(s for s in stale_evals if s["retelling"]["from"] == "agent:relayer-1")
        self.assertFalse(relayer_1["newer_revision_preceded_retelling"])

        # late-relayer derived from r1 AFTER r2 was already recorded
        late = next(s for s in stale_evals if s["retelling"]["from"] == "agent:late-relayer")
        self.assertTrue(late["newer_revision_preceded_retelling"])
        self.assertIn("Derived from revision 1 after author published revision 2", late["staleness_finding"])

    def test_event_opening(self) -> None:
        res = rc.inspect(FIXTURE_PATH, self.reader, event_id="event:retell-1")
        self.assertEqual(res["outcome"], "opened")
        self.assertEqual(res["format"], rc.FORMAT)
        self.assertIn("artifact:table-proposal", res["original_utf8"])
        self.assertFalse(res["origin_authenticated"])

        # Non-existent event returns failed
        missing = rc.inspect(FIXTURE_PATH, self.reader, event_id="event:does-not-exist")
        self.assertEqual(missing["outcome"], "failed")
        self.assertEqual(missing["code"], "event_unavailable")

    def test_hash_mismatch_failure(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)
            with FIXTURE_PATH.open("r", encoding="utf-8") as f:
                data = json.load(f)
            data["records"][0]["original_utf8"] = data["records"][0]["original_utf8"].replace("Morning", "Evening")
            corrupt_file = tmp_path / "corrupt.json"
            with corrupt_file.open("w", encoding="utf-8") as f:
                json.dump(data, f)

            res = rc.inspect(corrupt_file, self.reader)
            self.assertEqual(res["outcome"], "failed")
            self.assertEqual(res["code"], "original_hash_mismatch")

    def test_invalid_format_or_missing_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)
            # Missing file
            res = rc.inspect(tmp_path / "nonexistent.json", self.reader)
            self.assertEqual(res["outcome"], "failed")
            self.assertEqual(res["code"], "input_unavailable")

            # Invalid format header
            bad_file = tmp_path / "bad_format.json"
            bad_file.write_text(json.dumps({"format": "bad-format"}), encoding="utf-8")
            bad_res = rc.inspect(bad_file, self.reader)
            self.assertEqual(bad_res["outcome"], "failed")
            self.assertEqual(bad_res["code"], "invalid_fixture")

    def test_cycle_detection(self) -> None:
        event_a = {
            "protocol_version": "0.1-draft",
            "type": "event",
            "id": "event:cycle-1",
            "world": "civ:cycle",
            "sequence": 1,
            "timestamp": "2026-10-15T10:00:01Z",
            "kind": "artifact.recorded",
            "actor": "agent:a",
            "body": {
                "artifact_revision": {
                    "protocol_version": "0.1-draft",
                    "type": "artifact_revision",
                    "id": "sub:a",
                    "world": "civ:cycle",
                    "from": "agent:a",
                    "to": ["agent:b"],
                    "artifact_id": "art:cycle",
                    "media_type": "application/json",
                    "revision": 1,
                    "body": {
                        "retelling_proposal": {
                            "format": rc.PROPOSAL_FORMAT,
                            "title": "A",
                            "text": "A text",
                        }
                    },
                }
            },
        }
        event_b = {
            "protocol_version": "0.1-draft",
            "type": "event",
            "id": "event:cycle-2",
            "world": "civ:cycle",
            "sequence": 2,
            "timestamp": "2026-10-15T10:00:02Z",
            "kind": "artifact.recorded",
            "actor": "agent:b",
            "body": {
                "artifact_revision": {
                    "protocol_version": "0.1-draft",
                    "type": "artifact_revision",
                    "id": "sub:b",
                    "world": "civ:cycle",
                    "from": "agent:b",
                    "to": ["agent:a"],
                    "artifact_id": "art:b",
                    "media_type": "application/json",
                    "revision": 1,
                    "derived_from": {"from": "agent:a", "artifact_id": "art:cycle", "revision": 1},
                    "body": {
                        "retelling_record": {
                            "format": rc.RECORD_FORMAT,
                            "summary": "B summary",
                            "recorded_objections_noted": True,
                            "claims_consensus": False,
                        }
                    },
                }
            },
        }
        originals = [
            json.dumps(event_a, separators=(",", ":")),
            json.dumps(event_b, separators=(",", ":")),
        ]
        reconstructed = rc.reconstruct_retellings(originals)
        self.assertEqual(len(reconstructed["proposals"]), 1)
        self.assertEqual(len(reconstructed["retellings"]), 1)

    def test_cli_invocation(self) -> None:
        cmd = [
            sys.executable,
            str(Path(__file__).resolve().parent / "retelling_chain.py"),
            "--fixture",
            str(FIXTURE_PATH),
            "--reader-binary",
            str(self.reader),
        ]
        res = subprocess.run(cmd, capture_output=True, text=True, check=False)
        self.assertEqual(res.returncode, 0)
        parsed = json.loads(res.stdout)
        self.assertEqual(parsed["outcome"], "inspected")
        self.assertEqual(parsed["native_concerns_count"], 1)


if __name__ == "__main__":
    unittest.main()
