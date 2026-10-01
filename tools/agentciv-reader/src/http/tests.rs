use super::*;
use crate::tests::{WORLD, event, page};
use serde_json::{Value, json};
use std::io::Write;
use std::net::TcpListener;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

const TOKEN: &str = "private-test-token";

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    length: Option<usize>,
    chunked: bool,
    delay: Duration,
}

impl Reply {
    fn json(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Cache-Control".into(), "no-store".into()),
            ],
            body,
            length: None,
            chunked: false,
            delay: Duration::ZERO,
        }
    }
}

struct Mock {
    origin: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Mock {
    fn new<F>(mut respond: F) -> Self
    where
        F: FnMut(usize, &str) -> Reply + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server_origin = origin.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(stream) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(_) => break,
                };
                // Winsock may inherit the listener's nonblocking mode on accepted sockets.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut raw = Vec::new();
                while raw.len() < 16_384 && !raw.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    match stream.read(&mut byte) {
                        Ok(1) => raw.push(byte[0]),
                        _ => break,
                    }
                }
                let request = String::from_utf8(raw).unwrap();
                let index = {
                    let mut requests = collected.lock().unwrap();
                    let index = requests.len();
                    requests.push(request);
                    index
                };
                let reply = respond(index, &server_origin);
                thread::sleep(reply.delay);
                let mut head = format!("HTTP/1.1 {} Test\r\nConnection: close\r\n", reply.status);
                for (key, value) in reply.headers {
                    head.push_str(&format!("{key}: {value}\r\n"));
                }
                if reply.chunked {
                    head.push_str("Transfer-Encoding: chunked\r\n");
                } else {
                    head.push_str(&format!(
                        "Content-Length: {}\r\n",
                        reply.length.unwrap_or(reply.body.len())
                    ));
                }
                head.push_str("\r\n");
                if stream.write_all(head.as_bytes()).is_ok() {
                    if reply.chunked {
                        let body = format!(
                            "{:x}\r\n{}\r\n0\r\n\r\n",
                            reply.body.len(),
                            String::from_utf8_lossy(&reply.body)
                        );
                        let _ = stream.write_all(body.as_bytes());
                    } else {
                        let _ = stream.write_all(&reply.body);
                    }
                }
            }
        });
        Self {
            origin,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn descriptor(origin: &str) -> Value {
    json!({"protocol_version":"0.1-draft","type":"world","profile":"http-commons/0.1-draft",
        "id":WORLD,"capabilities":["events.read","messages.submit"],
        "endpoints":{"events":format!("{origin}/history?format=json"),"submit":format!("{origin}/message")},
        "history":{"visibility":"members","retention_seconds":86400},
        "authentication":{"events":"bearer","submit":"bearer"},"limits":{"max_payload_bytes":16384}})
}

fn config(origin: &str) -> Config {
    Config {
        origin: origin.into(),
        token: TOKEN.into(),
        world: WORLD.into(),
        traversal: Traversal::All,
        budgets: Budgets::default(),
    }
}

#[test]
fn actual_http_pages_preserve_originals_opaque_cursor_and_advertised_query() {
    let first = event("event:first", 0, "agent:a");
    let second = event("event:last", 8, "agent:b");
    let first_copy = first.clone();
    let second_copy = second.clone();
    let mock = Mock::new(move |index, origin| {
        Reply::json(match index {
            0 => descriptor(origin).to_string().into_bytes(),
            1 => page(std::slice::from_ref(&first_copy), true, "opaque:& cursor"),
            _ => page(std::slice::from_ref(&second_copy), false, "private:last"),
        })
    });
    let result = read(&config(&mock.origin)).unwrap();
    assert_eq!(result.snapshot.records, [first, second]);
    assert_eq!(result.report.pages, 2);
    assert!(result.report.reached_end);
    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].starts_with("GET /.well-known/agentciv "));
    assert!(!requests[0].to_ascii_lowercase().contains("authorization"));
    assert!(requests[1].starts_with("GET /history?format=json "));
    assert!(requests[2].starts_with("GET /history?format=json&after=opaque%3A%26+cursor "));
    assert!(requests[1].contains(&format!("Bearer {TOKEN}")));
    assert!(result.report.response_bytes > result.snapshot.records.iter().map(String::len).sum());
    let report = serde_json::to_string(&result.report).unwrap();
    for private in [TOKEN, "127.0.0.1", "opaque", "private:last"] {
        assert!(!report.contains(private));
    }
}

#[test]
fn first_page_mode_makes_one_actual_event_request() {
    let mock = Mock::new(|index, origin| {
        Reply::json(if index == 0 {
            descriptor(origin).to_string().into_bytes()
        } else {
            page(&[event("event:first", 1, "agent:a")], true, "next")
        })
    });
    let mut operator = config(&mock.origin);
    operator.traversal = Traversal::FirstPage;
    let result = read(&operator).unwrap();
    assert!(!result.report.reached_end);
    assert_eq!(result.report.pages, 1);
    assert_eq!(mock.requests().len(), 2);
}

