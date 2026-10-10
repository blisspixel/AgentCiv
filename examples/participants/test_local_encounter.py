"""Private persistent local rooms, authored acts, and honest bounded failures."""

from __future__ import annotations

import copy
import hashlib
import json
import os
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from decimal import Decimal
from types import SimpleNamespace
from contextlib import nullcontext
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples" / "http-commons"))
import collaboration as civic  # noqa: E402
import encounter_client as client  # noqa: E402
import encounter_private as private  # noqa: E402
import local_encounter as operator  # noqa: E402
import local_participant as wire  # noqa: E402
import walk  # noqa: E402

JsonObject = dict[str, object]
WORLD = "civ:manual-encounter-test"
A, B, C = "agent:encounter-a", "agent:encounter-b", "agent:encounter-c"


def obj(value: object) -> JsonObject:
    return civic.object_value(value)


def objects(value: object) -> list[JsonObject]:
    if not isinstance(value, list):
        raise AssertionError("expected object list")
    return [obj(item) for item in value]


def strings(value: object) -> list[str]:
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise AssertionError("expected exact original strings")
    return [str(item) for item in value]


def load(path: Path) -> JsonObject:
    return civic.decode(path.read_bytes())


def authored(kind: str, identifier: str, author: str, *, audience: list[str] | None = None) -> JsonObject:
    record: JsonObject = {"protocol_version": "0.1-draft", "type": kind, "id": identifier, "world": WORLD,
        "from": author, "to": [A, B, C] if audience is None else audience,
        "body": {"text": "An unfinished question: how should a return reader preserve older corrections?"}}
    if kind == "artifact_revision":
        record.update({"artifact_id": "artifact:shared-reader", "media_type": "application/json"})
    if kind in ("objection", "decline", "withdrawal"):
        record.update({"target_from": A, "artifact_id": "artifact:shared-reader", "revision": 1})
    if kind == "withdrawal":
        record.pop("to")
        record.pop("body")
    return record


def event(record: JsonObject, sequence: int) -> JsonObject:
    kind = {"message": "message.recorded", "artifact_revision": "artifact.recorded",
            "objection": "objection.recorded", "decline": "decline.recorded"}[str(record["type"])]
    stored = {**record, **({"revision": 1} if record["type"] == "artifact_revision" else {})}
    return {"protocol_version": "0.1-draft", "type": "event", "id": f"event:manual-{sequence}", "world": WORLD,
        "sequence": sequence, "timestamp": "2026-10-08T12:00:00Z", "kind": kind, "actor": record["from"],
        "body": {str(record["type"]): stored}}


