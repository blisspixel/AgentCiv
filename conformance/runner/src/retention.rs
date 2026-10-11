use std::time::Instant;

use super::*;

/// Bounded public observation against a fresh disposable world with an 8-30s window.
/// The operator must keep the local host clock steady. This is not an exact-clock test.
pub fn run_retention(discovery: &str, writer: (&str, &str), reader: (&str, &str)) -> Report {
    let mut cases = Vec::new();
    if let Err(detail) = observe(discovery, writer, reader, &mut cases) {
        cases.push(Case::failed("retention.observation", detail));
    }
    let mut report = Report {
        scope: "bounded-retention",
        cases,
    };
    redact(&mut report, &secret_tokens(writer.1, reader.1, None));
    report
}

fn observe(
    discovery: &str,
    writer: (&str, &str),
    reader: (&str, &str),
    cases: &mut Vec<Case>,
) -> Result<(), String> {
    if writer.0.is_empty()
        || writer.1.is_empty()
        || reader.0.is_empty()
        || reader.1.is_empty()
        || writer.0 == reader.0
        || writer.1 == reader.1
    {
        return Err("retention needs distinct nonempty credentials".to_owned());
    }
    let (client, world, events, submit, discovery) = extended_targets(discovery)?;
    let seconds = world["history"]["retention_seconds"]
        .as_u64()
        .ok_or("retention missing")?;
    if !(8..=30).contains(&seconds) {
        return Err(
            "retention observation requires a declared 8-30 second disposable window".to_owned(),
        );
    }
    let world_id = world["id"].as_str().ok_or("world id missing")?;
    for token in [writer.1, reader.1] {
        if !read_complete_history(&client, &events, world_id, token)?.is_empty() {
            return Err("retention observation requires fresh visible history".to_owned());
        }
    }
    cases.push(Case::passed("retention.setup"));
    let mut requests = Vec::new();
    let mut message = roundtrip_message(world_id, writer.0);
    message["id"] = json!("message:retention");
    message["to"] = json!([writer.0, reader.0]);
    requests.push((submit.clone(), message));
    if collaboration_advertised(&world) {
        let endpoint = endpoint_url(&discovery, &world, "collaborate")?;
        requests.push((
            endpoint,
            artifact_submission(
                world_id,
                writer.0,
                &[writer.0.to_owned(), reader.0.to_owned()],
                "submission:retention",
                "artifact:retention",
                "Retain a bounded original.",
            ),
        ));
    }
    let started = Instant::now();
    let mut receipts = Vec::new();
    for (endpoint, body) in &requests {
        receipts.push(send_record(&client, endpoint, writer.1, world_id, body)?);
    }
    let mut tombstone = None;
    let mut withdrawal = None;
    if requests.len() == 2 {
        let value = json!({"protocol_version":"0.1-draft","type":"withdrawal","id":"submission:retention-withdrawal","world":world_id,"from":writer.0,"to":[writer.0,reader.0],"target_from":writer.0,"artifact_id":"artifact:retention","revision":1});
        let receipt = send_record(&client, &requests[1].0, writer.1, world_id, &value)?;
        let history = read_complete_history(&client, &events, world_id, reader.1)?;
        let event = require_event(
            &history,
            receipt["event_id"]
                .as_str()
                .ok_or("withdrawal event missing")?,
        )?
        .clone();
        if event["kind"] != "artifact.withdrawn" || event["body"] != json!({}) {
            return Err("retention source is not an empty tombstone".to_owned());
        }
        tombstone = Some(event);
        withdrawal = Some((value, receipt));
    }
    let baseline = read_complete_history(&client, &events, world_id, reader.1)?;
    cases.push(Case::passed("retention.recorded"));
    std::thread::sleep(Duration::from_secs(seconds / 2));
    if started.elapsed() >= Duration::from_secs(seconds - 1) {
        return Err("retention inside-window timing margin was exceeded".to_owned());
    }
    if read_complete_history(&client, &events, world_id, reader.1)? != baseline {
        return Err("history or tombstone disappeared before the advertised minimum".to_owned());
    }
    cases.push(Case::passed("retention.minimum_history"));
    for ((endpoint, body), receipt) in requests.iter().zip(&receipts) {
        if send_record(&client, endpoint, writer.1, world_id, body)? != *receipt {
            return Err("inside-window retry changed its original receipt".to_owned());
        }
    }
    if let Some((body, receipt)) = &withdrawal
        && send_record(&client, &requests[1].0, writer.1, world_id, body)? != *receipt
    {
        return Err("inside-window withdrawal retry changed receipt".to_owned());
    }
    if started.elapsed() >= Duration::from_secs(seconds - 1) {
        return Err("retention retry completed outside the conservative timing margin".to_owned());
    }
    cases.push(Case::passed("retention.retry_inside"));
    // Wait beyond recording acknowledgement plus a two-second coarse-clock margin.
    let wait = Duration::from_secs(seconds + 2).saturating_sub(started.elapsed());
    std::thread::sleep(wait);
    let mut replacements = Vec::new();
    for ((endpoint, body), old) in requests.iter().zip(&receipts) {
        let mut changed = body.clone();
        changed["body"] = json!({"text":"A new submission after the disclosed retry interval."});
        let receipt = send_record(&client, endpoint, writer.1, world_id, &changed)?;
        if receipt["event_id"] == old["event_id"]
            || receipt["sequence"].as_u64() <= old["sequence"].as_u64()
        {
            return Err("after-window ID reuse did not produce a new recording".to_owned());
        }
        replacements.push((changed, receipt));
    }
    let mut reused = Case::passed("retention.reuse_after");
    reused.evidence = Some(
        json!({"advertised_seconds":seconds,"elapsed_milliseconds":started.elapsed().as_millis(),"requests":requests.iter().map(|(_,v)|v.to_string()).collect::<Vec<_>>(),"original_receipts":receipts,"original_tombstone":tombstone,"replacements":replacements,"clock_assumption":"steady local host clock; approximate samples, not the exact half-open boundary"}),
    );
    cases.push(reused);
    let history = read_complete_history(&client, &events, world_id, reader.1)?;
    for (body, receipt) in &replacements {
        if history
            .iter()
            .filter(|event| event["id"] == receipt["event_id"])
            .count()
            != 1
        {
            return Err("ID reuse is missing or duplicated in permitted history".to_owned());
        }
        let event = require_event(
            &history,
            receipt["event_id"]
                .as_str()
                .ok_or("replacement event missing")?,
        )?;
        let key = if body["type"] == "message" {
            "message"
        } else {
            "artifact_revision"
        };
        let mut expected = body.clone();
        if key == "artifact_revision" {
            expected["revision"] = receipt["revision"].clone();
        }
        if event["body"][key] != expected {
            return Err("after-window original bytes were not preserved".to_owned());
        }
    }
    cases.push(Case::passed("retention.no_duplicate"));
    Ok(())
}

fn send_record(
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
        .map_err(|_| "retention transport failed")?;
    check_receipt(
        response,
        world,
        body["id"].as_str().ok_or("retention id missing")?,
    )
}
