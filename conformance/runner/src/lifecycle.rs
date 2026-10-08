use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use super::*;

const CHECKPOINT_VERSION: &str = "agentciv-lifecycle/0.3-draft";
struct Credentials<'a> {
    writer: (&'a str, &'a str),
    reader: (&'a str, &'a str),
    peer: Option<(&'a str, &'a str)>,
}

/// Test operator-managed restart and visibility changes through public HTTP.
///
/// `prepare` writes a new checkpoint from a fresh members/addressed world;
/// `verify` follows an operator restart with unchanged database and policy;
/// `policy` follows an operator change to sender_only on that database.
/// The checkpoint contains synthetic test records and cursors, never credentials.
pub fn run_lifecycle(
    discovery_url: &str,
    writer: (&str, &str),
    reader: (&str, &str),
    peer: Option<(&str, &str)>,
    phase: &str,
    checkpoint: &Path,
) -> Report {
    let (scope, ids): (&'static str, &[&'static str]) = match phase {
        "prepare" => (
            "lifecycle-prepare",
            &[
                "lifecycle.world",
                "lifecycle.fresh",
                "lifecycle.seed",
                "lifecycle.checkpoint",
            ],
        ),
        "verify" => (
            "lifecycle-verify",
            &[
                "lifecycle.world",
                "restart.history",
                "restart.cursor",
                "restart.retry",
                "restart.no_duplicate",
                "restart.revision_sequence",
                "restart.concurrent_revisions",
            ],
        ),
        "policy" => (
            "lifecycle-policy",
            &[
                "lifecycle.world",
                "policy.cursor_expired",
                "policy.visibility",
            ],
        ),
        _ => {
            return Report {
                scope: "lifecycle-invalid",
                cases: vec![Case::failed(
                    "lifecycle.input",
                    "lifecycle phase must be prepare, verify, or policy",
                )],
            };
        }
    };
    let mut cases = Vec::new();
    let result = run_phase(
        discovery_url,
        Credentials {
            writer,
            reader,
            peer,
        },
        phase,
        checkpoint,
        &mut cases,
    );
    if let Err(detail) = result {
        let next = ids
            .iter()
            .find(|id| !cases.iter().any(|case| case.id == **id))
            .copied()
            .unwrap_or("lifecycle.input");
        cases.push(Case::failed(next, detail));
    }
    for id in ids {
        if !cases.iter().any(|case| case.id == *id) {
            cases.push(Case::skipped(id, "lifecycle prerequisite did not pass"));
        }
    }
    let mut report = Report { scope, cases };
    redact(&mut report, &secret_tokens(writer.1, reader.1, peer));
    report
}

