//! Bounded public same-ID attempts. The barrier synchronizes clients, not storage.
use std::sync::{Condvar, Mutex};
use std::thread;

use super::*;

const IDS: [&str; 3] = [
    "collaborate.concurrent_retry",
    "collaborate.concurrent_conflict",
    "collaborate.concurrent_retry_sequence",
];

#[derive(Clone, Copy)]
enum Kind {
    Exact,
    Conflict,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Conflict => "conflict",
        }
    }
}

struct Observed {
    status: StatusCode,
    no_store: bool,
    body: Value,
}

fn observe(response: Response) -> Result<Observed, String> {
    let status = response.status();
    let no_store = has_no_store(&response);
    let media = if status == StatusCode::OK {
        "application/json"
    } else {
        "application/problem+json"
    };
    Ok(Observed {
        status,
        no_store,
        body: read_json(response, media).map_err(|_| "invalid race response".to_owned())?,
    })
}

fn request(ctx: &CollaborationRun<'_>, kind: Kind, index: usize) -> Value {
    let label = kind.label();
    artifact_submission(
        ctx.world_id,
        ctx.writer.principal,
        &ctx.audience,
        &format!("submission:collaboration-race-{label}"),
        &format!("artifact:collaboration-race-{label}"),
        &format!(
            "Keep {label} candidate {} exactly.",
            if matches!(kind, Kind::Exact) {
                0
            } else {
                index
            }
        ),
    )
}

// A timed rendezvous avoids an unbounded wait if a worker fails before arrival.
fn race(ctx: &CollaborationRun<'_>, requests: &[Value; 2]) -> [Result<Observed, String>; 2] {
    let gate = (Mutex::new(0_u8), Condvar::new());
    thread::scope(|scope| {
        let workers: Vec<_> = requests
            .iter()
            .map(|request| {
                let gate = &gate;
                scope.spawn(move || {
                    let (lock, ready) = gate;
                    let mut count = lock.lock().map_err(|_| "race barrier failed".to_owned())?;
                    *count += 1;
                    ready.notify_all();
                    let (count, _) = ready
                        .wait_timeout_while(count, Duration::from_secs(5), |count| *count < 2)
                        .map_err(|_| "race barrier failed".to_owned())?;
                    if *count != 2 {
                        return Err("race barrier timed out".to_owned());
                    }
                    drop(count);
                    observe(
                        post_collaboration(ctx, ctx.writer.token, request.to_string().as_bytes())
                            .map_err(|_| "race request failed".to_owned())?,
                    )
                })
            })
            .collect();
        let mut responses = workers.into_iter().map(|worker| {
            worker
                .join()
                .unwrap_or_else(|_| Err("race worker failed".to_owned()))
        });
        [
            responses.next().expect("two workers"),
            responses.next().expect("two workers"),
        ]
    })
}

fn verify_responses(
    kind: Kind,
    requests: &[Value; 2],
    responses: &[Result<Observed, String>; 2],
) -> Result<usize, String> {
    let mut accepted = Vec::new();
    let mut conflicts = 0;
    for (index, result) in responses.iter().enumerate() {
        let response = result.as_ref().map_err(Clone::clone)?;
        if !response.no_store {
            return Err("race response omits no-store".to_owned());
        }
        if response.status == StatusCode::OK {
            validate(RECEIPT_SCHEMA, &response.body)
                .map_err(|_| "invalid race receipt".to_owned())?;
            if response.body["world"] != requests[index]["world"]
                || response.body["record_id"] != requests[index]["id"]
                || response.body["artifact_id"] != requests[index]["artifact_id"]
                || response.body["revision"] != 1
            {
                return Err("race receipt names the wrong record or revision".to_owned());
            }
            accepted.push(index);
        } else if response.status == StatusCode::CONFLICT {
            validate(PROBLEM_SCHEMA, &response.body)
                .map_err(|_| "invalid race conflict".to_owned())?;
            if response.body["status"] != 409 || response.body["code"] != "id_conflict" {
                return Err("race conflict uses the wrong problem".to_owned());
            }
            conflicts += 1;
        } else {
            return Err("race response has an unexpected status".to_owned());
        }
    }
    let expected = if matches!(kind, Kind::Exact) {
        (2, 0)
    } else {
        (1, 1)
    };
    if (accepted.len(), conflicts) != expected {
        return Err("race success and conflict counts differ".to_owned());
    }
    if matches!(kind, Kind::Exact)
        && responses[0].as_ref().expect("checked response").body
            != responses[1].as_ref().expect("checked response").body
    {
        return Err("simultaneous exact retries changed their receipt".to_owned());
    }
    Ok(accepted[0])
}

fn verify_history(
    ctx: &CollaborationRun<'_>,
    request: &Value,
    receipt: &Value,
    history: &[Value],
    revision: u64,
) -> Result<Value, String> {
    let event = require_event(
        history,
        receipt["event_id"]
            .as_str()
            .ok_or("race event id missing")?,
    )
    .map_err(|_| "race event missing or duplicated".to_owned())?;
    let mut stored = request.clone();
    stored["revision"] = json!(revision);
    if event["kind"] != "artifact.recorded"
        || event["actor"] != ctx.writer.principal
        || event["world"] != ctx.world_id
        || event["sequence"] != receipt["sequence"]
        || event["body"]["artifact_revision"] != stored
    {
        return Err("race event does not match its winning request and receipt".to_owned());
    }
    Ok(event.clone())
}

