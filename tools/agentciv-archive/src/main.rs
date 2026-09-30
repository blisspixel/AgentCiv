use agentciv_archive::{MAX_INPUT_BYTES, Result, export, inspect, validate};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{Read, Write};
use std::process::ExitCode;

fn read(path: &str) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "input_unreadable")?
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "input_unreadable")?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("input_too_large");
    }
    String::from_utf8(bytes).map_err(|_| "invalid_utf8")
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
        _ => {
            Err("usage: agentciv-archive export SNAPSHOT PERMIT | validate BUNDLE | inspect BUNDLE")
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
