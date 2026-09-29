"""Loopback checks for the local exemplars.

A URL is accepted when its scheme is http or https and its host is a
loopback address or the name localhost. These helpers do not resolve DNS
and do not open a socket.
"""

from __future__ import annotations

import ipaddress
import urllib.parse


def require_loopback(url: str) -> None:
    parts = urllib.parse.urlsplit(url)
    if parts.scheme not in {"http", "https"}:
        raise ValueError("URL scheme must be http or https")
    if parts.username is not None or parts.password is not None:
        raise ValueError("URL userinfo is rejected")
    try:
        port = parts.port
    except ValueError as error:
        raise ValueError("URL port is invalid") from error
    if port is not None and (port < 1 or port > 65535):
        raise ValueError("URL port is invalid")
    host = parts.hostname
    if host is None or host.endswith("."):
        raise ValueError("URL host must be loopback")
    if host.lower() == "localhost":
        return
    try:
        address = ipaddress.ip_address(host)
    except ValueError as error:
        raise ValueError("URL host must be loopback") from error
    if isinstance(address, ipaddress.IPv6Address) and address.ipv4_mapped is not None:
        raise ValueError("URL host must be loopback")
    if not address.is_loopback:
        raise ValueError("URL host must be loopback")


def loopback_origin(url: str) -> str:
    require_loopback(url)
    parts = urllib.parse.urlsplit(url)
    if parts.path not in {"", "/"} or parts.query or parts.fragment:
        raise ValueError("loopback origin must not include a path, query, or fragment")
    return url.rstrip("/")
