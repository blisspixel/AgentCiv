"""Request shapes for model providers.

Each builder returns a request object. Nothing in this module opens a
socket, imports an SDK, or reads process environment. deliver calls a
caller-supplied opener only when allow_send is true and the URL is
loopback. Remote shapes stay unsent, including when allow_send is true.
"""

from __future__ import annotations

import json
import re
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import TypeVar
from urllib.parse import quote

from loopback import loopback_origin, require_loopback

_MODEL = re.compile(r"^[@A-Za-z0-9][@A-Za-z0-9_.:@+/-]{0,200}$")
_ACCOUNT = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$")
_SECRET = re.compile(r"^[A-Za-z0-9._~+/=-]{1,4096}$")


class SendRefused(RuntimeError):
    pass


_SendResult = TypeVar("_SendResult")
JsonObject = dict[str, object]


@dataclass(frozen=True)
class PreparedRequest:
    method: str
    url: str
    headers: tuple[tuple[str, str], ...]
    body: bytes


def local_chat_request(
    *,
    base_url: str,
    model: str,
    prompt: str,
    credential: str | None = None,
) -> PreparedRequest:
    origin = loopback_origin(base_url)
    headers: list[tuple[str, str]] = []
    if credential is not None:
        headers.append(("Authorization", f"Bearer {_secret(credential)}"))
    return _post(origin + "/v1/chat/completions", _chat_body(model, prompt), headers)


def openrouter_chat_request(*, model: str, prompt: str, credential: str) -> PreparedRequest:
    return _post(
        "https://openrouter.ai/api/v1/chat/completions",
        _chat_body(model, prompt),
        [("Authorization", f"Bearer {_secret(credential)}")],
    )


def cloudflare_workers_ai_request(
    *,
    account_id: str,
    model: str,
    prompt: str,
    credential: str,
) -> PreparedRequest:
    if not isinstance(account_id, str) or _ACCOUNT.fullmatch(account_id) is None:
        raise ValueError("account id is invalid")
    model_name = _name(model)
    if model_name.startswith("/") or model_name.endswith("/"):
        raise ValueError("model is invalid")
    url = (
        "https://api.cloudflare.com/client/v4/accounts/"
        + quote(account_id, safe="")
        + "/ai/run/"
        + quote(model_name, safe="/@")
    )
    body = {"messages": [{"content": _prompt(prompt), "role": "user"}]}
    return _post(url, body, [("Authorization", f"Bearer {_secret(credential)}")])


def anthropic_messages_request(
    *,
    model: str,
    prompt: str,
    credential: str,
    max_tokens: int = 256,
) -> PreparedRequest:
    if type(max_tokens) is not int or not 1 <= max_tokens <= 8192:
        raise ValueError("max_tokens is out of range")
    body = {
        "max_tokens": max_tokens,
        "messages": [{"content": _prompt(prompt), "role": "user"}],
        "model": _name(model),
    }
    return _post(
        "https://api.anthropic.com/v1/messages",
        body,
        [
            ("anthropic-version", "2023-06-01"),
            ("x-api-key", _secret(credential)),
        ],
    )


def anthropic_managed_session_request(
    *,
    agent_id: str,
    environment_id: str,
    credential: str,
    text: str,
) -> PreparedRequest:
    """Shape for a Managed Agents session.

    The live product requires anthropic-beta managed-agents-2026-04-01 and
    an agent id plus an environment id that already exist. deliver refuses
    this request because its URL is not loopback.
    """

    body = {
        "agent": _name(agent_id, "agent id"),
        "environment_id": _name(environment_id, "environment id"),
        "initial_events": [
            {"content": [{"text": _prompt(text), "type": "text"}], "type": "user.message"}
        ],
    }
    return _post(
        "https://api.anthropic.com/v1/sessions",
        body,
        [
            ("anthropic-beta", "managed-agents-2026-04-01"),
            ("anthropic-version", "2023-06-01"),
            ("x-api-key", _secret(credential)),
        ],
    )


def openai_chat_request(*, model: str, prompt: str, credential: str) -> PreparedRequest:
    return _post(
        "https://api.openai.com/v1/chat/completions",
        _chat_body(model, prompt),
        [("Authorization", f"Bearer {_secret(credential)}")],
    )


def openai_responses_request(*, model: str, prompt: str, credential: str) -> PreparedRequest:
    body = {"input": _prompt(prompt), "model": _name(model)}
    return _post(
        "https://api.openai.com/v1/responses",
        body,
        [("Authorization", f"Bearer {_secret(credential)}")],
    )


def chat_completion_text(payload: Mapping[str, object]) -> str:
    choices = payload.get("choices")
    if not isinstance(choices, list) or not choices:
        raise ValueError("chat completion has no message text")
    choice = choices[0]
    if not isinstance(choice, dict):
        raise ValueError("chat completion has no message text")
    message = choice.get("message")
    if not isinstance(message, dict) or "content" not in message:
        raise ValueError("chat completion has no message text")
    text = message["content"]
    if not isinstance(text, str):
        raise ValueError("chat completion text is not a string")
    return text


def deliver(
    request: PreparedRequest,
    *,
    allow_send: bool = False,
    opener: Callable[[PreparedRequest], _SendResult] | None = None,
) -> _SendResult:
    if not isinstance(request, PreparedRequest):
        raise SendRefused("provider traffic stays unsent")
    if allow_send is not True or opener is None:
        raise SendRefused(
            "provider traffic stays unsent until allow_send is true and an opener is supplied"
        )
    try:
        require_loopback(request.url)
    except ValueError as error:
        raise SendRefused("this package delivers only a loopback URL") from error
    return opener(request)


def _chat_body(model: str, prompt: str) -> JsonObject:
    return {"messages": [{"content": _prompt(prompt), "role": "user"}], "model": _name(model)}


def _post(url: str, payload: Mapping[str, object], extra: list[tuple[str, str]]) -> PreparedRequest:
    headers = [("Accept", "application/json"), ("Content-Type", "application/json"), *extra]
    body = json.dumps(payload, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8")
    return PreparedRequest("POST", url, tuple(headers), body)


def _name(value: str, label: str = "model") -> str:
    if not isinstance(value, str) or _MODEL.fullmatch(value) is None or ".." in value:
        raise ValueError(f"{label} is invalid")
    return value


def _prompt(value: str) -> str:
    if not isinstance(value, str):
        raise ValueError("prompt must be a string")
    if len(value) > 100_000:
        raise ValueError("prompt is too long for this exemplar")
    return value


def _secret(value: str) -> str:
    if not isinstance(value, str) or _SECRET.fullmatch(value) is None:
        raise ValueError("credential is invalid")
    return value