fn run_phase(
    discovery_url: &str,
    credentials: Credentials<'_>,
    phase: &str,
    checkpoint_path: &Path,
    cases: &mut Vec<Case>,
) -> Result<(), String> {
    let Credentials {
        writer,
        reader,
        peer,
    } = credentials;
    let mut parties = vec![writer, reader];
    if let Some(peer) = peer {
        parties.push(peer);
    }
    for (index, (principal, token)) in parties.iter().enumerate() {
        if principal.is_empty()
            || token.is_empty()
            || parties[..index]
                .iter()
                .any(|(other, secret)| principal == other || token == secret)
        {
            return Err("lifecycle needs distinct nonempty principals and credentials".to_owned());
        }
    }
    let saved = if phase == "prepare" {
        if checkpoint_path.exists() {
            return Err("checkpoint already exists; prepare requires a new path".to_owned());
        }
        None
    } else {
        Some(load_checkpoint(checkpoint_path)?)
    };
    let (client, world, events, submit, discovery) = extended_targets(discovery_url)?;
    let world_id = world["id"].as_str().expect("validated world");
    let extended = collaboration_advertised(&world);
    if extended && (peer.is_none() || !collaboration_auth_is_bearer(&world)) {
        return Err(
            "advertised lifecycle collaboration requires a writing peer and bearer authentication"
                .to_owned(),
        );
    }
    if !extended {
        parties.truncate(2);
    }
    let ctx = CollaborationRun {
        client: &client,
        world_id,
        visibility: world["history"]["visibility"].as_str().unwrap_or(""),
        events: &events,
        collaborate: if extended {
            endpoint_url(&discovery, &world, "collaborate")?
        } else {
            submit.clone()
        },
        writer: Party {
            principal: writer.0,
            token: writer.1,
        },
        reader: Party {
            principal: reader.0,
            token: reader.1,
        },
        peer: peer
            .filter(|_| extended)
            .map(|(principal, token)| Party { principal, token }),
        audience: parties.iter().map(|party| party.0.to_owned()).collect(),
    };
    if let Some(saved) = saved.as_ref() {
        check_world(saved, &world, &parties, phase)?;
    } else if !matches!(ctx.visibility, "members" | "addressed") {
        return Err("lifecycle prepare requires members or addressed visibility".to_owned());
    }
    cases.push(Case::passed("lifecycle.world"));
    let phase_result = if let Some(saved) = saved {
        if phase == "policy" {
            verify_policy(&ctx, &saved, &parties, cases);
        } else {
            verify_restart(&ctx, &submit, &saved, &parties, cases);
        }
        Ok(())
    } else {
        prepare(&ctx, &submit, &world, &parties, checkpoint_path, cases)
    };
    if extended {
        if phase_result.is_ok() && cases.iter().all(|case| case.status == CaseStatus::Passed) {
            cases.push(Case::passed("lifecycle.collaboration"));
        } else {
            cases.push(Case::failed(
                "lifecycle.collaboration",
                "lifecycle extension assertions did not all pass",
            ));
        }
    } else {
        cases.push(Case::skipped_optional(
            "lifecycle.collaboration",
            NO_COLLABORATION,
        ));
    }
    phase_result
}

fn snapshot(ctx: &CollaborationRun<'_>, parties: &[(&str, &str)]) -> Result<Value, String> {
    let mut result = serde_json::Map::new();
    for &(principal, token) in parties {
        result.insert(principal.to_owned(), json!(read_history(ctx, token)?));
    }
    Ok(Value::Object(result))
}

fn initial_requests(world: &str, principals: &[&str]) -> Vec<Value> {
    let audience: Vec<_> = principals
        .iter()
        .map(|principal| (*principal).to_owned())
        .collect();
    let peer = *principals.get(2).unwrap_or(&principals[0]);
    let mut requests: Vec<_> = [principals[0], peer]
        .into_iter()
        .enumerate()
        .map(|(index, principal)| {
            let mut body = roundtrip_message(world, principal);
            body["id"] = json!(format!("message:lifecycle-{index}"));
            body["to"] = json!(audience);
            body["body"] = json!({"text":"Work and its context must survive an operator restart."});
            body
        })
        .collect();
    if principals.len() != 3 {
        return requests;
    }
    let mut source = artifact_submission(
        world,
        principals[0],
        &audience,
        "submission:lifecycle-source",
        "artifact:lifecycle",
        "Preserve sources and unresolved questions.",
    );
    source["continuity_note"] = json!({"aim":"Keep source context available.","resume_hint":"Read the objection and decline before choosing."});
    requests.push(source);
    for kind in ["objection", "decline"] {
        requests.push(json!({"protocol_version":"0.1-draft","type":kind,"id":format!("submission:lifecycle-{kind}"),"world":world,"from":peer,"to":audience,"artifact_id":"artifact:lifecycle","target_from":principals[0],"revision":1,"body":{"text":"This is a recorded participant act, not a host decision."}}));
    }
    let mut withdrawal: Value = serde_json::from_slice(&withdrawal_body(
        world,
        principals[0],
        principals[0],
        "submission:lifecycle-withdrawal",
    ))
    .expect("fixture withdrawal JSON");
    withdrawal["artifact_id"] = json!("artifact:lifecycle");
    requests.push(withdrawal);
    let mut continuation = artifact_submission(
        world,
        peer,
        &audience,
        "submission:lifecycle-continuation",
        "artifact:lifecycle-continuation",
        "The source was withdrawn; the objection and decline remain.",
    );
    continuation["derived_from"] =
        json!({"from":principals[0],"artifact_id":"artifact:lifecycle","revision":1});
    requests.push(continuation);
    for revision in [2, 3] {
        requests.push(artifact_submission(
            world,
            principals[0],
            &audience,
            &format!("submission:lifecycle-source-{revision}"),
            "artifact:lifecycle",
            &format!("Retain revision {revision} after the original withdrawal."),
        ));
    }
    requests
}

