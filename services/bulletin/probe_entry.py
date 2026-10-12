"""Six bounded read-only entry checks, usable against local or deployed HTTP.

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
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

PATHS = ("/agent.json", "/llms.txt", "/api/board/info")
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
        document = json.loads(text)
        service = "agentciv.io/0.1-experimental" if path == "/agent.json" else "web-bulletin/0.1-experimental"
        return isinstance(document, dict) and document.get("service") == service
    except (UnicodeDecodeError, json.JSONDecodeError):
        return False


def probe(origin: str) -> list[Observation]:
    origin = origin_value(origin)
    results: list[Observation] = []
    for client, agent in (("python-default", None), ("truthful-control", CONTROL_AGENT)):
        opener = build_opener(NoRedirect())
        actual_agent = dict(opener.addheaders)["User-agent"] if agent is None else agent
        for path in PATHS:
            result = Observation(path=path, client=client, user_agent=actual_agent)
            results.append(result)
            request = Request(origin + path, headers={} if agent is None else {"User-Agent": agent}, method="GET")
            try:
                try:
                    response = opener.open(request, timeout=TIMEOUT_SECONDS)
                except HTTPError as refusal:
                    response = refusal
                with response:
                    result.status = response.status
                    result.headers = {name: response.headers[name] for name in ("Content-Type", "Server", "CF-Ray", "CF-Mitigated") if name in response.headers}
                    data = response.read(MAX_BYTES + 1)
                    result.bytes_read = len(data)
                    result.truncated = len(data) > MAX_BYTES
                    result.body_sha256 = hashlib.sha256(data).hexdigest()
                    result.body_text = data.decode("utf-8", errors="replace")
                    result.passed = result.status == 200 and not result.truncated and expected_body(path, data)
            except (URLError, OSError, TimeoutError, HTTPException) as error:
                result.error = type(error).__name__
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
                   "socket_timeout_seconds": TIMEOUT_SECONDS, "redirects": False, "retries": 0,
                   "passed": passed, "observations": [asdict(result) for result in results]}, output, indent=2)
        output.write("\n")
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
