use std::io::Read;
use std::net::IpAddr;
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, CONTENT_TYPE, WWW_AUTHENTICATE};
use reqwest::{StatusCode, Url, redirect};
use serde_json::{Value, json};

const MAX_RESPONSE_BYTES: u64 = 1_048_576;
const WORLD_SCHEMA: &str = include_str!("../../../schemas/world.schema.json");
const PROBLEM_SCHEMA: &str = include_str!("../../../schemas/problem.schema.json");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseStatus {
    Passed,
    Failed,
    Skipped,
}

impl CaseStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug)]
pub struct Case {
    pub id: &'static str,
    pub status: CaseStatus,
    pub detail: String,
}

impl Case {
    fn passed(id: &'static str) -> Self {
        Self {
            id,
            status: CaseStatus::Passed,
            detail: String::new(),
        }
    }

    fn failed(id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            status: CaseStatus::Failed,
            detail: detail.into(),
        }
    }

    fn skipped(id: &'static str, detail: &'static str) -> Self {
        Self {
            id,
            status: CaseStatus::Skipped,
            detail: detail.to_owned(),
        }
    }
}

#[derive(Debug)]
pub struct Report {
    pub cases: Vec<Case>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.cases
            .iter()
            .all(|case| case.status == CaseStatus::Passed)
    }

    pub fn to_json(&self) -> Value {
        let counts = |status| {
            self.cases
                .iter()
                .filter(|case| case.status == status)
                .count()
        };
        json!({
            "profile": "http-commons/0.1-draft",
            "runner_scope": "unauthenticated-baseline",
            "cases": self.cases.iter().map(|case| json!({
                "id": case.id,
                "required": true,
                "status": case.status.as_str(),
                "detail": case.detail
            })).collect::<Vec<_>>(),
            "summary": {
                "passed": counts(CaseStatus::Passed),
                "failed": counts(CaseStatus::Failed),
                "skipped": counts(CaseStatus::Skipped)
            }
        })
    }
}

fn parse_allowed_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|error| format!("invalid URL: {error}"))?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("URL must not contain credentials or a fragment".to_owned());
    }
    match url.scheme() {
        "https" => Ok(url),
        "http" => {
            let host = url.host_str().ok_or("URL has no host")?;
            let ip = host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<IpAddr>()
                .map_err(|_| "HTTP is permitted only for a loopback IP address")?;
            if ip.is_loopback() {
                Ok(url)
            } else {
                Err("HTTP is permitted only for a loopback IP address".to_owned())
            }
        }
        _ => Err("URL must use HTTPS or loopback HTTP".to_owned()),
    }
}

fn read_json(response: Response, expected_type: &str) -> Result<Value, String> {
    let media_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if media_type != Some(expected_type) {
        return Err(format!("expected Content-Type: {expected_type}"));
    }
    let mut body = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| format!("could not read response: {error}"))?;
    if body.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("response exceeds 1 MiB runner limit".to_owned());
    }
    serde_json::from_slice(&body).map_err(|error| format!("invalid JSON response: {error}"))
}

fn validate(schema: &str, record: &Value) -> Result<(), String> {
    let schema: Value = serde_json::from_str(schema).map_err(|error| error.to_string())?;
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .map_err(|error| error.to_string())?;
    validator
        .validate(record)
        .map_err(|error| format!("response violates schema: {error}"))
}

fn discover(client: &Client, url: &Url) -> Result<Value, String> {
    let response = client
        .get(url.clone())
        .header(ACCEPT, "application/json")
        .send()
        .map_err(|error| format!("discovery request failed: {error}"))?;
    if response.status() != StatusCode::OK {
        return Err(format!("discovery returned HTTP {}", response.status()));
    }
    let world = read_json(response, "application/json")?;
    validate(WORLD_SCHEMA, &world)?;
    Ok(world)
}

fn endpoint_url(discovery: &Url, world: &Value, name: &str) -> Result<Url, String> {
    let raw = world["endpoints"][name]
        .as_str()
        .ok_or_else(|| format!("missing {name} endpoint"))?;
    let url = parse_allowed_url(raw)?;
    if url.origin() != discovery.origin() {
        return Err(format!("{name} endpoint has a different origin"));
    }
    Ok(url)
}

fn check_auth_response(response: Response) -> Result<(), String> {
    if response.status() != StatusCode::UNAUTHORIZED {
        return Err(format!("expected HTTP 401, got {}", response.status()));
    }
    let challenge = response
        .headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let scheme = challenge
        .split_once(' ')
        .map_or(challenge, |(scheme, _)| scheme);
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err("missing bearer challenge".to_owned());
    }
    let problem = read_json(response, "application/problem+json")?;
    validate(PROBLEM_SCHEMA, &problem)?;
    if problem["status"] != 401 || problem["code"] != "authentication_required" {
        return Err("problem status or code does not match authentication failure".to_owned());
    }
    Ok(())
}

