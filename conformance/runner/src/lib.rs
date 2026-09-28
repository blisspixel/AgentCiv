use std::io::Read;
use std::net::IpAddr;
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE, WWW_AUTHENTICATE};
use reqwest::{StatusCode, Url, redirect};
use serde_json::{Value, json};

const MAX_RESPONSE_BYTES: u64 = 1_048_576;
const WORLD_SCHEMA: &str = include_str!("../../../schemas/world.schema.json");
const PROBLEM_SCHEMA: &str = include_str!("../../../schemas/problem.schema.json");
const RECEIPT_SCHEMA: &str = include_str!("../../../schemas/receipt.schema.json");
const EVENT_PAGE_SCHEMA: &str = include_str!("../../../schemas/event-page.schema.json");
const EVENT_SCHEMA: &str = include_str!("../../../schemas/event.schema.json");
const MESSAGE_SCHEMA: &str = include_str!("../../../schemas/message.schema.json");
const AUTHORIZED_CASES: [&str; 5] = [
    "events.authorized",
    "submit.recorded",
    "submit.retry",
    "submit.conflict",
    "events.recorded",
];

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
    pub scope: &'static str,
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
            "runner_scope": self.scope,
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
    let mut registry = jsonschema::Registry::new();
    for source in [MESSAGE_SCHEMA, EVENT_SCHEMA, EVENT_PAGE_SCHEMA] {
        let resource: Value = serde_json::from_str(source).map_err(|error| error.to_string())?;
        let id = resource["$id"]
            .as_str()
            .ok_or("schema is missing its $id")?
            .to_owned();
        registry = registry
            .add(&id, resource)
            .map_err(|error| error.to_string())?;
    }
    let registry = registry.prepare().map_err(|error| error.to_string())?;
    let validator = jsonschema::options()
        .with_registry(&registry)
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
    if !has_no_store(&response) {
        return Err("unauthorized response is missing Cache-Control: no-store".to_owned());
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

fn has_no_store(response: &Response) -> bool {
    response
        .headers()
        .get_all(CACHE_CONTROL)
        .iter()
        .any(|value| {
            value.to_str().is_ok_and(|header| {
                header
                    .split(',')
                    .any(|directive| directive.trim().eq_ignore_ascii_case("no-store"))
            })
        })
}

fn check_page(response: Response, world_id: &str) -> Result<Value, String> {
    if response.status() != StatusCode::OK {
        return Err(format!("expected HTTP 200, got {}", response.status()));
    }
    if !has_no_store(&response) {
        return Err("restricted event response is missing Cache-Control: no-store".to_owned());
    }
    let page = read_json(response, "application/json")?;
    validate(EVENT_PAGE_SCHEMA, &page)?;
    if page["world"] != world_id {
        return Err("event page names a different world".to_owned());
    }
    Ok(page)
}

fn check_receipt(response: Response, world_id: &str, record_id: &str) -> Result<Value, String> {
    if response.status() != StatusCode::OK {
        return Err(format!("expected HTTP 200, got {}", response.status()));
    }
    if !has_no_store(&response) {
        return Err("submission response is missing Cache-Control: no-store".to_owned());
    }
    let receipt = read_json(response, "application/json")?;
    validate(RECEIPT_SCHEMA, &receipt)?;
    if receipt["world"] != world_id || receipt["record_id"] != record_id {
        return Err("receipt does not identify the submitted message".to_owned());
    }
    Ok(receipt)
}

fn check_conflict(response: Response) -> Result<(), String> {
    if response.status() != StatusCode::CONFLICT {
        return Err(format!("expected HTTP 409, got {}", response.status()));
    }
    if !has_no_store(&response) {
        return Err("submission response is missing Cache-Control: no-store".to_owned());
    }
    let problem = read_json(response, "application/problem+json")?;
    validate(PROBLEM_SCHEMA, &problem)?;
    if problem["status"] != 409 || problem["code"] != "id_conflict" {
        return Err("problem status or code does not match ID conflict".to_owned());
    }
    Ok(())
}

fn result_case(id: &'static str, result: Result<(), String>) -> Case {
    match result {
        Ok(()) => Case::passed(id),
        Err(detail) => Case::failed(id, detail),
    }
}

fn run_authorized_cases(
    client: &Client,
    world: &Value,
    events: &Url,
    submit: &Url,
    principal: &str,
    token: &str,
    cases: &mut Vec<Case>,
) {
    let world_id = world["id"].as_str().expect("validated world ID");
    let initial = client
        .get(events.clone())
        .header(ACCEPT, "application/json")
        .bearer_auth(token)
        .send()
        .map_err(|error| format!("authorized event request failed: {error}"))
        .and_then(|response| check_page(response, world_id))
        .and_then(|page| {
            if page["events"].as_array().is_some_and(Vec::is_empty) {
                Ok(page["next_cursor"]
                    .as_str()
                    .expect("validated cursor")
                    .to_owned())
            } else {
                Err("credentialed smoke test requires a fresh empty event view".to_owned())
            }
        });
    let cursor = match initial {
        Ok(cursor) => {
            cases.push(Case::passed("events.authorized"));
            cursor
        }
        Err(detail) => {
            cases.push(Case::failed("events.authorized", detail));
            cases.extend(
                AUTHORIZED_CASES[1..]
                    .iter()
                    .map(|id| Case::skipped(id, "fresh authorized event view is unavailable")),
            );
            return;
        }
    };

    let message = json!({
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": "message:conformance-roundtrip",
        "world": world_id,
        "from": principal,
        "to": [principal],
        "body": {"text": "conformance roundtrip"},
        "conformance_probe": {"preserve": true}
    });
    let bytes = message.to_string().into_bytes();
    let post = |body: Vec<u8>| {
        client
            .post(submit.clone())
            .header(CONTENT_TYPE, "application/json")
            .bearer_auth(token)
            .body(body)
            .send()
            .map_err(|error| format!("submission request failed: {error}"))
    };
    let first = post(bytes.clone())
        .and_then(|response| check_receipt(response, world_id, "message:conformance-roundtrip"));
    let receipt = match first {
        Ok(receipt) => {
            cases.push(Case::passed("submit.recorded"));
            receipt
        }
        Err(detail) => {
            cases.push(Case::failed("submit.recorded", detail));
            cases.extend(
                AUTHORIZED_CASES[2..]
                    .iter()
                    .map(|id| Case::skipped(id, "recording did not return a valid receipt")),
            );
            return;
        }
    };

    let retry = post(bytes)
        .and_then(|response| check_receipt(response, world_id, "message:conformance-roundtrip"))
        .and_then(|again| {
            if again == receipt {
                Ok(())
            } else {
                Err("byte-identical retry returned a different receipt".to_owned())
            }
        });
    cases.push(result_case("submit.retry", retry));

    let mut changed = message.clone();
    changed["body"]["text"] = json!("different bytes");
    let conflict = post(changed.to_string().into_bytes()).and_then(check_conflict);
    cases.push(result_case("submit.conflict", conflict));

    let mut next = events.clone();
    next.query_pairs_mut().append_pair("after", &cursor);
    let recorded = client
        .get(next)
        .header(ACCEPT, "application/json")
        .bearer_auth(token)
        .send()
        .map_err(|error| format!("event read after submission failed: {error}"))
        .and_then(|response| check_page(response, world_id))
        .and_then(|page| {
            let matching: Vec<_> = page["events"]
                .as_array()
                .expect("validated events")
                .iter()
                .filter(|event| event["id"] == receipt["event_id"])
                .collect();
            if matching.len() != 1 {
                return Err("receipt event was not found exactly once after the cursor".to_owned());
            }
            let event = matching[0];
            if event["world"] != world_id
                || event["sequence"] != receipt["sequence"]
                || event["kind"] != "message.recorded"
                || event["body"]["message"] != message
            {
                return Err("recorded event does not match receipt and submitted bytes".to_owned());
            }
            Ok(())
        });
    cases.push(result_case("events.recorded", recorded));
}

fn finish(mut cases: Vec<Case>, authenticated: bool, reason: &'static str) -> Report {
    if authenticated && !reason.is_empty() {
        cases.extend(AUTHORIZED_CASES.map(|id| Case::skipped(id, reason)));
    }
    Report {
        cases,
        scope: if authenticated {
            "credentialed-smoke"
        } else {
            "unauthenticated-baseline"
        },
    }
}

pub fn run(discovery_url: &str) -> Report {
    run_internal(discovery_url, None)
}

/// Exercise a fresh disposable local world with one credential bound to `principal`.
/// The token is used only in request headers and is never included in the report.
pub fn run_authenticated(discovery_url: &str, principal: &str, token: &str) -> Report {
    if principal.is_empty() || token.is_empty() {
        return Report {
            cases: vec![Case::failed(
                "credential.input",
                "principal and token must be nonempty",
            )],
            scope: "credentialed-smoke",
        };
    }
    let mut report = run_internal(discovery_url, Some((principal, token)));
    for case in &mut report.cases {
        case.detail = case.detail.replace(token, "[redacted]");
    }
    report
}

fn run_internal(discovery_url: &str, credential: Option<(&str, &str)>) -> Report {
    let mut cases = Vec::new();
    let authenticated = credential.is_some();
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
                return finish(cases, authenticated, "discovery did not succeed");
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
            return finish(cases, authenticated, "discovery did not succeed");
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
            return finish(cases, authenticated, "HTTP client could not be created");
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
            return finish(cases, authenticated, "discovery did not succeed");
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
            return finish(cases, authenticated, "endpoint is invalid");
        }
    };
    cases.push(Case::passed("endpoints.origin"));
    cases.push(auth_case(
        "events.authentication",
        client
            .get(events.clone())
            .header(ACCEPT, "application/json")
            .send(),
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
            .post(submit.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(message.to_string())
            .send(),
    ));
    if let Some((principal, token)) = credential {
        if cases.iter().all(|case| case.status == CaseStatus::Passed) {
            run_authorized_cases(
                &client, &world, &events, &submit, principal, token, &mut cases,
            );
        } else {
            cases.extend(
                AUTHORIZED_CASES.map(|id| Case::skipped(id, "unauthenticated baseline failed")),
            );
        }
    }
    finish(cases, authenticated, "")
}

#[cfg(test)]
mod tests;
