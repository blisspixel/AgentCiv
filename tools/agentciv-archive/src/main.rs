use agentciv_archive::{
    MAX_INPUT_BYTES, MAX_SEAL_BYTES, Result, SealError, compute_root_digest, export, inspect,
    inspect_seal_envelope, validate,
};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{Read, Write};
use std::process::ExitCode;

const USAGE: &str = "usage: agentciv-archive export SNAPSHOT PERMIT | validate BUNDLE | inspect BUNDLE | root-digest BUNDLE | inspect-seal BUNDLE SEAL | demo\nOptions: --help, --version";

fn read_bounded(path: &str, max_bytes: usize, too_large: &'static str) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "input_unreadable")?
        .take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "input_unreadable")?;
    if bytes.len() > max_bytes {
        return Err(too_large);
    }
    String::from_utf8(bytes).map_err(|_| "invalid_utf8")
}

fn read(path: &str) -> Result<String> {
    read_bounded(path, MAX_INPUT_BYTES, "input_too_large")
}

fn read_seal(path: &str) -> Result<String> {
    read_bounded(path, MAX_SEAL_BYTES, SealError::SealTooLarge.code())
}

fn run(args: &[String]) -> Result<Value> {
    match args {
        [command, snapshot, permit] if command == "export" => {
            serde_json::to_value(export(&read(snapshot)?, &read(permit)?)?)
                .map_err(|_| "serialization_failed")
        }
        [command, input] if command == "validate" => {
            let bundle = validate(&read(input)?)?;
            Ok(
                json!({"valid":true,"copy_integrity":"matched","source_authenticity":"unverified",
                "authority_transferred":false,"entries":bundle.entries.len()}),
            )
        }
        [command, input] if command == "inspect" => inspect(&read(input)?),
        [command, input] if command == "root-digest" => {
            let summary = compute_root_digest(&read(input)?).map_err(SealError::code)?;
            serde_json::to_value(summary).map_err(|_| "serialization_failed")
        }
        [command, bundle, seal] if command == "inspect-seal" => {
            let report = inspect_seal_envelope(&read(bundle)?, &read_seal(seal)?)
                .map_err(SealError::code)?;
            serde_json::to_value(report).map_err(|_| "serialization_failed")
        }
        [command] if command == "demo" => {
            let mut report = inspect(include_str!(
                "../../../conformance/fixtures/valid/archive-bundle.json"
            ))?;
            report["demo"] = json!({"source":"scripted_synthetic_fixture", "network_used":false,
                "files_written":false, "model_used":false});
            Ok(report)
        }
        _ => Err(USAGE),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let information = match args.as_slice() {
        [flag] if flag == "--help" || flag == "-h" => Some(USAGE.to_owned()),
        [flag] if flag == "--version" || flag == "-V" => {
            Some(format!("agentciv-archive {}", env!("CARGO_PKG_VERSION")))
        }
        _ => None,
    };
    if let Some(text) = information {
        return if writeln!(std::io::stdout().lock(), "{text}").is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    match run(&args).and_then(|value| {
        let output = serde_json::to_vec_pretty(&value).map_err(|_| "serialization_failed")?;
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(&output)
            .and_then(|()| stdout.write_all(b"\n"))
            .map_err(|_| "output_failed")
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => {
            eprintln!("{code}");
            ExitCode::FAILURE
        }
    }
}
