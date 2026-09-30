use std::sync::Barrier;
use std::thread;

use super::*;

pub(super) fn run_concurrency(
    client: &Client,
    events: &Url,
    submit: &Url,
    world_id: &str,
    writer: Party<'_>,
    cases: &mut Vec<Case>,
) {
    for (id, kind) in [
        ("submit.concurrent_retry", RaceKind::Exact),
        ("submit.concurrent_conflict", RaceKind::Conflict),
        ("submit.concurrent_distinct", RaceKind::Distinct),
    ] {
        cases.push(result_case(
            id,
            race(client, events, submit, world_id, writer, kind),
        ));
    }
}

#[derive(Clone, Copy)]
enum RaceKind {
    Exact,
    Conflict,
    Distinct,
}

fn race_messages(world_id: &str, principal: &str, kind: RaceKind) -> Vec<Value> {
    let (label, count) = match kind {
        RaceKind::Exact => ("exact", 4),
        RaceKind::Conflict => ("conflict", 2),
        RaceKind::Distinct => ("distinct", 4),
    };
    (0..count)
        .map(|index| {
            let mut message = roundtrip_message(world_id, principal);
            message["id"] = json!(match kind {
                RaceKind::Distinct => format!("message:concurrent-{label}-{index}"),
                _ => format!("message:concurrent-{label}"),
            });
            message["body"] = json!({"text": match kind {
                RaceKind::Exact => "same".to_owned(),
                _ => format!("body-{index}"),
            }});
            message
        })
        .collect()
}

