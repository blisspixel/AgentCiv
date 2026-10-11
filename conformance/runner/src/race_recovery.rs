use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use super::*;

pub(super) fn run(
    discovery: &str,
    writer: (&str, &str),
    reader: (&str, &str),
    peer: Option<(&str, &str)>,
    phase: &str,
    path: &Path,
) -> Report {
    let mut cases = Vec::new();
    if let Err(detail) = observe(discovery, writer, reader, peer, phase, path, &mut cases) {
        cases.push(Case::failed("race_recovery.input", detail));
    }
    let mut report = Report {
        scope: if phase == "race-prepare" {
            "race-recovery-prepare"
        } else {
            "race-recovery-verify"
        },
        cases,
    };
    redact(&mut report, &secret_tokens(writer.1, reader.1, peer));
    report
}

fn observe(
    discovery: &str,
    writer: (&str, &str),
    reader: (&str, &str),
    peer: Option<(&str, &str)>,
    phase: &str,
    path: &Path,
    cases: &mut Vec<Case>,
) -> Result<(), String> {
    if writer.0.is_empty()
        || writer.1.is_empty()
        || reader.0.is_empty()
        || reader.1.is_empty()
        || writer.0 == reader.0
        || writer.1 == reader.1
    {
        return Err("race recovery requires distinct nonempty credentials".to_owned());
    }
    let (client, world, events, _, discovery) = extended_targets(discovery)?;
    if !collaboration_advertised(&world) || !collaboration_auth_is_bearer(&world) {
        return Err("race recovery requires advertised bearer collaboration".to_owned());
    }
    let ctx = CollaborationRun {
        client: &client,
        world_id: world["id"].as_str().ok_or("world id missing")?,
        visibility: world["history"]["visibility"]
            .as_str()
            .ok_or("visibility missing")?,
        events: &events,
        collaborate: endpoint_url(&discovery, &world, "collaborate")?,
        writer: Party {
            principal: writer.0,
            token: writer.1,
        },
        reader: Party {
            principal: reader.0,
            token: reader.1,
        },
        peer: peer.map(|(principal, token)| Party { principal, token }),
        audience: vec![reader.0.to_owned()],
    };
    if phase == "race-prepare" {
        if path.exists() || !read_history(&ctx, writer.1)?.is_empty() {
            return Err("race prepare needs fresh history and a new checkpoint path".to_owned());
        }
        collaboration_retries::run(&ctx, cases);
        if cases.iter().any(|case| case.status != CaseStatus::Passed) {
            return Err("race prepare did not pass every actual observation".to_owned());
        }
        let saved = json!({"version":"same-id-recovery/0.1","world":world["id"],"visibility":world["history"]["visibility"],"principal":writer.0,"exact":cases[0].evidence,"conflict":cases[1].evidence});
        let bytes = serde_json::to_vec_pretty(&saved)
            .map_err(|_| "race checkpoint serialization failed")?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES
            || secret_tokens(writer.1, reader.1, peer)
                .iter()
                .any(|token| saved.to_string().contains(token))
        {
            return Err("race checkpoint exceeded bounds or reflected a credential".to_owned());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "race checkpoint create failed")?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| "race checkpoint write failed")?;
        cases.push(Case::passed("race_recovery.checkpoint"));
    } else {
        let file = std::fs::File::open(path).map_err(|_| "race checkpoint open failed")?;
        let mut bytes = Vec::new();
        file.take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "race checkpoint read failed")?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("race checkpoint exceeded bounds".to_owned());
        }
        let saved: Value =
            serde_json::from_slice(&bytes).map_err(|_| "race checkpoint JSON invalid")?;
        if saved["version"] != "same-id-recovery/0.1"
            || saved["world"] != world["id"]
            || saved["visibility"] != world["history"]["visibility"]
            || saved["principal"] != writer.0
        {
            return Err(
                "race checkpoint condition differs from current caller and world".to_owned(),
            );
        }
        for (id, key, conflict) in [
            ("restart.same_id_retry", "exact", false),
            ("restart.same_id_conflict", "conflict", true),
        ] {
            cases.push(result_case(
                id,
                collaboration_retries::verify_saved(&ctx, &saved[key], conflict),
            ));
        }
    }
    Ok(())
}
