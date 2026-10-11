//! Resolve a real interpreter for process fixtures, including python3-only hosts.
use std::path::PathBuf;
use std::process::Command;

pub fn interpreter() -> PathBuf {
    for name in ["python3", "python"] {
        let Ok(output) = Command::new(name)
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
        else {
            continue;
        };
        if output.status.success()
            && let Ok(value) = String::from_utf8(output.stdout)
        {
            let path = PathBuf::from(value.trim());
            // The runtime resolves its configured interpreter before use. A
            // Windows Store alias can report a path that exists but cannot be
            // canonicalized; keep looking for an installed interpreter then.
            if path.is_absolute()
                && path.is_file()
                && let Ok(resolved) = path.canonicalize()
            {
                return resolved;
            }
        }
    }
    panic!("process fixtures require Python 3.11 or newer as python3 or python");
}
