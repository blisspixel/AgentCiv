use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};

use super::*;

#[derive(Clone, Copy)]
enum Mode {
    Valid,
    BadAuth,
    BadChallenge,
    BadProblem,
    MissingNoStore,
    BadOrigin,
    Redirect,
}

fn mock_host(mode: Mode) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let discovery = format!("http://{address}/.well-known/agentciv");
    let handle = thread::spawn(move || {
        let requests = match mode {
            Mode::Valid
            | Mode::BadAuth
            | Mode::BadChallenge
            | Mode::BadProblem
            | Mode::MissingNoStore => 3,
            Mode::BadOrigin | Mode::Redirect => 1,
        };
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 2048];
            loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            let header_end = request
                .windows(4)
                .position(|part| part == b"\r\n\r\n")
                .unwrap()
                + 4;
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let body_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|length| length.parse::<usize>().ok())
                })
                .unwrap_or(0);
            while request.len() - header_end < body_length {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
            }
            let request = String::from_utf8_lossy(&request);
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            if request.starts_with("POST /submit ") {
                let submitted: Value =
                    serde_json::from_slice(&request.as_bytes()[header_end..][..body_length])
                        .unwrap();
                assert_eq!(submitted["world"], "civ:test");
                assert_eq!(submitted["type"], "message");
            }
            let (status, media_type, headers, body) = if request.starts_with("GET /.well-known/") {
                if matches!(mode, Mode::Redirect) {
                    (
                        "302 Found",
                        "text/plain",
                        "Location: https://example.invalid/\r\n",
                        String::new(),
                    )
                } else {
                    let origin = if matches!(mode, Mode::BadOrigin) {
                        "https://elsewhere.example".to_owned()
                    } else {
                        format!("http://{address}")
                    };
                    let world = json!({
                        "protocol_version": "0.1-draft",
                        "profile": "http-commons/0.1-draft",
                        "type": "world",
                        "id": "civ:test",
                        "capabilities": ["events.read", "messages.submit"],
                        "endpoints": {
                            "events": format!("{origin}/events"),
                            "submit": format!("{origin}/submit")
                        },
                        "history": {"visibility": "addressed", "retention_seconds": 86400},
                        "authentication": {"events": "bearer", "submit": "bearer"},
                        "limits": {"max_payload_bytes": 4096}
                    });
                    ("200 OK", "application/json", "", world.to_string())
                }
            } else if request.starts_with("GET /events ") && matches!(mode, Mode::BadAuth) {
                ("200 OK", "application/json", "", "{}".to_owned())
            } else {
                assert!(
                    request.starts_with("GET /events ") || request.starts_with("POST /submit ")
                );
                let status = if matches!(mode, Mode::BadProblem) {
                    200
                } else {
                    401
                };
                let problem = json!({
                    "type": "https://agentciv.io/problems/authentication-required",
                    "title": "Authentication required",
                    "status": status,
                    "code": "authentication_required"
                });
                (
                    "401 Unauthorized",
                    "application/problem+json",
                    match mode {
                        Mode::BadChallenge => {
                            "WWW-Authenticate: Basic\r\nCache-Control: no-store\r\n"
                        }
                        Mode::MissingNoStore => "WWW-Authenticate: Bearer\r\n",
                        _ => "WWW-Authenticate: Bearer\r\nCache-Control: no-store\r\n",
                    },
                    problem.to_string(),
                )
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {media_type}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (discovery, handle)
}

#[derive(Clone, Copy)]
enum AuthorizedMode {
    Valid,
    MissingNoStore,
    ChangedRetry,
    WrongEvent,
}

fn read_request(stream: &mut TcpStream) -> (String, Option<Value>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 2048];
    let header_end = loop {
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0);
        request.extend_from_slice(&buffer[..n]);
        if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
    let body_length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|length| length.parse::<usize>().ok())
        })
        .unwrap_or(0);
    while request.len() - header_end < body_length {
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0);
        request.extend_from_slice(&buffer[..n]);
    }
    let body = if body_length == 0 {
        None
    } else {
        Some(serde_json::from_slice(&request[header_end..header_end + body_length]).unwrap())
    };
    (headers, body)
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    media_type: &str,
    extra_headers: &str,
    body: &Value,
) {
    let body = body.to_string();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {media_type}\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}

