"""Lightweight stdlib checks for schemas/http-commons.openapi.yaml."""

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OPENAPI_PATH = ROOT / "schemas" / "http-commons.openapi.yaml"
PROTOCOL_PATH = ROOT / "PROTOCOL.md"
COLLABORATION_PATH = ROOT / "docs" / "COLLABORATION_PROFILE.md"
SCHEMAS_DIR = ROOT / "schemas"

REQUIRED_PATHS = (
    "/.well-known/agentciv",
    "/events",
    "/submit",
    "/collaborate",
)

CORE_ERROR_CODES = (
    "malformed_json",
    "invalid_cursor",
    "authentication_required",
    "forbidden",
    "id_conflict",
    "cursor_expired",
    "payload_too_large",
    "unsupported_media_type",
    "invalid_record",
    "unsupported_version",
    "unsupported_record_type",
    "wrong_world",
)

COLLABORATION_ERROR_CODES = ("unknown_target",)

REFERENCED_SCHEMAS = (
    "world.schema.json",
    "message.schema.json",
    "receipt.schema.json",
    "event.schema.json",
    "event-page.schema.json",
    "prefix-page.schema.json",
    "problem.schema.json",
    "collaboration-artifact.schema.json",
    "collaboration-objection.schema.json",
    "collaboration-decline.schema.json",
    "collaboration-withdrawal.schema.json",
)


class OpenApiSpecTests(unittest.TestCase):
    def setUp(self) -> None:
        self.assertTrue(OPENAPI_PATH.is_file(), f"missing {OPENAPI_PATH}")
        self.text = OPENAPI_PATH.read_text(encoding="utf-8")

    def test_openapi_header_draft_notice_and_style(self) -> None:
        self.assertTrue(self.text.startswith("openapi: 3.1.0\n"))
        self.assertIn("http-commons/0.1-draft", self.text)
        self.assertIn("PROTOCOL.md", self.text)
        self.assertIn("docs/COLLABORATION_PROFILE.md", self.text)
        self.assertIn("schemas/", self.text)
        self.assertNotIn("\t", self.text)
        self.assertNotIn("\u2014", self.text)
        self.assertNotIn("\u2013", self.text)
        self.assertTrue(self.text.endswith("\n"))
        for line_number, line in enumerate(self.text.splitlines(), start=1):
            self.assertEqual(
                line.rstrip(),
                line,
                f"trailing whitespace on line {line_number}",
            )

    def test_paths_match_protocol_and_collaboration_docs(self) -> None:
        protocol_text = PROTOCOL_PATH.read_text(encoding="utf-8")
        collaboration_text = COLLABORATION_PATH.read_text(encoding="utf-8")

        for route in REQUIRED_PATHS:
            pattern = rf"^  {re.escape(route)}:\s*$"
            self.assertRegex(
                self.text,
                re.compile(pattern, re.MULTILINE),
                f"missing OpenAPI path {route}",
            )

        self.assertIn("/.well-known/agentciv", protocol_text)
        self.assertIn("/events", protocol_text)
        self.assertIn("/submit", protocol_text)
        self.assertIn("/collaborate", collaboration_text)

    def test_problem_codes_cover_protocol_and_collaboration(self) -> None:
        protocol_text = PROTOCOL_PATH.read_text(encoding="utf-8")
        collaboration_text = COLLABORATION_PATH.read_text(encoding="utf-8")

        for code in CORE_ERROR_CODES:
            self.assertIn(code, protocol_text)
            self.assertIn(code, self.text, f"missing core error code {code}")

        for code in COLLABORATION_ERROR_CODES:
            self.assertIn(code, collaboration_text)
            self.assertIn(code, self.text, f"missing collaboration error code {code}")

    def test_referenced_normative_schemas_and_required_fields_exist(self) -> None:
        for filename in REFERENCED_SCHEMAS:
            schema_path = SCHEMAS_DIR / filename
            self.assertTrue(schema_path.is_file(), f"missing schema {filename}")
            self.assertIn(
                f"schemas/{filename}",
                self.text,
                f"OpenAPI spec should cite schemas/{filename}",
            )
            data = json.loads(schema_path.read_text(encoding="utf-8"))
            required = data.get("required", [])
            self.assertIsInstance(required, list)
            for field in required:
                self.assertIsInstance(field, str)
                self.assertIn(
                    f"- {field}\n",
                    self.text,
                    f"required field {field!r} from {filename} missing in OpenAPI spec",
                )


if __name__ == "__main__":
    unittest.main()
