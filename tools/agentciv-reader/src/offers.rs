//! Offline projection of example offers from a complete caller-view traversal.
//! This validates copied syntax, not a live grant, source truth, or acceptance.

use crate::{Budgets, Error, Result, Traversal, collect};
use agentciv_archive::{MAX_ENTRIES, MAX_INPUT_BYTES, MAX_RECORD_BYTES, parse_unique};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const FORMAT: &str = "agentciv-activity-offer/0.1-example";
pub const RESULT_LIMIT: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    snapshot: Snapshot,
    report: NativeReport,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    world: String,
    records: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct NativeReport {
    pub(crate) pages: usize,
    pub(crate) events: usize,
    pub(crate) response_bytes: usize,
    reached_end: bool,
    scope: String,
    copying_permission: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Offer {
    format: String,
    title: String,
    purpose: String,
    offered_scope: String,
    limitations: String,
    copying_conditions: String,
    source_refs: Vec<Source>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    event_id: String,
    from: String,
    artifact_id: String,
    revision: u64,
}

pub(crate) type Identity = (String, String, u64);

fn bounded_text(text: &str, scalars: usize, bytes: usize, empty: bool) -> bool {
    (empty || !text.is_empty())
        && text.len() <= bytes
        && text.chars().count() <= scalars
        && !text.chars().any(char::is_control)
}

fn valid_offer(value: &Value) -> Option<Offer> {
    let offer: Offer = serde_json::from_value(value.clone()).ok()?;
    let mut references = BTreeSet::new();
    (offer.format == FORMAT
        && [
            &offer.title,
            &offer.purpose,
            &offer.offered_scope,
            &offer.limitations,
            &offer.copying_conditions,
        ]
        .iter()
        .all(|text| bounded_text(text, 1024, 4096, false))
        && offer.source_refs.len() <= 16
        && offer.source_refs.iter().all(|source| {
            [&source.event_id, &source.from, &source.artifact_id]
                .iter()
                .all(|text| bounded_text(text, 256, 1024, false))
                && (1..=9_007_199_254_740_991).contains(&source.revision)
                && references.insert((
                    &source.event_id,
                    &source.from,
                    &source.artifact_id,
                    source.revision,
                ))
        }))
    .then_some(offer)
}

pub(crate) fn identity(record: &Value) -> Option<Identity> {
    Some((
        record["from"].as_str()?.to_owned(),
        record["artifact_id"].as_str()?.to_owned(),
        crate::integer(&record["revision"])?,
    ))
}

pub(crate) fn artifact(event: &Value) -> Option<&Value> {
    (event["kind"] == "artifact.recorded").then_some(&event["body"]["artifact_revision"])
}

pub(crate) fn original(index: usize, events: &[Value], records: &[String]) -> Value {
    json!({"event_id":events[index]["id"],"record_utf8":records[index]})
}

fn source_view(source: &Source, events: &[Value], records: &[String]) -> Value {
    let selected = events
        .iter()
        .position(|event| event["id"] == source.event_id);
    let expected = (
        source.from.clone(),
        source.artifact_id.clone(),
        source.revision,
    );
    let status = match selected {
        Some(index) if events[index]["kind"] == "artifact.withdrawn" => "withdrawn",
        Some(index) if artifact(&events[index]).and_then(identity).as_ref() == Some(&expected) => {
            "available"
        }
        _ => "unavailable",
    };
    let mut value = json!({"event_id":source.event_id,"from":source.from,"artifact_id":source.artifact_id,
        "revision":source.revision,"status":status});
    if status != "unavailable" {
        value["record_utf8"] = json!(records[selected.expect("available source index")]);
    }
    value
}

fn project_offer(index: usize, offer: &Offer, events: &[Value], records: &[String]) -> Value {
    let event = &events[index];
    let record = artifact(event).expect("eligible artifact");
    let current = identity(record).expect("validated identity");
    let mut relevant = BTreeSet::from([current.clone()]);
    let mut earlier = Vec::new();
    for (earlier_index, previous) in events.iter().enumerate() {
        if let Some(previous_identity) = artifact(previous).and_then(identity)
            && previous_identity.0 == current.0
            && previous_identity.1 == current.1
            && previous_identity.2 < current.2
        {
            relevant.insert(previous_identity);
            earlier.push(original(earlier_index, events, records));
        }
    }
    let sources: Vec<Value> = offer
        .source_refs
        .iter()
        .map(|source| {
            relevant.insert((
                source.from.clone(),
                source.artifact_id.clone(),
                source.revision,
            ));
            source_view(source, events, records)
        })
        .collect();
    let derivation = if let Some(derived) = record.get("derived_from") {
        let target = identity(derived).expect("validated derivation");
        relevant.insert(target.clone());
        let found = events
            .iter()
            .position(|event| artifact(event).and_then(identity).as_ref() == Some(&target));
        match found {
            Some(found) => {
                json!({"status":"available","selector":derived,"original":original(found,events,records)})
            }
            None => json!({"status":"unavailable","selector":derived}),
        }
    } else {
        json!({"status":"not_declared"})
    };
    let related: Vec<Value> = events
        .iter()
        .enumerate()
        .filter_map(|(related_index, event)| {
            let key = match event["kind"].as_str() {
                Some("objection.recorded") => "objection",
                Some("decline.recorded") => "decline",
                _ => return None,
            };
            let target = &event["body"][key];
            let target = (
                target["target_from"].as_str()?.to_owned(),
                target["artifact_id"].as_str()?.to_owned(),
                crate::integer(&target["revision"])?,
            );
            relevant
                .contains(&target)
                .then(|| original(related_index, events, records))
        })
        .collect();
    json!({"world":event["world"],"event_id":event["id"],"from":record["from"],
        "artifact_id":record["artifact_id"],"revision":record["revision"],"sequence":event["sequence"],
        "offer":record["body"]["activity_offer"],"record_utf8":records[index],
        "sources":sources,"earlier_revisions":earlier,"related_records":related,"derivation":derivation})
}

/// A full caller-view traversal whose exact originals were revalidated.
pub(crate) struct Validated {
    pub(crate) world: String,
    pub(crate) report: NativeReport,
    pub(crate) records: Vec<String>,
    pub(crate) events: Vec<Value>,
}

/// Revalidate every original in a saved `read` result. Failure returns no usable view.
pub(crate) fn validated(raw: &str) -> Result<Validated> {
    if raw.len() > MAX_INPUT_BYTES {
        return Err(Error::InputLimit);
    }
    let value = parse_unique(raw).map_err(|_| Error::InvalidJson)?;
    let input: Input = serde_json::from_value(value).map_err(|_| Error::InvalidView)?;
    let report = input.report;
    let records = input.snapshot.records;
    let world = input.snapshot.world;
    if report.scope != "current_caller_view"
        || report.copying_permission != "not_granted"
        || !report.reached_end
        || !(1..=256).contains(&report.pages)
        || report.pages < records.len().div_ceil(100)
        || report.events != records.len()
        || records.len() > MAX_ENTRIES
        || report.response_bytes > MAX_INPUT_BYTES
        || report.response_bytes < records.iter().map(String::len).sum::<usize>()
        || records.iter().any(|record| record.len() > MAX_RECORD_BYTES)
    {
        return Err(Error::InvalidView);
    }
    let mut offset = 0;
    let budgets = Budgets {
        max_response_bytes: MAX_INPUT_BYTES,
        max_total_bytes: MAX_INPUT_BYTES,
        ..Budgets::default()
    };
    collect(&world, &budgets, Traversal::All, None, |_, _, _| {
        let end = (offset + 100).min(records.len());
        let page = format!(
            "{{\"protocol_version\":\"0.1-draft\",\"type\":\"event_page\",\"world\":{},\"events\":[{}],\"has_more\":{},\"next_cursor\":\"projection:{end}\"}}",
            serde_json::to_string(&world).map_err(|_| Error::InvalidView)?,
            records[offset..end].join(","),
            end < records.len()
        );
        offset = end;
        Ok(page.into_bytes())
    })?;
    let events: Vec<Value> = records
        .iter()
        .map(|record| parse_unique(record).map_err(|_| Error::InvalidEvent))
        .collect::<Result<_>>()?;
    Ok(Validated {
        world,
        report,
        records,
        events,
    })
}

/// Revalidate all originals before selecting. Failure returns no usable projection.
/// Only records already in this input can affect output, including omission counts.
pub fn project(raw: &str, query: &str) -> Result<Value> {
    if !bounded_text(query, 128, 512, true) {
        return Err(Error::InvalidQuery);
    }
    let Validated {
        world,
        report,
        records,
        events,
    } = validated(raw)?;
    let records = &records;
    let mut latest = BTreeMap::new();
    let mut tombstones = Vec::new();
    for (index, event) in events.iter().enumerate() {
        if let Some((author, artifact_id, revision)) = artifact(event).and_then(identity) {
            latest
                .entry((author, artifact_id))
                .and_modify(|entry: &mut (u64, usize)| {
                    if revision > entry.0 {
                        *entry = (revision, index);
                    }
                })
                .or_insert((revision, index));
        }
        if event["kind"] == "artifact.withdrawn" || event["kind"] == "event.redacted" {
            tombstones.push(index);
        }
    }
    let mut superseded = 0;
    let mut malformed = 0;
    let mut uncertain = 0;
    let mut matches = Vec::new();
    for (index, event) in events.iter().enumerate() {
        let Some(record) = artifact(event) else {
            continue;
        };
        let Some(body) = record["body"].get("activity_offer") else {
            continue;
        };
        let (author, artifact_id, _) = identity(record).expect("validated artifact identity");
        if latest[&(author.clone(), artifact_id)].1 != index {
            superseded += 1;
            continue;
        }
        if tombstones.iter().any(|&withdrawn| {
            withdrawn > index
                && (events[withdrawn]["actor"].is_null() || events[withdrawn]["actor"] == author)
        }) {
            uncertain += 1;
            continue;
        }
        let Some(offer) = valid_offer(body) else {
            malformed += 1;
            continue;
        };
        if query.is_empty()
            || offer.title.contains(query)
            || offer.purpose.contains(query)
            || record["body"]["text"]
                .as_str()
                .is_some_and(|text| text.contains(query))
        {
            matches.push((index, offer));
        }
    }
    matches.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
    let truncated = matches.len() > RESULT_LIMIT;
    let offers: Vec<Value> = matches
        .iter()
        .take(RESULT_LIMIT)
        .map(|(index, offer)| project_offer(*index, offer, &events, records))
        .collect();
    let tombstones: Vec<Value> = tombstones
        .iter()
        .map(|&index| original(index, &events, records))
        .collect();
    let view = json!({"format":"agentciv-offer-view/0.1-example","world":world,"query":query,"offers":offers,"tombstones":tombstones,
        "report":{"scope":"current_caller_view","copying_permission":"not_granted","reached_end":true,"pages":report.pages,"events":report.events,"response_bytes":report.response_bytes,
            "displayed":offers.len(),"result_limit":RESULT_LIMIT,"truncated":truncated,"shortened_fields":[],
            "omissions":{"superseded_offer_revisions":superseded,"malformed_latest_offers":malformed,"uncertain_chain_offers":uncertain},
            "retrieval_times":"not_recorded_in_native_input","configured_budgets":"not_recorded_in_native_input",
            "freshness":"full_revalidation_required_before_reliance_not_atomic_or_continuing_authorization",
            "input_authentication":"not_established_by_offline_projection","source_support":"availability_and_exact_identity_only_not_truth_or_prose_support",
            "selection":"latest_retrieved_author_revision_newest_sequence_first_no_global_count",
            "recovery":"no_saved_cursor_or_stale_fallback_current_retention_may_omit_older_history"}});
    if serde_json::to_vec(&view).map_err(|_| Error::Output)?.len() > MAX_INPUT_BYTES {
        return Err(Error::Output);
    }
    Ok(view)
}

#[cfg(test)]
mod tests;
