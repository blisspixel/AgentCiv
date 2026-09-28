use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if (args.len() != 3 && args.len() != 5)
        || args[1] != "--discovery"
        || (args.len() == 5 && args[3] != "--principal")
    {
        eprintln!("usage: agentciv-conformance --discovery URL [--principal ID]");
        return ExitCode::FAILURE;
    }
    let report = if args.len() == 5 {
        let Ok(token) = env::var("AGENTCIV_CONFORMANCE_TOKEN") else {
            eprintln!("set AGENTCIV_CONFORMANCE_TOKEN for the credentialed smoke test");
            return ExitCode::FAILURE;
        };
        if token.is_empty() || args[4].is_empty() {
            eprintln!("token and principal must be nonempty");
            return ExitCode::FAILURE;
        }
        agentciv_conformance::run_authenticated(&args[2], &args[4], &token)
    } else {
        agentciv_conformance::run(&args[2])
    };
    println!("{}", report.to_json());
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
