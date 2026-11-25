use std::env;
use std::path::Path;
use std::process::ExitCode;

use agentciv_host::{load_config, serve};

const USAGE: &str = "usage: agentciv-host --config CONFIG\n\
A host configuration file (--config) is required and is produced by the local room wrapper.\n\
For local room setup, see docs/LOCAL_ENCOUNTER.md in a repository checkout.";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let (flag, path) = (args.next(), args.next());
    if path.is_none() {
        match flag.as_deref() {
            Some("--help" | "-h") => {
                println!("{USAGE}\nOptions: --help, --version");
                return ExitCode::SUCCESS;
            }
            Some("--version" | "-V") => {
                println!("agentciv-host {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            _ => {}
        }
    }
    let (Some(flag), Some(path)) = (flag, path) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    if flag != "--config" || args.next().is_some() {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    let config = match load_config(Path::new(&path)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("runtime failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(serve(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
