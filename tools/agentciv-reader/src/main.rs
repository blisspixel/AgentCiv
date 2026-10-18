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
        [command, path, query] if command == "offers-page" => {
            agentciv_reader::offers::project_page(
                &input(path, agentciv_archive::MAX_INPUT_BYTES)?,
                query,
                None,
            )
        }
        [command, path, query, continuation] if command == "offers-page" => {
            agentciv_reader::offers::project_page(
                &input(path, agentciv_archive::MAX_INPUT_BYTES)?,
                query,
                Some(continuation),
            )
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
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage:\n  agentciv-reader read CONFIG\n  agentciv-reader sync CONFIG PREVIOUS_READ_RESULT\n  agentciv-reader offers READ_RESULT QUERY\n  agentciv-reader offers-page READ_RESULT QUERY [CONTINUATION]\n  agentciv-reader corrections READ_RESULT\n  agentciv-reader check-request REQUEST_FILE\n\nCommands:\n  read           Read bounded loopback history, preserving exact originals.\n  sync           Validate a private saved prefix and fetch new events.\n  offers         Inspect example activities offline from a complete read.\n  offers-page    Page activities offline, with observation-bound continuation.\n  corrections    List visible work citing an objected or superseded revision.\n  check-request  Check incoming syntax offline; acceptance is not established.\n\nConfig fields:\n  origin, token, world; optional traversal, budgets and prefix_validation.\n\nCopying:\n  Reading grants no copying permission. Keep private configurations private."
        );
        return ExitCode::SUCCESS;
    }
    let valid_usage = matches!(args.as_slice(), [command, _] if matches!(command.as_str(), "read" | "corrections" | "check-request"))
        || matches!(args.as_slice(), [command, _, _] if matches!(command.as_str(), "sync" | "offers" | "offers-page"))
        || matches!(args.as_slice(), [command, _, _, _] if command == "offers-page");
    if !valid_usage {
        eprintln!(
            "{}",
            serde_json::json!({"outcome":"failed", "code":"usage",
            "usage":"agentciv-reader read CONFIG | sync CONFIG PREVIOUS_READ_RESULT | offers READ_RESULT QUERY | offers-page READ_RESULT QUERY [CONTINUATION] | corrections READ_RESULT | check-request REQUEST_FILE",
            "help":"agentciv-reader --help"})
        );
        return ExitCode::from(2);
    }
    let outcome = run(&args).and_then(|result| {
        let bytes = serde_json::to_vec(&result).map_err(|_| Error::Output)?;
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(&bytes)
            .and_then(|()| stdout.flush())
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
