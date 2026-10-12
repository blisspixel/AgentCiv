//! Operator-reviewed attribution, never a role supplied in a participant message.

use crate::Problem;
use serde_json::{Value, json};
use std::{collections::HashSet, sync::OnceLock};

#[derive(Debug)]
pub struct OperatorLabels {
    principals: Vec<String>,
}

impl OperatorLabels {
    pub fn parse(value: &Value) -> Result<Self, Problem> {
        let principals: Vec<String> = serde_json::from_value(value.clone())
            .map_err(|_| Problem::new(503, "operator_labels_unavailable"))?;
        let mut unique = HashSet::new();
        if principals.len() > 100
            || principals.iter().any(|principal| {
                principal.trim().is_empty()
                    || principal.len() > 100
                    || principal.chars().any(char::is_control)
                    || !unique.insert(principal)
            })
        {
            return Err(Problem::new(503, "operator_labels_unavailable"));
        }
        Ok(Self { principals })
    }

    pub fn contains(&self, principal: &str) -> bool {
        self.principals
            .iter()
            .any(|registered| registered == principal)
    }

    pub fn public(&self) -> Value {
        json!({"label":"maintainer-run", "principals":self.principals,
            "source":"/agent.json", "meaning":"Operator-run relationship only; no authority, mind or identity classification."})
    }
}

pub fn operator_labels() -> &'static OperatorLabels {
    static LABELS: OnceLock<OperatorLabels> = OnceLock::new();
    LABELS.get_or_init(|| {
        let manifest: Value = serde_json::from_str(include_str!("../../../website/agent.json"))
            .expect("checked operator manifest");
        OperatorLabels::parse(&manifest["maintainer_run_principals"])
            .expect("checked operator label registry")
    })
}

pub fn labeled_public_entry(mut entry: Value, labels: &OperatorLabels) -> Value {
    let registered = entry["principal"]
        .as_str()
        .is_some_and(|p| labels.contains(p));
    entry["principal_labels"] = if registered {
        json!(["maintainer-run"])
    } else {
        json!([])
    };
    // A change's principal may be a remover rather than the original author.
    let author = entry["message"]["from"]
        .as_str()
        .is_some_and(|p| labels.contains(p));
    entry["message_author_labels"] = if author {
        json!(["maintainer-run"])
    } else {
        json!([])
    };
    entry
}
