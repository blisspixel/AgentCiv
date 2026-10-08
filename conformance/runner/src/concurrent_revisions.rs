//! Four distinct public attempts exercise two independent authors' chains.
//! A client barrier does not establish any particular storage interleaving.
use std::sync::Barrier;
use std::thread;

use super::*;

const ARTIFACT: &str = "artifact:conformance-concurrent-revisions";
const IDS: [&str; 3] = [
    "collaborate.concurrent_revisions",
    "collaborate.concurrent_revisions_retry",
    "collaborate.concurrent_revisions_isolation",
];

struct Race {
    requests: Vec<Value>,
    bytes: Vec<Vec<u8>>,
    receipts: Vec<Value>,
    histories: [Vec<Value>; 2],
}

pub(super) fn requests(world: &str, principals: [&str; 2], audience: &[String]) -> Vec<Value> {
    principals
        .into_iter()
        .zip(["writer", "peer"])
        .flat_map(|(principal, label)| {
            (0..2).map(move |index| {
                artifact_submission(
                    world,
                    principal,
                    audience,
                    &format!("submission:concurrent-revisions-{label}-{index}"),
                    ARTIFACT,
                    &format!(
                        "Keep the exact {label} contribution {index} on its own author chain."
                    ),
                )
            })
        })
        .collect()
}

pub(super) fn run(ctx: &CollaborationRun<'_>, cases: &mut Vec<Case>) {
    let prepared = (|| {
        let peer = ctx
            .peer
            .ok_or("concurrent revisions require a distinct writing peer")?;
        let race = race(ctx, peer)?;
        let events = verify_records(ctx, peer, &race)?;
        Ok::<_, String>((peer, race, events))
    })();
    let (peer, race, events) = match prepared {
        Ok(prepared) => {
            cases.push(Case::passed(IDS[0]));
            prepared
        }
        Err(detail) => {
            cases.push(Case::failed(IDS[0], detail));
            skip_ids(
                cases,
                &IDS[1..],
                "concurrent revision prerequisite did not pass",
            );
            return;
        }
    };
    if let Err(detail) = verify_isolation(&events) {
        cases.push(Case::failed(IDS[2], detail));
        cases.push(Case::skipped(
            IDS[1],
            "concurrent chain isolation did not pass",
        ));
        return;
    }
    cases.push(Case::passed(IDS[2]));
    cases.push(result_case(IDS[1], retry(ctx, peer, &race)));
}

fn histories(ctx: &CollaborationRun<'_>, peer: Party<'_>) -> Result<[Vec<Value>; 2], String> {
    Ok([
        read_history(ctx, ctx.writer.token)?,
        read_history(ctx, peer.token)?,
    ])
}

fn race(ctx: &CollaborationRun<'_>, peer: Party<'_>) -> Result<Race, String> {
    let requests = requests(
        ctx.world_id,
        [ctx.writer.principal, peer.principal],
        &ctx.audience,
    );
    let bytes: Vec<_> = requests
        .iter()
        .map(|request| request.to_string().into_bytes())
        .collect();
    let barrier = Barrier::new(bytes.len());
    let responses = thread::scope(|scope| {
        let workers: Vec<_> = bytes
            .iter()
            .enumerate()
            .map(|(index, bytes)| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    let party = if index < 2 { ctx.writer } else { peer };
                    post_collaboration(ctx, party.token, bytes)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "concurrent artifact worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let receipts = requests
        .iter()
        .zip(responses)
        .map(|(request, response)| {
            check_receipt(
                response,
                ctx.world_id,
                request["id"].as_str().expect("fixture id"),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Race {
        requests,
        bytes,
        receipts,
        histories: histories(ctx, peer)?,
    })
}

fn verify_records(
    ctx: &CollaborationRun<'_>,
    peer: Party<'_>,
    race: &Race,
) -> Result<Vec<Value>, String> {
    let mut correlated = Vec::new();
    for (party_index, party) in [ctx.writer, peer].into_iter().enumerate() {
        let offset = party_index * 2;
        let mut revisions = std::collections::BTreeSet::new();
        let mut expected = Vec::new();
        for index in offset..offset + 2 {
            let receipt = &race.receipts[index];
            let revision = receipt["revision"]
                .as_u64()
                .ok_or("concurrent receipt omits assigned revision")?;
            if receipt["artifact_id"] != ARTIFACT || !revisions.insert(revision) {
                return Err(
                    "concurrent artifact receipt has the wrong chain or repeated revision"
                        .to_owned(),
                );
            }
            let event = require_event(
                &race.histories[party_index],
                receipt["event_id"]
                    .as_str()
                    .ok_or("concurrent receipt omits event id")?,
            )?;
            let mut stored = race.requests[index].clone();
            stored["revision"] = json!(revision);
            if event["world"] != ctx.world_id
                || event["kind"] != "artifact.recorded"
                || event["actor"] != party.principal
                || event["sequence"] != receipt["sequence"]
                || event["body"]["artifact_revision"] != stored
            {
                return Err(
                    "concurrent receipt and event do not preserve their exact author and request"
                        .to_owned(),
                );
            }
            expected.push(event.clone());
            correlated.push(event.clone());
        }
        if revisions != std::collections::BTreeSet::from([1, 2]) {
            return Err(
                "concurrent author chain did not assign exactly revisions 1 and 2".to_owned(),
            );
        }
        expected.sort_by_key(|event| event["sequence"].as_u64());
        let actual: Vec<_> = race.histories[party_index]
            .iter()
            .filter(|event| {
                event["body"]["artifact_revision"]["from"] == party.principal
                    && event["body"]["artifact_revision"]["artifact_id"] == ARTIFACT
            })
            .cloned()
            .collect();
        if actual != expected
            || actual
                .iter()
                .enumerate()
                .any(|(index, event)| event["body"]["artifact_revision"]["revision"] != (index + 1))
        {
            return Err("concurrent author history is missing, duplicated, changed, or numbered out of order".to_owned());
        }
    }
    Ok(correlated)
}

fn verify_isolation(events: &[Value]) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    let mut sequences = std::collections::HashSet::new();
    for event in events {
        if !ids.insert(event["id"].to_string()) || !sequences.insert(event["sequence"].to_string())
        {
            return Err(
                "independent concurrent chains reused an event identity or sequence".to_owned(),
            );
        }
    }
    Ok(())
}

fn retry(ctx: &CollaborationRun<'_>, peer: Party<'_>, race: &Race) -> Result<(), String> {
    for (index, bytes) in race.bytes.iter().enumerate() {
        let party = if index < 2 { ctx.writer } else { peer };
        let receipt = check_receipt(
            post_collaboration(ctx, party.token, bytes)?,
            ctx.world_id,
            race.requests[index]["id"].as_str().expect("fixture id"),
        )?;
        if receipt != race.receipts[index] {
            return Err("post-race exact artifact retry changed its original receipt".to_owned());
        }
    }
    if histories(ctx, peer)? != race.histories {
        return Err("post-race exact artifact retries changed permitted history".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