fn authorized_mock_host(mode: AuthorizedMode) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let discovery = format!("http://{address}/.well-known/agentciv");
    let handle = thread::spawn(move || {
        let requests = if matches!(mode, AuthorizedMode::MissingNoStore) {
            4
        } else {
            8
        };
        let mut submitted = None;
        for step in 0..requests {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = read_request(&mut stream);
            let lower = headers.to_ascii_lowercase();
            if step < 3 {
                assert!(!lower.contains("authorization:"));
            } else {
                assert!(lower.contains("authorization: bearer test-token\r\n"));
            }
            let no_store = "Cache-Control: no-store\r\n";
            match step {
                0 => {
                    assert!(headers.starts_with("GET /.well-known/agentciv "));
                    write_response(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        "",
                        &json!({
                            "protocol_version": "0.1-draft",
                            "profile": "http-commons/0.1-draft",
                            "type": "world",
                            "id": "civ:test",
                            "capabilities": ["events.read", "messages.submit"],
                            "endpoints": {
                                "events": format!("http://{address}/events"),
                                "submit": format!("http://{address}/submit")
                            },
                            "history": {"visibility": "addressed", "retention_seconds": 86400},
                            "authentication": {"events": "bearer", "submit": "bearer"},
                            "limits": {"max_payload_bytes": 4096}
                        }),
                    );
                }
                1 | 2 => {
                    assert!(
                        headers.starts_with("GET /events ") || headers.starts_with("POST /submit ")
                    );
                    write_response(
                        &mut stream,
                        "401 Unauthorized",
                        "application/problem+json",
                        "WWW-Authenticate: Bearer\r\nCache-Control: no-store\r\n",
                        &json!({
                            "type": "https://agentciv.io/problems/authentication-required",
                            "title": "Authentication required",
                            "status": 401,
                            "code": "authentication_required"
                        }),
                    );
                }
                3 => {
                    assert!(headers.starts_with("GET /events "));
                    write_response(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        if matches!(mode, AuthorizedMode::MissingNoStore) {
                            ""
                        } else {
                            no_store
                        },
                        &json!({
                            "protocol_version": "0.1-draft",
                            "type": "event_page",
                            "world": "civ:test",
                            "events": [],
                            "next_cursor": "cursor:start",
                            "has_more": false
                        }),
                    );
                }
                4 | 5 => {
                    assert!(headers.starts_with("POST /submit "));
                    let message = body.unwrap();
                    assert_eq!(message["from"], "agent:tester");
                    if step == 4 {
                        submitted = Some(message);
                    } else {
                        assert_eq!(submitted.as_ref().unwrap(), &message);
                    }
                    write_response(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        no_store,
                        &json!({
                            "protocol_version": "0.1-draft",
                            "type": "receipt",
                            "world": "civ:test",
                            "record_id": "message:conformance-roundtrip",
                            "event_id": if step == 5 && matches!(mode, AuthorizedMode::ChangedRetry) {
                                "event:2"
                            } else {
                                "event:1"
                            },
                            "sequence": 1,
                            "status": "recorded"
                        }),
                    );
                }
                6 => {
                    assert!(headers.starts_with("POST /submit "));
                    assert_ne!(submitted.as_ref().unwrap(), &body.unwrap());
                    write_response(
                        &mut stream,
                        "409 Conflict",
                        "application/problem+json",
                        no_store,
                        &json!({
                            "type": "https://agentciv.io/problems/id-conflict",
                            "title": "ID conflict",
                            "status": 409,
                            "code": "id_conflict"
                        }),
                    );
                }
                7 => {
                    assert!(headers.starts_with("GET /events?after=cursor%3Astart "));
                    let mut message = submitted.as_ref().unwrap().clone();
                    if matches!(mode, AuthorizedMode::WrongEvent) {
                        message["conformance_probe"]["preserve"] = json!(false);
                    }
                    write_response(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        no_store,
                        &json!({
                            "protocol_version": "0.1-draft",
                            "type": "event_page",
                            "world": "civ:test",
                            "events": [{
                                "protocol_version": "0.1-draft",
                                "type": "event",
                                "id": "event:1",
                                "world": "civ:test",
                                "sequence": 1,
                                "timestamp": "2026-09-28T18:00:00Z",
                                "kind": "message.recorded",
                                "actor": "agent:tester",
                                "body": {"message": message}
                            }],
                            "next_cursor": "cursor:end",
                            "has_more": false
                        }),
                    );
                }
                _ => unreachable!(),
            }
        }
    });
    (discovery, handle)
}

#[test]
fn baseline_passes_against_mock_host() {
    let (url, host) = mock_host(Mode::Valid);
    let report = run(&url);
    host.join().unwrap();
    assert!(report.passed(), "{}", report.to_json());
    assert_eq!(report.to_json()["summary"]["passed"], 5);
}