def native(events: list[JsonObject], *, complete: bool = True) -> JsonObject:
    originals = [json.dumps(value, ensure_ascii=False) for value in events]
    return {"snapshot": {"world": WORLD, "records": originals}, "report": {"pages": max(1, (len(events) + 99) // 100),
        "events": len(events), "response_bytes": sum(len(value.encode("utf-8")) for value in originals) + 100,
        "reached_end": complete, "scope": "current_caller_view", "copying_permission": "not_granted"}}


def free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def settings(root: Path, principal: str = A) -> tuple[Path, JsonObject, Path]:
    state = root / "private"
    private.create_directory(state)
    cache = state / "cache"
    private.create_directory(cache)
    value: JsonObject = {"format": client.CONFIG_FORMAT, "origin": "http://127.0.0.1:8787", "world": WORLD,
        "principal": principal, "token": "synthetic-fixture-credential-0123456789", "payload_bytes": 16384,
        "cache_directory": str(cache), "private_cache_permission": client.CACHE_PERMISSION}
    path = state / "client.json"
    private.write_new(path, json.dumps(value).encode())
    return path, value, cache


def projection(value: JsonObject) -> JsonObject:
    return {"format": "agentciv-offer-view/0.1-example", "world": WORLD, "offers": [],
        "report": {**obj(value["report"]), "truncated": False}}


class PrivateEncounterFilesTests(unittest.TestCase):
    def test_private_bytes_bounds_replacement_and_no_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary).resolve() / "private"
            private.create_directory(directory)
            path = directory / "record.json"
            private.write_new(path, b"original")
            self.assertEqual(private.read_bounded(path, 100), b"original")
            with self.assertRaises(private.EncounterError) as caught:
                private.write_new(path, b"replacement")
            self.assertEqual(caught.exception.code, "private_path_exists")
            self.assertEqual(path.read_bytes(), b"original")
            private.replace_private(path, b"new")
            self.assertEqual(private.read_bounded(path, 100), b"new")
            with self.assertRaises(private.EncounterError) as caught:
                private.read_bounded(path, 2)
            self.assertEqual(caught.exception.code, "private_file_too_large")
            for maximum in (0, -1, private.MAX_PRIVATE_BYTES + 1):
                with self.subTest(maximum=maximum), self.assertRaises(private.EncounterError):
                    private.read_bounded(path, maximum)
            self.assertEqual(set(item.name for item in directory.iterdir()), {"record.json"})

    def test_relative_checkout_reparse_and_foreign_owner_are_rejected(self) -> None:
        for path in (Path("relative-private-directory"), private.ROOT / ".agents" / "forbidden-private-state"):
            with self.subTest(path=path), self.assertRaises(private.EncounterError):
                private.create_directory(path)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary).resolve() / "private"
            private.create_directory(directory)
            with patch.object(Path, "lstat", return_value=SimpleNamespace(st_mode=stat.S_IFDIR | 0o700,
                st_file_attributes=0x400)), self.assertRaises(private.EncounterError) as caught:
                private.check_directory(directory)
            self.assertEqual(caught.exception.code, "private_path_link")
            if os.name == "nt":
                with patch.object(private, "_windows_acl", side_effect=private.EncounterError("private_permissions_invalid")), \
                    self.assertRaises(private.EncounterError):
                    private.check_directory(directory)
            else:
                with patch.object(os, "getuid", return_value=int(getattr(os, "getuid")()) + 1), self.assertRaises(private.EncounterError):
                    private.check_directory(directory)

    def test_symbolic_and_hard_links_cannot_be_private_state(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            target = root / "private"
            private.create_directory(target)
            source = target / "record.json"
            private.write_new(source, b"original")
            duplicate = target / "hard-linked.json"
            try:
                duplicate.hardlink_to(source)
            except OSError:
                self.skipTest("hard links unavailable on this filesystem")
            with self.assertRaises(private.EncounterError):
                private.check_file(source)
            duplicate.unlink()
            link = root / "directory-link"
            try:
                link.symlink_to(target, target_is_directory=True)
            except OSError:
                return  # The reparse guard above is exercised even without link-creation rights.
            with self.assertRaises(private.EncounterError) as caught:
                private.check_directory(link)
            self.assertEqual(caught.exception.code, "private_path_link")
            self.assertEqual(source.read_bytes(), b"original")

    def test_windows_acl_verification_fails_closed_without_private_trace(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary).resolve()
            with patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 2)), \
                self.assertRaises(private.EncounterError) as caught:
                private._windows_acl(path, "check", True)
            self.assertEqual(caught.exception.code, "private_permissions_invalid")
            for failure in (OSError("private-trace"), subprocess.TimeoutExpired("private-command", 15)):
                with self.subTest(failure=type(failure).__name__), patch.object(subprocess, "run", side_effect=failure), \
                    self.assertRaises(private.EncounterError) as caught:
                    private._windows_acl(path, "set", False)
                self.assertEqual(caught.exception.code, "private_permissions_unavailable")
                self.assertNotIn("private-trace", str(caught.exception))

    def test_lock_is_bounded_and_released_after_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary).resolve() / "private"
            private.create_directory(directory)
            with private.lock(directory), self.assertRaises(private.EncounterError) as caught:
                with private.lock(directory, timeout=0):
                    self.fail("a second writer acquired the held lock")
            self.assertEqual(caught.exception.code, "private_lock_unavailable")
            with self.assertRaisesRegex(ValueError, "controlled"), private.lock(directory):
                raise ValueError("controlled")
            with private.lock(directory, timeout=0):
                self.assertTrue((directory / ".encounter.lock").is_file())


    def test_tool_lookup_ignores_the_current_directory_and_relative_path_entries(self) -> None:
        suffix = ".exe" if sys.platform == "win32" else ""
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            planted = root / "planted"
            trusted = root / "trusted"
            planted.mkdir()
            trusted.mkdir()
            for directory in (planted, trusted):
                tool = directory / f"agentciv-reader{suffix}"
                tool.write_bytes(b"")
                tool.chmod(0o700)
            previous = Path.cwd()
            os.chdir(planted)
            try:
                with patch.dict(os.environ, {"PATH": os.pathsep.join([".", "planted", ""])}):
                    self.assertIsNone(private.find_executable("agentciv-reader"))
                with patch.dict(os.environ, {"PATH": os.pathsep.join([".", str(trusted)])}):
                    self.assertEqual(private.find_executable("agentciv-reader"), trusted / f"agentciv-reader{suffix}")
                with patch.dict(os.environ, {"PATH": str(trusted)}):
                    self.assertIsNone(private.find_executable("agentciv-missing"))
            finally:
                os.chdir(previous)

