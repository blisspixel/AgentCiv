//! Reviewed website deployment configuration and bounded prebuilt packaging.
//! Configuration checks do not inspect an account, plan, usage, or invoice.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use crate::{Directory, build};

// Deliberately exact: configuration changes need explicit review here as well as
// in the source service configuration. Unknown bindings cannot silently pass.
const APPROVED_CONFIG: &str = r#"name = "agentciv"
main = "build/index.js"
compatibility_date = "2026-10-03"
workers_dev = true

[build]
command = "worker-build --release --locked"

[assets]
directory = "../../website/dist"
binding = "ASSETS"
run_worker_first = ["/", "/board", "/board/*", "/api/*", "/report", "/.well-known/agentciv"]

[[durable_objects.bindings]]
name = "BOARD"
class_name = "Bulletin"

[[migrations]]
tag = "v1"
new_sqlite_classes = ["Bulletin"]
"#;
const BUILD_SECTION: &str = "[build]\ncommand = \"worker-build --release --locked\"\n\n";
const MAX_WORKER_BYTES: usize = 64 * 1024 * 1024;

/// Reject every source configuration change until its exact reviewed allowlist is
/// updated. Only CRLF is normalized; this is not a general TOML validator or a
/// live billing certificate. Return the same configuration with `[build]` removed.
pub fn checked_prebuilt_config(source: &str) -> Result<String, String> {
    let normalized = source.replace("\r\n", "\n");
    if normalized != APPROVED_CONFIG {
        return Err(
            "bulletin configuration differs from the reviewed Free-only deployment allowlist"
                .to_owned(),
        );
    }
    Ok(normalized.replacen(BUILD_SECTION, "", 1))
}

fn bounded_worker_file(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "prebuilt Worker file could not be read")?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "prebuilt Worker file could not be read")?;
    if bytes.is_empty() || bytes.len() > limit {
        return Err("prebuilt Worker file is empty or exceeds its byte limit".to_owned());
    }
    Ok(bytes)
}

