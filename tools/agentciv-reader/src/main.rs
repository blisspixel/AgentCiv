use agentciv_reader::http::{Config, MAX_CONFIG_BYTES, read, sync};
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
        [command, path] if command == "check-request" => {
            agentciv_reader::requests::check(&input(
                path,
                agentciv_reader::requests::MAX_REQUEST_BYTES,
            )?)?;
            Ok(serde_json::json!({
                "outcome": "valid", "scope": "request_syntax", "acceptance": "not_established"
            }))
        }
        [command, path] if command == "read" => {
            let result = read(&Config::parse(&input(path, MAX_CONFIG_BYTES)?)?)?;
            serde_json::to_value(result).map_err(|_| Error::Output)
        }
        [command, path, previous] if command == "sync" => {
            let result = sync(
                &Config::parse(&input(path, MAX_CONFIG_BYTES)?)?,
                &input(previous, agentciv_archive::MAX_INPUT_BYTES)?,
            )?;
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
            "Usage: agentciv-reader read CONFIG\n       agentciv-reader sync CONFIG PREVIOUS_READ_RESULT\n       agentciv-reader offers READ_RESULT QUERY\n       agentciv-reader corrections READ_RESULT\n       agentciv-reader check-request REQUEST_FILE\nRead-only loopback history and offline request syntax. CONFIG supplies origin, token, world, optional traversal, budgets and prefix_validation. Sync validates a private saved prefix and fetches new events. Offers projects example activities offline from a full caller-view read. Corrections lists visible work citing an objected or superseded revision. Check-request validates incoming syntax offline, without establishing host acceptance. Reading grants no copying permission."
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
