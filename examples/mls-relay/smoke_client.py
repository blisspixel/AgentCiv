#!/usr/bin/env python3
"""Zero-dependency Python 3.11+ loopback smoke client for AgentCiv's Tier 3
PQ-MLS Untrusted Relay (`mls-relay/0.1-draft`).

Uses only the Python standard library. Exercises the full HTTP relay lifecycle
against a running `agentciv-host mls-relay --writes-enabled` instance and
independently verifies RFC 9162 Merkle ciphertext root and consistency proofs.
"""
from __future__ import annotations

import argparse
import base64
from dataclasses import dataclass
import hashlib
from http.client import HTTPException
import json
import sys
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener
import uuid

PROFILE_ID = "mls-relay/0.1-draft"
PQ_CIPHERSUITE = "MLS_256_X25519MLKEM768_CHACHA20POLY1305_SHA512_MLDSA65"
MAX_RESPONSE_BYTES = 1_048_576  # 1 MiB
DEFAULT_TIMEOUT_SECONDS = 15.0
LOOPBACK_HOSTS = ("127.0.0.1", "localhost", "::1")
USER_AGENT = "AgentCivMlsRelaySmokeClient/0.1 (+https://github.com/blisspixel/AgentCiv)"

RFC9162_LEAF_PREFIX = b"\x00"
RFC9162_NODE_PREFIX = b"\x01"
EMPTY_ROOT_HASH = hashlib.sha256(b"").digest()


class SmokeClientError(RuntimeError):
    """Structured error raised when a relay check or HTTP call fails."""

    def __init__(
        self,
        message: str,
        *,
        status: int | None = None,
        payload: Any = None,
    ) -> None:
        super().__init__(message)
        self.status = status
        self.payload = payload


class _RefuseRedirect(HTTPRedirectHandler):
    """Refuse all HTTP 3xx redirects rather than following them automatically."""

    def redirect_request(
        self,
        req: Request,
        fp: Any,
        code: int,
        msg: str,
        headers: Any,
        newurl: str,
    ) -> None:
        return None


@dataclass(frozen=True)
class HttpResult:
    status: int
    headers: dict[str, str]
    body: Any


def validate_loopback_origin(origin: str) -> str:
    """Validate and canonicalize a loopback-only `http://` relay origin."""
    if not origin or not origin.isascii():
        raise ValueError("origin must be a non-empty ASCII URL")
    if any(ord(ch) <= 0x20 or ord(ch) == 0x7F for ch in origin):
        raise ValueError("origin must not contain whitespace or control characters")
    if "?" in origin or "#" in origin or "|" in origin:
        raise ValueError("origin must not contain query, fragment, or pipe characters")

    parsed = urlsplit(origin)
    if parsed.scheme != "http":
        raise ValueError("mls-relay smoke client requires loopback http:// origin")
    if parsed.username is not None or parsed.password is not None or "@" in parsed.netloc:
        raise ValueError("origin must not contain userinfo credentials")
    if parsed.path not in ("", "/"):
        raise ValueError("origin must not contain a path")
    if parsed.query or parsed.fragment:
        raise ValueError("origin must not contain a query or fragment")
    if not parsed.hostname:
        raise ValueError("origin must include a hostname")

    port = parsed.port
    if port is not None and port == 0:
        raise ValueError("origin port must be in 1..65535")

    host = parsed.hostname.lower()
    if host not in LOOPBACK_HOSTS:
        raise ValueError(
            "non-loopback origin is rejected; only 127.0.0.1, localhost, or [::1] are allowed"
        )

    host_part = f"[{host}]" if ":" in host else host
    if port is None or port == 80:
        return f"http://{host_part}"
    return f"http://{host_part}:{port}"


def _read_bounded_json(raw_stream: Any, max_bytes: int = MAX_RESPONSE_BYTES) -> Any:
    raw = raw_stream.read(max_bytes + 1)
    if len(raw) > max_bytes:
        raise SmokeClientError(f"response body exceeded {max_bytes} byte cap")
    if not raw:
        return None
    try:
        return json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise SmokeClientError(f"response body is not valid UTF-8 JSON: {exc}") from exc


