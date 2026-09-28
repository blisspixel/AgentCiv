"""Tests for documentation checks."""

from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import TestCase

from scripts.check_docs import check_file, check_repository, markdown_files


class DocumentationChecks(TestCase):
    def test_no_markdown_files(self) -> None:
        with TemporaryDirectory() as directory:
            self.assertIn("no Markdown files", check_repository(Path(directory))[0])

    def test_valid_local_and_external_links(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "target.md").write_text("# Target\n", encoding="utf-8")
            doc = root / "index.md"
            doc.write_text(
                "[local](target.md) [web](https://example.org) [section](#topic)\n",
                encoding="utf-8",
            )
            self.assertEqual(check_file(doc), [])
            self.assertEqual(markdown_files(root), [doc, root / "target.md"])
            self.assertEqual(check_repository(root), [])

    def test_missing_link_and_formatting(self) -> None:
        with TemporaryDirectory() as directory:
            doc = Path(directory) / "index.md"
            doc.write_text("[missing](gone.md)  \ntext\u2014text", encoding="utf-8")
            issues = check_file(doc)
            self.assertEqual(len(issues), 4)
            self.assertTrue(any("missing local link" in issue for issue in issues))
            self.assertTrue(any("trailing whitespace" in issue for issue in issues))
            self.assertTrue(any("em dash" in issue for issue in issues))
            self.assertTrue(any("missing final newline" in issue for issue in issues))

    def test_url_encoded_local_path(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "two words.md").write_text("ok\n", encoding="utf-8")
            doc = root / "index.md"
            doc.write_text("[link](two%20words.md#section)\n", encoding="utf-8")
            self.assertEqual(check_file(doc), [])