#[test]
fn bad_auth_response_is_reported() {
    let (url, host) = mock_host(Mode::BadAuth);
    let report = run(&url);
    host.join().unwrap();
    assert!(!report.passed());
    assert_eq!(report.to_json()["summary"]["failed"], 1);
    assert_eq!(report.cases[3].id, "events.authentication");
    assert_eq!(report.cases[3].status, CaseStatus::Failed);
}

#[test]
fn missing_bearer_challenge_is_reported() {
    let (url, host) = mock_host(Mode::BadChallenge);
    let report = run(&url);
    host.join().unwrap();
    assert_eq!(report.cases[3].status, CaseStatus::Failed);
    assert!(report.cases[3].detail.contains("bearer challenge"));
}

#[test]
fn missing_no_store_on_unauthorized_response_is_reported() {
    let (url, host) = mock_host(Mode::MissingNoStore);
    let report = run(&url);
    host.join().unwrap();
    assert_eq!(report.cases[3].status, CaseStatus::Failed);
    assert!(report.cases[3].detail.contains("no-store"));
}

#[test]
fn invalid_problem_is_reported() {
    let (url, host) = mock_host(Mode::BadProblem);
    let report = run(&url);
    host.join().unwrap();
    assert_eq!(report.cases[3].status, CaseStatus::Failed);
    assert!(report.cases[3].detail.contains("schema"));
}

#[test]
fn foreign_endpoint_is_not_contacted() {
    let (url, host) = mock_host(Mode::BadOrigin);
    let report = run(&url);
    host.join().unwrap();
    assert_eq!(report.cases[2].status, CaseStatus::Failed);
    assert_eq!(report.to_json()["summary"]["skipped"], 2);
}

#[test]
fn discovery_redirect_is_not_followed() {
    let (url, host) = mock_host(Mode::Redirect);
    let report = run(&url);
    host.join().unwrap();
    assert_eq!(report.cases[1].status, CaseStatus::Failed);
}

#[test]
fn only_loopback_http_is_allowed() {
    for url in [
        "http://example.com/world",
        "file:///tmp/world",
        "https://user:secret@example.com/world",
        "https://example.com/world#fragment",
        "https://example.com/world",
    ] {
        let report = run(url);
        assert_eq!(report.cases[0].status, CaseStatus::Failed, "{url}");
        assert_eq!(report.to_json()["summary"]["skipped"], 4);
    }
}

#[test]
fn authorized_roundtrip_checks_receipt_retry_conflict_and_event() {
    let (url, host) = authorized_mock_host(AuthorizedMode::Valid);
    let report = run_authenticated(&url, "agent:tester", "test-token");
    host.join().unwrap();
    assert!(report.passed(), "{}", report.to_json());
    assert_eq!(report.to_json()["runner_scope"], "credentialed-smoke");
    assert_eq!(report.to_json()["summary"]["passed"], 10);
    assert!(!report.to_json().to_string().contains("test-token"));
}

#[test]
fn authorized_read_requires_no_store_before_mutating_world() {
    let (url, host) = authorized_mock_host(AuthorizedMode::MissingNoStore);
    let report = run_authenticated(&url, "agent:tester", "test-token");
    host.join().unwrap();
    assert_eq!(report.cases[5].id, "events.authorized");
    assert_eq!(report.cases[5].status, CaseStatus::Failed);
    assert_eq!(report.to_json()["summary"]["skipped"], 4);
}

#[test]
fn changed_retry_receipt_is_reported() {
    let (url, host) = authorized_mock_host(AuthorizedMode::ChangedRetry);
    let report = run_authenticated(&url, "agent:tester", "test-token");
    host.join().unwrap();
    assert_eq!(report.cases[7].id, "submit.retry");
    assert_eq!(report.cases[7].status, CaseStatus::Failed);
    assert_eq!(report.cases[9].status, CaseStatus::Passed);
}

#[test]
fn altered_recorded_message_is_reported() {
    let (url, host) = authorized_mock_host(AuthorizedMode::WrongEvent);
    let report = run_authenticated(&url, "agent:tester", "test-token");
    host.join().unwrap();
    assert_eq!(report.cases[9].id, "events.recorded");
    assert_eq!(report.cases[9].status, CaseStatus::Failed);
}

#[test]
fn empty_credential_is_rejected_without_contacting_a_host() {
    let report = run_authenticated("http://127.0.0.1:1", "agent:tester", "");
    assert_eq!(report.cases[0].id, "credential.input");
    assert_eq!(report.cases[0].status, CaseStatus::Failed);
}