fn auth_case(id: &'static str, response: Result<Response, reqwest::Error>) -> Case {
    match response {
        Ok(response) => match check_auth_response(response) {
            Ok(()) => Case::passed(id),
            Err(detail) => Case::failed(id, detail),
        },
        Err(error) => Case::failed(id, format!("request failed: {error}")),
    }
}

pub fn run(discovery_url: &str) -> Report {
    let mut cases = Vec::new();
    let discovery = match parse_allowed_url(discovery_url) {
        Ok(url) => {
            let loopback = url
                .host_str()
                .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
                .is_some_and(|ip| ip.is_loopback());
            if !loopback {
                cases.push(Case::failed(
                    "discovery.url",
                    "this runner version accepts only loopback IP hosts",
                ));
                cases.extend([
                    Case::skipped("discovery.record", "discovery URL is not loopback"),
                    Case::skipped("endpoints.origin", "discovery did not succeed"),
                    Case::skipped("events.authentication", "discovery did not succeed"),
                    Case::skipped("submit.authentication", "discovery did not succeed"),
                ]);
                return Report { cases };
            }
            url
        }
        Err(detail) => {
            cases.push(Case::failed("discovery.url", detail));
            cases.extend([
                Case::skipped("discovery.record", "discovery URL is invalid"),
                Case::skipped("endpoints.origin", "discovery did not succeed"),
                Case::skipped("events.authentication", "discovery did not succeed"),
                Case::skipped("submit.authentication", "discovery did not succeed"),
            ]);
            return Report { cases };
        }
    };
    cases.push(Case::passed("discovery.url"));
    let client = match Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(redirect::Policy::none())
        .no_proxy()
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            cases.push(Case::failed("discovery.record", error.to_string()));
            cases.extend([
                Case::skipped("endpoints.origin", "HTTP client could not be created"),
                Case::skipped("events.authentication", "HTTP client could not be created"),
                Case::skipped("submit.authentication", "HTTP client could not be created"),
            ]);
            return Report { cases };
        }
    };
    let world = match discover(&client, &discovery) {
        Ok(world) => world,
        Err(detail) => {
            cases.push(Case::failed("discovery.record", detail));
            cases.extend([
                Case::skipped("endpoints.origin", "discovery did not succeed"),
                Case::skipped("events.authentication", "discovery did not succeed"),
                Case::skipped("submit.authentication", "discovery did not succeed"),
            ]);
            return Report { cases };
        }
    };
    cases.push(Case::passed("discovery.record"));
    let endpoints = endpoint_url(&discovery, &world, "events").and_then(|events| {
        endpoint_url(&discovery, &world, "submit").map(|submit| (events, submit))
    });
    let (events, submit) = match endpoints {
        Ok(endpoints) => endpoints,
        Err(detail) => {
            cases.push(Case::failed("endpoints.origin", detail));
            cases.extend([
                Case::skipped("events.authentication", "endpoint is invalid"),
                Case::skipped("submit.authentication", "endpoint is invalid"),
            ]);
            return Report { cases };
        }
    };
    cases.push(Case::passed("endpoints.origin"));
    cases.push(auth_case(
        "events.authentication",
        client.get(events).header(ACCEPT, "application/json").send(),
    ));
    let message = json!({
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": "conformance:unauthenticated",
        "world": world["id"],
        "from": "agent:untrusted",
        "to": ["agent:untrusted"],
        "body": {}
    });
    cases.push(auth_case(
        "submit.authentication",
        client
            .post(submit)
            .header(CONTENT_TYPE, "application/json")
            .body(message.to_string())
            .send(),
    ));
    Report { cases }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    use super::*;

    #[derive(Clone, Copy)]
    enum Mode {
        Valid,
        BadAuth,
        BadChallenge,
        BadProblem,
        BadOrigin,
        Redirect,
    }

    fn mock_host(mode: Mode) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let discovery = format!("http://{address}/.well-known/agentciv");
        let handle = thread::spawn(move || {
            let requests = match mode {
                Mode::Valid | Mode::BadAuth | Mode::BadChallenge | Mode::BadProblem => 3,
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
                let (status, media_type, headers, body) = if request
                    .starts_with("GET /.well-known/")
                {
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
                        if matches!(mode, Mode::BadChallenge) {
                            "WWW-Authenticate: Basic\r\n"
                        } else {
                            "WWW-Authenticate: Bearer\r\n"
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
}
