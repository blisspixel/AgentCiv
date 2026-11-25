use std::process::Command;

#[test]
fn information_needs_no_host_configuration_or_listener() {
    for flag in ["--help", "-h", "--version", "-V"] {
        let output = Command::new(env!("CARGO_BIN_EXE_agentciv-host"))
            .arg(flag)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("agentciv-host"));
        if matches!(flag, "--help" | "-h") {
            assert!(stdout.contains("--config"));
            assert!(stdout.contains("local room wrapper"));
            assert!(stdout.contains("docs/LOCAL_ENCOUNTER.md"));
        }
    }
    for args in [
        vec![],
        vec!["--version", "unexpected"],
        vec!["--help", "unexpected"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_agentciv-host"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("--config"));
        assert!(stderr.contains("local room wrapper"));
        assert!(stderr.contains("docs/LOCAL_ENCOUNTER.md"));
    }
}