fn race(
    client: &Client,
    events: &Url,
    submit: &Url,
    world_id: &str,
    writer: Party<'_>,
    kind: RaceKind,
) -> Result<(), String> {
    let messages = race_messages(world_id, writer.principal, kind);
    let barrier = Barrier::new(messages.len());
    let responses = thread::scope(|scope| {
        let workers: Vec<_> = messages
            .iter()
            .map(|message| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    client
                        .post(submit.clone())
                        .header(CONTENT_TYPE, "application/json")
                        .bearer_auth(writer.token)
                        .body(message.to_string())
                        .send()
                        .map_err(|error| format!("concurrent submission failed: {error}"))
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "concurrent request worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let mut accepted = Vec::new();
    let mut conflicts = 0;
    for (message, response) in messages.iter().zip(responses) {
        if response.status() == StatusCode::CONFLICT {
            expect_problem(response, StatusCode::CONFLICT, "id_conflict")?;
            conflicts += 1;
        } else {
            let receipt = check_receipt(
                response,
                world_id,
                message["id"].as_str().expect("generated message id"),
            )?;
            accepted.push((message.clone(), receipt));
        }
    }
    let history = read_complete_history(client, events, world_id, writer.token)?;
    verify_race(kind, &messages, &accepted, conflicts, &history)
}

fn verify_race(
    kind: RaceKind,
    messages: &[Value],
    accepted: &[(Value, Value)],
    conflicts: usize,
    history: &[Value],
) -> Result<(), String> {
    let expected = match kind {
        RaceKind::Conflict => (1, 1),
        _ => (messages.len(), 0),
    };
    if (accepted.len(), conflicts) != expected {
        return Err(
            "concurrent submissions returned an incorrect success/conflict count".to_owned(),
        );
    }
    if matches!(kind, RaceKind::Exact)
        && accepted
            .iter()
            .any(|(_, receipt)| receipt != &accepted[0].1)
    {
        return Err("concurrent exact retries returned different receipts".to_owned());
    }
    let recorded: Vec<_> = history
        .iter()
        .filter(|event| {
            messages
                .iter()
                .any(|message| event["body"]["message"]["id"] == message["id"])
        })
        .collect();
    let expected_events = if matches!(kind, RaceKind::Distinct) {
        messages.len()
    } else {
        1
    };
    if recorded.len() != expected_events {
        return Err(
            "concurrent submissions left an incorrect number of recorded events".to_owned(),
        );
    }
    for (message, receipt) in accepted {
        let event = require_event(
            history,
            receipt["event_id"]
                .as_str()
                .ok_or("receipt omits event id")?,
        )?;
        if event["body"]["message"] != *message
            || event["sequence"] != receipt["sequence"]
            || event["actor"] != message["from"]
            || event["kind"] != "message.recorded"
        {
            return Err("concurrent receipt does not match its recorded message".to_owned());
        }
    }
    let mut sequences = std::collections::HashSet::new();
    let mut event_ids = std::collections::HashSet::new();
    if recorded.iter().any(|event| {
        !sequences.insert(event["sequence"].to_string())
            || !event_ids.insert(event["id"].to_string())
    }) {
        return Err("concurrent events reused an event id or sequence".to_owned());
    }
    Ok(())
}

const HIDDEN_CASES: [&str; 5] = [
    "collaborate.hidden_visibility",
    "collaborate.hidden_derivation",
    "collaborate.hidden_objection",
    "collaborate.hidden_decline",
    "collaborate.hidden_withdrawal",
];

pub(super) fn run_hidden_cases(ctx: &CollaborationRun<'_>, cases: &mut Vec<Case>) {
    if ctx.visibility == "members" {
        for id in HIDDEN_CASES {
            cases.push(Case::skipped_optional(id, "members visibility cannot construct a hidden revision; run a separate addressed or sender_only world"));
        }
        return;
    }
    let Some(peer) = ctx.peer else {
        for id in HIDDEN_CASES {
            cases.push(Case::failed(
                id,
                "hidden revision cases require a distinct writing peer",
            ));
        }
        return;
    };
    let source = artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &[ctx.writer.principal.to_owned()],
        "submission:hidden-source",
        "artifact:hidden-source",
        "Private source content.",
    );
    let setup = (|| {
        let response = post_collaboration(ctx, ctx.writer.token, &source.to_string().into_bytes())?;
        let receipt = check_receipt(response, ctx.world_id, "submission:hidden-source")?;
        if receipt["revision"] != 1 || receipt["artifact_id"] != "artifact:hidden-source" {
            return Err("hidden source did not receive revision 1".to_owned());
        }
        let writer_history = read_history(ctx, ctx.writer.token)?;
        let source_event = require_event(
            &writer_history,
            receipt["event_id"]
                .as_str()
                .ok_or("hidden source receipt omits event id")?,
        )?;
        if source_event["body"]["artifact_revision"]["artifact_id"] != "artifact:hidden-source" {
            return Err("hidden source was not stored for its author".to_owned());
        }
        let peer_history = read_history(ctx, peer.token)?;
        ensure_hidden(&peer_history, &receipt)?;
        Ok((receipt, writer_history, peer_history))
    })();
    let (receipt, writer_before, peer_before) = match setup {
        Ok(prepared) => {
            cases.push(Case::passed(HIDDEN_CASES[0]));
            prepared
        }
        Err(detail) => {
            cases.push(Case::failed(HIDDEN_CASES[0], detail));
            skip_ids(cases, &HIDDEN_CASES[1..], "hidden revision setup failed");
            return;
        }
    };
    for (id, kind) in
        HIDDEN_CASES[1..]
            .iter()
            .zip(["artifact_revision", "objection", "decline", "withdrawal"])
    {
        let result = (|| {
            let record = hidden_record(ctx, peer, kind);
            let response = post_collaboration(ctx, peer.token, &record.to_string().into_bytes())?;
            expect_problem(response, StatusCode::UNPROCESSABLE_ENTITY, "unknown_target")?;
            let writer_after = read_history(ctx, ctx.writer.token)?;
            let peer_after = read_history(ctx, peer.token)?;
            if writer_after != writer_before || peer_after != peer_before {
                return Err("hidden target rejection changed permitted history".to_owned());
            }
            ensure_hidden(&peer_after, &receipt)
        })();
        cases.push(result_case(id, result));
    }
}

fn ensure_hidden(history: &[Value], receipt: &Value) -> Result<(), String> {
    if history.iter().any(|event| {
        event["id"] == receipt["event_id"]
            || event["body"]["artifact_revision"]["artifact_id"] == "artifact:hidden-source"
    }) {
        Err("writing peer can read the hidden source revision".to_owned())
    } else {
        Ok(())
    }
}

fn hidden_record(ctx: &CollaborationRun<'_>, peer: Party<'_>, kind: &str) -> Value {
    let id = format!("submission:hidden-{kind}");
    if kind == "artifact_revision" {
        let mut record = artifact_submission(
            ctx.world_id,
            peer.principal,
            &[peer.principal.to_owned()],
            &id,
            "artifact:hidden-derived",
            "A forbidden derivation.",
        );
        record["derived_from"] = json!({"from": ctx.writer.principal, "artifact_id": "artifact:hidden-source", "revision": 1});
        record
    } else {
        let mut record = json!({
            "protocol_version": "0.1-draft", "type": kind, "id": id,
            "world": ctx.world_id, "from": peer.principal, "to": [peer.principal],
            "artifact_id": "artifact:hidden-source", "target_from": ctx.writer.principal, "revision": 1
        });
        if kind != "withdrawal" {
            record["body"] = json!({"text": "A hidden target must look absent."});
        }
        record
    }
}

#[cfg(test)]
mod tests;
