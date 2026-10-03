use std::process::ExitCode;

fn main() -> ExitCode {
    match agentciv_directory::run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
