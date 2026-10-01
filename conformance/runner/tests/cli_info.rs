use std::process::Command;

#[test]
fn information_needs_no_discovery_or_network() {
    for flag in ["--help", "-h", "--version", "-V"] {
        let output = Command::new(env!("CARGO_BIN_EXE_agentciv-conformance"))
            .arg(flag)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("agentciv-conformance")
        );
    }
    for args in [
        vec![],
        vec!["--version", "unexpected"],
        vec!["--help", "unexpected"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_agentciv-conformance"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