pub(super) fn concurrent_requests(
    world: &str,
    writer: &str,
    peer: &str,
    audience: &[String],
) -> [Value; 4] {
    std::array::from_fn(|index| {
        let (principal, label) = if index < 2 {
            (writer, "writer")
        } else {
            (peer, "peer")
        };
        artifact_submission(
            world,
            principal,
            audience,
            &format!("submission:lifecycle-concurrent-{label}-{}", index % 2),
            "artifact:lifecycle",
            &format!("Independent {label} concurrent contribution {}.", index % 2),
        )
    })
}

fn record_concurrent(
    ctx: &CollaborationRun<'_>,
    parties: &[(&str, &str)],
) -> Result<Vec<Value>, String> {
    let requests = concurrent_requests(ctx.world_id, parties[0].0, parties[2].0, &ctx.audience);
    let barrier = std::sync::Barrier::new(requests.len());
    std::thread::scope(|scope| {
        let workers: Vec<_> = requests
            .iter()
            .map(|body| {
                let barrier = &barrier;
                scope.spawn(move || {
                    let principal = body["from"]
                        .as_str()
                        .ok_or("concurrent principal missing")?;
                    let token = parties
                        .iter()
                        .find(|party| party.0 == principal)
                        .ok_or("concurrent principal unavailable")?
                        .1;
                    barrier.wait();
                    record(
                        ctx,
                        &ctx.collaborate,
                        principal,
                        token,
                        body,
                        "collaborate",
                        None,
                    )
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "concurrent lifecycle worker panicked".to_owned())?
            })
            .collect()
    })
}

fn verify_concurrent_records(records: &[Value], principals: &[&str]) -> Result<(), String> {
    if records.len() != 4 || principals.len() != 3 {
        return Err("concurrent lifecycle roster incomplete".to_owned());
    }
    let audience = principals
        .iter()
        .map(|principal| (*principal).to_owned())
        .collect::<Vec<_>>();
    let expected = concurrent_requests(
        records[0]["receipt"]["world"]
            .as_str()
            .ok_or("concurrent world missing")?,
        principals[0],
        principals[2],
        &audience,
    );
    let mut ids = std::collections::HashSet::new();
    let mut event_ids = std::collections::HashSet::new();
    let mut sequences = std::collections::HashSet::new();
    for entry in records {
        let bytes = entry["bytes"]
            .as_str()
            .ok_or("concurrent request bytes missing")?;
        let body: Value =
            serde_json::from_str(bytes).map_err(|_| "concurrent request JSON invalid")?;
        let canonical = body.to_string();
        if !expected.contains(&body)
            || bytes != canonical
            || entry["principal"] != body["from"]
            || entry["operation"] != "collaborate"
            || !ids.insert(body["id"].to_string())
        {
            return Err("concurrent lifecycle exact request roster differs".to_owned());
        }
        let receipt = &entry["receipt"];
        let event = &entry["event_at_recording"];
        let mut stored = body.clone();
        stored["revision"] = receipt["revision"].clone();
        if receipt["record_id"] != body["id"]
            || receipt["artifact_id"] != "artifact:lifecycle"
            || receipt["world"] != body["world"]
            || event["world"] != body["world"]
            || event["id"] != receipt["event_id"]
            || event["sequence"] != receipt["sequence"]
            || event["actor"] != body["from"]
            || event["kind"] != "artifact.recorded"
            || event["body"]["artifact_revision"] != stored
            || !event_ids.insert(event["id"].to_string())
            || !sequences.insert(event["sequence"].to_string())
        {
            return Err("concurrent lifecycle receipt and exact event mapping differ".to_owned());
        }
    }
    for (principal, expected) in [(principals[0], vec![4, 5]), (principals[2], vec![1, 2])] {
        let mut entries: Vec<_> = records
            .iter()
            .filter(|entry| entry["principal"] == principal)
            .collect();
        entries.sort_by_key(|entry| entry["receipt"]["sequence"].as_u64().unwrap_or(u64::MAX));
        let revisions: Vec<_> = entries
            .iter()
            .map(|entry| entry["receipt"]["revision"].as_u64().unwrap_or(0))
            .collect();
        if revisions != expected {
            return Err("concurrent lifecycle independent revision sequence differs".to_owned());
        }
    }
    Ok(())
}

fn prepare(
    ctx: &CollaborationRun<'_>,
    submit: &Url,
    world: &Value,
    parties: &[(&str, &str)],
    checkpoint_path: &Path,
    cases: &mut Vec<Case>,
) -> Result<(), String> {
    let before = snapshot(ctx, parties)?;
    if before
        .as_object()
        .expect("snapshot")
        .values()
        .any(|events| !events.as_array().expect("snapshot events").is_empty())
    {
        return Err(
            "lifecycle prepare requires empty permitted history for every principal".to_owned(),
        );
    }
    let page = check_page(
        read_events(ctx.client, ctx.events, ctx.reader.token, None)?,
        ctx.world_id,
    )?;
    let cursor = page["next_cursor"].clone();
    cases.push(Case::passed("lifecycle.fresh"));
    let principals: Vec<_> = parties.iter().map(|party| party.0).collect();
    let initial = initial_requests(ctx.world_id, &principals);
    let mut records = Vec::new();
    for (index, body) in initial.iter().enumerate() {
        let principal = body["from"].as_str().ok_or("seed principal missing")?;
        let token = parties
            .iter()
            .find(|party| party.0 == principal)
            .ok_or("seed principal unavailable")?
            .1;
        let operation = if index < 2 { "submit" } else { "collaborate" };
        let endpoint = if index < 2 { submit } else { &ctx.collaborate };
        let revision = match index {
            7 => 2,
            8 => 3,
            _ => 1,
        };
        records.push(record(
            ctx,
            endpoint,
            principal,
            token,
            body,
            operation,
            Some(revision),
        )?);
    }
    if parties.len() == 3 {
        let mut concurrent = record_concurrent(ctx, parties)?;
        concurrent.sort_by_key(|entry| entry["receipt"]["sequence"].as_u64().unwrap_or(u64::MAX));
        verify_concurrent_records(&concurrent, &principals)?;
        records.extend(concurrent);
    }
    let after = snapshot(ctx, parties)?;
    verify_seed(&after, &records, parties)?;
    cases.push(Case::passed("lifecycle.seed"));
    let checkpoint = json!({"checkpoint_version":CHECKPOINT_VERSION,"world":world,"principals":parties.iter().map(|party|party.0).collect::<Vec<_>>(),"reader_cursor":cursor,"records":records,"history":after});
    let bytes = serde_json::to_vec_pretty(&checkpoint)
        .map_err(|error| format!("checkpoint serialization failed: {error}"))?;
    ensure_no_credentials(&bytes, parties)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(checkpoint_path)
        .map_err(|error| format!("checkpoint create failed: {error}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("checkpoint write failed: {error}"))?;
    cases.push(Case::passed("lifecycle.checkpoint"));
    Ok(())
}

fn ensure_no_credentials(bytes: &[u8], parties: &[(&str, &str)]) -> Result<(), String> {
    if parties.iter().any(|(_, token)| {
        if token.is_empty() {
            return false;
        }
        let encoded = serde_json::to_string(token).expect("credential JSON string");
        let escaped = &encoded.as_bytes()[1..encoded.len() - 1];
        bytes
            .windows(token.len())
            .any(|window| window == token.as_bytes())
            || bytes.windows(escaped.len()).any(|window| window == escaped)
    }) {
        Err("checkpoint contains a credential; refusing to persist it".to_owned())
    } else {
        Ok(())
    }
}

fn record(
    ctx: &CollaborationRun<'_>,
    endpoint: &Url,
    principal: &str,
    token: &str,
    body: &Value,
    operation: &str,
    expected_revision: Option<u64>,
) -> Result<Value, String> {
    let bytes = body.to_string();
    let response = ctx
        .client
        .post(endpoint.clone())
        .header(CONTENT_TYPE, "application/json")
        .bearer_auth(token)
        .body(bytes.clone())
        .send()
        .map_err(|error| format!("lifecycle submission failed: {error}"))?;
    let receipt = check_receipt(
        response,
        ctx.world_id,
        body["id"].as_str().ok_or("record omits id")?,
    )?;
    let history = read_history(ctx, token)?;
    let event = require_event(
        &history,
        receipt["event_id"]
            .as_str()
            .ok_or("lifecycle receipt omits event id")?,
    )?;
    let mut stored = body.clone();
    let field = match body["type"].as_str() {
        Some("message") => "message",
        Some("artifact_revision") => {
            if receipt["artifact_id"] != body["artifact_id"]
                || expected_revision.is_some_and(|expected| receipt["revision"] != expected)
            {
                return Err(
                    "lifecycle artifact receipt did not assign the expected revision".to_owned(),
                );
            }
            stored["revision"] = receipt["revision"].clone();
            "artifact_revision"
        }
        Some("objection") => "objection",
        Some("decline") => "decline",
        Some("withdrawal") => "withdrawal",
        _ => return Err("unexpected lifecycle record type".to_owned()),
    };
    if event["sequence"] != receipt["sequence"]
        || event["actor"] != principal
        || (field == "withdrawal"
            && (event["kind"] != "artifact.withdrawn" || event["body"] != json!({})))
        || (field != "withdrawal" && event["body"][field] != stored)
    {
        return Err("lifecycle submission does not match its recorded event".to_owned());
    }
    Ok(
        json!({"operation":operation,"principal":principal,"bytes":bytes,"receipt":receipt,"event_at_recording":event}),
    )
}

fn verify_seed(history: &Value, records: &[Value], parties: &[(&str, &str)]) -> Result<(), String> {
    if records.len() == 13 {
        let principals: Vec<_> = parties.iter().map(|party| party.0).collect();
        verify_concurrent_records(&records[9..], &principals)?;
    }
    let mut expected_events = Vec::new();
    let mut event_ids = std::collections::HashSet::new();
    let mut last_sequence = None;
    for (index, record) in records.iter().enumerate() {
        let event = &record["event_at_recording"];
        if index == 5 {
            if event["id"] != records[2]["event_at_recording"]["id"]
                || event["sequence"] != records[2]["event_at_recording"]["sequence"]
                || event["timestamp"] != records[2]["event_at_recording"]["timestamp"]
                || event["actor"] != records[2]["event_at_recording"]["actor"]
            {
                return Err("lifecycle withdrawal changed its original event identity".to_owned());
            }
            expected_events[2] = event.clone();
        } else {
            let sequence = event["sequence"]
                .as_u64()
                .ok_or("seed event sequence missing")?;
            if last_sequence.is_some_and(|previous| sequence <= previous)
                || !event_ids.insert(event["id"].to_string())
            {
                return Err("lifecycle seed events repeat identity or sequence".to_owned());
            }
            last_sequence = Some(sequence);
            expected_events.push(event.clone());
        }
    }
    for &(principal, _) in parties {
        let events = history[principal]
            .as_array()
            .ok_or("seed history missing principal")?;
        if events.len() != if records.len() == 13 { 12 } else { 2 } {
            return Err(
                "lifecycle seed visible event count differs from its bounded layout".to_owned(),
            );
        }
        for entry in records {
            let event = require_event(
                events,
                entry["receipt"]["event_id"]
                    .as_str()
                    .ok_or("seed receipt missing event")?,
            )?;
            if event["sequence"] != entry["receipt"]["sequence"] {
                return Err("lifecycle seed receipt sequence mismatch".to_owned());
            }
        }
        if records.len() == 13 {
            let source = require_event(
                events,
                records[2]["receipt"]["event_id"]
                    .as_str()
                    .ok_or("source missing event id")?,
            )?;
            if source["kind"] != "artifact.withdrawn" || source["body"] != json!({}) {
                return Err("lifecycle withdrawal did not preserve an empty tombstone".to_owned());
            }
            if source["id"] != records[2]["event_at_recording"]["id"]
                || source["sequence"] != records[2]["event_at_recording"]["sequence"]
                || source["timestamp"] != records[2]["event_at_recording"]["timestamp"]
            {
                return Err("lifecycle withdrawal changed its original event identity".to_owned());
            }
        }
        if events != &expected_events {
            return Err("lifecycle history does not match the submitted records".to_owned());
        }
    }
    Ok(())
}

fn load_checkpoint(path: &Path) -> Result<Value, String> {
    let file = File::open(path).map_err(|error| format!("checkpoint open failed: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("checkpoint read failed: {error}"))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("checkpoint exceeds the size limit".to_owned());
    }
    let saved: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("checkpoint JSON invalid: {error}"))?;
    validate_checkpoint(&saved)?;
    Ok(saved)
}

fn validate_checkpoint(saved: &Value) -> Result<(), String> {
    if saved["checkpoint_version"] != CHECKPOINT_VERSION {
        return Err("unsupported checkpoint version".to_owned());
    }
    validate(WORLD_SCHEMA, &saved["world"])?;
    let principals = saved["principals"]
        .as_array()
        .ok_or("checkpoint principals missing")?;
    let extended = collaboration_advertised(&saved["world"]);
    if principals.len() != if extended { 3 } else { 2 }
        || principals
            .iter()
            .any(|principal| principal.as_str().is_none_or(str::is_empty))
        || principals[0] == principals[1]
        || (extended && (principals[0] == principals[2] || principals[1] == principals[2]))
    {
        return Err("checkpoint requires three distinct principals".to_owned());
    }
    if saved["reader_cursor"].as_str().is_none_or(str::is_empty) {
        return Err("checkpoint reader cursor missing".to_owned());
    }
    let records = saved["records"]
        .as_array()
        .ok_or("checkpoint records missing")?;
    if records.len() != if extended { 13 } else { 2 } {
        return Err(
            "checkpoint synthetic submission count does not match advertised capabilities"
                .to_owned(),
        );
    }
    let mut record_ids = std::collections::HashSet::new();
    let principal_names: Vec<_> = principals
        .iter()
        .map(|principal| principal.as_str().expect("checked principal"))
        .collect();
    let initial = initial_requests(
        saved["world"]["id"].as_str().expect("validated world"),
        &principal_names,
    );
    for (index, entry) in records.iter().enumerate() {
        if !matches!(entry["operation"].as_str(), Some("submit" | "collaborate"))
            || !principals.contains(&entry["principal"])
        {
            return Err("checkpoint submission operation or principal invalid".to_owned());
        }
        let body: Value = serde_json::from_str(
            entry["bytes"]
                .as_str()
                .ok_or("checkpoint request bytes missing")?,
        )
        .map_err(|error| format!("checkpoint request JSON invalid: {error}"))?;
        if entry["bytes"].as_str() != Some(body.to_string().as_str()) {
            return Err(
                "checkpoint request bytes differ from the canonical prepared record".to_owned(),
            );
        }
        validate(RECEIPT_SCHEMA, &entry["receipt"])?;
        validate(EVENT_SCHEMA, &entry["event_at_recording"])?;
        let kind = body["type"]
            .as_str()
            .ok_or("checkpoint request type missing")?;
        let schema = match kind {
            "message" if entry["operation"] == "submit" => MESSAGE_SCHEMA,
            "artifact_revision" if entry["operation"] == "collaborate" => {
                include_str!("../../../schemas/collaboration-artifact.schema.json")
            }
            "objection" if entry["operation"] == "collaborate" => {
                include_str!("../../../schemas/collaboration-objection.schema.json")
            }
            "decline" if entry["operation"] == "collaborate" => {
                include_str!("../../../schemas/collaboration-decline.schema.json")
            }
            "withdrawal" if entry["operation"] == "collaborate" => {
                include_str!("../../../schemas/collaboration-withdrawal.schema.json")
            }
            _ => return Err("checkpoint request type does not match its endpoint".to_owned()),
        };
        validate(schema, &body)?;
        let expected_kind = match index {
            0 | 1 => "message",
            2 | 6 | 7 | 8 | 9..=12 => "artifact_revision",
            3 => "objection",
            4 => "decline",
            5 => "withdrawal",
            _ => unreachable!("validated record count"),
        };
        let expected_principal = if extended && matches!(index, 1 | 3 | 4 | 6) {
            &principals[2]
        } else {
            &principals[0]
        };
        if kind != expected_kind
            || (index < 9 && &entry["principal"] != expected_principal)
            || (index < initial.len() && body != initial[index])
            || !record_ids.insert((entry["principal"].to_string(), body["id"].to_string()))
        {
            return Err(
                "checkpoint submission layout or unique record identity invalid".to_owned(),
            );
        }
        let event = &entry["event_at_recording"];
        if event["id"] != entry["receipt"]["event_id"]
            || event["sequence"] != entry["receipt"]["sequence"]
            || event["actor"] != entry["principal"]
            || event["world"] != saved["world"]["id"]
        {
            return Err("checkpoint event and receipt mismatch".to_owned());
        }
        let (field, event_kind) = match kind {
            "message" => ("message", "message.recorded"),
            "artifact_revision" => ("artifact_revision", "artifact.recorded"),
            "objection" => ("objection", "objection.recorded"),
            "decline" => ("decline", "decline.recorded"),
            _ => ("withdrawal", "artifact.withdrawn"),
        };
        let expected_artifact = if index == 6 {
            "artifact:lifecycle-continuation"
        } else {
            "artifact:lifecycle"
        };
        if kind != "message" && body["artifact_id"] != expected_artifact {
            return Err("checkpoint artifact chain differs from the bounded layout".to_owned());
        }
        if matches!(kind, "objection" | "decline" | "withdrawal")
            && (body["target_from"] != principals[0] || body["revision"] != 1)
        {
            return Err("checkpoint citation differs from the original revision".to_owned());
        }
        let mut stored = body.clone();
        if kind == "artifact_revision" {
            if body.get("revision").is_some()
                || entry["receipt"]["revision"]
                    != match index {
                        7 => 2,
                        8 => 3,
                        9..=12 => entry["receipt"]["revision"]
                            .as_u64()
                            .ok_or("concurrent revision missing")?,
                        _ => 1,
                    }
                || entry["receipt"]["artifact_id"] != body["artifact_id"]
            {
                return Err("checkpoint artifact revision or receipt invalid".to_owned());
            }
            stored["revision"] = entry["receipt"]["revision"].clone();
        }
        if event["kind"] != event_kind
            || (kind == "withdrawal" && event["body"] != json!({}))
            || (kind != "withdrawal" && event["body"][field] != stored)
        {
            return Err("checkpoint event content does not match its submitted bytes".to_owned());
        }
        if body["from"] != entry["principal"]
            || body["world"] != saved["world"]["id"]
            || body["id"] != entry["receipt"]["record_id"]
            || entry["receipt"]["world"] != saved["world"]["id"]
        {
            return Err("checkpoint request and receipt mismatch".to_owned());
        }
    }
    if extended {
        verify_concurrent_records(&records[9..], &principal_names)?;
    }
    for principal in principals {
        let events = saved["history"][principal.as_str().expect("checked principal")]
            .as_array()
            .ok_or("checkpoint principal history missing")?;
        for event in events {
            validate(EVENT_SCHEMA, event)?;
            if event["world"] != saved["world"]["id"] {
                return Err("checkpoint event world mismatch".to_owned());
            }
        }
    }
    let parties: Vec<_> = principals
        .iter()
        .map(|principal| (principal.as_str().expect("validated principal"), ""))
        .collect();
    verify_seed(&saved["history"], records, &parties)?;
    Ok(())
}

fn check_world(
    saved: &Value,
    world: &Value,
    parties: &[(&str, &str)],
    phase: &str,
) -> Result<(), String> {
    if saved["world"]["id"] != world["id"]
        || saved["world"]["profile"] != world["profile"]
        || saved["world"]["protocol_version"] != world["protocol_version"]
        || saved["principals"] != json!(parties.iter().map(|party| party.0).collect::<Vec<_>>())
        || collaboration_advertised(&saved["world"]) != collaboration_advertised(world)
    {
        return Err("checkpoint world, profile, version, or principals differ".to_owned());
    }
    if phase == "policy" {
        if world["history"]["visibility"] != "sender_only" {
            return Err("policy phase requires sender_only visibility".to_owned());
        }
    } else if world["history"] != saved["world"]["history"] {
        return Err("restart verification requires unchanged history policy".to_owned());
    }
    Ok(())
}

fn verify_restart(
    ctx: &CollaborationRun<'_>,
    submit: &Url,
    saved: &Value,
    parties: &[(&str, &str)],
    cases: &mut Vec<Case>,
) {
    let concurrent_before = if parties.len() == 3 {
        verify_concurrent_history(ctx, saved, parties)
    } else {
        Ok(())
    };
    cases.push(result_case(
        "restart.history",
        snapshot(ctx, parties).and_then(|history| compare_history(&history, &saved["history"])),
    ));
    cases.push(result_case(
        "restart.cursor",
        (|| {
            let page = check_page(
                read_events(
                    ctx.client,
                    ctx.events,
                    ctx.reader.token,
                    saved["reader_cursor"].as_str(),
                )?,
                ctx.world_id,
            )?;
            if page["events"] != saved["history"][ctx.reader.principal] || page["has_more"] != false
            {
                return Err(
                    "pre-restart reader cursor did not resume the saved permitted events"
                        .to_owned(),
                );
            }
            Ok(())
        })(),
    ));
    cases.push(result_case("restart.retry",( || {
        for entry in saved["records"].as_array().expect("validated records") {
            let principal=entry["principal"].as_str().expect("validated principal");
            let token=parties.iter().find(|party|party.0==principal).ok_or("saved retry principal missing")?.1;
            let endpoint=if entry["operation"] == "submit" {submit} else {&ctx.collaborate};
            let response=ctx.client.post(endpoint.clone()).header(CONTENT_TYPE,"application/json").bearer_auth(token).body(entry["bytes"].as_str().expect("validated bytes").to_owned()).send().map_err(|error|format!("restart retry failed: {error}"))?;
            let receipt=check_receipt(response,ctx.world_id,entry["receipt"]["record_id"].as_str().expect("validated receipt"))?;
            if receipt != entry["receipt"] { return Err("restart retry did not return the saved receipt; verify must run inside the retention window".to_owned()); }
        }
        Ok(())
    })()));
    cases.push(result_case(
        "restart.no_duplicate",
        snapshot(ctx, parties).and_then(|history| compare_history(&history, &saved["history"])),
    ));
    if parties.len() == 3 {
        cases.push(result_case(
            "restart.revision_sequence",
            (|| {
                let history = snapshot(ctx, parties)?;
                verify_seed(
                    &history,
                    saved["records"].as_array().expect("validated records"),
                    parties,
                )
            })(),
        ));
        cases.push(result_case(
            "restart.concurrent_revisions",
            concurrent_before.and_then(|()| verify_concurrent_history(ctx, saved, parties)),
        ));
    } else {
        cases.push(Case::skipped_optional(
            "restart.revision_sequence",
            NO_COLLABORATION,
        ));
        cases.push(Case::skipped_optional(
            "restart.concurrent_revisions",
            NO_COLLABORATION,
        ));
    }
}

fn verify_concurrent_history(
    ctx: &CollaborationRun<'_>,
    saved: &Value,
    parties: &[(&str, &str)],
) -> Result<(), String> {
    let history = snapshot(ctx, parties)?;
    verify_seed(
        &history,
        saved["records"].as_array().expect("validated records"),
        parties,
    )
}

fn compare_history(actual: &Value, expected: &Value) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err("permitted history differs from the checkpoint".to_owned())
    }
}

fn verify_policy(
    ctx: &CollaborationRun<'_>,
    saved: &Value,
    parties: &[(&str, &str)],
    cases: &mut Vec<Case>,
) {
    cases.push(result_case(
        "policy.cursor_expired",
        read_events(
            ctx.client,
            ctx.events,
            ctx.reader.token,
            saved["reader_cursor"].as_str(),
        )
        .and_then(|response| expect_problem(response, StatusCode::GONE, "cursor_expired")),
    ));
    cases.push(result_case(
        "policy.visibility",
        (|| {
            let actual = snapshot(ctx, parties)?;
            let mut expected = serde_json::Map::new();
            for &(principal, _) in parties {
                let filtered: Vec<_> = saved["history"][principal]
                    .as_array()
                    .expect("validated history")
                    .iter()
                    .filter(|event| event["actor"] == principal)
                    .cloned()
                    .collect();
                expected.insert(principal.to_owned(), json!(filtered));
            }
            compare_history(&actual, &Value::Object(expected))
        })(),
    ));
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mock_tests;