/// Package only generated public assets and the two named Worker modules. Existing
/// destinations are never overwritten. Failed filesystem writes can leave a
/// partial destination, which is not a usable package and must not be deployed.
/// The caller supplies freshly built trusted modules; a dry run separately checks
/// their module imports and deployability without rebuilding Rust source.
pub fn package(
    directory: &Directory,
    worker_directory: &Path,
    source_config: &str,
    output: &Path,
) -> Result<(), String> {
    let config = checked_prebuilt_config(source_config)?;
    let javascript = bounded_worker_file(&worker_directory.join("index.js"), MAX_WORKER_BYTES)?;
    let wasm = bounded_worker_file(&worker_directory.join("index_bg.wasm"), MAX_WORKER_BYTES)?;
    if std::str::from_utf8(&javascript).is_err()
        || !wasm.starts_with(b"\0asm\x01\0\0\0")
        || javascript.len().saturating_add(wasm.len()) > MAX_WORKER_BYTES
    {
        return Err(
            "prebuilt Worker modules are invalid or exceed the combined byte limit".to_owned(),
        );
    }
    fs::create_dir(output)
        .map_err(|_| "package output must be a fresh directory with an existing parent")?;
    let service = output.join("services/bulletin");
    fs::create_dir_all(service.join("build"))
        .map_err(|_| "package directory could not be created")?;
    build(directory, &output.join("website/dist"))?;
    for (path, bytes) in [
        (service.join("wrangler.toml"), config.as_bytes()),
        (service.join("build/index.js"), javascript.as_slice()),
        (service.join("build/index_bg.wasm"), wasm.as_slice()),
    ] {
        fs::write(path, bytes).map_err(|_| "package file could not be written")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> Directory {
        crate::parse(br#"{"schema_version":1,"updated":"2026-10-08","entries":[]}"#).unwrap()
    }

    fn worker(path: &Path) {
        fs::create_dir(path).unwrap();
        fs::write(
            path.join("index.js"),
            b"import wasm from './index_bg.wasm'; export default {};",
        )
        .unwrap();
        fs::write(path.join("index_bg.wasm"), b"\0asm\x01\0\0\0").unwrap();
    }

    #[test]
    fn source_config_is_reviewed_and_only_build_section_is_removed() {
        let source = include_str!("../../../services/bulletin/wrangler.toml");
        assert_eq!(source.replace("\r\n", "\n"), APPROVED_CONFIG);
        let prebuilt = checked_prebuilt_config(source).unwrap();
        assert!(!prebuilt.contains("[build]"));
        assert!(!prebuilt.contains("worker-build"));
        assert_eq!(prebuilt, APPROVED_CONFIG.replace(BUILD_SECTION, ""));
        assert_eq!(
            checked_prebuilt_config(&APPROVED_CONFIG.replace('\n', "\r\n")).unwrap(),
            prebuilt
        );
    }

    #[test]
    fn unknown_bindings_triggers_and_other_source_changes_fail_closed() {
        for change in [
            "\n[ai]\nbinding = \"AI\"\n",
            "\n[[r2_buckets]]\nbinding = \"BUCKET\"\nbucket_name = \"paid\"\n",
            "\n[triggers]\ncrons = [\"* * * * *\"]\n",
            "\nlogpush = true\n",
            "\nusage_model = \"standard\"\n",
            "\n[observability]\nenabled = true\n",
            "\n[[durable_objects.bindings]]\nname = \"OTHER\"\nclass_name = \"Other\"\n",
        ] {
            assert!(checked_prebuilt_config(&format!("{APPROVED_CONFIG}{change}")).is_err());
        }
        for original in [
            "2026-10-03",
            "ASSETS",
            "BOARD",
            "new_sqlite_classes",
            "worker-build --release --locked",
        ] {
            assert!(
                checked_prebuilt_config(&APPROVED_CONFIG.replace(original, "changed")).is_err()
            );
        }
    }

    #[test]
    fn package_copies_only_named_public_inputs_and_never_overwrites() {
        let temporary = tempfile::tempdir().unwrap();
        let modules = temporary.path().join("worker");
        worker(&modules);
        fs::write(modules.join(".dev.vars"), "PRIVATE_FIXTURE_SENTINEL").unwrap();
        fs::write(modules.join("unexpected.js"), "not selected").unwrap();
        let output = temporary.path().join("package");
        package(&directory(), &modules, APPROVED_CONFIG, &output).unwrap();
        let config = output.join("services/bulletin/wrangler.toml");
        assert_eq!(
            fs::read_to_string(config).unwrap(),
            checked_prebuilt_config(APPROVED_CONFIG).unwrap()
        );
        assert_eq!(
            fs::read(output.join("services/bulletin/build/index_bg.wasm")).unwrap(),
            b"\0asm\x01\0\0\0"
        );
        assert!(
            output
                .join("website/dist/.well-known/agentciv-services")
                .is_file()
        );
        assert!(!output.join("services/bulletin/build/.dev.vars").exists());
        assert!(
            !output
                .join("services/bulletin/build/unexpected.js")
                .exists()
        );
        assert!(!output.join("Cargo.toml").exists());
        assert!(package(&directory(), &modules, APPROVED_CONFIG, &output).is_err());
        assert!(output.join("website/dist/agent.json").is_file());
    }

    #[test]
    fn invalid_or_missing_modules_fail_before_creating_a_package() {
        let temporary = tempfile::tempdir().unwrap();
        let modules = temporary.path().join("worker");
        let output = temporary.path().join("package");
        assert!(package(&directory(), &modules, APPROVED_CONFIG, &output).is_err());
        worker(&modules);
        fs::write(modules.join("index_bg.wasm"), b"not wasm").unwrap();
        assert!(package(&directory(), &modules, APPROVED_CONFIG, &output).is_err());
        fs::write(modules.join("index_bg.wasm"), b"\0asm\x01\0\0\0").unwrap();
        fs::write(modules.join("index.js"), [0xff]).unwrap();
        assert!(package(&directory(), &modules, APPROVED_CONFIG, &output).is_err());
        fs::write(modules.join("index.js"), []).unwrap();
        assert!(package(&directory(), &modules, APPROVED_CONFIG, &output).is_err());
        fs::write(modules.join("index.js"), b"12345").unwrap();
        assert_eq!(
            bounded_worker_file(&modules.join("index.js"), 4).unwrap_err(),
            "prebuilt Worker file is empty or exceeds its byte limit"
        );
        assert!(bounded_worker_file(&modules.join("index.js"), 5).is_ok());
        assert!(!output.exists());
    }
}
