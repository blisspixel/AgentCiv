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
const EXTENDED_CASES: [&str; 12] = [
    "submit.forbidden",
    "submit.unsupported_version",
    "submit.malformed_json",
    "submit.wrong_world",
    "submit.from_mismatch",
    "submit.unsupported_record_type",
    "submit.unsupported_media_type",
    "submit.payload_too_large",
    "events.invalid_cursor",
    "events.foreign_cursor",
    "events.visibility",
    "events.pagination",
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
    redact(&mut report, &[token]);
    report
}

/// Run the smoke test, then the refusal, visibility, cursor, and pagination cases.
///
/// `reader` must be a different principal with read access and no write access.
/// The world must be fresh and empty. Cursor expiry and process restart are outside
/// this scope: the profile has no public operation that changes policy or stops the host.
pub fn run_extended(
    discovery_url: &str,
    writer: &str,
    writer_token: &str,
    reader: &str,
    reader_token: &str,
) -> Report {
    if writer.is_empty()
        || writer_token.is_empty()
        || reader.is_empty()
        || reader_token.is_empty()
        || writer == reader
        || writer_token == reader_token
    {
        return Report {
            cases: vec![Case::failed(
                "credential.input",
                "writer and reader need distinct nonempty principals and tokens",
            )],
            scope: "credentialed-extended",
        };
    }
    let smoke = run_authenticated(discovery_url, writer, writer_token);
    if !smoke.passed() {
        let mut cases = smoke.cases;
        cases.extend(EXTENDED_CASES.map(|id| Case::skipped(id, "credentialed smoke did not pass")));
        let mut report = Report {
            cases,
            scope: "credentialed-extended",
        };
        redact(&mut report, &[writer_token, reader_token]);
        return report;
    }
    let mut cases = smoke.cases;
    match extended_targets(discovery_url) {
        Ok((client, world, events, submit)) => run_extended_cases(
            &client,
            &world,
            Endpoints {
                events: &events,
                submit: &submit,
            },
            Party {
                principal: writer,
                token: writer_token,
            },
            Party {
                principal: reader,
                token: reader_token,
            },
            &mut cases,
        ),
        Err(detail) => {
            cases.push(Case::failed("submit.forbidden", detail));
            cases.extend(
                EXTENDED_CASES[1..]
                    .iter()
                    .map(|id| Case::skipped(id, "extended discovery did not succeed")),
            );
        }
    }
    let mut report = Report {
        cases,
        scope: "credentialed-extended",
    };
    redact(&mut report, &[writer_token, reader_token]);
    report
}

fn redact(report: &mut Report, secrets: &[&str]) {
    for case in &mut report.cases {
        for secret in secrets {
            case.detail = case.detail.replace(secret, "[redacted]");
        }
    }
}

fn extended_targets(discovery_url: &str) -> Result<(Client, Value, Url, Url), String> {
    let discovery = parse_allowed_url(discovery_url)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|error| error.to_string())?;
    let world = discover(&client, &discovery)?;
    let events = endpoint_url(&discovery, &world, "events")?;
    let submit = endpoint_url(&discovery, &world, "submit")?;
    Ok((client, world, events, submit))
}

struct Party<'a> {
    principal: &'a str,
    token: &'a str,
}

struct Endpoints<'a> {
    events: &'a Url,
    submit: &'a Url,
}

