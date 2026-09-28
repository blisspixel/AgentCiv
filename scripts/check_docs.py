"""Check basic repository documentation conventions and local links."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")


def markdown_files(root: Path) -> list[Path]:
    return sorted(root.rglob("*.md"))


def check_file(path: Path) -> list[str]:
    issues: list[str] = []
    content = path.read_text(encoding="utf-8")
    for number, line in enumerate(content.splitlines(), start=1):
        if line.rstrip() != line:
            issues.append(f"{path}:{number}: trailing whitespace")
        if "\u2014" in line:
            issues.append(f"{path}:{number}: em dash")
        for match in LINK.finditer(line):
            target = match.group(1).strip()
            parsed = urlsplit(target)
            if parsed.scheme or target.startswith("#"):
                continue
            local = unquote(parsed.path)
            if not local or not (path.parent / local).exists():
                issues.append(f"{path}:{number}: missing local link: {target}")
    if not content.endswith("\n"):
        issues.append(f"{path}: missing final newline")
    return issues


def check_repository(root: Path) -> list[str]:
    files = markdown_files(root)
    if not files:
        return [f"{root}: no Markdown files found"]
    return [issue for path in files for issue in check_file(path)]


def main() -> int:
    issues = check_repository(Path(__file__).resolve().parents[1])
    if issues:
        print("\n".join(issues), file=sys.stderr)
        return 1
    print("Documentation checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
