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
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("agentciv-host")
        );
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
    }
}
