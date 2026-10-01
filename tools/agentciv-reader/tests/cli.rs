use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn invoke(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_agentciv-reader"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_and_version_succeed_without_network_or_private_configuration() {
    let help = invoke(&["--help"]);
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("read CONFIG")
    );
    let version = invoke(&["--version"]);
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        "agentciv-reader 0.1.0"
    );
}

#[test]
fn usage_missing_file_and_bad_input_never_emit_partial_snapshot() {
    for args in [
        vec![],
        vec!["write", "private-path"],
        vec!["read", "missing-private-file"],
    ] {
        let result = invoke(&args);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let failure: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(failure["outcome"], "failed");
        assert!(
            !String::from_utf8(result.stderr)
                .unwrap()
                .contains("private-path")
        );
    }
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("private-config.json");
    for (bytes,code) in [(vec![0xff],"invalid_utf8"),(vec![b'x';65_537],"input_too_large"),
        (b"{private-credential-and-host-secret".to_vec(),"invalid_json"),
        (json!({"origin":"https://example.com","world":"civ:private","token":"private-credential"}).to_string().into_bytes(),"invalid_origin")]
    {
        fs::write(&path,bytes).unwrap();let result=invoke(&["read",path.to_str().unwrap()]);
        assert!(!result.status.success());assert!(result.stdout.is_empty());
        let failure:Value=serde_json::from_slice(&result.stderr).unwrap();assert_eq!(failure,json!({"outcome":"failed","code":code}));
    }
}

#[test]
fn cli_reads_actual_http_ignores_proxy_environment_and_feeds_separate_archive_selection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let descriptor=json!({"protocol_version":"0.1-draft","type":"world","profile":"http-commons/0.1-draft",
        "id":"civ:cli","capabilities":["events.read","messages.submit"],
        "endpoints":{"events":format!("{origin}/events"),"submit":format!("{origin}/submit")},
        "history":{"visibility":"members","retention_seconds":86400},"authentication":{"events":"bearer","submit":"bearer"},
        "limits":{"max_payload_bytes":16384}}).to_string();
    let original = "{ \"protocol_version\":\"0.1-draft\",\"type\":\"event\",\"id\":\"event:cli\",\"world\":\"civ:cli\",\"sequence\":1e0,\"timestamp\":\"2026-09-30T00:00:00Z\",\"kind\":\"unfamiliar.recorded\",\"body\":{\"source\":\"\\u00e9\"} }";
    let page = format!(
        "{{\"protocol_version\":\"0.1-draft\",\"type\":\"event_page\",\"world\":\"civ:cli\",\"events\":[{original}],\"has_more\":false,\"next_cursor\":\"private-cursor\"}}"
    );
    let thread = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = Vec::new();
        for body in [descriptor, page] {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(_) => panic!("missing request"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") && request.len() < 16384 {
                let mut byte = [0];
                if stream.read(&mut byte).unwrap() == 0 {
                    break;
                }
                request.push(byte[0]);
            }
            requests.push(String::from_utf8(request).unwrap());
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
        }
        requests
    });
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("private-config.json");
    fs::write(
        &path,
        json!({"origin":origin,"world":"civ:cli","token":"private-token"}).to_string(),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_agentciv-reader"))
        .args(["read", path.to_str().unwrap()])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "")
        .output()
        .unwrap();
    let requests = thread.join().unwrap();
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    assert!(!requests[0].contains("private-token"));
    assert!(requests[1].contains("Bearer private-token"));
    assert!(requests.iter().all(|request| request.starts_with("GET ")));
    let output: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(output["snapshot"]["records"][0], original);
    assert_eq!(output["report"]["pages"], 1);
    assert_eq!(output["report"]["copying_permission"], "not_granted");
    let permit =
        json!({"world":"civ:cli","audience":["agent:recipient"],"event_ids":["event:cli"]});
    let bundle =
        agentciv_archive::export(&output["snapshot"].to_string(), &permit.to_string()).unwrap();
    assert_eq!(bundle.entries[0].record_utf8, original);
}