fn run_extended_cases(
    client: &Client,
    world: &Value,
    endpoints: Endpoints<'_>,
    writer: Party<'_>,
    reader: Party<'_>,
    cases: &mut Vec<Case>,
) {
    let world_id = world["id"].as_str().expect("validated world ID");
    let events = endpoints.events;
    let submit = endpoints.submit;
    let writer_token = writer.token;
    let reader_token = reader.token;
    let writer = writer.principal;
    let reader = reader.principal;
    let message = |id: &str, from: &str, world: &str| {
        json!({
            "protocol_version": "0.1-draft",
            "type": "message",
            "id": id,
            "world": world,
            "from": from,
            "to": [from],
            "body": {"text": id}
        })
        .to_string()
        .into_bytes()
    };
    let post = |token: &str, content_type: &str, body: Vec<u8>| {
        client
            .post(submit.clone())
            .header(CONTENT_TYPE, content_type)
            .bearer_auth(token)
            .body(body)
            .send()
            .map_err(|error| format!("submission request failed: {error}"))
    };
    cases.push(result_case(
        "submit.forbidden",
        post(
            reader_token,
            "application/json",
            message("message:denied", reader, world_id),
        )
        .and_then(|response| expect_problem(response, StatusCode::FORBIDDEN, "forbidden")),
    ));
    let mut version = json!({
        "protocol_version": "9",
        "type": "message",
        "id": "message:version",
        "world": world_id,
        "from": writer,
        "to": [writer],
        "body": {}
    });
    cases.push(result_case(
        "submit.unsupported_version",
        post(
            writer_token,
            "application/json",
            version.to_string().into_bytes(),
        )
        .and_then(|response| {
            expect_problem(
                response,
                StatusCode::UNPROCESSABLE_ENTITY,
                "unsupported_version",
            )
        }),
    ));
    cases.push(result_case(
        "submit.malformed_json",
        post(writer_token, "application/json", b"{".to_vec()).and_then(|response| {
            expect_problem(response, StatusCode::BAD_REQUEST, "malformed_json")
        }),
    ));
    cases.push(result_case(
        "submit.wrong_world",
        post(
            writer_token,
            "application/json",
            message("message:elsewhere", writer, "civ:elsewhere"),
        )
        .and_then(|response| {
            expect_problem(response, StatusCode::UNPROCESSABLE_ENTITY, "wrong_world")
        }),
    ));
    cases.push(result_case(
        "submit.from_mismatch",
        post(
            writer_token,
            "application/json",
            message("message:mismatch", reader, world_id),
        )
        .and_then(|response| expect_problem(response, StatusCode::FORBIDDEN, "forbidden")),
    ));
    version["protocol_version"] = json!("0.1-draft");
    version["type"] = json!("action");
    version["id"] = json!("message:action");
    cases.push(result_case(
        "submit.unsupported_record_type",
        post(
            writer_token,
            "application/json",
            version.to_string().into_bytes(),
        )
        .and_then(|response| {
            expect_problem(
                response,
                StatusCode::UNPROCESSABLE_ENTITY,
                "unsupported_record_type",
            )
        }),
    ));
    cases.push(result_case(
        "submit.unsupported_media_type",
        post(writer_token, "text/plain", b"hello".to_vec()).and_then(|response| {
            expect_problem(
                response,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
            )
        }),
    ));
    cases.push(result_case(
        "submit.payload_too_large",
        oversized_body(world).and_then(|body| {
            post(writer_token, "application/json", body).and_then(|response| {
                expect_problem(response, StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large")
            })
        }),
    ));
    let mut unknown = events.clone();
    unknown
        .query_pairs_mut()
        .append_pair("after", "not-a-cursor");
    cases.push(result_case(
        "events.invalid_cursor",
        client
            .get(unknown)
            .header(ACCEPT, "application/json")
            .bearer_auth(writer_token)
            .send()
            .map_err(|error| format!("cursor request failed: {error}"))
            .and_then(|response| {
                expect_problem(response, StatusCode::BAD_REQUEST, "invalid_cursor")
            }),
    ));
    let writer_page = read_events(client, events, writer_token, None)
        .and_then(|page| check_page_value(page, world_id));
    let cursor = match writer_page {
        Ok(page) => page["next_cursor"]
            .as_str()
            .expect("validated cursor")
            .to_owned(),
        Err(detail) => {
            cases.push(Case::failed("events.foreign_cursor", detail));
            cases.push(Case::skipped(
                "events.visibility",
                "writer event page was unreadable",
            ));
            cases.push(Case::skipped(
                "events.pagination",
                "writer event page was unreadable",
            ));
            return;
        }
    };
    let mut foreign = events.clone();
    foreign.query_pairs_mut().append_pair("after", &cursor);
    cases.push(result_case(
        "events.foreign_cursor",
        client
            .get(foreign)
            .header(ACCEPT, "application/json")
            .bearer_auth(reader_token)
            .send()
            .map_err(|error| format!("foreign cursor request failed: {error}"))
            .and_then(|response| expect_problem(response, StatusCode::FORBIDDEN, "forbidden")),
    ));
    cases.push(result_case(
        "events.visibility",
        read_events(client, events, reader_token, None)
            .and_then(|page| check_page_value(page, world_id))
            .and_then(|page| {
                visible_to_reader(world["history"]["visibility"].as_str().unwrap_or(""), &page)
            }),
    ));
    cases.push(result_case(
        "events.pagination",
        check_pagination(client, events, submit, writer, writer_token, world_id),
    ));
}

fn expect_problem(response: Response, status: StatusCode, code: &str) -> Result<(), String> {
    if response.status() != status {
        return Err(format!(
            "expected HTTP {}, got {}",
            status.as_u16(),
            response.status()
        ));
    }
    if !has_no_store(&response) {
        return Err("response is missing Cache-Control: no-store".to_owned());
    }
    let problem = read_json(response, "application/problem+json")?;
    validate(PROBLEM_SCHEMA, &problem)?;
    if problem["status"] != u64::from(status.as_u16()) || problem["code"] != code {
        return Err(format!("problem status or code does not match {code}"));
    }
    Ok(())
}

fn oversized_body(world: &Value) -> Result<Vec<u8>, String> {
    let limit = world["limits"]["max_payload_bytes"]
        .as_u64()
        .ok_or("world omits max_payload_bytes")?;
    if !(1024..1_048_576).contains(&limit) {
        return Err(
            "published payload limit is outside the range this runner will send".to_owned(),
        );
    }
    Ok(vec![b'x'; usize::try_from(limit).unwrap_or(1024) + 1])
}

fn read_events(
    client: &Client,
    events: &Url,
    token: &str,
    after: Option<&str>,
) -> Result<Response, String> {
    let mut url = events.clone();
    if let Some(after) = after {
        url.query_pairs_mut().append_pair("after", after);
    }
    client
        .get(url)
        .header(ACCEPT, "application/json")
        .bearer_auth(token)
        .send()
        .map_err(|error| format!("event request failed: {error}"))
}

fn check_page_value(response: Response, world_id: &str) -> Result<Value, String> {
    check_page(response, world_id)
}

fn visible_to_reader(visibility: &str, page: &Value) -> Result<(), String> {
    let seen = page["events"]
        .as_array()
        .expect("validated events")
        .iter()
        .any(|event| event["body"]["message"]["id"] == "message:conformance-roundtrip");
    match visibility {
        "members" if seen => Ok(()),
        "members" => Err("reader could not see a members-visible recorded message".to_owned()),
        "addressed" | "sender_only" if !seen => Ok(()),
        "addressed" | "sender_only" => {
            Err("reader saw a message outside the advertised audience".to_owned())
        }
        _ => Err("world advertises an unknown history visibility".to_owned()),
    }
}

fn check_pagination(
    client: &Client,
    events: &Url,
    submit: &Url,
    writer: &str,
    writer_token: &str,
    world_id: &str,
) -> Result<(), String> {
    for index in 0..100 {
        let body = json!({
            "protocol_version": "0.1-draft",
            "type": "message",
            "id": format!("message:page-{index}"),
            "world": world_id,
            "from": writer,
            "to": [writer],
            "body": {"text": "page"}
        });
        let response = client
            .post(submit.clone())
            .header(CONTENT_TYPE, "application/json")
            .bearer_auth(writer_token)
            .body(body.to_string())
            .send()
            .map_err(|error| format!("page submission failed: {error}"))?;
        check_receipt(response, world_id, &format!("message:page-{index}"))?;
    }
    let first = check_page_value(read_events(client, events, writer_token, None)?, world_id)?;
    let events_first = first["events"].as_array().expect("validated events");
    if events_first.len() != 100 || first["has_more"] != true {
        return Err("first page did not contain 100 events with has_more".to_owned());
    }
    let cursor = first["next_cursor"]
        .as_str()
        .expect("validated cursor")
        .to_owned();
    if events_first.iter().any(|event| {
        event["sequence"]
            .as_i64()
            .is_some_and(|sequence| cursor == sequence.to_string())
    }) {
        return Err("cursor reveals an event sequence".to_owned());
    }
    let second = check_page_value(
        read_events(client, events, writer_token, Some(&cursor))?,
        world_id,
    )?;
    let events_second = second["events"].as_array().expect("validated events");
    if events_second.is_empty() || second["has_more"] != false {
        return Err("second page did not contain only the remaining event".to_owned());
    }
    let first_ids: Vec<_> = events_first
        .iter()
        .filter_map(|event| event["id"].as_str())
        .collect();
    if events_second.iter().any(|event| {
        event["id"]
            .as_str()
            .is_some_and(|id| first_ids.contains(&id))
    }) {
        return Err("second page repeated an event from the first".to_owned());
    }
    if first_ids.len() + events_second.len() != 101 {
        return Err("pages did not cover the 101 recorded events".to_owned());
    }
    Ok(())
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