#[test]
fn config_rejects_invalid_inputs_before_network_and_defaults_are_partial() {
    let raw = json!({"origin":"http://127.0.0.1:1","token":TOKEN,"world":WORLD,"budgets":{"max_pages":1}});
    let parsed = Config::parse(&raw.to_string()).unwrap();
    assert_eq!(parsed.budgets.max_pages, 1);
    assert_eq!(parsed.budgets.max_events, 256);
    assert_eq!(parsed.traversal, Traversal::All);
    for (field, value) in [
        ("token", json!("")),
        ("token", json!("a\r\nsecret")),
        ("token", json!("é")),
        ("token", json!("a".repeat(4097))),
        ("world", json!("")),
        ("world", json!(TOKEN)),
        ("unknown", json!(true)),
        ("traversal", json!("inferred")),
        ("budgets", json!({"seconds":0})),
        ("budgets", json!({"arbitrary":1})),
    ] {
        let mut invalid = raw.clone();
        invalid[field] = value;
        let error = Config::parse(&invalid.to_string()).err().unwrap();
        assert_eq!(error, Error::Configuration);
        assert_eq!(error.code(), "invalid_configuration");
    }
    for input in [
        "{\"token\":1,\"\\u0074oken\":2}",
        "{\"budgets\":{\"x\":1,\"x\":2}}",
        "NaN",
    ] {
        assert_eq!(Config::parse(input).err().unwrap(), Error::InvalidJson);
    }
    assert_eq!(
        Config::parse(&"x".repeat(MAX_CONFIG_BYTES + 1))
            .err()
            .unwrap(),
        Error::InputLimit
    );
}

#[test]
fn remote_hostname_alias_auth_path_query_and_fragment_origins_are_rejected() {
    for origin in [
        "https://127.0.0.1",
        "https://example.com",
        "http://192.0.2.1",
        "http://localhost:1",
        "http://2130706433:1",
        "http://0177.0.0.1:1",
        "http://0x7f000001:1",
        "http://user:secret@127.0.0.1",
        "http://127.0.0.1/path",
        "http://127.0.0.1?query",
        "http://127.0.0.1#fragment",
        "http://[::2]",
        "http://127.0.0.1:0",
        "http://127.0.0.1:65536",
    ] {
        let error = read(&config(origin)).unwrap_err();
        assert_eq!(error, Error::InvalidOrigin);
        assert_eq!(error.code(), "invalid_origin");
    }
    assert!(config("http://[::1]:1234").validate().is_ok());
}

#[test]
fn discovery_world_and_any_advertised_cross_origin_fail_without_bearer() {
    for bad in [
        "world",
        "cross-origin",
        "userinfo",
        "fragment",
        "reserved-query",
        "missing-field",
        "version",
    ] {
        let mock = Mock::new(move |_, origin| {
            let mut value = descriptor(origin);
            match bad {
                "world" => value["id"] = json!("civ:other"),
                "cross-origin" => {
                    value["endpoints"]["optional"] = json!("http://127.0.0.1:1/elsewhere")
                }
                "userinfo" => {
                    value["endpoints"]["events"] = json!(origin.replace("http://", "http://user@"))
                }
                "fragment" => {
                    value["endpoints"]["events"] = json!(format!("{origin}/events#private"))
                }
                "reserved-query" => {
                    value["endpoints"]["events"] =
                        json!(format!("{origin}/events?after=preselected"))
                }
                "missing-field" => {
                    value.as_object_mut().unwrap().remove("history");
                }
                _ => value["protocol_version"] = json!("future"),
            }
            Reply::json(value.to_string().into_bytes())
        });
        let error = read(&config(&mock.origin)).unwrap_err();
        assert!(matches!(
            error,
            Error::InvalidDiscovery | Error::InvalidEndpoint
        ));
        assert!(!error.code().is_empty());
        let requests = mock.requests();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].contains(TOKEN));
    }
}

#[test]
fn redirects_are_not_followed_with_or_without_bearer() {
    for redirected_index in [0, 1] {
        let mock = Mock::new(move |index, origin| {
            let mut reply = Reply::json(if index == 0 {
                descriptor(origin).to_string().into_bytes()
            } else {
                page(&[], false, "last")
            });
            if index == redirected_index {
                reply.status = 302;
                reply
                    .headers
                    .push(("Location".into(), format!("{origin}/target")));
            }
            reply
        });
        let error = read(&config(&mock.origin)).unwrap_err();
        assert_eq!(error, Error::Http);
        assert_eq!(error.code(), "http_failed");
        assert_eq!(mock.requests().len(), redirected_index + 1);
    }
}

