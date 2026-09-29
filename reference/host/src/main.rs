use std::env;
use std::path::Path;
use std::process::ExitCode;

use agentciv_host::{load_config, serve};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let (Some(flag), Some(path)) = (args.next(), args.next()) else {
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
