use std::fs;
use std::path::{Path, PathBuf};

use crate::CheckResult;

fn workflow_files(root: &Path) -> CheckResult<Vec<PathBuf>> {
    let dir = root.join(".github/workflows");
    let mut files = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext, "yml" | "yaml"))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn strip_yaml_comment(raw: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (index, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single
                && !in_double
                && (index == 0 || raw[..index].ends_with(char::is_whitespace)) =>
            {
                return raw[..index].trim();
            }
            _ => {}
        }
    }
    raw.trim()
}

fn unquote_yaml_scalar(raw: &str) -> &str {
    let cleaned = strip_yaml_comment(raw);
    if cleaned.len() >= 2
        && ((cleaned.starts_with('\'') && cleaned.ends_with('\''))
            || (cleaned.starts_with('"') && cleaned.ends_with('"')))
    {
        &cleaned[1..cleaned.len() - 1]
    } else {
        cleaned
    }
}

fn starts_block_scalar(value: &str) -> bool {
    let cleaned = strip_yaml_comment(value);
    matches!(cleaned, "|" | "|-" | "|+" | ">" | ">-" | ">+")
}

fn parse_key_value(trimmed: &str) -> Option<(&str, &str)> {
    let (key, rest) = trimmed.split_once(':')?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some((key.trim(), rest.trim()))
}

struct ConcurrencyBlock {
    start_line: usize,
    indent: usize,
    has_queue_max: bool,
}

fn check_concurrency_inline(
    path: &Path,
    line_number: usize,
    value: &str,
    issues: &mut Vec<String>,
) {
    let cleaned = strip_yaml_comment(value);
    if cleaned.starts_with('{') && cleaned.ends_with('}') {
        let inner = &cleaned[1..cleaned.len() - 1];
        let mut has_queue_max = false;
        for part in inner.split(',') {
            let Some((key, raw_val)) = parse_key_value(part.trim()) else {
                continue;
            };
            if key == "cancel-in-progress" {
                let val = unquote_yaml_scalar(raw_val);
                if !val.eq_ignore_ascii_case("false") {
                    issues.push(format!(
                        "{}:{line_number}: cancel-in-progress must be false, found `{val}`",
                        path.display()
                    ));
                }
            } else if key == "queue" && unquote_yaml_scalar(raw_val).eq_ignore_ascii_case("max") {
                has_queue_max = true;
            }
        }
        if !has_queue_max {
            issues.push(format!(
                "{}:{line_number}: concurrency block must set `queue: max`",
                path.display()
            ));
        }
    } else {
        issues.push(format!(
            "{}:{line_number}: concurrency block must set `queue: max`",
            path.display()
        ));
    }
}

fn check_workflow_file(path: &Path) -> CheckResult<Vec<String>> {
    let content = fs::read_to_string(path)?;
    let mut issues = Vec::new();
    let mut block_scalar_indent: Option<usize> = None;
    let mut active_concurrency: Option<ConcurrencyBlock> = None;

    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();

        if let Some(scalar_indent) = block_scalar_indent {
            if indent > scalar_indent {
                continue;
            }
            block_scalar_indent = None;
        }

        if let Some(block) = active_concurrency.as_ref()
            && indent <= block.indent
        {
            if !block.has_queue_max {
                issues.push(format!(
                    "{}:{}: concurrency block must set `queue: max`",
                    path.display(),
                    block.start_line
                ));
            }
            active_concurrency = None;
        }

        let key_line = trimmed
            .strip_prefix("- ")
            .map(str::trim_start)
            .unwrap_or(trimmed);
        let Some((key, value)) = parse_key_value(key_line) else {
            continue;
        };

        if starts_block_scalar(value) {
            block_scalar_indent = Some(indent);
            continue;
        }

        if key == "concurrency" {
            let cleaned = strip_yaml_comment(value);
            if cleaned.is_empty() {
                active_concurrency = Some(ConcurrencyBlock {
                    start_line: line_number,
                    indent,
                    has_queue_max: false,
                });
            } else {
                check_concurrency_inline(path, line_number, value, &mut issues);
            }
            continue;
        }

        if key == "cancel-in-progress" {
            let val = unquote_yaml_scalar(value);
            if !val.eq_ignore_ascii_case("false") {
                issues.push(format!(
                    "{}:{line_number}: cancel-in-progress must be false, found `{val}`",
                    path.display()
                ));
            }
            continue;
        }

        if key == "queue"
            && let Some(block) = active_concurrency.as_mut()
            && unquote_yaml_scalar(value).eq_ignore_ascii_case("max")
        {
            block.has_queue_max = true;
        }
    }

    if let Some(block) = active_concurrency
        && !block.has_queue_max
    {
        issues.push(format!(
            "{}:{}: concurrency block must set `queue: max`",
            path.display(),
            block.start_line
        ));
    }

    Ok(issues)
}

