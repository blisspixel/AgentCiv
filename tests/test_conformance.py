"""Validate draft wire fixtures independently of any world implementation."""

import json
from pathlib import Path
from unittest import TestCase

from jsonschema import Draft202012Validator, FormatChecker
from jsonschema.exceptions import ValidationError

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = ROOT / "schemas"
FIXTURES = ROOT / "conformance" / "fixtures" / "valid"


class SchemaConformance(TestCase):
    def test_schema_and_valid_fixture(self) -> None:
        for path in sorted(SCHEMAS.glob("*.schema.json")):
            with self.subTest(schema=path.name):
                schema = json.loads(path.read_text(encoding="utf-8"))
                Draft202012Validator.check_schema(schema)
                record = json.loads(
                    (FIXTURES / path.name.replace(".schema", "")).read_text(
                        encoding="utf-8"
                    )
                )
                Draft202012Validator(schema, format_checker=FormatChecker()).validate(
                    record
                )

    def test_required_fields_and_version_are_enforced(self) -> None:
        for path in sorted(SCHEMAS.glob("*.schema.json")):
            schema = json.loads(path.read_text(encoding="utf-8"))
            record = json.loads(
                (FIXTURES / path.name.replace(".schema", "")).read_text(
                    encoding="utf-8"
                )
            )
            validator = Draft202012Validator(schema, format_checker=FormatChecker())
            with self.subTest(schema=path.name, case="version"):
                self.assertRaises(
                    ValidationError,
                    validator.validate,
                    {**record, "protocol_version": "unsupported"},
                )
            for field in schema["required"]:
                with self.subTest(schema=path.name, case=f"missing {field}"):
                    self.assertRaises(
                        ValidationError,
                        validator.validate,
                        {key: value for key, value in record.items() if key != field},
                    )

    def test_envelope_accepts_ephemeral_record_without_identity_or_world(self) -> None:
        schema = json.loads((SCHEMAS / "envelope.schema.json").read_text())
        record = json.loads((FIXTURES / "envelope.json").read_text())
        self.assertNotIn("id", record)
        self.assertNotIn("world", record)
        Draft202012Validator(schema).validate(record)

    def test_profile_records_share_the_minimum_envelope(self) -> None:
        envelope = json.loads((SCHEMAS / "envelope.schema.json").read_text())
        validator = Draft202012Validator(envelope, format_checker=FormatChecker())
        for path in sorted(FIXTURES.glob("*.json")):
            with self.subTest(record=path.name):
                validator.validate(json.loads(path.read_text(encoding="utf-8")))
