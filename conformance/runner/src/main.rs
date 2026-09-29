use std::env;
use std::process::ExitCode;

const USAGE: &str =
    "usage: agentciv-conformance --discovery URL [--principal ID] [--reader ID] [--peer ID]";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut discovery = None;
    let mut principal = None;
    let mut reader = None;
    let mut peer = None;
    let mut index = 0;
    while index < args.len() {
        let value = args.get(index + 1).map(String::as_str);
        match args[index].as_str() {
            "--discovery" => discovery = value,
            "--principal" => principal = value,
            "--reader" => reader = value,
            "--peer" => peer = value,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
        index += 2;
    }
    if discovery.is_none() || index != args.len() {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    if peer.is_some() && reader.is_none() {
        eprintln!("--peer requires --reader");
        return ExitCode::FAILURE;
    }
    let report = match (principal, reader) {
        (None, None) => agentciv_conformance::run(discovery.expect("discovery")),
        (Some(principal), None) => {
            let Ok(token) = env::var("AGENTCIV_CONFORMANCE_TOKEN") else {
                eprintln!("set AGENTCIV_CONFORMANCE_TOKEN for the credentialed smoke test");
                return ExitCode::FAILURE;
            };
            if token.is_empty() || principal.is_empty() {
                eprintln!("token and principal must be nonempty");
                return ExitCode::FAILURE;
            }
            agentciv_conformance::run_authenticated(
                discovery.expect("discovery"),
                principal,
                &token,
            )
        }
        (Some(principal), Some(reader)) => {
            let Ok(token) = env::var("AGENTCIV_CONFORMANCE_TOKEN") else {
                eprintln!("set AGENTCIV_CONFORMANCE_TOKEN for the writer");
                return ExitCode::FAILURE;
            };
            let Ok(reader_token) = env::var("AGENTCIV_CONFORMANCE_READER_TOKEN") else {
                eprintln!("set AGENTCIV_CONFORMANCE_READER_TOKEN for the read-only principal");
                return ExitCode::FAILURE;
            };
            if token.is_empty()
                || reader_token.is_empty()
                || principal.is_empty()
                || reader.is_empty()
            {
                eprintln!("tokens and principals must be nonempty");
                return ExitCode::FAILURE;
            }
            if let Some(peer) = peer {
                let Ok(peer_token) = env::var("AGENTCIV_CONFORMANCE_PEER_TOKEN") else {
                    eprintln!("set AGENTCIV_CONFORMANCE_PEER_TOKEN for the second writer");
                    return ExitCode::FAILURE;
                };
                if peer.is_empty() || peer_token.is_empty() {
                    eprintln!("tokens and principals must be nonempty");
                    return ExitCode::FAILURE;
                }
                agentciv_conformance::run_extended_with_peer(
                    discovery.expect("discovery"),
                    principal,
                    &token,
                    reader,
                    &reader_token,
                    Some((peer, &peer_token)),
                )
            } else {
                agentciv_conformance::run_extended(
                    discovery.expect("discovery"),
                    principal,
                    &token,
                    reader,
                    &reader_token,
                )
            }
        }
        (None, Some(_)) => {
            eprintln!("--reader requires --principal");
            return ExitCode::FAILURE;
        }
    };
    println!("{}", report.to_json());
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
