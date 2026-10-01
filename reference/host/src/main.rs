use std::env;
use std::path::Path;
use std::process::ExitCode;

use agentciv_host::{load_config, serve};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let (flag, path) = (args.next(), args.next());
    if path.is_none() {
        match flag.as_deref() {
            Some("--help" | "-h") => {
                println!("usage: agentciv-host --config CONFIG\nOptions: --help, --version");
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
        eprintln!("usage: agentciv-host --config CONFIG");
        return ExitCode::FAILURE;
    };
    if flag != "--config" || args.next().is_some() {
        eprintln!("usage: agentciv-host --config CONFIG");
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
