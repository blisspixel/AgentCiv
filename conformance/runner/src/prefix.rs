//! Conditional prefix validation checks over the public interface.
use super::*;
use sha2::{Digest, Sha256};

const IDS: [&str; 4] = [
    "prefix.baseline",
    "prefix.append",
    "prefix.hidden",
    "prefix.withdrawal",
];

fn digest(records: &[String]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"agentciv-prefix/0.1\0");
    for raw in records {
        hash.update((raw.len() as u64).to_be_bytes());
        hash.update(raw.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn page(
    ctx: &CollaborationRun<'_>,
    cursor: Option<&str>,
    originals: &mut Vec<String>,
) -> Result<Value, String> {
    let mut url = ctx.events.clone();
    url.query_pairs_mut().append_pair("prefix", "validate");
    if let Some(cursor) = cursor {
        url.query_pairs_mut().append_pair("after", cursor);
    }
    let response = ctx
        .client
        .get(url)
        .bearer_auth(ctx.writer.token)
        .send()
        .map_err(|_| "prefix request failed")?;
    if response.status() != StatusCode::OK || !has_no_store(&response) {
        return Err("prefix page was not a private successful response".into());
    }
    if !response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err("prefix page did not use JSON media".into());
    }
    let length = response.content_length();
    if length.is_some_and(|bytes| bytes > MAX_RESPONSE_BYTES) {
        return Err("prefix page exceeded bound".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "prefix body unavailable")?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("prefix page exceeded bound".into());
    }
    let raw = std::str::from_utf8(&bytes).map_err(|_| "prefix page invalid UTF-8")?;
    let value: Value = serde_json::from_str(raw).map_err(|_| "prefix page invalid JSON")?;
    if raw.contains(ctx.writer.token) || value.to_string().contains(ctx.writer.token) {
        return Err("prefix page reflected credential".into());
    }
    validate(EVENT_PAGE_SCHEMA, &value)?;
    if value["world"] != ctx.world_id {
        return Err("prefix page has wrong world".into());
    }
    if value["prefix_validation"] != "prefix-view/0.1-experimental"
        || value["validated_prefix_sha256"] != digest(originals)
    {
        return Err("prefix validation disagreed with originals".into());
    }
    // Preserve each original JSON span, including its key order and number spelling.
    let fields: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(raw).map_err(|_| "prefix fields invalid")?;
    let rows: Vec<Box<serde_json::value::RawValue>> =
        serde_json::from_str(fields.get("events").ok_or("prefix events missing")?.get())
            .map_err(|_| "prefix events invalid")?;
    originals.extend(rows.into_iter().map(|row| row.get().to_owned()));
    if value["prefix_sha256"] != digest(originals) {
        return Err("prefix result disagreed with originals".into());
    }
    Ok(value)
}

pub(super) fn run(world: &Value, ctx: &CollaborationRun<'_>, cases: &mut Vec<Case>) {
    if !world["capabilities"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "events.prefix.validate"))
    {
        cases.extend(
            IDS.map(|id| Case::skipped_optional(id, "prefix validation is not advertised")),
        );
        return;
    }
    let outcome = exercise(ctx, cases);
    if let Err(detail) = outcome {
        if let Some(id) = IDS
            .iter()
            .find(|id| !cases.iter().any(|case| case.id == **id))
        {
            cases.push(Case::failed(id, detail));
        }
        for id in IDS {
            if !cases.iter().any(|case| case.id == id) {
                cases.push(Case::skipped(id, "earlier prefix check failed"));
            }
        }
    }
}

fn exercise(ctx: &CollaborationRun<'_>, cases: &mut Vec<Case>) -> Result<(), String> {
    let initial = artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &ctx.audience,
        "submission:prefix-initial",
        "artifact:prefix-return",
        "Keep the original until its author withdraws it.",
    );
    let receipt = check_receipt(
        post_collaboration(ctx, ctx.writer.token, initial.to_string().as_bytes())?,
        ctx.world_id,
        "submission:prefix-initial",
    )?;
    let mut originals = Vec::new();
    let mut cursor = None;
    let mut ended = false;
    for _ in 0..8 {
        let result = page(ctx, cursor.as_deref(), &mut originals)?;
        cursor = result["next_cursor"].as_str().map(str::to_owned);
        if result["has_more"] == false {
            ended = true;
            break;
        }
    }
    if !ended
        || !originals.iter().any(|raw| {
            serde_json::from_str::<Value>(raw).is_ok_and(|event| event["id"] == receipt["event_id"])
        })
    {
        return Err("prefix baseline did not contain the recorded original".into());
    }
    cases.push(Case::passed(IDS[0]));
    let next = artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &ctx.audience,
        "submission:prefix-next",
        "artifact:prefix-return",
        "A later revision leaves earlier history checkable.",
    );
    let next_receipt = check_receipt(
        post_collaboration(ctx, ctx.writer.token, next.to_string().as_bytes())?,
        ctx.world_id,
        "submission:prefix-next",
    )?;
    let result = page(ctx, cursor.as_deref(), &mut originals)?;
    if result["has_more"] != false
        || result["events"]
            .as_array()
            .is_none_or(|items| items.len() != 1 || items[0]["id"] != next_receipt["event_id"])
    {
        return Err("prefix resume did not return just the new event".into());
    }
    cursor = result["next_cursor"].as_str().map(str::to_owned);
    cases.push(Case::passed(IDS[1]));
    if ctx.visibility == "members" || ctx.peer.is_none() {
        cases.push(Case::skipped_optional(
            IDS[2],
            "this setup has no hidden peer event",
        ));
    } else {
        let peer = ctx.peer.as_ref().ok_or("peer missing")?;
        let hidden = artifact_submission(
            ctx.world_id,
            peer.principal,
            &[peer.principal.to_owned()],
            "submission:prefix-hidden",
            "artifact:prefix-hidden",
            "Hidden peer activity must not change this prefix.",
        );
        check_receipt(
            post_collaboration(ctx, peer.token, hidden.to_string().as_bytes())?,
            ctx.world_id,
            "submission:prefix-hidden",
        )?;
        let unchanged = page(ctx, cursor.as_deref(), &mut originals)?;
        if unchanged["events"] != json!([])
            || unchanged["next_cursor"].as_str() != cursor.as_deref()
        {
            return Err("hidden activity changed the caller prefix".into());
        }
        cases.push(Case::passed(IDS[2]));
    }
    let withdrawal = json!({"protocol_version":"0.1-draft","type":"withdrawal","id":"withdrawal:prefix-initial","world":ctx.world_id,"from":ctx.writer.principal,"target_from":ctx.writer.principal,"artifact_id":"artifact:prefix-return","revision":1});
    check_receipt(
        post_collaboration(ctx, ctx.writer.token, withdrawal.to_string().as_bytes())?,
        ctx.world_id,
        "withdrawal:prefix-initial",
    )?;
    let mut url = ctx.events.clone();
    url.query_pairs_mut()
        .append_pair("prefix", "validate")
        .append_pair("after", cursor.as_deref().ok_or("cursor missing")?);
    expect_problem(
        ctx.client
            .get(url)
            .bearer_auth(ctx.writer.token)
            .send()
            .map_err(|_| "withdrawn prefix unavailable")?,
        StatusCode::GONE,
        "prefix_changed",
    )?;
    cases.push(Case::passed(IDS[3]));
    Ok(())
}
