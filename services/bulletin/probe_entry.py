"""Bounded manifest-derived public GET/HEAD checks for local or deployed HTTP.

No credentials, retries, redirects, browser impersonation, or posting. A local
pass cannot establish deployment or identify the provider's blocking rule.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import hashlib
from http.client import HTTPException
import json
from pathlib import Path
import sys
import time
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

MAX_INTERFACES = 24
MAX_PATHS = 30
MAX_REQUESTS = 64
MAX_SECONDS = 120
MISSING_PATH = "/.agentciv-entry-probe-missing"
PUBLIC_PATHS = ("/", "/robots.txt", "/.well-known/agentciv-services", "/llms.txt", "/api/board/info", MISSING_PATH)
CONTROL_AGENT = "AgentCivEntryProbe/0.1 (+https://github.com/blisspixel/AgentCiv)"
MAX_BYTES = 65_536
TIMEOUT_SECONDS = 5


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req: Request, fp: Any, code: int, msg: str,
                         headers: Any, newurl: str) -> None:
        return None


@dataclass
class Observation:
    path: str
    client: str
    user_agent: str
    method: str = "GET"
    accept: str | None = None
    status: int | None = None
    headers: dict[str, str] | None = None
    body_text: str = ""
    body_sha256: str | None = None
    bytes_read: int = 0
    truncated: bool = False
    error: str | None = None
    passed: bool = False


def origin_value(value: str) -> str:
    parsed = urlsplit(value)
    if (parsed.scheme not in ("http", "https") or not parsed.hostname
            or parsed.username is not None or parsed.password is not None
            or parsed.path not in ("", "/") or parsed.query or parsed.fragment
            or (parsed.scheme == "http" and parsed.hostname not in ("127.0.0.1", "localhost", "::1"))):
        raise ValueError("origin must be HTTPS or loopback HTTP, without credentials, path, query, or fragment")
    # Accessing port also rejects malformed or out-of-range ports before any GET.
    _ = parsed.port
    return value.rstrip("/")


def expected_body(path: str, data: bytes) -> bool:
    try:
        text = data.decode("utf-8")
        if path == "/llms.txt":
            return text.startswith("# AgentCiv service instructions\n") or text.startswith("# AgentCiv service instructions\r\n")
        if path == "/robots.txt":
            return "User-agent:" in text
        if path.endswith(".md"):
            return text.startswith("Generated: ") and "source commit:" in text.splitlines()[0] and "\n" in text
        if path == "/research":
            return "<title>" in text and "AgentCiv" in text
        document = json.loads(text)
        if not isinstance(document, dict):
            return False
        if path in ("/", "/agent.json", "/.well-known/agentciv-services"):
            return document.get("service") == "agentciv.io/0.1-experimental"
        if path == "/api/board/info":
            return document.get("service") == "web-bulletin/0.1-experimental"
        if path.startswith("/api/board/posts"):
            return isinstance(document.get("posts"), list)
        if path.startswith("/api/board/changes"):
            return isinstance(document.get("changes"), list)
        if path.startswith("/api/board/conversations/"):
            return isinstance(document.get("post"), dict) and isinstance(document.get("replies"), dict)
        # Catalogs, schemas and future advertised JSON reads must be JSON objects;
        # a 200 HTML challenge is never an access success.
        return bool(document)
    except (UnicodeDecodeError, ValueError, RecursionError):
        return False


def descriptor_paths(data: bytes) -> list[str]:
    """Only bounded same-origin read paths, never arbitrary descriptor URLs."""
    def pairs(items: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate descriptor member")
            result[key] = value
        return result
    document = json.loads(data.decode("utf-8"), object_pairs_hook=pairs)
    if not isinstance(document, dict) or document.get("service") != "agentciv.io/0.1-experimental":
        raise ValueError("invalid descriptor")
    interfaces = document.get("interfaces")
    if not isinstance(interfaces, dict) or not 1 <= len(interfaces) <= MAX_INTERFACES:
        raise ValueError("invalid interfaces")
    paths: list[str] = []
    values = list(interfaces.values())
    conversation = document.get("conversation_read")
    if conversation is not None:
        if not isinstance(conversation, dict) or conversation.get("probe_path") != "/api/board/conversations/1":
            raise ValueError("invalid conversation probe")
        values.append(conversation["probe_path"])
    for value in values:
        if not isinstance(value, str) or len(value) > 2048 or any(ord(char) < 33 or ord(char) == 127 for char in value):
            raise ValueError("invalid read path")
        parsed = urlsplit(value)
        if (not value.startswith("/") or value.startswith("//") or "\\" in value
            or parsed.scheme or parsed.netloc or parsed.fragment or "{" in value or "}" in value
            or parsed.path in ("/board/publish", "/theme") or "%" in parsed.path):
            raise ValueError("invalid read path")
        paths.append(value)
    return list(dict.fromkeys(paths))


def probe(origin: str) -> list[Observation]:
    origin = origin_value(origin)
    results: list[Observation] = []
    deadline = time.monotonic() + MAX_SECONDS
    clients = (("python-default", None), ("truthful-control", CONTROL_AGENT))
    discovered: set[str] = set()

    def fetch(path: str, client: str, agent: str | None, method: str = "GET", accept: str | None = None) -> Observation:
        opener = build_opener(NoRedirect())
        actual_agent = dict(opener.addheaders)["User-agent"] if agent is None else agent
        result = Observation(path=path, client=client, user_agent=actual_agent, method=method, accept=accept)
        results.append(result)
        remaining = deadline - time.monotonic()
        if remaining <= 0 or len(results) > MAX_REQUESTS:
            result.error = "probe_budget_exceeded"
            return result
        headers = {} if agent is None else {"User-Agent": agent}
        if accept is not None:
            headers["Accept"] = accept
        request = Request(origin + path, headers=headers, method=method)
        try:
            try:
                response = opener.open(request, timeout=min(TIMEOUT_SECONDS, remaining))
            except HTTPError as refusal:
                response = refusal
            with response:
                result.status = response.status
                result.headers = {name: response.headers[name] for name in ("Content-Type", "Server", "CF-Ray", "CF-Mitigated", "Vary", "Cache-Control") if name in response.headers}
                chunks = bytearray()
                while len(chunks) <= MAX_BYTES:
                    if time.monotonic() >= deadline:
                        raise TimeoutError("probe deadline")
                    chunk = response.read1(min(8192, MAX_BYTES + 1 - len(chunks)))
                    if not chunk:
                        break
                    chunks.extend(chunk)
                data = bytes(chunks)
                result.bytes_read = len(data)
                result.truncated = len(data) > MAX_BYTES
                result.body_sha256 = hashlib.sha256(data).hexdigest()
                result.body_text = data.decode("utf-8", errors="replace")
                if method == "HEAD":
                    result.passed = result.status == 200 and not data and (accept is None or response.headers.get_content_type() == accept)
                elif accept is not None:
                    result.passed = (result.status == 200 and not result.truncated
                                     and response.headers.get_content_type() == accept
                                     and "accept" in response.headers.get("Vary", "").lower()
                                     and expected_body("/llms.txt", data))
                elif path == MISSING_PATH:
                    result.passed = result.status == 404 and not result.truncated
                elif path == "/api/board/conversations/1" and result.status == 404:
                    document = json.loads(data)
                    result.passed = isinstance(document, dict) and document.get("code") == "post_not_found"
                else:
                    result.passed = result.status == 200 and not result.truncated and expected_body(path, data)
                if path == "/agent.json" and method == "GET" and result.passed:
                    try:
                        discovered.update(descriptor_paths(data))
                    except (ValueError, UnicodeError, RecursionError):
                        result.error = "invalid_descriptor"
                        result.passed = False
        except (URLError, OSError, TimeoutError, HTTPException, ValueError, RecursionError) as error:
            result.error = type(error).__name__
            result.passed = False
        return result

    # Bootstrap each identity once. A control descriptor can supply paths to
    # diagnose a refused default client; its success never excuses that refusal.
    for client, agent in clients:
        fetch("/agent.json", client, agent)
    paths = list(dict.fromkeys([*PUBLIC_PATHS, *sorted(discovered)]))
    if len(paths) > MAX_PATHS:
        for result in results:
            result.passed = False
            result.error = "descriptor_path_limit"
        return results
    for client, agent in clients:
        for path in paths:
            fetch(path, client, agent)
        fetch("/agent.json", client, agent, "HEAD")
        for accept in ("text/markdown", "text/plain"):
            for method in ("GET", "HEAD"):
                fetch("/", client, agent, method, accept)
    return results


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--origin", required=True)
    parser.add_argument("--output", required=True, type=Path, help="fresh JSON file, never overwritten")
    args = parser.parse_args(argv)
    origin = origin_value(args.origin)
    # Reserve the output before network work; an existing record is never replaced.
    with args.output.open("x", encoding="utf-8") as output:
        results = probe(origin)
        passed = all(result.passed for result in results)
        json.dump({"origin": origin, "observed_at": datetime.now(timezone.utc).isoformat(),
                   "python_version": sys.version, "max_bytes_plus_sentinel": MAX_BYTES + 1,
                   "socket_timeout_seconds": TIMEOUT_SECONDS, "max_seconds": MAX_SECONDS,
                   "max_requests": MAX_REQUESTS, "descriptor_interfaces_limit": MAX_INTERFACES, "redirects": False, "retries": 0,
                   "passed": passed, "observations": [asdict(result) for result in results]}, output, indent=2)
        output.write("\n")
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
