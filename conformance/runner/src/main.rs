use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || args[1] != "--discovery" {
        eprintln!("usage: agentciv-conformance --discovery URL");
        return ExitCode::FAILURE;
    }
    let report = agentciv_conformance::run(&args[2]);
    println!("{}", report.to_json());
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
