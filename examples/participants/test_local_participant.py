"""Local checks for the scripted participant and the dormant provider shapes.

The participant talks only to the Python host in this repository. Provider
builders are not allowed to open a socket. A stand-in opener counts calls.
"""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import threading
import types
import unittest
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import local_participant  # noqa: E402
import providers  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
WRITER = "agent:writer"
READER = "agent:reader"
WRITER_TOKEN = "writer-token-value"
READER_TOKEN = "reader-token-value"


def load_host() -> types.ModuleType:
    path = ROOT / "implementations" / "http-commons-python" / "host.py"
    spec = importlib.util.spec_from_file_location("agentciv_python_host_for_participants", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("python host could not be loaded")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    try:
        spec.loader.exec_module(module)
    except Exception:
        sys.modules.pop(spec.name, None)
        raise
    return module


HOST = load_host()


def header(request: providers.PreparedRequest, name: str) -> str | None:
    for key, value in request.headers:
        if key == name:
            return value
    return None


class ParticipantTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        directory = Path(self.temporary.name)
        self.server = HOST.start_server(
            HOST.HostConfig(
                world_id="civ:participants",
                title="Participants",
                database_path=directory / "world.sqlite",
                listen=("127.0.0.1", 0),
                visibility="members",
                retention_seconds=60,
                max_payload_bytes=4096,
                credentials=(
                    HOST.Credential(WRITER, WRITER_TOKEN, True, True),
                    HOST.Credential(READER, READER_TOKEN, True, False),
                ),
            )
        )

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.temporary.cleanup()

    def test_scripted_draft_is_readable_by_another_principal(self) -> None:
        origin = self.server.origin
        recorded = local_participant.record_message(
            origin,
            WRITER_TOKEN,
            WRITER,
            [READER],
            text="café question",
            message_id="message:scripted-1",
        )
        receipt = recorded["receipt"]
        self.assertEqual(receipt["status"], "recorded")
        self.assertEqual(receipt["record_id"], "message:scripted-1")
        self.assertTrue(str(receipt["event_id"]).startswith("event:"))
        self.assertIsInstance(receipt["sequence"], int)
        self.assertEqual(recorded["record"]["later_note"], "preserve-me")

        again = local_participant.record_message(
            origin,
            WRITER_TOKEN,
            WRITER,
            [READER],
            text="café question",
            message_id="message:scripted-1",
        )
        self.assertEqual(again["receipt"]["event_id"], receipt["event_id"])

        page = local_participant.read_page(origin, READER_TOKEN)
        found = local_participant.messages_in(page)
        self.assertEqual([item["id"] for item in found], ["message:scripted-1"])
        body = found[0]["body"]
        if not isinstance(body, dict):
            raise AssertionError("message body was not an object")
        self.assertEqual(body["text"], "café question")
        self.assertEqual(body["draft"], "scripted")
        self.assertEqual(found[0]["later_note"], "preserve-me")
        self.assertEqual(found[0]["from"], WRITER)

    def test_reader_write_and_bad_draft_surface_host_codes(self) -> None:
        origin = self.server.origin
        with self.assertRaises(local_participant.ParticipantError) as denied:
            local_participant.record_message(
                origin,
                READER_TOKEN,
                READER,
                [WRITER],
                text="no",
                message_id="message:denied",
            )
        self.assertEqual(denied.exception.status, 403)
        self.assertEqual(denied.exception.code, "forbidden")

        def bad_version(
            *,
            world: str,
            principal: str,
            recipients: list[str],
            text: str,
            message_id: str,
        ) -> dict[str, object]:
            record = local_participant.scripted_draft(
                world=world,
                principal=principal,
                recipients=recipients,
                text=text,
                message_id=message_id,
            )
            record["protocol_version"] = "9"
            return record

        with self.assertRaises(local_participant.ParticipantError) as version:
            local_participant.record_message(
                origin,
                WRITER_TOKEN,
                WRITER,
                [READER],
                text="no",
                message_id="message:version",
                draft=bad_version,
            )
        self.assertEqual(version.exception.status, 422)
        self.assertEqual(version.exception.code, "unsupported_version")

        with self.assertRaises(local_participant.ParticipantError) as conflict:
            local_participant.record_message(
                origin,
                WRITER_TOKEN,
                WRITER,
                [READER],
                text="café question",
                message_id="message:scripted-1",
            )
            local_participant.record_message(
                origin,
                WRITER_TOKEN,
                WRITER,
                [READER],
                text="different bytes",
                message_id="message:scripted-1",
            )
        self.assertEqual(conflict.exception.status, 409)
        self.assertEqual(conflict.exception.code, "id_conflict")

    def test_empty_cursor_is_sent_and_rejected(self) -> None:
        with self.assertRaises(local_participant.ParticipantError) as caught:
            local_participant.read_page(self.server.origin, READER_TOKEN, after="")
        self.assertEqual(caught.exception.status, 400)
        self.assertEqual(caught.exception.code, "invalid_cursor")

    def test_a_lying_draft_is_not_submitted(self) -> None:
        def lying(
            *,
            world: str,
            principal: str,
            recipients: list[str],
            text: str,
            message_id: str,
        ) -> dict[str, object]:
            record = local_participant.scripted_draft(
                world=world,
                principal=principal,
                recipients=recipients,
                text=text,
                message_id=message_id,
            )
            record["from"] = "agent:someone-else"
            return record

        with self.assertRaises(ValueError):
            local_participant.record_message(
                self.server.origin,
                WRITER_TOKEN,
                WRITER,
                [READER],
                text="no",
                message_id="message:lie",
                draft=lying,
            )
        page = local_participant.read_page(self.server.origin, READER_TOKEN)
        self.assertNotIn("message:lie", [item["id"] for item in local_participant.messages_in(page)])

    def test_advertised_endpoint_on_another_origin_is_refused(self) -> None:
        discovery = local_participant.discover(self.server.origin)
        discovery["endpoints"] = {
            "submit": "https://example.com/submit",
            "events": "https://example.com/events",
        }
        with self.assertRaises(local_participant.ParticipantError) as caught:
            local_participant.world_endpoints(self.server.origin, discovery)
        self.assertEqual(caught.exception.code, "unexpected_discovery")
        other_port = urllib.parse.urlsplit(self.server.origin).port
        assert other_port is not None
        discovery["endpoints"] = {
            "submit": f"http://127.0.0.1:{other_port + 1}/submit",
            "events": f"http://127.0.0.1:{other_port + 1}/events",
        }
        with self.assertRaises(local_participant.ParticipantError) as port:
            local_participant.world_endpoints(self.server.origin, discovery)
        self.assertEqual(port.exception.code, "unexpected_discovery")


class ClientBoundaryTests(unittest.TestCase):
    def test_non_loopback_and_userinfo_never_become_a_request(self) -> None:
        with self.assertRaises(ValueError):
            local_participant.discover("http://example.com")
        with self.assertRaises(ValueError):
            local_participant.discover("http://user:secret@127.0.0.1:9")
        with self.assertRaises(ValueError):
            local_participant.record_message(
                "http://10.0.0.1:9",
                WRITER_TOKEN,
                WRITER,
                [READER],
                text="no",
                message_id="message:remote",
            )

    def test_empty_recipients_fail_before_discovery(self) -> None:
        with self.assertRaises(ValueError):
            local_participant.record_message(
                "http://127.0.0.1:9",
                WRITER_TOKEN,
                WRITER,
                [],
                text="no",
                message_id="message:empty",
            )

    def test_redirect_is_not_followed(self) -> None:
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                self.send_response(302)
                self.send_header("Location", "http://127.0.0.1:1/away")
                self.send_header("Content-Length", "0")
                self.end_headers()

            def log_message(self, fmt: str, *args: object) -> None:
                return

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            origin = HOST.http_origin(server.server_address)
            if not isinstance(origin, str):
                raise AssertionError("listen address was not a host and port")
            with self.assertRaises(local_participant.ParticipantError) as caught:
                local_participant.discover(origin)
            self.assertEqual(caught.exception.code, "redirect")
        finally:
            server.shutdown()
            server.server_close()

    def test_participant_source_names_no_remote_provider(self) -> None:
        source = local_participant.__file__
        if not isinstance(source, str):
            raise AssertionError("participant module has no file")
        text = Path(source).read_text(encoding="utf-8")
        for needle in (
            "openrouter.ai",
            "api.openai.com",
            "api.anthropic.com",
            "api.cloudflare.com",
            "os.environ",
        ):
            self.assertNotIn(needle, text)


class ProviderTests(unittest.TestCase):
    def test_shapes_keep_credentials_out_of_the_url_and_body(self) -> None:
        credential = "example-credential"
        requests = [
            providers.openrouter_chat_request(
                model="example/model", prompt="hello", credential=credential
            ),
            providers.cloudflare_workers_ai_request(
                account_id="exampleaccount",
                model="@cf/example/model",
                prompt="hello",
                credential=credential,
            ),
            providers.anthropic_messages_request(
                model="example-model", prompt="hello", credential=credential
            ),
            providers.anthropic_managed_session_request(
                agent_id="agent_example",
                environment_id="env_example",
                credential=credential,
                text="hello",
            ),
            providers.openai_chat_request(model="example-model", prompt="hello", credential=credential),
            providers.openai_responses_request(
                model="example-model", prompt="hello", credential=credential
            ),
        ]
        for request in requests:
            self.assertEqual(request.method, "POST")
            self.assertTrue(request.url.startswith("https://"))
            self.assertNotIn(credential, request.url)
            self.assertNotIn(credential, request.body.decode("utf-8"))
            self.assert_unsent(request)

        cloudflare = requests[1]
        self.assertEqual(
            cloudflare.url,
            "https://api.cloudflare.com/client/v4/accounts/exampleaccount/ai/run/@cf/example/model",
        )
        self.assertEqual(header(cloudflare, "Authorization"), f"Bearer {credential}")
        self.assertNotIn("model", json.loads(cloudflare.body))

        messages = requests[2]
        managed = requests[3]
        self.assertEqual(messages.url, "https://api.anthropic.com/v1/messages")
        self.assertEqual(managed.url, "https://api.anthropic.com/v1/sessions")
        self.assertIsNone(header(messages, "anthropic-beta"))
        self.assertEqual(header(messages, "anthropic-version"), "2023-06-01")
        self.assertEqual(header(messages, "x-api-key"), credential)
        self.assertEqual(header(managed, "anthropic-beta"), "managed-agents-2026-04-01")
        self.assertEqual(json.loads(messages.body)["max_tokens"], 256)
        self.assertEqual(json.loads(managed.body)["agent"], "agent_example")
        self.assertEqual(json.loads(managed.body)["environment_id"], "env_example")

        responses = requests[5]
        self.assertEqual(responses.url, "https://api.openai.com/v1/responses")
        parsed = json.loads(responses.body)
        self.assertEqual(parsed["input"], "hello")
        self.assertNotIn("messages", parsed)
        chat = requests[4]
        self.assertEqual(chat.url, "https://api.openai.com/v1/chat/completions")
        self.assertIn("messages", json.loads(chat.body))

    def test_local_delivery_uses_only_the_supplied_opener(self) -> None:
        request = providers.local_chat_request(
            base_url="http://127.0.0.1:11434",
            model="example-model",
            prompt="hello",
        )
        self.assertEqual(request.url, "http://127.0.0.1:11434/v1/chat/completions")
        self.assertIsNone(header(request, "Authorization"))
        calls: list[providers.PreparedRequest] = []

        def opener(prepared: providers.PreparedRequest) -> bytes:
            calls.append(prepared)
            return b'{"choices":[{"message":{"content":"local text"}}]}'

        with self.assertRaises(providers.SendRefused):
            providers.deliver(request, opener=opener)
        with self.assertRaises(providers.SendRefused):
            providers.deliver(request, allow_send="yes", opener=opener)  # type: ignore[arg-type]
        self.assertEqual(calls, [])
        payload = providers.deliver(request, allow_send=True, opener=opener)
        self.assertEqual(providers.chat_completion_text(json.loads(payload)), "local text")
        self.assertEqual(calls, [request])

        ipv6 = providers.local_chat_request(
            base_url="http://[::1]:11434",
            model="example-model",
            prompt="hello",
        )
        self.assertEqual(ipv6.url, "http://[::1]:11434/v1/chat/completions")

    def test_local_and_remote_builders_reject_bad_inputs(self) -> None:
        with self.assertRaises(ValueError):
            providers.local_chat_request(
                base_url="http://example.com", model="example-model", prompt="hello"
            )
        with self.assertRaises(ValueError):
            providers.local_chat_request(
                base_url="http://10.0.0.1:11434", model="example-model", prompt="hello"
            )
        with self.assertRaises(ValueError):
            providers.local_chat_request(
                base_url="http://127.0.0.1.example.com:11434",
                model="example-model",
                prompt="hello",
            )
        with self.assertRaises(ValueError):
            providers.local_chat_request(
                base_url="http://[::ffff:127.0.0.1]:11434",
                model="example-model",
                prompt="hello",
            )
        with self.assertRaises(ValueError):
            providers.local_chat_request(
                base_url="http://user:secret@127.0.0.1:11434",
                model="example-model",
                prompt="hello",
            )
        with self.assertRaises(ValueError):
            providers.local_chat_request(
                base_url="http://127.0.0.1:11434/v1",
                model="example-model",
                prompt="hello",
            )
        with self.assertRaises(ValueError):
            providers.openrouter_chat_request(model="example/model", prompt="hello", credential="")
        with self.assertRaises(ValueError):
            providers.openrouter_chat_request(
                model="example/model", prompt="hello", credential="has space"
            )
        with self.assertRaises(ValueError):
            providers.openrouter_chat_request(
                model="example/model", prompt="hello", credential="line\nbreak"
            )
        with self.assertRaises(ValueError):
            providers.anthropic_messages_request(
                model="example-model",
                prompt="hello",
                credential="example-credential",
                max_tokens=True,
            )
        with self.assertRaises(ValueError):
            providers.chat_completion_text({"choices": []})
        with self.assertRaises(ValueError):
            providers.chat_completion_text({"choices": [{"message": {"content": ["parts"]}}]})

    def test_provider_module_does_not_open_sockets_or_read_the_environment(self) -> None:
        source = providers.__file__
        if not isinstance(source, str):
            raise AssertionError("providers module has no file")
        text = Path(source).read_text(encoding="utf-8")
        self.assertNotIn("urlopen", text)
        self.assertNotIn("os.environ", text)
        self.assertNotIn("socket.", text)
        forged = providers.PreparedRequest("POST", "https://example.com/v1", (), b"{}")

        def opener(_request: providers.PreparedRequest) -> bytes:
            raise AssertionError("opener was called")

        with self.assertRaises(providers.SendRefused):
            providers.deliver(forged, allow_send=True, opener=opener)

    def assert_unsent(self, request: providers.PreparedRequest) -> None:
        def opener(_request: providers.PreparedRequest) -> bytes:
            raise AssertionError("opener was called")

        with self.assertRaises(providers.SendRefused):
            providers.deliver(request, opener=opener)
        with self.assertRaises(providers.SendRefused):
            providers.deliver(request, allow_send=True, opener=opener)


if __name__ == "__main__":
    unittest.main()