def http_json_request(
    origin: str,
    method: str,
    path: str,
    *,
    payload: dict[str, Any] | None = None,
    timeout: float = DEFAULT_TIMEOUT_SECONDS,
    max_response_bytes: int = MAX_RESPONSE_BYTES,
) -> HttpResult:
    """Send a bounded, proxy-free, no-redirect JSON HTTP request to a loopback relay."""
    canonical_origin = validate_loopback_origin(origin)
    if not path.startswith("/"):
        raise ValueError("request path must start with '/'")

    url = f"{canonical_origin}{path}"
    data: bytes | None = None
    headers: dict[str, str] = {
        "Accept": "application/json",
        "User-Agent": USER_AGENT,
    }
    if payload is not None:
        data = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        headers["Content-Type"] = "application/json"

    req = Request(url, data=data, headers=headers, method=method)
    opener = build_opener(ProxyHandler({}), _RefuseRedirect())

    try:
        with opener.open(req, timeout=timeout) as resp:
            status = int(resp.status)
            resp_headers = {k.lower(): v for k, v in resp.headers.items()}
            body = _read_bounded_json(resp, max_bytes=max_response_bytes)
            return HttpResult(status=status, headers=resp_headers, body=body)
    except HTTPError as exc:
        try:
            status = int(exc.code)
            resp_headers = {k.lower(): v for k, v in exc.headers.items()}
            body = _read_bounded_json(exc, max_bytes=max_response_bytes)
            return HttpResult(status=status, headers=resp_headers, body=body)
        finally:
            exc.close()
    except (URLError, HTTPException, TimeoutError, OSError) as exc:
        raise SmokeClientError(f"HTTP request failed for {method} {path}: {exc}") from exc


def _b64(data: bytes) -> str:
    return base64.b64encode(data).decode("ascii")


def ciphertext_leaf_hash(blob_sha256: bytes) -> bytes:
    """Compute RFC 9162 leaf hash `SHA-256(0x00 || blob_sha256)`."""
    if len(blob_sha256) != 32:
        raise ValueError("blob_sha256 must be 32 bytes")
    return hashlib.sha256(RFC9162_LEAF_PREFIX + blob_sha256).digest()


def _node_hash(left: bytes, right: bytes) -> bytes:
    return hashlib.sha256(RFC9162_NODE_PREFIX + left + right).digest()


def _largest_power_of_two_less_than(n: int) -> int:
    return 1 << ((n - 1).bit_length() - 1)


def merkle_root_hash(leaves: list[bytes]) -> bytes:
    """Compute RFC 9162 Merkle Tree Hash `MTH(leaves)`."""
    n = len(leaves)
    if n == 0:
        return EMPTY_ROOT_HASH
    if n == 1:
        return leaves[0]
    k = _largest_power_of_two_less_than(n)
    return _node_hash(merkle_root_hash(leaves[:k]), merkle_root_hash(leaves[k:]))


def verify_consistency_proof(
    first_size: int,
    second_size: int,
    first_root: bytes,
    second_root: bytes,
    proof: list[bytes],
) -> bool:
    """Verify an RFC 9162 consistency proof between `(first_size, first_root)` and `(second_size, second_root)`."""
    if first_size < 0 or second_size < 0 or first_size > second_size:
        return False
    if first_size == 0 and first_root != EMPTY_ROOT_HASH:
        return False
    if first_size == second_size:
        return len(proof) == 0 and first_root == second_root
    if first_size == 0:
        return len(proof) == 0
    if not proof:
        return False

    fn = first_size - 1
    sn = second_size - 1
    while (fn & 1) == 1:
        fn >>= 1
        sn >>= 1

    if (first_size & (first_size - 1)) == 0:
        fr = first_root
        sr = first_root
        rest = proof
    else:
        fr = proof[0]
        sr = proof[0]
        rest = proof[1:]

    for node in rest:
        if sn == 0:
            return False
        if (fn & 1) == 1 or fn == sn:
            fr = _node_hash(node, fr)
            sr = _node_hash(node, sr)
            while fn != 0 and (fn & 1) == 0:
                fn >>= 1
                sn >>= 1
        else:
            sr = _node_hash(sr, node)
        fn >>= 1
        sn >>= 1

    return sn == 0 and fr == first_root and sr == second_root