fn chain<'a>(history: &'a [Value], request: &Value) -> Vec<&'a Value> {
    history
        .iter()
        .filter(|event| {
            event["body"]["artifact_revision"]["from"] == request["from"]
                && event["body"]["artifact_revision"]["artifact_id"] == request["artifact_id"]
        })
        .collect()
}

fn exercise(ctx: &CollaborationRun<'_>, kind: Kind, evidence: &mut Value) -> Result<(), String> {
    let requests = [request(ctx, kind, 0), request(ctx, kind, 1)];
    evidence["request_bytes"] = json!(requests.iter().map(Value::to_string).collect::<Vec<_>>());
    let responses = race(ctx, &requests);
    evidence["responses"] = json!(responses.iter().map(|response| match response {
        Ok(response) => json!({"status":response.status.as_u16(), "no_store":response.no_store, "body":response.body}),
        Err(error) => json!({"error":error}),
    }).collect::<Vec<_>>());
    let winner = verify_responses(kind, &requests, &responses)?;
    let request = &requests[winner];
    let receipt = &responses[winner].as_ref().expect("verified winner").body;
    let history =
        read_history(ctx, ctx.writer.token).map_err(|_| "race history unavailable".to_owned())?;
    let first = verify_history(ctx, request, receipt, &history, 1)?;
    evidence["winning_event"] = first.clone();
    if chain(&history, request) != vec![&first] {
        return Err("race recorded an extra or losing revision".to_owned());
    }
    let mut later = request.clone();
    later["id"] = json!(format!(
        "{}-later",
        request["id"].as_str().expect("fixture id")
    ));
    later["body"]["text"] = json!("Continue after both simultaneous responses.");
    evidence["later_request_bytes"] = json!(later.to_string());
    let later_receipt = check_receipt(
        post_collaboration(ctx, ctx.writer.token, later.to_string().as_bytes())
            .map_err(|_| "later race request failed".to_owned())?,
        ctx.world_id,
        later["id"].as_str().expect("fixture id"),
    )
    .map_err(|_| "later race receipt invalid".to_owned())?;
    evidence["later_receipt"] = later_receipt.clone();
    if later_receipt["artifact_id"] != request["artifact_id"] || later_receipt["revision"] != 2 {
        return Err("race retry or conflict consumed the next revision".to_owned());
    }
    let retry = check_receipt(
        post_collaboration(ctx, ctx.writer.token, request.to_string().as_bytes())
            .map_err(|_| "post-race retry request failed".to_owned())?,
        ctx.world_id,
        request["id"].as_str().expect("fixture id"),
    )
    .map_err(|_| "post-race retry receipt invalid".to_owned())?;
    evidence["retry_receipt"] = retry.clone();
    if &retry != receipt {
        return Err("post-race retry changed its original receipt".to_owned());
    }
    let history = read_history(ctx, ctx.writer.token)
        .map_err(|_| "later race history unavailable".to_owned())?;
    let second = verify_history(ctx, &later, &later_receipt, &history, 2)?;
    evidence["later_event"] = second.clone();
    if chain(&history, request) != vec![&first, &second]
        || second["sequence"].as_u64() <= first["sequence"].as_u64()
        || second["id"] == first["id"]
    {
        return Err("later race history changed or duplicated its chain".to_owned());
    }
    Ok(())
}

pub(super) fn run(ctx: &CollaborationRun<'_>, cases: &mut Vec<Case>) {
    let mut passed = true;
    for (id, kind) in [(IDS[0], Kind::Exact), (IDS[1], Kind::Conflict)] {
        let mut evidence = json!({"workers":2, "barrier_timeout_seconds":5,
            "request_timeout_seconds":10, "max_requests":20,
            "max_history_pages":8, "max_response_bytes":MAX_RESPONSE_BYTES});
        let result = exercise(ctx, kind, &mut evidence);
        passed &= result.is_ok();
        // Never export credential reflections from even a faulty host.
        let serialized = evidence.to_string();
        let reflected = [Some(ctx.writer), Some(ctx.reader), ctx.peer]
            .into_iter()
            .flatten()
            .any(|party| {
                let encoded = serde_json::to_string(party.token).expect("string serialization");
                serialized.contains(party.token)
                    || serialized.contains(&encoded[1..encoded.len() - 1])
            });
        let mut case = result_case(id, result);
        if reflected {
            case = Case::failed(
                id,
                "race evidence withheld because a host reflected a credential",
            );
            passed = false;
        } else {
            case.evidence = Some(evidence);
        }
        cases.push(case);
    }
    cases.push(if passed {
        Case::passed(IDS[2])
    } else {
        Case::skipped(
            IDS[2],
            "same-ID race and subsequent chain checks did not both pass",
        )
    });
}

#[cfg(test)]
mod tests;
