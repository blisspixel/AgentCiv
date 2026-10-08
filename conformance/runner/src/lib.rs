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
const EXTENDED_CASES: [&str; 17] = [
    "submit.forbidden",
    "submit.unsupported_version",
    "submit.malformed_json",
    "submit.wrong_world",
    "submit.from_mismatch",
    "submit.unsupported_record_type",
    "submit.unsupported_media_type",
    "submit.payload_too_large",
    "submit.json_charset",
    "events.invalid_cursor",
    "events.empty_cursor",
    "events.foreign_cursor",
    "events.visibility",
    "events.pagination",
    "submit.concurrent_retry",
    "submit.concurrent_conflict",
    "submit.concurrent_distinct",
];
const COLLABORATION_CASES: [&str; 31] = [
    "collaborate.client_revision",
    "collaborate.forbidden",
    "collaborate.unauthenticated",
    "collaborate.payload_too_large",
    "collaborate.unsupported_media_type",
    "collaborate.malformed_json",
    "collaborate.unsupported_version",
    "collaborate.unsupported_record_type",
    "collaborate.partial_note",
    "collaborate.wrong_world",
    "collaborate.from_mismatch",
    "collaborate.revision",
    "collaborate.retry",
    "collaborate.json_charset",
    "collaborate.conflict",
    "collaborate.objection",
    "collaborate.decline",
    "collaborate.absent_revision",
    "collaborate.withdrawal_forbidden",
    "collaborate.withdrawal",
    "collaborate.withdrawn_citation",
    "collaborate.unknown_target",
    "collaborate.other_chain",
    "collaborate.hidden_visibility",
    "collaborate.hidden_derivation",
    "collaborate.hidden_objection",
    "collaborate.hidden_decline",
    "collaborate.hidden_withdrawal",
    "collaborate.revision_sequence",
    "collaborate.revision_sequence_retry",
    "collaborate.revision_sequence_rejection",
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
    pub required: bool,
}

impl Case {
    fn passed(id: &'static str) -> Self {
        Self {
            id,
            status: CaseStatus::Passed,
            detail: String::new(),
            required: true,
        }
    }