def _expect(condition: bool, message: str, result: HttpResult | None = None) -> None:
    if not condition:
        raise SmokeClientError(
            message,
            status=result.status if result is not None else None,
            payload=result.body if result is not None else None,
        )


def _assert_security_headers(result: HttpResult, label: str) -> None:
    _expect(
        result.headers.get("cache-control") == "no-store",
        f"{label}: expected Cache-Control: no-store",
        result,
    )
    _expect(
        result.headers.get("x-content-type-options") == "nosniff",
        f"{label}: expected X-Content-Type-Options: nosniff",
        result,
    )


def run_smoke(
    origin: str,
    *,
    timeout: float = DEFAULT_TIMEOUT_SECONDS,
    prefix: str | None = None,
) -> dict[str, Any]:
    """Run the full `mls-relay/0.1-draft` loopback lifecycle smoke test."""
    canonical_origin = validate_loopback_origin(origin)
    run_id = prefix if prefix is not None else uuid.uuid4().hex[:10]
    kp_id = f"kp:{run_id}:bob-01"
    recipient_id = f"bob-{run_id}#02"
    group_id = f"grp:{run_id}:pq-room"

    # 1. GET /mls/v1/info
    info_res = http_json_request(canonical_origin, "GET", "/mls/v1/info", timeout=timeout)
    _expect(info_res.status == 200, f"GET /mls/v1/info returned {info_res.status}", info_res)
    _assert_security_headers(info_res, "GET /mls/v1/info")
    _expect(isinstance(info_res.body, dict), "info response must be a JSON object", info_res)
    _expect(info_res.body.get("profile") == PROFILE_ID, "unexpected relay profile", info_res)
    _expect(
        info_res.body.get("writes_enabled") is True,
        "relay writes_enabled must be true for smoke lifecycle",
        info_res,
    )
    suites = info_res.body.get("supported_ciphersuites")
    _expect(
        isinstance(suites, list) and PQ_CIPHERSUITE in suites,
        f"relay must advertise {PQ_CIPHERSUITE}",
        info_res,
    )

    # 2. POST /mls/v1/key-packages
    kp_res = http_json_request(
        canonical_origin,
        "POST",
        "/mls/v1/key-packages",
        payload={
            "kp_id": kp_id,
            "principal_id": recipient_id,
            "ciphersuite": PQ_CIPHERSUITE,
            "blob_b64": _b64(f"pq-keypackage-{run_id}".encode("utf-8")),
        },
        timeout=timeout,
    )
    _expect(kp_res.status == 201, f"POST /mls/v1/key-packages returned {kp_res.status}", kp_res)
    _expect(
        isinstance(kp_res.body, dict)
        and kp_res.body.get("kp_id") == kp_id
        and kp_res.body.get("consumed") is False,
        "key-package publish response mismatch",
        kp_res,
    )

    # 3. POST /mls/v1/key-packages/consume (first consume succeeds, second returns 404)
    consume1 = http_json_request(
        canonical_origin,
        "POST",
        "/mls/v1/key-packages/consume",
        payload={"principal_id": recipient_id},
        timeout=timeout,
    )
    _expect(
        consume1.status == 200,
        f"first consume returned {consume1.status}",
        consume1,
    )
    _expect(
        isinstance(consume1.body, dict)
        and consume1.body.get("kp_id") == kp_id
        and consume1.body.get("consumed") is True,
        "first consume did not return expected single-use KeyPackage",
        consume1,
    )

    consume2 = http_json_request(
        canonical_origin,
        "POST",
        "/mls/v1/key-packages/consume",
        payload={"principal_id": recipient_id},
        timeout=timeout,
    )
    _expect(
        consume2.status == 404
        and isinstance(consume2.body, dict)
        and consume2.body.get("code") == "not_found",
        "second consume must fail with 404 not_found (single-use check)",
        consume2,
    )

    # 4. POST /mls/v1/groups
    grp_res = http_json_request(
        canonical_origin,
        "POST",
        "/mls/v1/groups",
        payload={"group_id": group_id, "ciphersuite": PQ_CIPHERSUITE},
        timeout=timeout,
    )
    _expect(grp_res.status == 201, f"POST /mls/v1/groups returned {grp_res.status}", grp_res)
    _expect(
        isinstance(grp_res.body, dict)
        and grp_res.body.get("group_id") == group_id
        and grp_res.body.get("current_epoch") == 0,
        "create group response mismatch",
        grp_res,
    )

    # 5. POST /mls/v1/groups/{id}/commits (epoch 0 -> 1 with Welcome)
    commit1 = http_json_request(
        canonical_origin,
        "POST",
        f"/mls/v1/groups/{quote(group_id, safe='')}/commits",
        payload={
            "expected_epoch": 0,
            "commit_blob_b64": _b64(f"commit-0-to-1-{run_id}".encode("utf-8")),
            "welcomes": [
                {
                    "recipient_id": recipient_id,
                    "blob_b64": _b64(f"welcome-bob-{run_id}".encode("utf-8")),
                }
            ],
        },
        timeout=timeout,
    )
    _expect(commit1.status == 201, f"commit 1 returned {commit1.status}", commit1)
    _expect(
        isinstance(commit1.body, dict)
        and commit1.body.get("previous_epoch") == 0
        and commit1.body.get("new_epoch") == 1
        and isinstance(commit1.body.get("welcome_seqs"), list)
        and len(commit1.body["welcome_seqs"]) == 1,
        "commit 1 receipt mismatch",
        commit1,
    )

    # Capture checkpoint at tree_size == 1
    cp1 = http_json_request(
        canonical_origin,
        "GET",
        f"/mls/v1/groups/{quote(group_id, safe='')}/checkpoint",
        timeout=timeout,
    )
    _expect(cp1.status == 200, f"checkpoint 1 returned {cp1.status}", cp1)
    _expect(
        isinstance(cp1.body, dict)
        and cp1.body.get("tree_size") == 1
        and isinstance(cp1.body.get("root_hash_hex"), str),
        "checkpoint 1 payload mismatch",
        cp1,
    )
    root1_bytes = bytes.fromhex(cp1.body["root_hash_hex"])

    # 6. Competing Commit at stale epoch 0 -> 409 epoch_conflict
    conflict = http_json_request(
        canonical_origin,
        "POST",
        f"/mls/v1/groups/{quote(group_id, safe='')}/commits",
        payload={
            "expected_epoch": 0,
            "commit_blob_b64": _b64(f"competing-commit-{run_id}".encode("utf-8")),
        },
        timeout=timeout,
    )
    _expect(
        conflict.status == 409
        and isinstance(conflict.body, dict)
        and conflict.body.get("code") == "epoch_conflict"
        and conflict.body.get("current_epoch") == 1,
        "competing commit must fail with 409 epoch_conflict (current_epoch=1)",
        conflict,
    )

    # 7. POST /mls/v1/groups/{id}/messages at epoch 1
    msg1 = http_json_request(
        canonical_origin,
        "POST",
        f"/mls/v1/groups/{quote(group_id, safe='')}/messages",
        payload={
            "epoch": 1,
            "blob_b64": _b64(f"private-message-1-{run_id}".encode("utf-8")),
        },
        timeout=timeout,
    )
    _expect(msg1.status == 201, f"submit message returned {msg1.status}", msg1)
    _expect(
        isinstance(msg1.body, dict)
        and msg1.body.get("kind") == "private_message"
        and msg1.body.get("epoch") == 1,
        "private message response mismatch",
        msg1,
    )

    # 8. Second Commit (epoch 1 -> 2) so consistency proof spans 1 -> 3 leaves
    commit2 = http_json_request(
        canonical_origin,
        "POST",
        f"/mls/v1/groups/{quote(group_id, safe='')}/commits",
        payload={
            "expected_epoch": 1,
            "commit_blob_b64": _b64(f"commit-1-to-2-{run_id}".encode("utf-8")),
        },
        timeout=timeout,
    )
    _expect(commit2.status == 201, f"commit 2 returned {commit2.status}", commit2)
    _expect(
        isinstance(commit2.body, dict) and commit2.body.get("new_epoch") == 2,
        "commit 2 did not advance to epoch 2",
        commit2,
    )

    # 9. GET /mls/v1/groups/{id}/envelopes
    envs = http_json_request(
        canonical_origin,
        "GET",
        f"/mls/v1/groups/{quote(group_id, safe='')}/envelopes?after_seq=0&limit=50",
        timeout=timeout,
    )
    _expect(envs.status == 200, f"fetch envelopes returned {envs.status}", envs)
    _expect(
        isinstance(envs.body, dict)
        and isinstance(envs.body.get("envelopes"), list)
        and len(envs.body["envelopes"]) == 3,
        "expected 3 envelopes (commit, private_message, commit)",
        envs,
    )
    envelope_list = envs.body["envelopes"]
    client_leaves = [
        ciphertext_leaf_hash(bytes.fromhex(item["blob_sha256"]))
        for item in envelope_list
    ]
    recomputed_root3 = merkle_root_hash(client_leaves)

    # 10. GET /mls/v1/welcomes/{recipient}
    welcomes = http_json_request(
        canonical_origin,
        "GET",
        f"/mls/v1/welcomes/{quote(recipient_id, safe='')}?after_seq=0&limit=50",
        timeout=timeout,
    )
    _expect(welcomes.status == 200, f"fetch welcomes returned {welcomes.status}", welcomes)
    _expect(
        isinstance(welcomes.body, dict)
        and isinstance(welcomes.body.get("welcomes"), list)
        and len(welcomes.body["welcomes"]) == 1
        and welcomes.body["welcomes"][0].get("recipient_id") == recipient_id,
        "expected 1 welcome for recipient",
        welcomes,
    )

    # 11. GET /mls/v1/groups/{id}/checkpoint?old_size=1 and verify RFC 9162 consistency proof
    cp3 = http_json_request(
        canonical_origin,
        "GET",
        f"/mls/v1/groups/{quote(group_id, safe='')}/checkpoint?old_size=1",
        timeout=timeout,
    )
    _expect(cp3.status == 200, f"checkpoint 3 returned {cp3.status}", cp3)
    _expect(
        isinstance(cp3.body, dict)
        and cp3.body.get("tree_size") == 3
        and cp3.body.get("consistency_from_size") == 1
        and isinstance(cp3.body.get("root_hash_hex"), str)
        and isinstance(cp3.body.get("consistency_proof_hex"), list),
        "checkpoint 3 payload mismatch",
        cp3,
    )
    root3_bytes = bytes.fromhex(cp3.body["root_hash_hex"])
    _expect(
        root3_bytes == recomputed_root3,
        "relay root_hash_hex does not match client-recomputed RFC 9162 root",
        cp3,
    )
    proof_bytes = [bytes.fromhex(h) for h in cp3.body["consistency_proof_hex"]]
    _expect(
        verify_consistency_proof(1, 3, root1_bytes, root3_bytes, proof_bytes),
        "RFC 9162 consistency proof failed verification",
        cp3,
    )

    return {
        "outcome": "ok",
        "profile": PROFILE_ID,
        "origin": canonical_origin,
        "group_id": group_id,
        "recipient_id": recipient_id,
        "tree_size": 3,
        "root_hash_hex": cp3.body["root_hash_hex"],
        "consistency_verified": True,
        "checks_passed": 11,
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Zero-dependency loopback smoke client for AgentCiv mls-relay/0.1-draft."
    )
    parser.add_argument(
        "--origin",
        required=True,
        help="Loopback relay origin URL (for example http://127.0.0.1:8788)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=DEFAULT_TIMEOUT_SECONDS,
        help=f"Per-request timeout in seconds (default: {DEFAULT_TIMEOUT_SECONDS})",
    )
    parser.add_argument(
        "--prefix",
        default=None,
        help="Optional identifier prefix for deterministic runs",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        summary = run_smoke(args.origin, timeout=args.timeout, prefix=args.prefix)
    except (ValueError, SmokeClientError) as exc:
        error_payload: dict[str, Any] = {"outcome": "error", "detail": str(exc)}
        if isinstance(exc, SmokeClientError) and exc.status is not None:
            error_payload["status"] = exc.status
        print(json.dumps(error_payload), file=sys.stderr)
        return 1

    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
