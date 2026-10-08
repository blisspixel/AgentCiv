//! Public assertions for a dedicated, bounded author revision chain.
use super::*;

const ARTIFACT: &str = "artifact:conformance-revision-sequence";
const IDS: [&str; 3] = [
    "collaborate.revision_sequence",
    "collaborate.revision_sequence_retry",
    "collaborate.revision_sequence_rejection",
];

struct Recorded {
    request: Value,
    bytes: Vec<u8>,
    receipt: Value,
    event: Value,
}

pub(super) fn request(ctx: &CollaborationRun<'_>, revision: u64) -> Value {
    artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &ctx.audience,
        &format!("submission:conformance-sequence-{revision}"),
        ARTIFACT,
        &format!("Retain exact revision {revision} and its earlier context."),
    )
}

pub(super) fn run(ctx: &CollaborationRun<'_>, cases: &mut Vec<Case>) {
    let sequence = (|| {
        let first = record(ctx, 1)?;
        let second = record(ctx, 2)?;
        let rejection = rejected(ctx, &second);
        let third = record(ctx, 3)?;
        let recorded = vec![first, second, third];
        check_chain(ctx, &recorded)?;
        Ok::<_, String>((recorded, rejection))
    })();
    match sequence {
        Ok((recorded, rejection)) => {
            cases.push(Case::passed(IDS[0]));
            cases.push(result_case(IDS[2], rejection));
            cases.push(result_case(IDS[1], retry(ctx, &recorded)));
        }
        Err(detail) => {
            cases.push(Case::failed(IDS[0], detail));
            skip_ids(
                cases,
                &IDS[1..],
                "revision sequence prerequisite did not pass",
            );
        }
    }
}

fn record(ctx: &CollaborationRun<'_>, revision: u64) -> Result<Recorded, String> {
    let request = request(ctx, revision);
    let bytes = request.to_string().into_bytes();
    let receipt = check_receipt(
        post_collaboration(ctx, ctx.writer.token, &bytes)?,
        ctx.world_id,
        request["id"].as_str().expect("fixture id"),
    )?;
    if receipt["artifact_id"] != ARTIFACT || receipt["revision"] != revision {
        return Err("sequence receipt assigned the wrong artifact or revision".to_owned());
    }
    let history = read_history(ctx, ctx.writer.token)?;
    let event = require_event(
        &history,
        receipt["event_id"]
            .as_str()
            .ok_or("sequence receipt omits event id")?,
    )?
    .clone();
    let recorded = Recorded {
        request,
        bytes,
        receipt,
        event,
    };
    check_record(ctx, &recorded, revision)?;
    Ok(recorded)
}

fn check_record(
    ctx: &CollaborationRun<'_>,
    record: &Recorded,
    revision: u64,
) -> Result<(), String> {
    let mut stored = record.request.clone();
    stored["revision"] = json!(revision);
    if record.event["kind"] != "artifact.recorded"
        || record.event["actor"] != ctx.writer.principal
        || record.event["world"] != ctx.world_id
        || record.event["id"] != record.receipt["event_id"]
        || record.event["sequence"] != record.receipt["sequence"]
        || record.event["body"]["artifact_revision"] != stored
    {
        return Err(
            "sequence event does not match its exact submitted record and receipt".to_owned(),
        );
    }
    Ok(())
}

fn check_chain(ctx: &CollaborationRun<'_>, records: &[Recorded]) -> Result<(), String> {
    let history = read_history(ctx, ctx.writer.token)?;
    let actual: Vec<_> = history
        .iter()
        .filter(|event| {
            event["body"]["artifact_revision"]["from"] == ctx.writer.principal
                && event["body"]["artifact_revision"]["artifact_id"] == ARTIFACT
        })
        .collect();
    let expected: Vec<_> = records.iter().map(|record| &record.event).collect();
    if actual != expected {
        return Err("revision sequence is missing, duplicated, reordered, or changed".to_owned());
    }
    let mut ids = std::collections::HashSet::new();
    let mut previous = None;
    for event in actual {
        let sequence = event["sequence"]
            .as_u64()
            .ok_or("sequence is not an integer")?;
        if !ids.insert(event["id"].to_string()) || previous.is_some_and(|old| sequence <= old) {
            return Err("revision sequence reused event identity or sequence".to_owned());
        }
        previous = Some(sequence);
    }
    Ok(())
}

fn rejected(ctx: &CollaborationRun<'_>, second: &Recorded) -> Result<(), String> {
    let before = read_history(ctx, ctx.writer.token)?;
    let mut supplied = request(ctx, 3);
    supplied["id"] = json!("submission:conformance-sequence-client");
    supplied["revision"] = json!(3);
    expect_problem(
        post_collaboration(ctx, ctx.writer.token, supplied.to_string().as_bytes())?,
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_record",
    )?;
    if read_history(ctx, ctx.writer.token)? != before {
        return Err("client-supplied revision changed permitted history".to_owned());
    }
    let mut conflict = second.request.clone();
    conflict["body"]["text"] = json!("Conflicting bytes must not allocate a revision.");
    expect_problem(
        post_collaboration(ctx, ctx.writer.token, conflict.to_string().as_bytes())?,
        StatusCode::CONFLICT,
        "id_conflict",
    )?;
    if read_history(ctx, ctx.writer.token)? != before {
        return Err("conflicting revision changed permitted history".to_owned());
    }
    Ok(())
}

fn retry(ctx: &CollaborationRun<'_>, recorded: &[Recorded]) -> Result<(), String> {
    let before = read_history(ctx, ctx.writer.token)?;
    for record in &recorded[1..] {
        let receipt = check_receipt(
            post_collaboration(ctx, ctx.writer.token, &record.bytes)?,
            ctx.world_id,
            record.request["id"].as_str().expect("fixture id"),
        )?;
        if receipt != record.receipt {
            return Err("sequence retry changed its original receipt".to_owned());
        }
    }
    if read_history(ctx, ctx.writer.token)? != before {
        return Err("sequence retry changed permitted history".to_owned());
    }
    check_chain(ctx, recorded)
}

#[cfg(test)]
mod tests;
