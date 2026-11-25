"""Unit and dual-host tests for show --since (#170), init --seed reading-table (#168),
and configurable session/retention duration knobs (#178).
"""

from __future__ import annotations

import io
import json
import socket
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples" / "http-commons"))
import collaboration as civic  # noqa: E402
import encounter_client as client  # noqa: E402
import encounter_private as private  # noqa: E402
import local_encounter as operator  # noqa: E402
import local_participant as wire  # noqa: E402

JsonObject = dict[str, object]
WORLD = "civ:local-room-features-test"
WRITER = "agent:writer"
READER = "agent:reader"


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def _make_config(root: Path, principal: str = WRITER) -> tuple[Path, JsonObject, Path]:
    state = root / f"private-{principal.replace(':', '-')}"
    private.create_directory(state)
    cache = state / "cache"
    private.create_directory(cache)
    value: JsonObject = {
        "format": client.CONFIG_FORMAT,
        "origin": "http://127.0.0.1:8787",
        "world": WORLD,
        "principal": principal,
        "token": "synthetic-fixture-credential-0123456789",
        "payload_bytes": 16384,
        "cache_directory": str(cache),
        "private_cache_permission": client.CACHE_PERMISSION,
        "members": [WRITER, READER],
    }
    path = state / "client.json"
    private.write_new(path, json.dumps(value).encode("utf-8"))
    return path, value, cache


def _save_synthetic_view(
    cache: Path,
    name: str,
    principal: str,
    events: list[JsonObject],
    *,
    started: str,
    finished: str,
) -> Path:
    originals = [json.dumps(item, ensure_ascii=False, separators=(",", ":")) for item in events]
    report: JsonObject = {
        "pages": 1,
        "events": len(originals),
        "response_bytes": sum(len(item.encode("utf-8")) for item in originals) + 64,
        "reached_end": True,
        "scope": "current_caller_view",
        "copying_permission": "not_granted",
    }
    view: JsonObject = {
        "format": client.VIEW_FORMAT,
        "world": WORLD,
        "principal": principal,
        "snapshot": {"world": WORLD, "records": originals},
        "transport": report,
        "offers": {
            "format": "agentciv-offer-view/0.1-example",
            "world": WORLD,
            "offers": [],
            "report": {**report, "truncated": False},
        },
        "retrieval_started": started,
        "retrieval_finished": finished,
        "copying_permission": "not_granted",
        "private_cache_permission": client.CACHE_PERMISSION,
        "freshness": "bounded_full_read_not_atomic",
        "configured_budgets": client.BUDGETS,
    }
    path = cache / name
    private.write_new(path, json.dumps(view, ensure_ascii=False, indent=2).encode("utf-8"))
    return path


def _event(
    seq: int,
    event_id: str,
    kind: str,
    actor: str,
    body: JsonObject,
    timestamp: str = "2026-11-25T12:00:00Z",
) -> JsonObject:
    return {
        "protocol_version": "0.1-draft",
        "type": "event",
        "id": event_id,
        "world": WORLD,
        "sequence": seq,
        "timestamp": timestamp,
        "kind": kind,
        "actor": actor,
        "body": body,
    }