    fn failed(id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            status: CaseStatus::Failed,
            detail: detail.into(),
            required: true,
        }
    }

    fn skipped(id: &'static str, detail: &'static str) -> Self {
        Self {
            id,
            status: CaseStatus::Skipped,
            detail: detail.to_owned(),
            required: true,
        }
    }

    fn skipped_optional(id: &'static str, detail: &'static str) -> Self {
        Self {
            id,
            status: CaseStatus::Skipped,
            detail: detail.to_owned(),
            required: false,
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
        self.cases.iter().all(|case| match case.status {
            CaseStatus::Passed => true,
            CaseStatus::Failed => false,
            CaseStatus::Skipped => !case.required,
        })
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
                "required": case.required,
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
        return Err("receipt does not identify the submitted record".to_owned());
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

fn roundtrip_message(world_id: &str, principal: &str) -> Value {
    json!({
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": "message:conformance-roundtrip",
        "world": world_id,
        "from": principal,
        "to": [principal],
        "body": {"text": "conformance roundtrip"},
        "conformance_probe": {"preserve": true}
    })
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

    let message = roundtrip_message(world_id, principal);
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
/// Collaboration cases are included by [`run_extended_with_peer`].
pub fn run_extended(
    discovery_url: &str,
    writer: &str,
    writer_token: &str,
    reader: &str,
    reader_token: &str,
) -> Report {
    run_extended_with_peer(
        discovery_url,
        writer,
        writer_token,
        reader,
        reader_token,
        None,
    )
}

/// Run [`run_extended`] and, when discovery advertises `collaboration.submit`,
/// the collaboration cases.
///
/// `peer`, when supplied, must be a third principal with its own token. Those
/// cases that need a second writer fail when the capability is advertised and
/// `peer` is absent. A host that does not advertise the capability skips the
/// collaboration cases, and a missing peer does not fail that report.
pub fn run_extended_with_peer(
    discovery_url: &str,
    writer: &str,
    writer_token: &str,
    reader: &str,
    reader_token: &str,
    peer: Option<(&str, &str)>,
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
    if let Some((peer_principal, peer_token)) = peer
        && (peer_principal.is_empty()
            || peer_token.is_empty()
            || peer_principal == writer
            || peer_principal == reader
            || peer_token == writer_token
            || peer_token == reader_token)
    {
        return Report {
            cases: vec![Case::failed(
                "credential.input",
                "peer needs a nonempty principal and token, distinct from the writer and the reader",
            )],
            scope: "credentialed-extended",
        };
    }
    let secrets = secret_tokens(writer_token, reader_token, peer);
    let smoke = run_authenticated(discovery_url, writer, writer_token);
    if !smoke.passed() {
        let mut cases = smoke.cases;
        cases.extend(EXTENDED_CASES.map(|id| Case::skipped(id, "credentialed smoke did not pass")));
        let mut report = Report {
            cases,
            scope: "credentialed-extended",
        };
        redact(&mut report, &secrets);
        return report;
    }
    let mut cases = smoke.cases;
    match extended_targets(discovery_url) {
        Ok((client, world, events, submit, discovery)) => run_extended_cases(
            &client,
            &world,
            Endpoints {
                events: &events,
                submit: &submit,
                discovery: &discovery,
                peer: peer.map(|(principal, token)| Party { principal, token }),
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
    redact(&mut report, &secrets);
    report
}

fn secret_tokens<'a>(
    writer_token: &'a str,
    reader_token: &'a str,
    peer: Option<(&str, &'a str)>,
) -> Vec<&'a str> {
    let mut secrets = vec![writer_token, reader_token];
    if let Some((_, token)) = peer {
        secrets.push(token);
    }
    secrets
}

fn redact(report: &mut Report, secrets: &[&str]) {
    for case in &mut report.cases {
        for secret in secrets {
            case.detail = case.detail.replace(secret, "[redacted]");
        }
    }
}

fn extended_targets(discovery_url: &str) -> Result<(Client, Value, Url, Url, Url), String> {
    let discovery = parse_allowed_url(discovery_url)?;
    if !discovery
        .host_str()
        .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback())
    {
        return Err("this runner version accepts only loopback IP hosts".to_owned());
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|error| error.to_string())?;
    let world = discover(&client, &discovery)?;
    let events = endpoint_url(&discovery, &world, "events")?;
    let submit = endpoint_url(&discovery, &world, "submit")?;
    Ok((client, world, events, submit, discovery))
}

#[derive(Clone, Copy)]
struct Party<'a> {
    principal: &'a str,
    token: &'a str,
}

struct Endpoints<'a> {
    events: &'a Url,
    submit: &'a Url,
    discovery: &'a Url,
    peer: Option<Party<'a>>,
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
    let charset_message = roundtrip_message(world_id, writer);
    cases.push(result_case(
        "submit.json_charset",
        post(
            writer_token,
            "application/json; charset=utf-8",
            charset_message.to_string().into_bytes(),
        )
        .and_then(|response| check_receipt(response, world_id, "message:conformance-roundtrip"))
        .and_then(|receipt| {
            let page =
                check_page_value(read_events(client, events, writer_token, None)?, world_id)?;
            confirm_charset_retry(&page, &receipt, &charset_message)
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
    let mut empty_cursor = events.clone();
    empty_cursor.query_pairs_mut().append_pair("after", "");
    cases.push(result_case(
        "events.empty_cursor",
        client
            .get(empty_cursor)
            .header(ACCEPT, "application/json")
            .bearer_auth(writer_token)
            .send()
            .map_err(|error| format!("empty cursor request failed: {error}"))
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
            skip_ids(
                cases,
                &EXTENDED_CASES[14..],
                "writer event page was unreadable",
            );
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
    adversarial::run_concurrency(
        client,
        events,
        submit,
        world_id,
        Party {
            principal: writer,
            token: writer_token,
        },
        cases,
    );
    run_collaboration_cases(
        &CollaborationInput {
            client,
            world,
            events,
            discovery: endpoints.discovery,
            writer: Party {
                principal: writer,
                token: writer_token,
            },
            reader: Party {
                principal: reader,
                token: reader_token,
            },
            peer: endpoints.peer,
        },
        cases,
    );
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

fn confirm_charset_retry(page: &Value, receipt: &Value, message: &Value) -> Result<(), String> {
    let events = page["events"].as_array().expect("validated events");
    let mut found = None;
    let mut count = 0_usize;
    for event in events {
        if event["body"]["message"]["id"] == "message:conformance-roundtrip" {
            count += 1;
            found = Some(event);
        }
    }
    if count != 1 {
        return Err(format!(
            "charset retry left {count} recorded roundtrip events"
        ));
    }
    let event = found.expect("one roundtrip event");
    if event["id"] != receipt["event_id"]
        || event["sequence"] != receipt["sequence"]
        || event["body"]["message"] != *message
    {
        return Err("charset retry receipt does not match the recorded message".to_owned());
    }
    Ok(())
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

const NO_COLLABORATION: &str = "host does not advertise collaboration.submit";
const NO_REVISION: &str = "revision was not recorded";

struct CollaborationInput<'a> {
    client: &'a Client,
    world: &'a Value,
    events: &'a Url,
    discovery: &'a Url,
    writer: Party<'a>,
    reader: Party<'a>,
    peer: Option<Party<'a>>,
}

struct CollaborationRun<'a> {
    client: &'a Client,
    world_id: &'a str,
    visibility: &'a str,
    events: &'a Url,
    collaborate: Url,
    writer: Party<'a>,
    reader: Party<'a>,
    peer: Option<Party<'a>>,
    audience: Vec<String>,
}

struct RecordedRevision {
    record: Value,
    bytes: Vec<u8>,
    receipt: Value,
    event_id: String,
    timestamp: String,
}

fn collaboration_advertised(world: &Value) -> bool {
    world
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(|item| item == "collaboration.submit"))
}

fn collaboration_auth_is_bearer(world: &Value) -> bool {
    match world
        .get("authentication")
        .and_then(|authentication| authentication.get("collaborate"))
    {
        None => true,
        Some(Value::String(method)) => method.eq_ignore_ascii_case("bearer"),
        Some(_) => false,
    }
}

/// The fixture puts the reader in `to`, so `addressed` reveals the revision
/// and `sender_only` does not.
fn addressed_reader_can_see(visibility: &str) -> Result<bool, String> {
    match visibility {
        "members" | "addressed" => Ok(true),
        "sender_only" => Ok(false),
        _ => Err("world advertises an unknown history visibility".to_owned()),
    }
}

/// A peer who cannot see the revision gets `unknown_target` before `forbidden`.
fn foreign_withdrawal_status(visibility: &str) -> Result<(&'static str, StatusCode), String> {
    match visibility {
        "members" | "addressed" => Ok(("forbidden", StatusCode::FORBIDDEN)),
        "sender_only" => Ok(("unknown_target", StatusCode::UNPROCESSABLE_ENTITY)),
        _ => Err("world advertises an unknown history visibility".to_owned()),
    }
}

fn skip_ids(cases: &mut Vec<Case>, ids: &[&'static str], detail: &'static str) {
    for id in ids {
        cases.push(Case::skipped(id, detail));
    }
}

fn run_collaboration_cases(input: &CollaborationInput<'_>, cases: &mut Vec<Case>) {
    if !collaboration_advertised(input.world) {
        for id in COLLABORATION_CASES {
            cases.push(Case::skipped_optional(id, NO_COLLABORATION));
        }
        return;
    }
    if !collaboration_auth_is_bearer(input.world) {
        cases.push(Case::failed(
            "collaborate.client_revision",
            "collaboration authentication is not bearer",
        ));
        skip_ids(
            cases,
            &COLLABORATION_CASES[1..],
            "collaboration authentication is not bearer",
        );
        return;
    }
    let collaborate = match endpoint_url(input.discovery, input.world, "collaborate") {
        Ok(url) => url,
        Err(detail) => {
            cases.push(Case::failed("collaborate.client_revision", detail));
            skip_ids(
                cases,
                &COLLABORATION_CASES[1..],
                "collaboration endpoint is not usable",
            );
            return;
        }
    };
    let world_id = input.world["id"].as_str().expect("validated world ID");
    let visibility = input.world["history"]["visibility"].as_str().unwrap_or("");
    let mut audience = vec![input.reader.principal.to_owned()];
    if let Some(peer) = input.peer {
        audience.push(peer.principal.to_owned());
    }
    let ctx = CollaborationRun {
        client: input.client,
        world_id,
        visibility,
        events: input.events,
        collaborate,
        writer: input.writer,
        reader: input.reader,
        peer: input.peer,
        audience,
    };
    adversarial::run_hidden_cases(&ctx, cases);
    cases.push(result_case(
        "collaborate.client_revision",
        client_revision(&ctx),
    ));
    cases.push(result_case(
        "collaborate.forbidden",
        denied_collaboration(&ctx),
    ));
    push_collaboration_rejections(&ctx, input.world, cases);
    revisions::run(&ctx, cases);
    let recorded = match recorded_revision(&ctx) {
        Ok(recorded) => {
            cases.push(Case::passed("collaborate.revision"));
            Some(recorded)
        }
        Err(detail) => {
            cases.push(Case::failed("collaborate.revision", detail));
            None
        }
    };
    let Some(recorded) = recorded else {
        skip_ids(
            cases,
            &[
                "collaborate.retry",
                "collaborate.json_charset",
                "collaborate.conflict",
                "collaborate.objection",
                "collaborate.decline",
                "collaborate.absent_revision",
                "collaborate.withdrawal_forbidden",
                "collaborate.withdrawal",
                "collaborate.withdrawn_citation",
            ],
            NO_REVISION,
        );
        cases.push(result_case(
            "collaborate.unknown_target",
            unknown_target(&ctx),
        ));
        cases.push(Case::skipped("collaborate.other_chain", NO_REVISION));
        return;
    };
    cases.push(result_case(
        "collaborate.retry",
        retry_revision(&ctx, &recorded),
    ));
    cases.push(result_case(
        "collaborate.json_charset",
        charset_revision(&ctx, &recorded),
    ));
    cases.push(result_case(
        "collaborate.conflict",
        conflict_revision(&ctx, &recorded),
    ));
    cases.push(result_case(
        "collaborate.objection",
        speech(
            &ctx,
            &recorded,
            "objection",
            "submission:conformance-objection",
            "The pages should stay separable.",
        ),
    ));
    cases.push(result_case(
        "collaborate.decline",
        speech(
            &ctx,
            &recorded,
            "decline",
            "submission:conformance-decline",
            "I will not take up this revision.",
        ),
    ));
    cases.push(result_case(
        "collaborate.absent_revision",
        absent_revision(&ctx, &recorded),
    ));
    cases.push(result_case(
        "collaborate.withdrawal_forbidden",
        foreign_withdrawal(&ctx, &recorded),
    ));
    cases.push(result_case(
        "collaborate.withdrawal",
        author_withdrawal(&ctx, &recorded),
    ));
    cases.push(result_case(
        "collaborate.withdrawn_citation",
        withdrawn_citation(&ctx, &recorded),
    ));
    cases.push(result_case(
        "collaborate.unknown_target",
        unknown_target(&ctx),
    ));
    cases.push(result_case(
        "collaborate.other_chain",
        other_chain(&ctx, &recorded),
    ));
}

fn artifact_submission(
    world_id: &str,
    principal: &str,
    audience: &[String],
    id: &str,
    artifact_id: &str,
    text: &str,
) -> Value {
    json!({
        "protocol_version": "0.1-draft",
        "type": "artifact_revision",
        "id": id,
        "artifact_id": artifact_id,
        "world": world_id,
        "from": principal,
        "to": audience,
        "media_type": "application/json",
        "body": {"text": text}
    })
}

fn post_collaboration(
    ctx: &CollaborationRun<'_>,
    token: &str,
    body: &[u8],
) -> Result<Response, String> {
    send_collaboration(ctx, Some(token), "application/json", body)
}

fn send_collaboration(
    ctx: &CollaborationRun<'_>,
    token: Option<&str>,
    content_type: &str,
    body: &[u8],
) -> Result<Response, String> {
    let mut request = ctx
        .client
        .post(ctx.collaborate.clone())
        .header(CONTENT_TYPE, content_type);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    request
        .body(body.to_vec())
        .send()
        .map_err(|error| format!("collaboration request failed: {error}"))
}

fn read_history(ctx: &CollaborationRun<'_>, token: &str) -> Result<Vec<Value>, String> {
    read_complete_history(ctx.client, ctx.events, ctx.world_id, token)
}

fn read_complete_history(
    client: &Client,
    events: &Url,
    world_id: &str,
    token: &str,
) -> Result<Vec<Value>, String> {
    let mut after = None;
    let mut seen_cursors = Vec::new();
    let mut collected = Vec::new();
    let mut event_ids = std::collections::HashSet::new();
    let mut last_sequence = None;
    for _ in 0..8 {
        let page = check_page_value(
            read_events(client, events, token, after.as_deref())?,
            world_id,
        )?;
        let batch = page["events"].as_array().expect("validated events");
        for event in batch {
            let sequence = event["sequence"].as_u64().ok_or("event sequence missing")?;
            if last_sequence.is_some_and(|previous| sequence <= previous)
                || !event_ids.insert(event["id"].to_string())
            {
                return Err(
                    "event history repeats an id or does not increase in sequence".to_owned(),
                );
            }
            last_sequence = Some(sequence);
        }
        collected.extend(batch.iter().cloned());
        if page["has_more"] != true {
            return Ok(collected);
        }
        let cursor = page["next_cursor"]
            .as_str()
            .expect("validated cursor")
            .to_owned();
        if seen_cursors.contains(&cursor) {
            return Err("event cursor did not advance".to_owned());
        }
        seen_cursors.push(cursor.clone());
        after = Some(cursor);
    }
    Err("event history exceeded the runner page cap".to_owned())
}

fn require_event<'a>(history: &'a [Value], event_id: &str) -> Result<&'a Value, String> {
    let matches: Vec<_> = history
        .iter()
        .filter(|event| event["id"].as_str() == Some(event_id))
        .collect();
    if matches.len() != 1 {
        return Err(format!("event {event_id} appeared {} times", matches.len()));
    }
    Ok(matches[0])
}

fn submission_id(event: &Value) -> Option<&str> {
    ["message", "artifact_revision", "objection", "decline"]
        .into_iter()
        .find_map(|key| event["body"][key]["id"].as_str())
}

/// The reader can read and cannot write. The record would be valid if they
/// could, so a host that skips the write grant does not fail it as a sender
/// mismatch.
fn denied_collaboration(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let record = artifact_submission(
        ctx.world_id,
        ctx.reader.principal,
        &ctx.audience,
        "submission:conformance-forbidden",
        "artifact:conformance-denied",
        "A reader cannot append.",
    );
    let response = post_collaboration(ctx, ctx.reader.token, &record.to_string().into_bytes())?;
    expect_problem(response, StatusCode::FORBIDDEN, "forbidden")?;
    if stored_submission(ctx, "submission:conformance-forbidden")? {
        return Err("a denied collaboration was stored".to_owned());
    }
    Ok(())
}

fn stored_submission(ctx: &CollaborationRun<'_>, id: &str) -> Result<bool, String> {
    for token in [ctx.reader.token, ctx.writer.token] {
        let history = read_history(ctx, token)?;
        if history.iter().any(|event| submission_id(event) == Some(id)) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn push_collaboration_rejections(ctx: &CollaborationRun<'_>, world: &Value, cases: &mut Vec<Case>) {
    cases.push(result_case(
        "collaborate.unauthenticated",
        unauthenticated_collaboration(ctx),
    ));
    cases.push(result_case(
        "collaborate.payload_too_large",
        oversized_collaboration(ctx, world),
    ));
    cases.push(result_case(
        "collaborate.unsupported_media_type",
        unidentified_rejection(
            ctx,
            "text/plain",
            b"hello",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ),
    ));
    cases.push(result_case(
        "collaborate.malformed_json",
        unidentified_rejection(
            ctx,
            "application/json",
            b"{",
            StatusCode::BAD_REQUEST,
            "malformed_json",
        ),
    ));
    cases.push(result_case(
        "collaborate.unsupported_version",
        version_collaboration(ctx),
    ));
    cases.push(result_case(
        "collaborate.unsupported_record_type",
        message_on_collaborate(ctx),
    ));
    cases.push(result_case("collaborate.partial_note", partial_note(ctx)));
    cases.push(result_case(
        "collaborate.wrong_world",
        wrong_world_collaboration(ctx),
    ));
    cases.push(result_case(
        "collaborate.from_mismatch",
        from_mismatch_collaboration(ctx),
    ));
}

fn rejected_artifact(ctx: &CollaborationRun<'_>, id: &str) -> Value {
    artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &ctx.audience,
        id,
        "artifact:conformance-rejected",
        "This record must not be stored.",
    )
}

fn unauthenticated_collaboration(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let record = rejected_artifact(ctx, "submission:conformance-unauthenticated");
    let before = read_history(ctx, ctx.writer.token)?.len();
    let response = send_collaboration(
        ctx,
        None,
        "application/json",
        &record.to_string().into_bytes(),
    )?;
    check_auth_response(response)?;
    if stored_submission(ctx, "submission:conformance-unauthenticated")? {
        return Err("an unauthenticated collaboration was stored".to_owned());
    }
    let after = read_history(ctx, ctx.writer.token)?.len();
    if after == before {
        Ok(())
    } else {
        Err("an unauthenticated collaboration changed the writer's history".to_owned())
    }
}

fn oversized_collaboration(ctx: &CollaborationRun<'_>, world: &Value) -> Result<(), String> {
    let body = oversized_body(world)?;
    unidentified_rejection(
        ctx,
        "application/json",
        &body,
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload_too_large",
    )
}

fn version_collaboration(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let mut record = rejected_artifact(ctx, "submission:conformance-version");
    record["protocol_version"] = json!("9");
    reject_record(
        ctx,
        &record,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unsupported_version",
    )
}

/// A message is not a collaboration record, even when it would be valid on submit.
fn message_on_collaborate(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let record = json!({
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": "submission:conformance-message",
        "world": ctx.world_id,
        "from": ctx.writer.principal,
        "to": [ctx.writer.principal],
        "body": {"text": "Messages stay on the submit endpoint."}
    });
    reject_record(
        ctx,
        &record,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unsupported_record_type",
    )
}

/// One field is not a continuity note. The whole note is omitted, or both fields are present.
fn partial_note(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let mut record = rejected_artifact(ctx, "submission:conformance-partial-note");
    record["continuity_note"] = json!({"aim": "Only half a note."});
    reject_record(
        ctx,
        &record,
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_record",
    )
}

fn wrong_world_collaboration(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let mut record = rejected_artifact(ctx, "submission:conformance-world");
    record["world"] = json!("civ:elsewhere");
    reject_record(
        ctx,
        &record,
        StatusCode::UNPROCESSABLE_ENTITY,
        "wrong_world",
    )
}

fn from_mismatch_collaboration(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let record = artifact_submission(
        ctx.world_id,
        ctx.reader.principal,
        &ctx.audience,
        "submission:conformance-mismatch",
        "artifact:conformance-rejected",
        "The sender is not the credential.",
    );
    reject_record(ctx, &record, StatusCode::FORBIDDEN, "forbidden")
}

fn reject_record(
    ctx: &CollaborationRun<'_>,
    record: &Value,
    status: StatusCode,
    code: &str,
) -> Result<(), String> {
    let id = record["id"].as_str().unwrap_or("");
    unidentified_rejection(
        ctx,
        "application/json",
        &record.to_string().into_bytes(),
        status,
        code,
    )?;
    if !id.is_empty() && stored_submission(ctx, id)? {
        return Err(format!("{id} was stored"));
    }
    Ok(())
}

fn unidentified_rejection(
    ctx: &CollaborationRun<'_>,
    content_type: &str,
    body: &[u8],
    status: StatusCode,
    code: &str,
) -> Result<(), String> {
    let before = read_history(ctx, ctx.writer.token)?.len();
    let response = send_collaboration(ctx, Some(ctx.writer.token), content_type, body)?;
    expect_problem(response, status, code)?;
    let after = read_history(ctx, ctx.writer.token)?.len();
    if after != before {
        return Err(format!(
            "rejected collaboration changed the writer's history from {before} to {after}"
        ));
    }
    Ok(())
}

fn client_revision(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let mut record = artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &ctx.audience,
        "submission:conformance-client-revision",
        "artifact:client-revision",
        "The host assigns the revision.",
    );
    record["revision"] = json!(1);
    let response = post_collaboration(ctx, ctx.writer.token, &record.to_string().into_bytes())?;
    expect_problem(response, StatusCode::UNPROCESSABLE_ENTITY, "invalid_record")?;
    let history = read_history(ctx, ctx.writer.token)?;
    if history
        .iter()
        .any(|event| submission_id(event) == Some("submission:conformance-client-revision"))
    {
        return Err("client-supplied revision was stored".to_owned());
    }
    Ok(())
}

fn recorded_revision(ctx: &CollaborationRun<'_>) -> Result<RecordedRevision, String> {
    let mut record = artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &ctx.audience,
        "submission:conformance-revision",
        "artifact:conformance",
        "Keep the pages addressable.",
    );
    record["continuity_note"] = json!({
        "aim": "Leave work a later participant can resume or reject.",
        "resume_hint": "Read the objection before choosing."
    });
    record["conformance_probe"] = json!({"preserve": true});
    let bytes = record.to_string().into_bytes();
    let response = post_collaboration(ctx, ctx.writer.token, &bytes)?;
    let receipt = check_receipt(response, ctx.world_id, "submission:conformance-revision")?;
    if receipt["artifact_id"] != "artifact:conformance" || receipt["revision"] != json!(1) {
        return Err("artifact receipt is missing artifact_id or revision 1".to_owned());
    }
    if receipt.get("aim").is_some()
        || receipt.get("resume_hint").is_some()
        || receipt.get("continuity_note").is_some()
    {
        return Err("artifact receipt carries the continuity note".to_owned());
    }
    let event_id = receipt["event_id"]
        .as_str()
        .ok_or("receipt omits event_id")?
        .to_owned();
    let history = read_history(ctx, ctx.writer.token)?;
    let event = require_event(&history, &event_id)?;
    if event["kind"] != "artifact.recorded"
        || event["actor"] != ctx.writer.principal
        || event["sequence"] != receipt["sequence"]
        || event["body"]["artifact_revision"]["revision"] != json!(1)
        || event["body"]["artifact_revision"]["body"]["text"] != "Keep the pages addressable."
        || event["body"]["artifact_revision"]["continuity_note"]["aim"]
            != "Leave work a later participant can resume or reject."
        || event["body"]["artifact_revision"]["continuity_note"]["resume_hint"]
            != "Read the objection before choosing."
        || event["body"]["artifact_revision"]["conformance_probe"]["preserve"] != true
    {
        return Err("recorded revision does not match the receipt".to_owned());
    }
    let stored_to = event["body"]["artifact_revision"]["to"]
        .as_array()
        .ok_or("stored revision drops to")?;
    if !stored_to
        .iter()
        .any(|item| item.as_str() == Some(ctx.reader.principal))
    {
        return Err("stored revision drops the reader from to".to_owned());
    }
    let visible = addressed_reader_can_see(ctx.visibility)?;
    let reader_history = read_history(ctx, ctx.reader.token)?;
    let seen = reader_history
        .iter()
        .any(|item| item["id"].as_str() == Some(event_id.as_str()));
    if seen != visible {
        return Err(if visible {
            "reader could not see the recorded revision".to_owned()
        } else {
            "reader saw a revision outside the advertised audience".to_owned()
        });
    }
    let timestamp = event["timestamp"]
        .as_str()
        .ok_or("recorded revision omits timestamp")?
        .to_owned();
    Ok(RecordedRevision {
        record,
        bytes,
        receipt,
        event_id,
        timestamp,
    })
}

fn retry_revision(ctx: &CollaborationRun<'_>, recorded: &RecordedRevision) -> Result<(), String> {
    let response = post_collaboration(ctx, ctx.writer.token, &recorded.bytes)?;
    let again = check_receipt(response, ctx.world_id, "submission:conformance-revision")?;
    if again == recorded.receipt {
        Ok(())
    } else {
        Err("byte-identical retry returned a different receipt".to_owned())
    }
}

/// The charset parameter does not make the same bytes a different record.
fn charset_revision(ctx: &CollaborationRun<'_>, recorded: &RecordedRevision) -> Result<(), String> {
    let response = send_collaboration(
        ctx,
        Some(ctx.writer.token),
        "application/json; charset=utf-8",
        &recorded.bytes,
    )?;
    let again = check_receipt(response, ctx.world_id, "submission:conformance-revision")?;
    if again != recorded.receipt {
        return Err("charset retry returned a different receipt".to_owned());
    }
    let history = read_history(ctx, ctx.writer.token)?;
    let count = history
        .iter()
        .filter(|event| submission_id(event) == Some("submission:conformance-revision"))
        .count();
    if count == 1 {
        Ok(())
    } else {
        Err(format!("charset retry left {count} recorded revisions"))
    }
}

fn conflict_revision(
    ctx: &CollaborationRun<'_>,
    recorded: &RecordedRevision,
) -> Result<(), String> {
    let mut changed = recorded.record.clone();
    changed["body"]["text"] = json!("A different revision body.");
    let bytes = changed.to_string().into_bytes();
    if bytes == recorded.bytes {
        return Err("conflict fixture did not change the bytes".to_owned());
    }
    let response = post_collaboration(ctx, ctx.writer.token, &bytes)?;
    check_conflict(response)
}

fn revision_intact(event: &Value) -> Result<(), String> {
    if event["kind"] != "artifact.recorded" {
        return Err("cited revision is no longer artifact.recorded".to_owned());
    }
    if event["body"]["artifact_revision"]["body"]["text"] != "Keep the pages addressable." {
        return Err("cited revision body was changed".to_owned());
    }
    if event["body"]["artifact_revision"]["continuity_note"]["aim"]
        != "Leave work a later participant can resume or reject."
    {
        return Err("cited revision lost its continuity note".to_owned());
    }
    Ok(())
}

fn speech(
    ctx: &CollaborationRun<'_>,
    recorded: &RecordedRevision,
    kind: &str,
    id: &str,
    text: &str,
) -> Result<(), String> {
    let record = json!({
        "protocol_version": "0.1-draft",
        "type": kind,
        "id": id,
        "world": ctx.world_id,
        "from": ctx.writer.principal,
        "to": ctx.audience,
        "artifact_id": "artifact:conformance",
        "target_from": ctx.writer.principal,
        "revision": 1,
        "body": {"text": text}
    });
    let response = post_collaboration(ctx, ctx.writer.token, &record.to_string().into_bytes())?;
    let receipt = check_receipt(response, ctx.world_id, id)?;
    let event_id = receipt["event_id"]
        .as_str()
        .ok_or("receipt omits event_id")?;
    if event_id == recorded.event_id {
        return Err("speech reused the revision event".to_owned());
    }
    let history = read_history(ctx, ctx.writer.token)?;
    let event = require_event(&history, event_id)?;
    let expected_kind = format!("{kind}.recorded");
    if event["kind"].as_str() != Some(expected_kind.as_str())
        || event["body"][kind]["body"]["text"] != text
        || event["body"][kind]["revision"] != json!(1)
        || event["body"][kind]["target_from"] != ctx.writer.principal
    {
        return Err("recorded speech does not match the submission".to_owned());
    }
    revision_intact(require_event(&history, &recorded.event_id)?)
}

/// Revision 1 is already visible. Citing revision 2 uses the same
/// `unknown_target` as a missing artifact, and it must leave revision 1 intact.
fn absent_revision(ctx: &CollaborationRun<'_>, recorded: &RecordedRevision) -> Result<(), String> {
    let record = json!({
        "protocol_version": "0.1-draft",
        "type": "objection",
        "id": "submission:conformance-absent-revision",
        "world": ctx.world_id,
        "from": ctx.writer.principal,
        "to": ctx.audience,
        "artifact_id": "artifact:conformance",
        "target_from": ctx.writer.principal,
        "revision": 2,
        "body": {"text": "Revision 2 was never assigned."}
    });
    let response = post_collaboration(ctx, ctx.writer.token, &record.to_string().into_bytes())?;
    expect_problem(response, StatusCode::UNPROCESSABLE_ENTITY, "unknown_target")?;
    if stored_submission(ctx, "submission:conformance-absent-revision")? {
        return Err("a citation of an unassigned revision was stored".to_owned());
    }
    let history = read_history(ctx, ctx.writer.token)?;
    revision_intact(require_event(&history, &recorded.event_id)?)
}

fn withdrawal_body(world_id: &str, from: &str, target_from: &str, id: &str) -> Vec<u8> {
    json!({
        "protocol_version": "0.1-draft",
        "type": "withdrawal",
        "id": id,
        "world": world_id,
        "from": from,
        "artifact_id": "artifact:conformance",
        "target_from": target_from,
        "revision": 1
    })
    .to_string()
    .into_bytes()
}

fn foreign_withdrawal(
    ctx: &CollaborationRun<'_>,
    recorded: &RecordedRevision,
) -> Result<(), String> {
    let Some(peer) = ctx.peer else {
        return Err("collaboration.submit requires a distinct writing peer".to_owned());
    };
    let (code, status) = foreign_withdrawal_status(ctx.visibility)?;
    let body = withdrawal_body(
        ctx.world_id,
        peer.principal,
        ctx.writer.principal,
        "submission:conformance-withdrawal-peer",
    );
    let response = post_collaboration(ctx, peer.token, &body)?;
    expect_problem(response, status, code)?;
    let history = read_history(ctx, ctx.writer.token)?;
    let event = require_event(&history, &recorded.event_id)?;
    if event["kind"] != "artifact.recorded" {
        return Err("a forbidden withdrawal changed the revision".to_owned());
    }
    revision_intact(event)
}

fn author_withdrawal(
    ctx: &CollaborationRun<'_>,
    recorded: &RecordedRevision,
) -> Result<(), String> {
    let body = withdrawal_body(
        ctx.world_id,
        ctx.writer.principal,
        ctx.writer.principal,
        "submission:conformance-withdrawal",
    );
    let response = post_collaboration(ctx, ctx.writer.token, &body)?;
    let receipt = check_receipt(response, ctx.world_id, "submission:conformance-withdrawal")?;
    if receipt["event_id"] != recorded.receipt["event_id"]
        || receipt["sequence"] != recorded.receipt["sequence"]
    {
        return Err("withdrawal receipt does not name the original revision event".to_owned());
    }
    let history = read_history(ctx, ctx.writer.token)?;
    let event = require_event(&history, &recorded.event_id)?;
    if event["kind"] != "artifact.withdrawn"
        || event["body"] != json!({})
        || event["timestamp"] != recorded.timestamp
        || event["sequence"] != recorded.receipt["sequence"]
    {
        return Err("withdrawal did not leave a stable tombstone".to_owned());
    }
    for (id, kind) in [
        ("submission:conformance-objection", "objection.recorded"),
        ("submission:conformance-decline", "decline.recorded"),
    ] {
        let speech_event = history
            .iter()
            .find(|item| submission_id(item) == Some(id))
            .ok_or_else(|| format!("{id} disappeared after withdrawal"))?;
        if speech_event["kind"] != kind {
            return Err(format!("{id} changed kind after withdrawal"));
        }
    }
    Ok(())
}

/// A withdrawn revision stays visible. Citing it records a new objection and leaves the tombstone.
fn withdrawn_citation(
    ctx: &CollaborationRun<'_>,
    recorded: &RecordedRevision,
) -> Result<(), String> {
    let record = json!({
        "protocol_version": "0.1-draft",
        "type": "objection",
        "id": "submission:conformance-withdrawn-citation",
        "world": ctx.world_id,
        "from": ctx.writer.principal,
        "to": ctx.audience,
        "artifact_id": "artifact:conformance",
        "target_from": ctx.writer.principal,
        "revision": 1,
        "body": {"text": "The withdrawn revision remains citable."}
    });
    let response = post_collaboration(ctx, ctx.writer.token, &record.to_string().into_bytes())?;
    let receipt = check_receipt(
        response,
        ctx.world_id,
        "submission:conformance-withdrawn-citation",
    )?;
    if receipt["event_id"] == recorded.event_id {
        return Err("citation replaced the withdrawn revision".to_owned());
    }
    let history = read_history(ctx, ctx.writer.token)?;
    let tombstone = require_event(&history, &recorded.event_id)?;
    if tombstone["kind"] != "artifact.withdrawn"
        || tombstone["body"] != json!({})
        || tombstone["timestamp"] != recorded.timestamp
        || tombstone["sequence"] != recorded.receipt["sequence"]
    {
        return Err("citing a withdrawn revision changed the tombstone".to_owned());
    }
    let event_id = receipt["event_id"]
        .as_str()
        .ok_or("receipt omits event_id")?;
    let event = require_event(&history, event_id)?;
    if event["kind"] == "objection.recorded"
        && event["actor"] == ctx.writer.principal
        && event["body"]["objection"]["revision"] == json!(1)
        && event["body"]["objection"]["artifact_id"] == "artifact:conformance"
    {
        Ok(())
    } else {
        Err("withdrawn revision was not citable".to_owned())
    }
}

fn unknown_target(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    let mut record = artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &ctx.audience,
        "submission:conformance-missing",
        "artifact:conformance-missing",
        "This citation has no target.",
    );
    record["derived_from"] = json!({
        "from": ctx.writer.principal,
        "artifact_id": "artifact:missing",
        "revision": 1
    });
    let response = post_collaboration(ctx, ctx.writer.token, &record.to_string().into_bytes())?;
    expect_problem(response, StatusCode::UNPROCESSABLE_ENTITY, "unknown_target")?;
    let history = read_history(ctx, ctx.writer.token)?;
    if history
        .iter()
        .any(|event| submission_id(event) == Some("submission:conformance-missing"))
    {
        return Err("unknown target was stored".to_owned());
    }
    Ok(())
}

fn other_chain(ctx: &CollaborationRun<'_>, recorded: &RecordedRevision) -> Result<(), String> {
    let Some(peer) = ctx.peer else {
        return Err("collaboration.submit requires a distinct writing peer".to_owned());
    };
    let record = artifact_submission(
        ctx.world_id,
        peer.principal,
        &ctx.audience,
        "submission:conformance-other-chain",
        "artifact:conformance",
        "A separate chain.",
    );
    let response = post_collaboration(ctx, peer.token, &record.to_string().into_bytes())?;
    let receipt = check_receipt(response, ctx.world_id, "submission:conformance-other-chain")?;
    if receipt["artifact_id"] != "artifact:conformance" || receipt["revision"] != json!(1) {
        return Err("second chain did not start at revision 1".to_owned());
    }
    if receipt.get("aim").is_some()
        || receipt.get("resume_hint").is_some()
        || receipt.get("continuity_note").is_some()
    {
        return Err("second chain receipt carries a continuity note".to_owned());
    }
    let event_id = receipt["event_id"]
        .as_str()
        .ok_or("receipt omits event_id")?;
    if event_id == recorded.event_id {
        return Err("second chain reused the first principal's event".to_owned());
    }
    let history = read_history(ctx, peer.token)?;
    let event = require_event(&history, event_id)?;
    if event["kind"] != "artifact.recorded"
        || event["actor"] != peer.principal
        || event["body"]["artifact_revision"]["from"] != peer.principal
        || event["body"]["artifact_revision"]["revision"] != json!(1)
        || event["body"]["artifact_revision"]["artifact_id"] != "artifact:conformance"
    {
        return Err("second chain was not stored for the peer".to_owned());
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

mod adversarial;
mod lifecycle;
mod revisions;
pub use lifecycle::run_lifecycle;
