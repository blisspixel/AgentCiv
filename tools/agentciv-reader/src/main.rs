use agentciv_reader::http::{Config, MAX_CONFIG_BYTES, read};
use agentciv_reader::{Error, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::process::ExitCode;

fn input(path: &str, maximum: usize) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| Error::InputUnavailable)?
        .take((maximum + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::InputUnavailable)?;
    if bytes.len() > maximum {
        return Err(Error::InputLimit);
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)
}

fn run(args: &[String]) -> Result<serde_json::Value> {
    match args {
        [command, path] if command == "read" => {
            let result = read(&Config::parse(&input(path, MAX_CONFIG_BYTES)?)?)?;
            serde_json::to_value(result).map_err(|_| Error::Output)
        }
        [command, path, query] if command == "offers" => agentciv_reader::offers::project(
            &input(path, agentciv_archive::MAX_INPUT_BYTES)?,
            query,
        ),
        [command, path] if command == "corrections" => {
            agentciv_reader::corrections::project(&input(path, agentciv_archive::MAX_INPUT_BYTES)?)
        }
        _ => Err(Error::Configuration),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        println!("agentciv-reader {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args == ["--help"] {
        println!(
            "Usage: agentciv-reader read CONFIG\n       agentciv-reader offers READ_RESULT QUERY\n       agentciv-reader corrections READ_RESULT\nRead-only loopback history. CONFIG supplies origin, token, world, optional traversal and budgets. Offers projects example activities offline from a full caller-view read. Corrections lists visible work citing an objected or superseded revision. Reading grants no copying permission."
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
