use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::TcpStream;
use std::path::Path;

use super::*;

pub(super) fn run(
    discovery: &str,
    writer: (&str, &str),
    reader: (&str, &str),
    phase: &str,
    path: &Path,
) -> Report {
    let mut cases = Vec::new();
    if let Err(detail) = observe(discovery, writer, reader, phase, path, &mut cases) {
        cases.push(Case::failed("crash.input", detail));
    }
    let mut report = Report {
        scope: if phase == "crash-prepare" {
            "crash-recovery-prepare"
        } else {
            "crash-recovery-verify"
        },
        cases,
    };
    redact(&mut report, &secret_tokens(writer.1, reader.1, None));
    report
}

fn requests(
    world: &str,
    writer: &str,
    reader: &str,
    uncertain: bool,
    extended: bool,
) -> Vec<Value> {
    let label = if uncertain {
        "uncertain"
    } else {
        "acknowledged"
    };
    let mut message = roundtrip_message(world, writer);
    message["id"] = json!(format!("message:crash-{label}"));
    message["to"] = json!([writer, reader]);
    message["body"] = json!({"text":format!("Keep the actual {label} outcome.")});
    let mut rows = vec![message];
    if extended {
        rows.push(artifact_submission(
            world,
            writer,
            &[writer.to_owned(), reader.to_owned()],
            &format!("submission:crash-{label}"),
            &format!("artifact:crash-{label}"),
            "Keep the original bytes and retry outcome.",
        ));
    }
    rows
}

fn observe(
    discovery: &str,
    writer: (&str, &str),
    reader: (&str, &str),
    phase: &str,
    path: &Path,
    cases: &mut Vec<Case>,
) -> Result<(), String> {
    if writer.0.is_empty()
        || writer.1.is_empty()
        || reader.0.is_empty()
        || reader.1.is_empty()
        || writer.0 == reader.0
        || writer.1 == reader.1
        || !writer.1.bytes().all(|b| (0x21..=0x7e).contains(&b))
    {
        return Err(
            "crash observation requires distinct nonempty credentials and a valid bearer token"
                .to_owned(),
        );
    }
    let (client, world, events, submit, discovery) = extended_targets(discovery)?;
    let extended = collaboration_advertised(&world);
    let endpoints = if extended {
        vec![submit, endpoint_url(&discovery, &world, "collaborate")?]
    } else {
        vec![submit]
    };
    let world_id = world["id"].as_str().ok_or("world id missing")?;
    let acknowledged = requests(world_id, writer.0, reader.0, false, extended);
    let uncertain = requests(world_id, writer.0, reader.0, true, extended);
    if phase == "crash-prepare" {
        if path.exists() || !read_complete_history(&client, &events, world_id, writer.1)?.is_empty()
        {
            return Err("crash prepare needs fresh history and a new checkpoint path".to_owned());
        }
        let mut receipts = Vec::new();
        for (endpoint, body) in endpoints.iter().zip(&acknowledged) {
            receipts.push(record(&client, endpoint, writer.1, world_id, body)?);
        }
        cases.push(Case::passed("crash.acknowledged"));
        let saved = json!({"version":"crash-recovery/0.1","world":world_id,"visibility":world["history"]["visibility"],"principal":writer.0,"acknowledged":acknowledged,"receipts":receipts,"uncertain":uncertain});
        let serialized = saved.to_string();
        for token in [writer.1, reader.1] {
            let encoded = serde_json::to_string(token).map_err(|_| "credential encoding failed")?;
            if serialized.contains(token) || serialized.contains(&encoded[1..encoded.len() - 1]) {
                return Err("crash checkpoint would reflect a credential".to_owned());
            }
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "crash checkpoint create failed")?;
        file.write_all(serialized.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|_| "crash checkpoint write failed")?;
        cases.push(Case::passed("crash.checkpoint"));
        for (endpoint, body) in endpoints.iter().zip(&uncertain) {
            send_without_observing_response(endpoint, writer.1, body)?;
        }
        cases.push(Case::passed("crash.uncertain_dispatched"));
    } else {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|_| "crash checkpoint open failed")?
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "crash checkpoint read failed")?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("crash checkpoint exceeded its byte budget".to_owned());
        }
        let saved: Value =
            serde_json::from_slice(&bytes).map_err(|_| "crash checkpoint JSON invalid")?;
        if saved["version"] != "crash-recovery/0.1"
            || saved["world"] != world_id
            || saved["visibility"] != world["history"]["visibility"]
            || saved["principal"] != writer.0
            || saved["acknowledged"] != json!(acknowledged)
            || saved["uncertain"] != json!(uncertain)
            || saved["receipts"].as_array().map(Vec::len) != Some(acknowledged.len())
        {
            return Err("crash checkpoint condition differs from declared inputs".to_owned());
        }
        let before = read_complete_history(&client, &events, world_id, writer.1)?;
        for (index, (endpoint, body)) in endpoints.iter().zip(&acknowledged).enumerate() {
            let receipt = &saved["receipts"][index];
            validate(RECEIPT_SCHEMA, receipt)?;
            verify_original(&before, body, receipt)?;
            if record(&client, endpoint, writer.1, world_id, body)? != *receipt {
                return Err("acknowledged retry changed across process interruption".to_owned());
            }
        }
        cases.push(Case::passed("crash.acknowledged_history"));
        let mut observations = Vec::new();
        for (endpoint, body) in endpoints.iter().zip(&uncertain) {
            let prior: Vec<_> = before
                .iter()
                .filter(|event| submission_id(event) == body["id"].as_str())
                .collect();
            if prior.len() > 1 {
                return Err("uncertain submission was duplicated before retry".to_owned());
            }
            let receipt = record(&client, endpoint, writer.1, world_id, body)?;
            if let Some(event) = prior.first()
                && (event["id"] != receipt["event_id"] || event["sequence"] != receipt["sequence"])
            {
                return Err(
                    "uncertain retry allocated another event after an earlier commit".to_owned(),
                );
            }
            let after = read_complete_history(&client, &events, world_id, writer.1)?;
            verify_original(&after, body, &receipt)?;
            if after
                .iter()
                .filter(|event| submission_id(event) == body["id"].as_str())
                .count()
                != 1
                || record(&client, endpoint, writer.1, world_id, body)? != receipt
            {
                return Err("uncertain retry was lost or duplicated".to_owned());
            }
            let mut conflict = body.clone();
            conflict["body"] = json!({"text":"Different bytes remain conflicting."});
            let response = client
                .post(endpoint.clone())
                .bearer_auth(writer.1)
                .header(CONTENT_TYPE, "application/json")
                .body(conflict.to_string())
                .send()
                .map_err(|_| "crash conflict transport failed")?;
            expect_problem(response, StatusCode::CONFLICT, "id_conflict")?;
            observations.push(json!({"request_bytes":body.to_string(),"present_before_retry":!prior.is_empty(),"receipt":receipt}));
        }
        let mut outcome = Case::passed("crash.uncertain_retry");
        outcome.evidence = Some(
            json!({"observations":observations,"limit":"Client did not read the original response; this does not establish the server's exact kill interleaving or power-loss recovery."}),
        );
        cases.push(outcome);
        let after = read_complete_history(&client, &events, world_id, writer.1)?;
        if after.len() != acknowledged.len() + uncertain.len() {
            return Err("crash observation left an unexpected event count".to_owned());
        }
        cases.push(Case::passed("crash.no_duplicate"));
    }
    Ok(())
}

