use agentciv_reader::http::{Config, MAX_CONFIG_BYTES, read};
use agentciv_reader::{Error, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::process::ExitCode;

fn run(args: &[String]) -> Result<agentciv_reader::ReadResult> {
    let [command, path] = args else {
        return Err(Error::Configuration);
    };
    if command != "read" {
        return Err(Error::Configuration);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| Error::InputUnavailable)?
        .take((MAX_CONFIG_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::InputUnavailable)?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(Error::InputLimit);
    }
    let raw = std::str::from_utf8(&bytes).map_err(|_| Error::InvalidUtf8)?;
    read(&Config::parse(raw)?)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        println!("agentciv-reader {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args == ["--help"] {
        println!(
            "Usage: agentciv-reader read CONFIG\nRead-only loopback history. CONFIG supplies origin, token, world, optional traversal and budgets. Reading grants no copying permission."
        );
        return ExitCode::SUCCESS;
    }
    let outcome = run(&args).and_then(|result| {
        let bytes = serde_json::to_vec(&result).map_err(|_| Error::Output)?;
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|_| Error::Output)
    });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let failure = serde_json::json!({"outcome":"failed","code":error.code()});
            eprintln!("{failure}");
            ExitCode::FAILURE
        }
    }
}