class ShowSinceDigestTests(unittest.TestCase):
    def test_unchanged_views_report_zero_new_records_since_seq(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config, _, cache = _make_config(Path(temporary).resolve())
            msg = _event(
                3,
                "event:m-3",
                "message.recorded",
                WRITER,
                {
                    "message": {
                        "protocol_version": "0.1-draft",
                        "type": "message",
                        "id": "message:3",
                        "world": WORLD,
                        "from": WRITER,
                        "to": [READER],
                        "body": {"text": "initial note"},
                    }
                },
            )
            prior = _save_synthetic_view(
                cache,
                "view-prior.json",
                WRITER,
                [msg],
                started="2026-11-25T12:00:00+00:00",
                finished="2026-11-25T12:00:01+00:00",
            )
            later = _save_synthetic_view(
                cache,
                "view-later.json",
                WRITER,
                [msg],
                started="2026-11-25T12:05:00+00:00",
                finished="2026-11-25T12:05:01+00:00",
            )
            with patch.object(wire, "exchange") as network:
                rendered = client.show(config, later, since=prior)
            network.assert_not_called()
            self.assertIn("read-only digest from saved caller views", rendered)
            self.assertIn("0 new records since seq 3 (1 unchanged).", rendered)

    def test_digest_counts_all_record_types_and_links_to_originals(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config, _, cache = _make_config(Path(temporary).resolve())
            art1 = _event(
                1,
                "event:art-1",
                "artifact.recorded",
                WRITER,
                {
                    "artifact_revision": {
                        "protocol_version": "0.1-draft",
                        "type": "artifact_revision",
                        "id": "sub:art-1",
                        "world": WORLD,
                        "from": WRITER,
                        "to": [READER],
                        "artifact_id": "artifact:plan",
                        "media_type": "application/json",
                        "revision": 1,
                        "body": {"text": "draft one"},
                    }
                },
            )
            art1_withdrawn = _event(1, "event:art-1", "artifact.withdrawn", WRITER, {})
            msg2 = _event(
                2,
                "event:msg-2",
                "message.recorded",
                READER,
                {
                    "message": {
                        "protocol_version": "0.1-draft",
                        "type": "message",
                        "id": "message:2",
                        "world": WORLD,
                        "from": READER,
                        "to": [WRITER],
                        "body": {"text": "hello\u001b[31mworld"},
                    }
                },
            )
            art3 = _event(
                3,
                "event:art-3",
                "artifact.recorded",
                WRITER,
                {
                    "artifact_revision": {
                        "protocol_version": "0.1-draft",
                        "type": "artifact_revision",
                        "id": "sub:art-3",
                        "world": WORLD,
                        "from": WRITER,
                        "to": [READER],
                        "artifact_id": "artifact:plan",
                        "media_type": "application/json",
                        "revision": 2,
                        "body": {"text": "revised"},
                    }
                },
            )
            obj4 = _event(
                4,
                "event:obj-4",
                "objection.recorded",
                READER,
                {
                    "objection": {
                        "protocol_version": "0.1-draft",
                        "type": "objection",
                        "id": "sub:obj-4",
                        "world": WORLD,
                        "from": READER,
                        "to": [WRITER],
                        "target_from": WRITER,
                        "artifact_id": "artifact:plan",
                        "revision": 2,
                        "body": {"text": "disagree on scope"},
                    }
                },
            )
            dec5 = _event(
                5,
                "event:dec-5",
                "decline.recorded",
                READER,
                {
                    "decline": {
                        "protocol_version": "0.1-draft",
                        "type": "decline",
                        "id": "sub:dec-5",
                        "world": WORLD,
                        "from": READER,
                        "to": [WRITER],
                        "target_from": WRITER,
                        "artifact_id": "artifact:plan",
                        "revision": 2,
                        "body": {"text": "declining this task"},
                    }
                },
            )

            prior = _save_synthetic_view(
                cache,
                "view-1.json",
                WRITER,
                [art1],
                started="2026-11-25T12:00:00+00:00",
                finished="2026-11-25T12:00:01+00:00",
            )
            later = _save_synthetic_view(
                cache,
                "view-2.json",
                WRITER,
                [art1_withdrawn, msg2, art3, obj4, dec5],
                started="2026-11-25T12:10:00+00:00",
                finished="2026-11-25T12:10:01+00:00",
            )
            with patch.object(wire, "exchange") as network:
                rendered = client.show(config, later, since=prior)
            network.assert_not_called()
            self.assertIn(
                "Digest since seq 1: 1 new messages, 1 revisions, 1 objections, 1 declines, 1 withdrawals, 0 unchanged.",
                rendered,
            )
            self.assertNotIn("\u001b", rendered)
            self.assertIn("hello\\u001b[31mworld", rendered)
            for eid in ("event:msg-2", "event:art-3", "event:obj-4", "event:dec-5", "event:art-1"):
                self.assertIn(f'original: show --event-id "{eid}"', rendered)
            for forbidden in ("karma", "delivered", "consensus", "reply now", "online"):
                self.assertNotIn(forbidden, rendered.lower())

    def test_digest_rejects_conflicting_flags_reversed_times_and_cross_principal_views(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, _, cache = _make_config(root, WRITER)
            _, _, other_cache = _make_config(root, READER)
            v1 = _save_synthetic_view(
                cache,
                "view-1.json",
                WRITER,
                [],
                started="2026-11-25T12:00:00+00:00",
                finished="2026-11-25T12:00:01+00:00",
            )
            v2 = _save_synthetic_view(
                cache,
                "view-2.json",
                WRITER,
                [],
                started="2026-11-25T12:05:00+00:00",
                finished="2026-11-25T12:05:01+00:00",
            )
            other_view = _save_synthetic_view(
                other_cache,
                "view-other.json",
                READER,
                [],
                started="2026-11-25T12:00:00+00:00",
                finished="2026-11-25T12:00:01+00:00",
            )
            with self.assertRaises(private.EncounterError) as conflict:
                client.show(config, v2, event_id="event:1", since=v1)
            self.assertEqual(conflict.exception.code, "configuration_invalid")

            with self.assertRaises(private.EncounterError) as reversed_err:
                client.show(config, v1, since=v2)
            self.assertEqual(reversed_err.exception.code, "reversed_observations")

            with self.assertRaises(private.EncounterError) as foreign_err:
                client.show(config, v2, since=other_view)
            self.assertEqual(foreign_err.exception.code, "invalid_private_view")

    def test_show_since_cli_prints_digest(self) -> None:
        with patch.object(
            sys,
            "argv",
            ["local_encounter.py", "show", "view-later.json", "--since", "view-prior.json", "--config", "client.json"],
        ), patch.object(client, "show", return_value="digest output") as showing, patch("builtins.print") as printed:
            self.assertEqual(operator.main(), 0)
        showing.assert_called_once_with(
            Path("client.json"), Path("view-later.json"), event_id=None, since=Path("view-prior.json")
        )
        printed.assert_called_once_with("digest output")


class DurationAndRetentionKnobTests(unittest.TestCase):
    def test_serve_and_retention_defaults_and_bounds(self) -> None:
        self.assertEqual(operator.DEFAULT_SERVE_SECONDS, 7200)
        self.assertEqual(operator.MAX_SERVE_SECONDS, 604800)
        self.assertEqual(operator.DEFAULT_RETENTION_SECONDS, 86400)
        self.assertEqual(operator.MAX_RETENTION_SECONDS, 31536000)

        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary).resolve() / "room"
            for bad_retention in (0, -1, 31536001):
                with self.subTest(bad_retention=bad_retention), self.assertRaises(private.EncounterError) as caught:
                    operator.initialize(state, WORLD, [WRITER], [READER], _free_port(), retention_seconds=bad_retention)
                self.assertEqual(caught.exception.code, "configuration_invalid")

            operator.initialize(state, WORLD, [WRITER], [READER], _free_port(), retention_seconds=2592000)
            for bad_seconds in (0, -5, 604801):
                with self.subTest(bad_seconds=bad_seconds), self.assertRaises(private.EncounterError) as caught:
                    operator.serve(state, host="python", seconds=bad_seconds)
                self.assertEqual(caught.exception.code, "configuration_invalid")

    def test_cli_help_separates_three_duration_and_budget_knobs(self) -> None:
        for subcommand, expected_tokens in (
            ("init", ("--retention-seconds", "86400", "31536000", "--seed", "reading-table", "serve --seconds", "trial budgets")),
            ("serve", ("--seconds", "7200", "604800", "init --retention-seconds", "trial budgets", "chain bounded sessions")),
            ("show", ("--since", "read-only digest")),
        ):
            with self.subTest(subcommand=subcommand):
                buf = io.StringIO()
                with patch.object(sys, "argv", ["local_encounter.py", subcommand, "--help"]), redirect_stdout(buf):
                    with self.assertRaises(SystemExit) as exited:
                        operator.main()
                self.assertEqual(exited.exception.code, 0)
                help_text = " ".join(buf.getvalue().split())
                for token in expected_tokens:
                    self.assertIn(token, help_text)

    def test_docs_record_interventions_and_separate_knobs(self) -> None:
        guide = (operator.ROOT / "docs" / "LOCAL_ENCOUNTER.md").read_text(encoding="utf-8")
        interventions = (operator.ROOT / "docs" / "INTERVENTIONS.md").read_text(encoding="utf-8")
        for token in (
            "Three separate duration and budget knobs",
            "7200",
            "604800",
            "init --retention-seconds",
            "Per-visit trial budget",
            "--seed reading-table",
            "show --since",
            "maintainer-run:reading-table-seed",
            "never a spontaneously formed community",
        ):
            self.assertIn(token, guide)
        for token in (
            "## 2026-11-25: Local room read-only digest, opt-in reading-table seed, and two-hour serve window",
            "issue 170",
            "issue 168",
            "issue 178",
            "--seed reading-table",
            "7200",
        ):
            self.assertIn(token, interventions)
        # Guard against em/en dashes in newly documented sections.
        self.assertNotIn("\u2014", guide)
        self.assertNotIn("\u2013", guide)


class DualHostSeedAndDigestTests(unittest.TestCase):
    host_binary: Path | None
    reader_binary: Path | None

    @classmethod
    def setUpClass(cls) -> None:
        cls.host_binary = private.find_tool("agentciv-host")
        cls.reader_binary = private.find_tool("agentciv-reader")
        if cls.reader_binary is None:
            raise AssertionError("agentciv-reader binary must be built for dual-host tests")

    def _hosts(self) -> list[tuple[str, Path | None]]:
        hosts: list[tuple[str, Path | None]] = [("python", None)]
        if self.host_binary is not None:
            hosts.insert(0, ("rust", self.host_binary))
        return hosts

    def test_default_init_remains_empty_control_and_invalid_seed_options_fail(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            with self.assertRaises(private.EncounterError) as bad_seed:
                operator.initialize(root / "bad-seed", WORLD, [WRITER], [READER], _free_port(), seed="unknown")
            self.assertEqual(bad_seed.exception.code, "configuration_invalid")

            with self.assertRaises(private.EncounterError) as conflict_principal:
                operator.initialize(
                    root / "bad-principal",
                    WORLD,
                    [operator.SEED_PRINCIPAL],
                    [READER],
                    _free_port(),
                    seed="reading-table",
                )
            self.assertEqual(conflict_principal.exception.code, "configuration_invalid")

            state = root / "empty-control"
            port = _free_port()
            initialized = operator.initialize(state, WORLD, [WRITER], [READER], port)
            self.assertNotIn("seed", initialized)
            clients = initialized["clients"]
            assert isinstance(clients, list)
            writer_cfg = Path(str(civic.object_value(clients[0])["config"]))
            origin = str(initialized["origin"])

            argv = operator._host_argv(state / "host.json", "python", None)
            owned = operator._start(argv, origin)
            try:
                operator._wait_ready(owned)
                read_result = client.read(writer_cfg, reader_binary=self.reader_binary)
                self.assertEqual(read_result["events"], 0)
            finally:
                operator._stop_owned(owned)

    def test_seed_reading_table_and_show_since_across_both_hosts_and_restart(self) -> None:
        assert self.reader_binary is not None
        for host_name, binary in self._hosts():
            with self.subTest(host=host_name), tempfile.TemporaryDirectory() as temporary:
                state = Path(temporary).resolve() / f"seeded-{host_name}"
                port = _free_port()
                initialized = operator.initialize(
                    state,
                    WORLD,
                    [WRITER],
                    [READER],
                    port,
                    seed="reading-table",
                    host=host_name,
                    host_binary=binary,
                )
                self.assertEqual(initialized["outcome"], "initialized")
                self.assertEqual(initialized["seed"], "reading-table")
                self.assertEqual(initialized["seed_principal"], operator.SEED_PRINCIPAL)
                self.assertEqual(initialized["seeded_events"], 7)
                self.assertIn("not a spontaneously formed community", str(initialized["seed_disclosure"]))

                # Confirm no resident seed credential remains in host.json.
                host_cfg = json.loads((state / "host.json").read_text(encoding="utf-8"))
                principals = [item["principal"] for item in host_cfg["credentials"]]
                self.assertEqual(principals, [WRITER, READER])
                self.assertNotIn(operator.SEED_PRINCIPAL, principals)

                clients = initialized["clients"]
                assert isinstance(clients, list)
                writer_cfg = Path(str(civic.object_value(clients[0])["config"]))
                reader_cfg = Path(str(civic.object_value(clients[1])["config"]))
                origin = str(initialized["origin"])

                argv = operator._host_argv(state / "host.json", host_name, binary)
                # Session 1: read seeded room, inspect offers, and publish one new message.
                owned = operator._start(argv, origin)
                try:
                    operator._wait_ready(owned)
                    writer_view_1 = client.read(writer_cfg, reader_binary=self.reader_binary)
                    reader_view_1 = client.read(reader_cfg, reader_binary=self.reader_binary)
                    self.assertEqual(writer_view_1["events"], 7)
                    self.assertEqual(reader_view_1["events"], 7)

                    view_1_path = Path(str(writer_view_1["view_path"]))
                    raw_view_1 = json.loads(view_1_path.read_text(encoding="utf-8"))
                    originals = raw_view_1["snapshot"]["records"]
                    self.assertEqual(len(originals), 7)
                    for span in originals:
                        ev = json.loads(span)
                        self.assertEqual(ev["actor"], operator.SEED_PRINCIPAL)
                        inner_body = next(iter(ev["body"].values()))["body"]
                        self.assertEqual(inner_body["seed_disclosure"]["seeded"], True)
                        self.assertEqual(inner_body["seed_disclosure"]["operator_attribution"], "maintainer-run")

                    offers_page = client.offers(writer_cfg, view_1_path, reader_binary=self.reader_binary)
                    offers_list = offers_page["offers"]
                    assert isinstance(offers_list, list)
                    offer_titles = {
                        str(civic.object_value(civic.object_value(item)["offer"])["title"])
                        for item in offers_list
                    }
                    self.assertEqual(offer_titles, {"Passage notes", "Open questions"})

                    say_result = client.say(
                        writer_cfg,
                        "A newcomer note after inspecting the seeded reading table.",
                        basis=view_1_path,
                        reader_binary=self.reader_binary,
                    )
                    self.assertEqual(say_result["outcome"], "recorded")
                finally:
                    operator._stop_owned(owned)

                # Session 2 (chained bounded session after restart): verify state retained and show --since digest.
                owned2 = operator._start(argv, origin)
                try:
                    operator._wait_ready(owned2)
                    reader_view_2 = client.read(reader_cfg, reader_binary=self.reader_binary)
                    self.assertEqual(reader_view_2["events"], 8)
                finally:
                    operator._stop_owned(owned2)

                reader_path_1 = Path(str(reader_view_1["view_path"]))
                reader_path_2 = Path(str(reader_view_2["view_path"]))
                digest = client.show(reader_cfg, reader_path_2, since=reader_path_1)
                self.assertIn("Digest since seq 7: 1 new messages, 0 revisions, 0 objections, 0 declines, 0 withdrawals, 7 unchanged.", digest)
                self.assertIn("A newcomer note after inspecting the seeded reading table.", digest)
                self.assertIn("original: show --event-id", digest)

                # Comparing view 2 against itself shows 0 new records since seq 8.
                no_change = client.show(reader_cfg, reader_path_2, since=reader_path_2)
                self.assertIn("0 new records since seq 8 (8 unchanged).", no_change)


if __name__ == "__main__":
    unittest.main()