fn record(
    client: &Client,
    endpoint: &Url,
    token: &str,
    world: &str,
    body: &Value,
) -> Result<Value, String> {
    let response = client
        .post(endpoint.clone())
        .bearer_auth(token)
        .header(CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .map_err(|_| "crash record transport failed")?;
    check_receipt(
        response,
        world,
        body["id"].as_str().ok_or("crash id missing")?,
    )
}

fn verify_original(history: &[Value], body: &Value, receipt: &Value) -> Result<(), String> {
    let event = require_event(
        history,
        receipt["event_id"]
            .as_str()
            .ok_or("crash receipt event missing")?,
    )?;
    let mut expected = body.clone();
    let (key, kind) = if body["type"] == "message" {
        ("message", "message.recorded")
    } else {
        expected["revision"] = receipt["revision"].clone();
        ("artifact_revision", "artifact.recorded")
    };
    if receipt["record_id"] != body["id"]
        || event["body"][key] != expected
        || event["actor"] != body["from"]
        || event["sequence"] != receipt["sequence"]
        || event["kind"] != kind
    {
        return Err("crash receipt and exact original do not correlate".to_owned());
    }
    Ok(())
}

fn send_without_observing_response(
    endpoint: &Url,
    token: &str,
    body: &Value,
) -> Result<(), String> {
    let addresses = endpoint
        .socket_addrs(|| Some(80))
        .map_err(|_| "uncertain endpoint resolution failed")?;
    let address = *addresses
        .first()
        .ok_or("uncertain endpoint has no address")?;
    if !address.ip().is_loopback() || endpoint.scheme() != "http" {
        return Err("uncertain dispatch requires loopback HTTP".to_owned());
    }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))
        .map_err(|_| "uncertain connection failed")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| "uncertain write timeout unavailable")?;
    let bytes = body.to_string();
    let path = match endpoint.query() {
        Some(query) => format!("{}?{query}", endpoint.path()),
        None => endpoint.path().to_owned(),
    };
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",
        endpoint.authority(),
        bytes.len()
    );
    stream
        .write_all(request.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|_| "uncertain request dispatch failed")?;
    // Deliberately drop the connection without receiving any acknowledgement.
    Ok(())
}
