use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::net::TcpListener;
use std::process::{Command, Output};
use tempfile::TempDir;

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agentciv-reader"))
        .args(args)
        .output()
        .unwrap()
}

fn valid(output: &Output) {
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(output.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"outcome":"valid","scope":"request_syntax","acceptance":"not_established"})
    );
}

fn failed(output: &Output, code: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap(),
        json!({"outcome":"failed","code":code})
    );
}

#[test]
fn every_incoming_type_has_only_a_fixed_offline_syntax_result_and_unchanged_bytes() {
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("private-request.json");
    for raw in [
        include_str!("../../../conformance/fixtures/valid/message.json"),
        include_str!("../../../conformance/fixtures/valid/collaboration-artifact.json"),
        include_str!("../../../conformance/fixtures/valid/collaboration-objection.json"),
        include_str!("../../../conformance/fixtures/valid/collaboration-decline.json"),
        include_str!("../../../conformance/fixtures/valid/collaboration-withdrawal.json"),
    ] {
        fs::write(&path, raw).unwrap();
        let before = Sha256::digest(raw.as_bytes());
        valid(&invoke(&["check-request", path.to_str().unwrap()]));
        assert_eq!(Sha256::digest(fs::read(&path).unwrap()), before);
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
    }
    let help = invoke(&["--help"]);
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("check-request REQUEST_FILE")
    );
}

#[test]
fn invalid_private_inputs_leave_no_partial_output_and_never_echo_input_or_path() {
    failed(&invoke(&["check-request"]), "invalid_configuration");
    failed(
        &invoke(&["check-request", "PRIVATE-missing-file", "extra"]),
        "invalid_configuration",
    );
    failed(
        &invoke(&["check-request", "PRIVATE-missing-file"]),
        "input_unavailable",
    );
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("PRIVATE-sensitive-filename.json");
    let original = include_str!("../../../conformance/fixtures/valid/collaboration-objection.json");
    let mut objection: Value = serde_json::from_str(original).unwrap();
    objection["revision"] = json!("RAW-CONTROL");
    let rounded_fraction = objection
        .to_string()
        .replace("\"RAW-CONTROL\"", "1.00000000000000000001");
    for (bytes, code) in [
        (vec![0xff, 0xfe], "invalid_utf8"),
        (vec![b'x'; 16_385], "input_too_large"),
        (b"{PRIVATE-token-\x1b[31m".to_vec(), "invalid_json"),
        (
            b"{\"id\":\"PRIVATE-one\",\"\\u0069d\":\"PRIVATE-two\"}".to_vec(),
            "invalid_json",
        ),
        (
            b"{\"type\":\"PRIVATE-unknown\"}".to_vec(),
            "invalid_request",
        ),
        (rounded_fraction.into_bytes(), "invalid_request"),
    ] {
        fs::write(&path, &bytes).unwrap();
        let before = Sha256::digest(&bytes);
        failed(&invoke(&["check-request", path.to_str().unwrap()]), code);
        assert_eq!(Sha256::digest(fs::read(&path).unwrap()), before);
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
    }
    failed(
        &invoke(&["check-request", temporary.path().to_str().unwrap()]),
        "input_unavailable",
    );
}

#[test]
fn request_byte_ceiling_accepts_multibyte_input_above_the_stored_event_ceiling() {
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("private-request.json");
    let mut request: Value = serde_json::from_str(include_str!(
        "../../../conformance/fixtures/valid/message.json"
    ))
    .unwrap();
    request["body"] = json!({"text":""});
    let base = request.to_string().len();
    for size in [16_001, 16_384, 16_385] {
        let padding = size - base;
        request["body"]["text"] = json!(format!(
            "{}{}",
            "界".repeat(padding / 3),
            "x".repeat(padding % 3)
        ));
        let raw = request.to_string();
        assert_eq!(raw.len(), size);
        fs::write(&path, raw.as_bytes()).unwrap();
        let output = invoke(&["check-request", path.to_str().unwrap()]);
        if size <= 16_384 {
            valid(&output);
        } else {
            failed(&output, "input_too_large");
        }
        assert_eq!(fs::read(&path).unwrap(), raw.as_bytes());
    }
}

#[test]
fn untrusted_schema_names_and_record_links_make_no_request_or_additional_file() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!(
        "http://{}/PRIVATE-do-not-fetch",
        listener.local_addr().unwrap()
    );
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("private-request.json");
    let mut request: Value = serde_json::from_str(include_str!(
        "../../../conformance/fixtures/valid/collaboration-artifact.json"
    ))
    .unwrap();
    request["$schema"] = json!(url);
    request["$ref"] = json!(url);
    request["media_type"] = json!(url);
    request["body"] =
        json!({"url":url, "$ref":url, "command":"PRIVATE-do-not-execute", "authority":true});
    let raw = request.to_string();
    fs::write(&path, &raw).unwrap();
    valid(&invoke(&["check-request", path.to_str().unwrap()]));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read(&path).unwrap(), raw.as_bytes());
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
}

#[test]
fn failed_stdout_write_has_a_fixed_error_instead_of_reported_success() {
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("private-request.json");
    fs::write(
        &path,
        include_str!("../../../conformance/fixtures/valid/message.json"),
    )
    .unwrap();
    let sink = temporary.path().join("read-only-output.json");
    fs::write(&sink, b"unchanged").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_agentciv-reader"))
        .args(["check-request", path.to_str().unwrap()])
        .stdout(fs::File::open(&sink).unwrap())
        .output()
        .unwrap();
    failed(&output, "output_failed");
    assert_eq!(fs::read(&sink).unwrap(), b"unchanged");
}