#[test]
fn changed_permission_expiry_or_invalid_credential_after_page_one_discards_view() {
    for (status, expected) in [
        (401, Error::Authentication),
        (403, Error::Forbidden),
        (410, Error::CursorExpired),
    ] {
        let mock = Mock::new(move |index, origin| {
            if index == 0 {
                return Reply::json(descriptor(origin).to_string().into_bytes());
            }
            if index == 1 {
                return Reply::json(page(
                    &[event("event:one", 1, "agent:a")],
                    true,
                    "sensitive-cursor",
                ));
            }
            let mut reply =
                Reply::json(format!("{{\"detail\":\"{TOKEN} private source\"}}").into_bytes());
            reply.status = status;
            reply
        });
        let error = read(&config(&mock.origin)).unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.code().contains(TOKEN));
        assert_eq!(mock.requests().len(), 3);
    }
}

#[test]
fn escaped_credential_reflection_in_discovery_and_event_extensions_is_rejected() {
    for reflected_index in [0, 1] {
        let mock = Mock::new(move |index, origin| {
            let mut value = if index == 0 {
                descriptor(origin)
            } else {
                serde_json::from_slice(&page(&[], false, "last")).unwrap()
            };
            if index == reflected_index {
                value["unknown"] = json!({"nested":[TOKEN]});
            }
            Reply::json(
                value
                    .to_string()
                    .replace("private-test-token", "pr\\u0069vate-test-token")
                    .into_bytes(),
            )
        });
        let error = read(&config(&mock.origin)).unwrap_err();
        assert_eq!(error, Error::CredentialReflection);
        assert_eq!(mock.requests().len(), reflected_index + 1);
    }
}

#[test]
fn content_type_and_restricted_no_store_are_enforced() {
    for failure in ["type", "cache"] {
        let mock = Mock::new(move |index, origin| {
            let mut reply = Reply::json(if index == 0 {
                descriptor(origin).to_string().into_bytes()
            } else {
                page(&[], false, "last")
            });
            if index == 1 {
                if failure == "type" {
                    reply.headers[0].1 = "text/html; charset=utf-8".into();
                } else {
                    reply.headers.retain(|(key, _)| key != "Cache-Control");
                }
            }
            reply
        });
        let error = read(&config(&mock.origin)).unwrap_err();
        assert_eq!(
            error,
            if failure == "type" {
                Error::ContentType
            } else {
                Error::CacheControl
            }
        );
        assert!(!error.code().is_empty());
    }
    let mock = Mock::new(|index, origin| {
        let mut reply = Reply::json(if index == 0 {
            descriptor(origin).to_string().into_bytes()
        } else {
            page(&[], false, "last")
        });
        reply.headers[0].1 = "Application/JSON; charset=utf-8".into();
        reply.headers[1].1 = "max-age=0, NO-STORE".into();
        reply
    });
    assert!(read(&config(&mock.origin)).unwrap().report.reached_end);
}

#[test]
fn declared_streamed_and_aggregate_oversize_stop_before_json_parse() {
    for kind in ["declared", "chunked", "aggregate"] {
        let mock = Mock::new(move |index, origin| {
            let mut reply = Reply::json(if index == 0 {
                descriptor(origin).to_string().into_bytes()
            } else {
                vec![b'x'; 2048]
            });
            if index == 1 {
                if kind == "declared" {
                    reply.length = Some(4096);
                }
                if kind == "chunked" {
                    reply.chunked = true;
                }
            }
            reply
        });
        let mut operator = config(&mock.origin);
        operator.budgets.max_response_bytes = 1024;
        if kind == "aggregate" {
            operator.budgets.max_total_bytes = 1024;
            operator.budgets.max_response_bytes = 4096;
        }
        let error = read(&operator).unwrap_err();
        assert_eq!(
            error,
            if kind == "aggregate" {
                Error::TotalLimit
            } else {
                Error::ResponseLimit
            }
        );
        assert!(!error.code().is_empty());
        assert_eq!(mock.requests().len(), 2);
    }
}

#[test]
fn interrupted_response_and_global_deadline_have_fixed_failure_without_retry() {
    for delayed in [false, true] {
        let mock = Mock::new(move |index, origin| {
            let mut reply = Reply::json(if index == 0 {
                descriptor(origin).to_string().into_bytes()
            } else {
                page(&[], false, "last")
            });
            if index == 1 {
                if delayed {
                    reply.delay = Duration::from_millis(1200);
                } else {
                    reply.length = Some(reply.body.len() + 5);
                }
            }
            reply
        });
        let mut operator = config(&mock.origin);
        operator.budgets.seconds = 1;
        let error = read(&operator).unwrap_err();
        assert_eq!(
            error,
            if delayed {
                Error::Deadline
            } else {
                Error::Transport
            }
        );
        assert!(!error.code().is_empty());
        assert_eq!(mock.requests().len(), 2);
    }
}
