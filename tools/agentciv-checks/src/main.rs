use std::env;
use std::path::Path;
use std::process::ExitCode;

use agentciv_checks::{check_docs, check_schemas};

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut issues = Vec::new();
    match check_docs(&root) {
        Ok(found) => issues.extend(found),
        Err(error) => {
            eprintln!("documentation check failed: {error}");
            return ExitCode::FAILURE;
        }
    }
    match check_schemas(&root) {
        Ok(found) => issues.extend(found),
        Err(error) => {
            eprintln!("schema check failed: {error}");
            return ExitCode::FAILURE;
        }
    }
    if issues.is_empty() {
        println!("Documentation and schema checks passed.");
        ExitCode::SUCCESS
    } else {
        for issue in issues {
            eprintln!("{issue}");
        }
        ExitCode::FAILURE
    }
}