class EncounterClientUnitTests(unittest.TestCase):
    def test_empty_read_publishes_nothing_and_saved_view_cannot_be_overwritten(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, value, cache = settings(root)
            current = native([])
            with patch.object(client, "_native_read", return_value=current), \
                patch.object(client, "_project", return_value=projection(current)), patch.object(wire, "exchange") as posted:
                result = client.read(config, reader_binary=Path(__file__))
                posted.assert_not_called()
                saved = Path(str(result["view_path"]))
                before = saved.read_bytes()
                with self.assertRaises(private.EncounterError):
                    client.read(config, saved, reader_binary=Path(__file__))
                self.assertEqual(saved.read_bytes(), before)
            self.assertEqual(result["outcome"], "read")
            self.assertEqual(result["events"], 0)
            view = load(saved)
            self.assertEqual(obj(view["snapshot"])["records"], [])
            self.assertEqual(view["copying_permission"], "not_granted")
            self.assertNotIn(str(value["token"]), json.dumps(result))
            self.assertNotIn(str(value["token"]), json.dumps(view))
            self.assertEqual(list(cache.glob("journal-*.json")), [])

    def test_incomplete_or_unbounded_reader_result_creates_no_current_view(self) -> None:
        for condition in ("partial", "wrong_count", "credential", "truncated", "process_limit"):
            with self.subTest(condition=condition), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                config, value, cache = settings(root)
                current = native([])
                projected = projection(current)
                if condition == "partial":
                    current["report"] = {**obj(current["report"]), "reached_end": False}
                elif condition == "wrong_count":
                    current["report"] = {**obj(current["report"]), "events": 1}
                elif condition == "credential":
                    current["untrusted"] = value["token"]
                elif condition == "truncated":
                    projected["report"] = {**obj(projected["report"]), "truncated": True}
                with patch.object(client, "_native_read", return_value=current,
                    side_effect=private.EncounterError("reader_failed") if condition == "process_limit" else None), \
                    patch.object(client, "_project", return_value=projected), patch.object(wire, "exchange") as posted:
                    with self.assertRaises(private.EncounterError):
                        client.read(config, reader_binary=Path(__file__))
                    posted.assert_not_called()
                self.assertEqual(list(cache.glob("view-*.json")), [])
                self.assertEqual(list(cache.glob("journal-*.json")), [])

    def test_lost_reply_and_invalid_receipt_preserve_exact_uncertain_request_without_retry(self) -> None:
        for condition in ("lost_reply", "malformed_receipt", "false_receipt", "server_error"):
            with self.subTest(condition=condition), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                config, _, cache = settings(root)
                record = authored("message", "message:uncertain", A)
                record["body"] = {"text": "A supplied account: café, Ελληνικά, 日本語.", "unknown": {"retain": True}}
                raw = (json.dumps(record, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
                source = root / "authored-public.json"
                source.write_bytes(raw)
                current = native([])
                response = (200, b"private-malformed-receipt") if condition == "malformed_receipt" else (200, b'{}')
                if condition == "server_error":
                    response = (500, b'{"code":"storage_failed"}')
                with patch.object(client, "_native_read", return_value=current), \
                    patch.object(client, "_project", return_value=projection(current)), \
                    patch.object(client, "_endpoint", return_value=("http://127.0.0.1:8787/submit", "messages.submit")), \
                    patch.object(wire, "exchange", return_value=response,
                        side_effect=wire.ParticipantError(0, "unreachable") if condition == "lost_reply" else None) as posted:
                    result = client.submit(config, source, reader_binary=Path(__file__))
                posted.assert_called_once()
                self.assertEqual(posted.call_args.kwargs["body"], raw)
                self.assertEqual(result["outcome"], "uncertain")
                journal = load(Path(str(result["journal_path"])))
                self.assertEqual(journal["state"], "uncertain")
                self.assertEqual(str(journal["request_utf8"]).encode("utf-8"), raw)
                self.assertEqual(journal["request_sha256"], hashlib.sha256(raw).hexdigest())
                self.assertNotIn("receipt", journal)
                self.assertNotIn("event_record_utf8", journal)
                self.assertNotIn("private-malformed-receipt", json.dumps(result))
                self.assertNotIn("private-malformed-receipt", json.dumps(journal))
                self.assertEqual(len(list(cache.glob("journal-*.json"))), 1)

    def test_boolean_numeric_readback_mutations_remain_uncertain(self) -> None:
        for submitted, returned in ((True, 1), (False, 0), (1, True), (0, False)):
            with self.subTest(submitted=submitted, returned=returned), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                config, _, _ = settings(root)
                record = authored("message", "message:typed-readback", A)
                record["body"] = {"text": "Preserve nested JSON types.", "nested": [{"value": submitted}]}
                record["unknown"] = {"nested": [submitted]}
                source = root / "authored-types.json"
                raw = json.dumps(record).encode()
                source.write_bytes(raw)
                changed = copy.deepcopy(record)
                changed["body"] = {"text": "Preserve nested JSON types.", "nested": [{"value": returned}]}
                changed["unknown"] = {"nested": [returned]}
                before, after = native([]), native([event(changed, 1)])
                receipt = {"protocol_version": "0.1-draft", "type": "receipt", "status": "recorded", "world": WORLD,
                    "record_id": record["id"], "event_id": "event:manual-1", "sequence": 1}
                with patch.object(client, "_native_read", side_effect=[before, after]), \
                    patch.object(client, "_project", side_effect=[projection(before), projection(after)]), \
                    patch.object(client, "_endpoint", return_value=("http://127.0.0.1:8787/submit", "messages.submit")), \
                    patch.object(wire, "exchange", return_value=(200, json.dumps(receipt).encode())) as posted:
                    result = client.submit(config, source, reader_binary=Path(__file__))
                posted.assert_called_once()
                self.assertEqual(result["outcome"], "uncertain")
                journal = load(Path(str(result["journal_path"])))
                self.assertEqual(journal["state"], "uncertain")
                self.assertEqual(journal["failure_code"], "publication_outcome_unverified")
                self.assertEqual(str(journal["request_utf8"]).encode(), raw)
                self.assertNotIn("event_record_utf8", journal)
        self.assertTrue(client._json_equal({"n": [1]}, {"n": [Decimal("1.0")]}))
        self.assertFalse(client._json_equal({"n": [True]}, {"n": [1]}))

    def test_untrustworthy_problem_responses_do_not_establish_rejection(self) -> None:
        valid: JsonObject = {"type": "https://agentciv.io/problems/forbidden", "title": "Forbidden", "status": 403, "code": "forbidden"}
        cases = ((400, {**valid, "status": 400, "code": "storage_failed"}),
                 (403, {**valid, "status": 400}),
                 (403, {"status": 403, "code": "forbidden"}),
                 (403, {**valid, "status": True}),
                 (403, {**valid, "detail": 1}),
                 (403, {**valid, "type": "problem"}),
                 (422, {**valid, "status": 422, "code": "unknown_target"}),
                 (403, valid))
        for status, response in cases:
            with self.subTest(response=response), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                config, _, _ = settings(root)
                source = root / "authored-problem.json"
                raw = json.dumps(authored("message", "message:problem-response", A)).encode()
                source.write_bytes(raw)
                current = native([])
                with patch.object(client, "_native_read", return_value=current), \
                    patch.object(client, "_project", return_value=projection(current)), \
                    patch.object(client, "_endpoint", return_value=("http://127.0.0.1:8787/submit", "messages.submit")), \
                    patch.object(wire, "exchange", return_value=(status, json.dumps(response).encode())) as posted:
                    result = client.submit(config, source, reader_binary=Path(__file__))
                posted.assert_called_once()
                expected = "rejected" if response == valid else "uncertain"
                self.assertEqual(result["outcome"], expected)
                journal = load(Path(str(result["journal_path"])))
                self.assertEqual(journal["state"], expected)
                self.assertEqual(str(journal["request_utf8"]).encode(), raw)
                self.assertNotIn("event_record_utf8", journal)

    def test_older_withdrawal_and_changed_access_block_stale_basis_before_publication(self) -> None:
        for condition in ("withdrawn", "access_changed", "partial"):
            with self.subTest(condition=condition), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                config, value, cache = settings(root, B)
                parent = event(authored("artifact_revision", "submission:parent", A), 1)
                initial = native([parent])
                with patch.object(client, "_native_read", return_value=initial), \
                    patch.object(client, "_project", return_value=projection(initial)):
                    saved = client.read(config, reader_binary=Path(__file__))
                basis = Path(str(saved["view_path"]))
                previous_bytes = basis.read_bytes()
                source = root / "authored-objection.json"
                source.write_text(json.dumps(authored("objection", "submission:stale-objection", B)), encoding="utf-8")
                changed = native([{**parent, "kind": "artifact.withdrawn", "body": {}}]) if condition == "withdrawn" else native([])
                if condition == "partial":
                    changed["report"] = {**obj(changed["report"]), "reached_end": False}
                with patch.object(client, "_native_read", return_value=changed), \
                    patch.object(client, "_project", return_value=projection(changed)), patch.object(wire, "exchange") as posted:
                    with self.assertRaises(private.EncounterError):
                        client.submit(config, source, basis, reader_binary=Path(__file__))
                    posted.assert_not_called()
                self.assertEqual(basis.read_bytes(), previous_bytes)
                self.assertEqual(list(cache.glob("journal-*.json")), [])
                self.assertNotIn(str(value["token"]), basis.read_text(encoding="utf-8"))

    def test_authored_and_configuration_limits_fail_before_network(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, _, cache = settings(root)
            source = root / "oversized-authored.json"
            source.write_bytes(b" " * 16385)
            with patch.object(wire, "exchange") as posted, self.assertRaises(private.EncounterError):
                client.submit(config, source, reader_binary=Path(__file__))
            posted.assert_not_called()
            self.assertEqual(list(cache.glob("journal-*.json")), [])
            oversized = config.parent / "oversized-config.json"
            private.write_new(oversized, b" " * (client.MAX_CONFIG + 1))
            with patch.object(wire, "exchange") as posted, self.assertRaises(private.EncounterError):
                client.read(oversized, reader_binary=Path(__file__))
            posted.assert_not_called()

    def test_private_cache_capacity_and_native_failure_never_create_a_current_view(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, _, cache = settings(root)
            current = native([])
            with patch.object(client, "_native_read", return_value=current), \
                patch.object(client, "_project", return_value=projection(current)), patch.object(client, "MAX_FILES", 1):
                first = client.read(config, reader_binary=Path(__file__))
                original = Path(str(first["view_path"])).read_bytes()
                with self.assertRaises(private.EncounterError) as caught:
                    client.read(config, reader_binary=Path(__file__))
                self.assertEqual(caught.exception.code, "private_cache_full")
                self.assertEqual(Path(str(first["view_path"])).read_bytes(), original)
            self.assertEqual(len(list(cache.glob("view-*.json"))), 1)
        variants = [subprocess.CompletedProcess([], 1, b"private-stdout", b"private-stderr"),
            subprocess.CompletedProcess([], 0, b"private-not-json", b""),
            subprocess.CompletedProcess([], 0, b"x" * (client.MAX_OUTPUT + 1), b"")]
        for completed in variants:
            with self.subTest(returncode=completed.returncode, bytes=len(completed.stdout)), \
                patch.object(subprocess, "run", return_value=completed), self.assertRaises(private.EncounterError) as caught:
                client._process(Path("reader"), "read", Path("private-config"))
            self.assertNotIn("private-stderr", str(caught.exception))
            self.assertNotIn("private-stdout", str(caught.exception))

    def test_deep_json_is_a_fixed_failure_before_network_or_journaling(self) -> None:
        deep = b'{"nested":' + b"[" * 2000 + b"0" + b"]" * 2000 + b"}"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, _, cache = settings(root)
            source = root / "deep-authored.json"
            source.write_bytes(deep)
            with patch.object(wire, "exchange") as posted, self.assertRaises(private.EncounterError) as caught:
                client.submit(config, source, reader_binary=Path(__file__))
            posted.assert_not_called()
            self.assertRegex(caught.exception.code, r"^[a-z_]+$")
            self.assertEqual(list(cache.glob("journal-*.json")), [])
            private.write_new(config.parent / "host.json", deep)
            with self.assertRaises(private.EncounterError) as caught:
                operator._configuration(config.parent)
            self.assertEqual(caught.exception.code, "configuration_invalid")
            with patch("local_encounter.json.loads", side_effect=RecursionError), \
                self.assertRaises(private.EncounterError) as caught:
                operator._json(deep)
            self.assertEqual(caught.exception.code, "configuration_invalid")


class EncounterOperatorTests(unittest.TestCase):
    def test_initialization_issues_separate_private_grants_without_printing_credentials(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary).resolve() / "room"
            arguments = ["local_encounter.py", "init", "--state", str(state), "--world", WORLD,
                "--writer", A, "--reader", C, "--port", str(free_port())]
            with patch.object(sys, "argv", arguments), patch("builtins.print") as printed:
                self.assertEqual(operator.main(), 0)
            result = json.loads(printed.call_args.args[0])
            self.assertEqual(result["outcome"], "initialized")
            host = load(state / "host.json")
            credentials = objects(host["credentials"])
            self.assertEqual(len({str(item["token"]) for item in credentials}), 2)
            for credential in credentials:
                self.assertNotIn(str(credential["token"]), str(printed.call_args))
            self.assertIs(next(item["write"] for item in credentials if item["principal"] == C), False)
            self.assertIs(next(item["write"] for item in credentials if item["principal"] == A), True)
            before = (state / "host.json").read_bytes()
            with self.assertRaises(private.EncounterError) as caught:
                operator.initialize(state, WORLD, [A], [C])
            self.assertEqual(caught.exception.code, "private_path_exists")
            self.assertEqual((state / "host.json").read_bytes(), before)

    def test_invalid_grants_and_failed_private_permissions_create_no_credentials(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            for writers, readers, port in (([], [C], 8787), ([A], [A], 8787), ([A], [C], 0),
                                           ([f"agent:{number}" for number in range(17)], [], 8787)):
                state = root / "invalid"
                with self.subTest(writers=len(writers), port=port), self.assertRaises(private.EncounterError):
                    operator.initialize(state, WORLD, writers, readers, port)
                self.assertFalse(state.exists())
            state = root / "permission-failed"
            with patch.object(private, "_permissions", side_effect=private.EncounterError("private_permissions_invalid")), \
                self.assertRaises(private.EncounterError):
                operator.initialize(state, WORLD, [A], [])
            self.assertEqual(list(state.iterdir()), [])

    def test_serve_interrupt_and_startup_failure_stop_only_owned_child_and_retain_state(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary).resolve() / "room"
            operator.initialize(state, WORLD, [A], [], free_port())
            database = state / "world.sqlite"
            private.replace_private(database, b"owned-state-preserve")
            owned = operator.OwnedHost(MagicMock(), threading.Event(), MagicMock())
            for failure in (KeyboardInterrupt(), private.EncounterError("host_start_failed")):
                with self.subTest(failure=type(failure).__name__), \
                    patch.object(operator, "_start", return_value=owned), \
                    patch.object(operator, "_wait_ready", side_effect=failure), \
                    patch.object(operator, "_stop_owned") as stopped, patch("builtins.print"):
                    if isinstance(failure, KeyboardInterrupt):
                        result = operator.serve(state, host="python", seconds=1)
                        self.assertEqual(result["reason"], "interrupted")
                        self.assertTrue(result["state_retained"])
                    else:
                        with self.assertRaises(private.EncounterError) as caught:
                            operator.serve(state, host="python", seconds=1)
                        self.assertEqual(caught.exception.code, "host_start_failed")
                    stopped.assert_called_once_with(owned)
                self.assertEqual(database.read_bytes(), b"owned-state-preserve")

    def test_owned_child_cleanup_escalates_and_never_targets_another_process(self) -> None:
        child = MagicMock()
        child.poll.return_value = None
        child.wait.side_effect = [subprocess.TimeoutExpired("owned-host", 3), 0]
        drain = MagicMock()
        operator._stop_owned(operator.OwnedHost(child, threading.Event(), drain))
        child.terminate.assert_called_once_with()
        child.kill.assert_called_once_with()
        self.assertEqual(child.wait.call_count, 2)
        drain.join.assert_called_once_with(timeout=1)
        child = MagicMock()
        child.poll.return_value = 1
        operator._stop_owned(operator.OwnedHost(child, threading.Event(), MagicMock()))
        child.terminate.assert_not_called()
        child.kill.assert_not_called()

    def test_session_and_soft_storage_limits_stop_owned_child_and_retain_private_state(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary).resolve() / "room"
            operator.initialize(state, WORLD, [A], [], free_port())
            database = state / "world.sqlite"
            private.replace_private(database, b"retained-private-state")
            for reason in ("session_limit", "storage_soft_cutoff"):
                child = MagicMock()
                child.poll.return_value = None
                owned = operator.OwnedHost(child, threading.Event(), MagicMock())
                times = [0.0, 0.0, 0.5, 1.1] if reason == "session_limit" else [0.0, 0.0]
                sizes = [0, 0] if reason == "session_limit" else [0, 1048576]
                with self.subTest(reason=reason), patch.object(private, "lock", return_value=nullcontext()), \
                    patch.object(operator, "_start", return_value=owned), patch.object(operator, "_wait_ready"), \
                    patch.object(operator, "_stop_owned") as stopped, patch.object(operator, "_storage_bytes", side_effect=sizes), \
                    patch("local_encounter.time.monotonic", side_effect=times), patch("local_encounter.time.sleep"), \
                    patch("builtins.print"):
                    result = operator.serve(state, host="python", seconds=1, max_storage_bytes=1048576)
                self.assertEqual(result["reason"], reason)
                self.assertTrue(result["state_retained"])
                stopped.assert_called_once_with(owned)
                private.check_file(database)
                self.assertEqual(database.read_bytes(), b"retained-private-state")
            with patch.object(operator, "_storage_bytes", return_value=1048576), patch.object(operator, "_start") as started, \
                self.assertRaises(private.EncounterError) as caught:
                operator.serve(state, host="python", seconds=1, max_storage_bytes=1048576)
            self.assertEqual(caught.exception.code, "storage_limit")
            started.assert_not_called()

    def test_missing_or_foreign_owned_database_is_refused_before_startup(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary).resolve() / "room"
            operator.initialize(state, WORLD, [A], [], free_port())
            database = state / "world.sqlite"
            database.unlink()
            with patch.object(operator, "_start") as started, self.assertRaises(private.EncounterError):
                operator.serve(state, host="python", seconds=1)
            started.assert_not_called()
            self.assertFalse(database.exists())
            private.write_new(database, b"")
            original_check = private.check_file

            def refuse_foreign_owner(path: Path) -> None:
                if path == database:
                    raise private.EncounterError("private_permissions_invalid")
                original_check(path)

            with patch.object(private, "check_file", side_effect=refuse_foreign_owner), \
                patch.object(operator, "_start") as started, self.assertRaises(private.EncounterError) as caught:
                operator.serve(state, host="python", seconds=1)
            started.assert_not_called()
            self.assertEqual(caught.exception.code, "private_permissions_invalid")
            self.assertEqual(database.read_bytes(), b"")

    def test_existing_recovery_sidecars_are_preserved_and_invalid_ones_block_startup(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary).resolve() / "room"
            operator.initialize(state, WORLD, [A], [], free_port())
            retained = {state / "world.sqlite-wal": b"retained-wal-recovery-bytes",
                        state / "world.sqlite-shm": b"retained-shared-memory-bytes"}
            for path, raw in retained.items():
                private.check_file(path)
                self.assertEqual(path.read_bytes(), b"")
                private.replace_private(path, raw)
            with patch.object(operator, "_start", side_effect=private.EncounterError("host_start_failed")) as started, \
                self.assertRaises(private.EncounterError) as caught:
                operator.serve(state, host="python", seconds=1)
            started.assert_called_once()
            self.assertEqual(caught.exception.code, "host_start_failed")
            for path, raw in retained.items():
                private.check_file(path)
                self.assertEqual(path.read_bytes(), raw)
            original_check = private.check_file

            def refuse_foreign_sidecar(path: Path) -> None:
                if path == state / "world.sqlite-wal":
                    raise private.EncounterError("private_permissions_invalid")
                original_check(path)

            with patch.object(private, "check_file", side_effect=refuse_foreign_sidecar), \
                patch.object(operator, "_start") as started, self.assertRaises(private.EncounterError) as caught:
                operator.serve(state, host="python", seconds=1)
            started.assert_not_called()
            self.assertEqual(caught.exception.code, "private_permissions_invalid")
            for path, raw in retained.items():
                self.assertEqual(path.read_bytes(), raw)

    def test_cli_denial_and_uncertainty_are_failures_without_credentials_or_trace(self) -> None:
        for outcome in ("rejected", "uncertain"):
            with self.subTest(outcome=outcome), patch.object(sys, "argv", ["local_encounter.py", "submit",
                "--config", "private-config", "--record", "authored.json"]), \
                patch.object(client, "submit", return_value={"outcome": outcome}), patch("builtins.print") as printed:
                self.assertEqual(operator.main(), 1)
                self.assertEqual(json.loads(printed.call_args.args[0])["outcome"], outcome)
        with patch.object(sys, "argv", ["local_encounter.py", "read", "--config", "private-config"]), \
            patch.object(client, "read", side_effect=private.EncounterError("reader_failed")), patch("builtins.print") as printed:
            self.assertEqual(operator.main(), 1)
            self.assertEqual(json.loads(printed.call_args.args[0]), {"outcome": "failed", "code": "reader_failed"})


class LocalEncounterProcessTests(unittest.TestCase):
    reader_binary: Path
    host_binary: Path

    @classmethod
    def setUpClass(cls) -> None:
        binaries = walk.build_rust_binaries("agentciv-reader", "agentciv-host")
        cls.reader_binary = binaries["agentciv-reader"]
        cls.host_binary = binaries["agentciv-host"]

    def test_authored_shared_history_survives_restart_without_hidden_material_on_both_hosts(self) -> None:
        for host in ("python", "rust"):
            with self.subTest(host=host), tempfile.TemporaryDirectory(prefix="agentciv-manual-room-") as temporary:
                root = Path(temporary).resolve()
                state = root / "room"
                initialized = operator.initialize(state, WORLD, [A, B], [C], free_port())
                database = state / "world.sqlite"
                private.check_file(database)
                self.assertEqual(database.read_bytes(), b"")
                for sidecar in (state / "world.sqlite-wal", state / "world.sqlite-shm"):
                    private.check_file(sidecar)
                    self.assertEqual(sidecar.read_bytes(), b"")
                configs = {str(item["principal"]): Path(str(item["config"])) for item in objects(initialized["clients"])}
                origin = str(initialized["origin"])
                argv = walk.python_argv(state / "host.json") if host == "python" else [str(self.host_binary), "--config", str(state / "host.json")]
                owned = operator._start(argv, origin)
                sent: list[tuple[JsonObject, bytes, JsonObject]] = []

                def send(record: JsonObject, *, raw: bytes | None = None, basis: Path | None = None) -> JsonObject:
                    exact = raw if raw is not None else (json.dumps(record, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
                    path = root / f"authored-{len(sent)}.json"
                    path.write_bytes(exact)
                    result = client.submit(configs[str(record["from"])], path, basis, reader_binary=self.reader_binary)
                    sent.append((record, exact, result))
                    return result

                try:
                    operator._wait_ready(owned)
                    empty = client.read(configs[C], reader_binary=self.reader_binary)
                    self.assertEqual(empty["events"], 0)
                    empty_view = load(Path(str(empty["view_path"])))
                    self.assertEqual(obj(empty_view["snapshot"])["records"], [])
                    message = authored("message", "message:authored-unicode", A)
                    message["body"] = {"text": "An authored account: café, Ελληνικά, 日本語.\nStill unfinished.",
                        "extension": {"keep": ["original", "unknown-field"], "number": 7}}
                    raw_message = (" \n" + json.dumps(message, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
                    first = send(message, raw=raw_message)
                    self.assertEqual(first["outcome"], "recorded")
                    journal = load(Path(str(first["journal_path"])))
                    self.assertEqual(str(journal["request_utf8"]).encode("utf-8"), raw_message)
                    self.assertEqual(journal["request_sha256"], hashlib.sha256(raw_message).hexdigest())
                    self.assertEqual(obj(obj(civic.decode(str(journal["event_record_utf8"]).encode())["body"])["message"]), message)
                    settings_a = load(configs[A])
                    endpoint = obj(wire.discover(origin)["endpoints"])["submit"]
                    status, raw = wire.exchange("POST", str(endpoint), token=str(settings_a["token"]), body=raw_message)
                    self.assertEqual(status, 200)
                    self.assertEqual(civic.decode(raw)["event_id"], first["event_id"])
                    status, _ = wire.exchange("POST", str(endpoint), token=str(settings_a["token"]), body=raw_message + b" ")
                    self.assertEqual(status, 409)

                    parent = send(authored("artifact_revision", "submission:parent", A))
                    continuation = authored("artifact_revision", "submission:continuation", B)
                    continuation["artifact_id"] = "artifact:independent-continuation"
                    continuation["derived_from"] = {"from": A, "artifact_id": "artifact:shared-reader", "revision": 1}
                    self.assertEqual(send(continuation)["outcome"], "recorded")
                    objection = send(authored("objection", "submission:open-objection", B))
                    decline = send(authored("decline", "submission:decline", B))
                    self.assertEqual(objection["outcome"], "recorded")
                    self.assertEqual(decline["outcome"], "recorded")
                    hidden = authored("message", "message:hidden-identifier-sentinel", B, audience=[B])
                    hidden["body"] = {"text": "Restricted-room-content-sentinel"}
                    self.assertEqual(send(hidden)["outcome"], "recorded")
                    before_read = client.read(configs[C], reader_binary=self.reader_binary)
                    before = load(Path(str(before_read["view_path"])))
                    before_rows = strings(obj(before["snapshot"])["records"])
                    self.assertEqual(before_read["events"], 5)
                    denied = send(authored("message", "message:reader-denied", C))
                    self.assertEqual(denied["outcome"], "rejected")
                    denied_journal = load(Path(str(denied["journal_path"])))
                    self.assertEqual((denied_journal["state"], denied_journal["failure_code"]), ("rejected", "forbidden"))
                    self.assertNotIn("event_record_utf8", denied_journal)
                    basis_b = client.read(configs[B], reader_binary=self.reader_binary)
                    withdrawn = send(authored("withdrawal", "submission:withdraw-parent", A))
                    self.assertEqual(withdrawn["outcome"], "recorded")
                    self.assertEqual(withdrawn["event_id"], parent["event_id"])
                    before_journals = set(Path(str(load(configs[B])["cache_directory"])).glob("journal-*.json"))
                    with self.assertRaises(private.EncounterError) as caught:
                        send(authored("objection", "submission:stale-objection", B), basis=Path(str(basis_b["view_path"])))
                    self.assertEqual(caught.exception.code, "stale_private_basis")
                    self.assertEqual(set(Path(str(load(configs[B])["cache_directory"])).glob("journal-*.json")), before_journals)
                finally:
                    operator._stop_owned(owned)

                self.assertTrue((state / "world.sqlite").is_file())
                private.check_directory(state)
                private.check_file(state / "world.sqlite")
                for sidecar in (state / "world.sqlite-wal", state / "world.sqlite-shm"):
                    if sidecar.exists():
                        private.check_file(sidecar)
                operator._storage_bytes(state, verify=True)
                operator._provision_sidecars(state)
                restarted = operator._start(argv, origin)
                try:
                    operator._wait_ready(restarted)
                    returned = client.read(configs[C], reader_binary=self.reader_binary)
                    view = load(Path(str(returned["view_path"])))
                finally:
                    operator._stop_owned(restarted)
                private.check_file(database)
                for sidecar in (state / "world.sqlite-wal", state / "world.sqlite-shm"):
                    if sidecar.exists():
                        private.check_file(sidecar)
                snapshot = obj(view["snapshot"])
                returned_rows = strings(snapshot["records"])
                events = [civic.decode(row.encode()) for row in returned_rows]
                self.assertEqual(returned["events"], 5)
                self.assertNotIn("message:hidden-identifier-sentinel", json.dumps(view))
                self.assertNotIn("Restricted-room-content-sentinel", json.dumps(view))
                old_parent = next(civic.decode(row.encode()) for row in before_rows if civic.decode(row.encode())["id"] == parent["event_id"])
                tombstone = next(value for value in events if value["id"] == parent["event_id"])
                self.assertEqual((tombstone["sequence"], tombstone["timestamp"]), (old_parent["sequence"], old_parent["timestamp"]))
                self.assertEqual((tombstone["kind"], tombstone["body"]), ("artifact.withdrawn", {}))
                for result in (first, objection, decline):
                    retained_journal = load(Path(str(result["journal_path"])))
                    original = str(retained_journal["event_record_utf8"])
                    self.assertIn(original, returned_rows)
                derived = next(obj(obj(value["body"])["artifact_revision"]) for value in events
                    if value["kind"] == "artifact.recorded" and value["actor"] == B)
                self.assertEqual(derived["derived_from"], {"from": A, "artifact_id": "artifact:shared-reader", "revision": 1})
                self.assertFalse(any(value["actor"] == C for value in events))
                self.assertEqual(view["copying_permission"], "not_granted")

    def test_port_collision_is_a_fixed_failure_and_preserves_the_initialized_room(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agentciv-port-collision-") as temporary, socket.socket() as occupied:
            occupied.bind(("127.0.0.1", 0))
            occupied.listen(1)
            state = Path(temporary).resolve() / "room"
            operator.initialize(state, WORLD, [A], [], int(occupied.getsockname()[1]))
            config_before = (state / "host.json").read_bytes()
            with self.assertRaises(private.EncounterError) as caught:
                operator.serve(state, host="rust", host_binary=self.host_binary, seconds=1)
            self.assertEqual(caught.exception.code, "host_start_failed")
            self.assertEqual((state / "host.json").read_bytes(), config_before)
            self.assertTrue((state / "client-01.json").is_file())


if __name__ == "__main__":
    unittest.main()