/// Check GitHub Actions workflows for the never-cancel concurrency policy.
pub fn check_workflows(root: &Path) -> CheckResult<Vec<String>> {
    let files = workflow_files(root)?;
    if files.is_empty() {
        return Ok(vec![format!(
            "{}: no workflow files found",
            root.join(".github/workflows").display()
        )]);
    }
    let mut issues = Vec::new();
    for path in files {
        issues.extend(check_workflow_file(&path)?);
    }
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_workflow(root: &Path, name: &str, body: &str) {
        let dir = root.join(".github/workflows");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn real_repository_workflows_pass() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let issues = check_workflows(&root).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn empty_workflows_directory_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
        let issues = check_workflows(dir.path()).unwrap();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("no workflow files found"));
    }

    #[test]
    fn workflow_without_concurrency_block_passes() {
        let dir = tempfile::tempdir().unwrap();
        write_workflow(
            dir.path(),
            "audit.yml",
            "name: Audit\non: workflow_dispatch\njobs:\n  run:\n    runs-on: ubuntu-24.04\n    steps:\n      - run: |\n          echo 'cancel-in-progress: true'\n          echo 'concurrency: bad'\n",
        );
        let issues = check_workflows(dir.path()).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn concurrency_with_cancel_false_and_queue_max_passes() {
        let dir = tempfile::tempdir().unwrap();
        write_workflow(
            dir.path(),
            "ci.yaml",
            "name: CI\non: push\nconcurrency:\n  group: ci-${{ github.ref }}\n  cancel-in-progress: false # never cancel\n  queue: max # FIFO queue\njobs:\n  test:\n    runs-on: ubuntu-24.04\n    steps:\n      - run: echo ok\n",
        );
        let issues = check_workflows(dir.path()).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn cancel_in_progress_true_fails() {
        let dir = tempfile::tempdir().unwrap();
        write_workflow(
            dir.path(),
            "ci.yml",
            "name: CI\nconcurrency:\n  group: ci\n  cancel-in-progress: true\n  queue: max\n",
        );
        let issues = check_workflows(dir.path()).unwrap();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("cancel-in-progress must be false"));
    }

    #[test]
    fn cancel_in_progress_expression_fails() {
        let dir = tempfile::tempdir().unwrap();
        write_workflow(
            dir.path(),
            "ci.yml",
            "name: CI\nconcurrency:\n  group: ci\n  cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}\n  queue: max\n",
        );
        let issues = check_workflows(dir.path()).unwrap();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("cancel-in-progress must be false"));
    }

    #[test]
    fn concurrency_without_queue_max_fails_at_block_end_and_eof() {
        let dir = tempfile::tempdir().unwrap();
        write_workflow(
            dir.path(),
            "ci.yml",
            "name: CI\nconcurrency:\n  group: ci\n  cancel-in-progress: false\njobs:\n  deploy:\n    runs-on: ubuntu-24.04\n    concurrency:\n      group: deploy\n      queue: single\n",
        );
        let issues = check_workflows(dir.path()).unwrap();
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(issues.iter().all(|issue| issue.contains("queue: max")));
    }

    #[test]
    fn scalar_and_inline_concurrency_forms_are_checked() {
        let dir = tempfile::tempdir().unwrap();
        write_workflow(
            dir.path(),
            "scalar.yml",
            "name: Scalar\nconcurrency: my-group\n",
        );
        write_workflow(
            dir.path(),
            "inline-bad.yml",
            "name: InlineBad\nconcurrency: { group: ci, cancel-in-progress: \"true\" }\n",
        );
        write_workflow(
            dir.path(),
            "inline-good.yml",
            "name: InlineGood\nconcurrency: { group: ci, cancel-in-progress: 'false', queue: 'max' }\n",
        );
        let issues = check_workflows(dir.path()).unwrap();
        assert_eq!(issues.len(), 3, "{issues:?}");
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("scalar.yml") && issue.contains("queue: max"))
        );
        assert!(issues.iter().any(|issue| {
            issue.contains("inline-bad.yml") && issue.contains("cancel-in-progress must be false")
        }));
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("inline-bad.yml") && issue.contains("queue: max"))
        );
    }
}
