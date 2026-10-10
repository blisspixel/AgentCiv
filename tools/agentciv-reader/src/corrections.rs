//! Offline view of later work that declared a disputed or superseded revision as its basis.
//! It follows only stored `derived_from` citations in a complete caller-view traversal.
//! A listing does not establish repair, that an objection is right, or anything about hidden records.

use crate::offers::{Identity, Validated, artifact, identity, original, validated};
use crate::{Error, Result};
use agentciv_archive::MAX_INPUT_BYTES;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const FORMAT: &str = "agentciv-correction-view/0.1-example";
pub const RESULT_LIMIT: usize = 50;

fn selector((from, artifact_id, revision): &Identity) -> Value {
    json!({"from":from,"artifact_id":artifact_id,"revision":revision})
}

fn objection_target(event: &Value) -> Option<Identity> {
    if event["kind"] != "objection.recorded" {
        return None;
    }
    let target = &event["body"]["objection"];
    Some((
        target["target_from"].as_str()?.to_owned(),
        target["artifact_id"].as_str()?.to_owned(),
        crate::integer(&target["revision"])?,
    ))
}

struct Graph {
    revisions: BTreeMap<Identity, usize>,
    citations: BTreeMap<usize, Identity>,
    children: BTreeMap<Identity, Vec<usize>>,
}

impl Graph {
    fn new(events: &[Value]) -> Self {
        let mut graph = Self {
            revisions: BTreeMap::new(),
            citations: BTreeMap::new(),
            children: BTreeMap::new(),
        };
        for (index, event) in events.iter().enumerate() {
            let Some(record) = artifact(event) else {
                continue;
            };
            let Some(current) = identity(record) else {
                continue;
            };
            graph.revisions.entry(current).or_insert(index);
            if let Some(cited) = record.get("derived_from").and_then(identity) {
                graph.children.entry(cited.clone()).or_default().push(index);
                graph.citations.insert(index, cited);
            }
        }
        graph
    }

    fn chain(&self, events: &[Value], index: usize) -> Identity {
        artifact(&events[index])
            .and_then(identity)
            .expect("indexed artifact identity")
    }

    /// Breadth-first over declared citations. The target's own chain is listed separately,
    /// and a visited set bounds the walk even if copied input contains a citation cycle.
    fn dependents(&self, events: &[Value], records: &[String], target: &Identity) -> Vec<Value> {
        let mut found = Vec::new();
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([(target.clone(), 0_usize)]);
        while let Some((parent, depth)) = queue.pop_front() {
            for &child in self.children.get(&parent).map_or(&[][..], Vec::as_slice) {
                let current = self.chain(events, child);
                if (current.0 == target.0 && current.1 == target.1) || !visited.insert(child) {
                    continue;
                }
                found.push(json!({"event_id":events[child]["id"],"from":current.0,
                    "artifact_id":current.1,"revision":current.2,"sequence":events[child]["sequence"],
                    "depth":depth + 1,"cites":selector(&parent),"record_utf8":records[child]}));
                queue.push_back((current, depth + 1));
            }
        }
        found
    }
}

/// Project corrections from a saved full `read` result, using the same input checks as offers.
pub fn project(raw: &str) -> Result<Value> {
    let Validated {
        world,
        report,
        records,
        events,
    } = validated(raw).map_err(|error| match error {
        Error::InvalidView => Error::InvalidCorrectionView,
        other => other,
    })?;
    let graph = Graph::new(&events);
    let mut objections: BTreeMap<Identity, Vec<usize>> = BTreeMap::new();
    for (index, event) in events.iter().enumerate() {
        if let Some(target) = objection_target(event) {
            objections.entry(target).or_default().push(index);
        }
    }
    let mut targets: BTreeSet<Identity> = objections.keys().cloned().collect();
    targets.extend(
        graph
            .revisions
            .keys()
            .filter(|candidate| {
                graph.revisions.keys().any(|later| {
                    later.0 == candidate.0 && later.1 == candidate.1 && later.2 > candidate.2
                })
            })
            .cloned(),
    );
    let mut concerns = Vec::new();
    for target in &targets {
        let objected = objections.get(target).map_or(&[][..], Vec::as_slice);
        let mut later: Vec<usize> = graph
            .revisions
            .iter()
            .filter(|(candidate, _)| {
                candidate.0 == target.0 && candidate.1 == target.1 && candidate.2 > target.2
            })
            .map(|(_, &index)| index)
            .collect();
        later.sort_unstable();
        let dependents = graph.dependents(&events, &records, target);
        if objected.is_empty() && dependents.is_empty() {
            continue;
        }
        let mut reasons = Vec::new();
        if !objected.is_empty() {
            reasons.push("objected");
        }
        if !later.is_empty() {
            reasons.push("later_revision_by_author");
        }
        let found = graph.revisions.get(target).copied();
        let mut concern = json!({"target":selector(target),
            "target_status":if found.is_some() {"available"} else {"unavailable"},
            "reasons":reasons,
            "objections":objected.iter().map(|&index| original(index, &events, &records)).collect::<Vec<_>>(),
            "later_revisions":later.iter().map(|&index| original(index, &events, &records)).collect::<Vec<_>>(),
            "dependents":dependents});
        if let Some(index) = found {
            concern["target_original"] = original(index, &events, &records);
        }
        let first = found
            .into_iter()
            .chain(objected.iter().copied())
            .min()
            .unwrap_or(usize::MAX);
        concerns.push((first, concern));
    }
    concerns.sort_by_key(|(first, _)| *first);
    let truncated = concerns.len() > RESULT_LIMIT;
    let concerns: Vec<Value> = concerns
        .into_iter()
        .take(RESULT_LIMIT)
        .map(|(_, concern)| concern)
        .collect();
    let unresolved: Vec<Value> = graph
        .citations
        .iter()
        .filter(|(_, cited)| !graph.revisions.contains_key(*cited))
        .map(|(&index, cited)| {
            let current = graph.chain(&events, index);
            json!({"event_id":events[index]["id"],"from":current.0,"artifact_id":current.1,
                "revision":current.2,"cites":selector(cited),"lineage":"unknown","record_utf8":records[index]})
        })
        .collect();
    let view = json!({"format":FORMAT,"world":world,"concerns":concerns,"unresolved_citations":unresolved,
        "report":{"scope":"current_caller_view","copying_permission":"not_granted","reached_end":true,
            "pages":report.pages,"events":report.events,"response_bytes":report.response_bytes,
            "displayed":concerns.len(),"result_limit":RESULT_LIMIT,"truncated":truncated,"shortened_fields":[],
            "dependency":"declared_derived_from_citations_only_never_inferred",
            "own_chain":"later_revisions_by_the_target_author_listed_separately_not_as_dependents",
            "visibility":"caller_view_only_hidden_dependents_objections_and_counts_not_represented",
            "unavailable_targets":"absent_hidden_or_withdrawn_cannot_be_distinguished",
            "interpretation":"listing_establishes_neither_repair_nor_that_an_objection_or_revision_is_right",
            "freshness":"full_revalidation_required_before_reliance_not_atomic_or_continuing_authorization",
            "input_authentication":"not_established_by_offline_projection"}});
    if serde_json::to_vec(&view).map_err(|_| Error::Output)?.len() > MAX_INPUT_BYTES {
        return Err(Error::Output);
    }
    Ok(view)
}

#[cfg(test)]
mod tests;
