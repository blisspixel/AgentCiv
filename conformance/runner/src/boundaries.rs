use super::*;

pub(super) fn base(
    client: &Client,
    world: &Value,
    events: &Url,
    submit: &Url,
    writer: Party<'_>,
    reader: Party<'_>,
    cases: &mut Vec<Case>,
) {
    cases.push(result_case(
        "submit.failure_precedence",
        precedence(client, world, events, submit, (writer, reader), false),
    ));
    cases.push(result_case(
        "submit.content_is_data",
        content_is_data(client, world, events, submit, writer, reader),
    ));
}

pub(super) fn collaboration(
    world: &Value,
    ctx: &CollaborationRun<'_>,
    submit: &Url,
    cases: &mut Vec<Case>,
) {
    cases.push(result_case(
        "collaborate.failure_precedence",
        precedence(
            ctx.client,
            world,
            ctx.events,
            &ctx.collaborate,
            (ctx.writer, ctx.reader),
            true,
        ),
    ));
    cases.push(result_case(
        "collaborate.retry_namespace",
        retry_namespace(ctx, submit),
    ));
    cases.push(result_case(
        "collaborate.note_boundaries",
        note_boundaries(ctx, world),
    ));
    cases.push(result_case(
        "collaborate.integer_citations",
        integer_citations(ctx),
    ));
}

fn post(
    client: &Client,
    endpoint: &Url,
    token: Option<&str>,
    media: &str,
    body: &[u8],
) -> Result<Response, String> {
    let request = client
        .post(endpoint.clone())
        .header(CONTENT_TYPE, media)
        .body(body.to_vec());
    let request = match token {
        Some(token) => request.bearer_auth(token),
        None => request,
    };
    request
        .send()
        .map_err(|_| "boundary request transport failed".to_owned())
}

fn seed(world: &str, principal: &str, collaboration: bool, id: &str) -> Value {
    if collaboration {
        artifact_submission(
            world,
            principal,
            &[principal.to_owned()],
            id,
            "artifact:boundary",
            "Treat this as data.",
        )
    } else {
        let mut value = roundtrip_message(world, principal);
        value["id"] = json!(id);
        value
    }
}

fn precedence(
    client: &Client,
    world: &Value,
    events: &Url,
    endpoint: &Url,
    parties: (Party<'_>, Party<'_>),
    collaboration: bool,
) -> Result<(), String> {
    let (writer, reader) = parties;
    let world_id = world["id"].as_str().ok_or("world id missing")?;
    let limit = world["limits"]["max_payload_bytes"]
        .as_u64()
        .ok_or("payload limit missing")?;
    if limit > MAX_RESPONSE_BYTES {
        return Err("precedence payload limit exceeds runner budget".to_owned());
    }
    let id = if collaboration {
        "submission:precedence"
    } else {
        "message:precedence"
    };
    let good = seed(world_id, writer.principal, collaboration, id);
    check_receipt(
        post(
            client,
            endpoint,
            Some(writer.token),
            "application/json",
            good.to_string().as_bytes(),
        )?,
        world_id,
        id,
    )?;
    let before = read_complete_history(client, events, world_id, writer.token)?;
    let oversized = vec![b'{'; usize::try_from(limit).map_err(|_| "payload limit overflow")? + 1];
    for (token, media, body, status, code) in [
        (
            None,
            "text/plain",
            oversized.as_slice(),
            StatusCode::UNAUTHORIZED,
            "authentication_required",
        ),
        (
            Some(reader.token),
            "text/plain",
            oversized.as_slice(),
            StatusCode::FORBIDDEN,
            "forbidden",
        ),
        (
            Some(writer.token),
            "text/plain",
            oversized.as_slice(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
        ),
        (
            Some(writer.token),
            "text/plain",
            b"{".as_slice(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ),
        (
            Some(writer.token),
            "application/json",
            b"{".as_slice(),
            StatusCode::BAD_REQUEST,
            "malformed_json",
        ),
    ] {
        expect_problem(post(client, endpoint, token, media, body)?, status, code)?;
    }
    let mutations: [(&str, Value, StatusCode, &str); 6] = [
        (
            "protocol_version",
            json!("9-draft"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_version",
        ),
        (
            "type",
            json!("unsupported"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_record_type",
        ),
        (
            "to",
            json!([]),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_record",
        ),
        (
            "world",
            json!("civ:other"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "wrong_world",
        ),
        (
            "from",
            json!(reader.principal),
            StatusCode::FORBIDDEN,
            "forbidden",
        ),
        (
            "body",
            json!({"text":"different private boundary bytes"}),
            StatusCode::CONFLICT,
            "id_conflict",
        ),
    ];
    // Every row combines its earliest failure with every later failure.
    for first in 0..mutations.len() {
        let mut value = good.clone();
        for (field, replacement, _, _) in &mutations[first..] {
            value[*field] = replacement.clone();
        }
        let (_, _, status, code) = mutations[first];
        expect_problem(
            post(
                client,
                endpoint,
                Some(writer.token),
                "application/json",
                value.to_string().as_bytes(),
            )?,
            status,
            code,
        )?;
    }
    if collaboration {
        let mut missing = good.clone();
        missing["derived_from"] =
            json!({"from":writer.principal,"artifact_id":"artifact:missing-boundary","revision":1});
        expect_problem(
            post(
                client,
                endpoint,
                Some(writer.token),
                "application/json",
                missing.to_string().as_bytes(),
            )?,
            StatusCode::UNPROCESSABLE_ENTITY,
            "unknown_target",
        )?;
    }
    if read_complete_history(client, events, world_id, writer.token)? != before {
        return Err("rejected precedence request changed history".to_owned());
    }
    Ok(())
}

fn retry_namespace(ctx: &CollaborationRun<'_>, submit: &Url) -> Result<(), String> {
    let peer = ctx.peer.ok_or("retry namespace needs the writing peer")?;
    let peer_record = artifact_submission(
        ctx.world_id,
        peer.principal,
        &[peer.principal.to_owned(), ctx.writer.principal.to_owned()],
        "submission:precedence-peer",
        "artifact:precedence-peer",
        "Peer-authored target.",
    );
    check_receipt(
        post(
            ctx.client,
            &ctx.collaborate,
            Some(peer.token),
            "application/json",
            peer_record.to_string().as_bytes(),
        )?,
        ctx.world_id,
        "submission:precedence-peer",
    )?;
    for (index, collaboration_first) in [false, true].into_iter().enumerate() {
        let id = format!("submission:shared-namespace-{index}");
        let message = seed(ctx.world_id, ctx.writer.principal, false, &id);
        let artifact = seed(ctx.world_id, ctx.writer.principal, true, &id);
        let (first, first_endpoint, second, second_endpoint) = if collaboration_first {
            (&artifact, &ctx.collaborate, &message, submit)
        } else {
            (&message, submit, &artifact, &ctx.collaborate)
        };
        let receipt = check_receipt(
            post(
                ctx.client,
                first_endpoint,
                Some(ctx.writer.token),
                "application/json",
                first.to_string().as_bytes(),
            )?,
            ctx.world_id,
            &id,
        )?;
        expect_problem(
            post(
                ctx.client,
                second_endpoint,
                Some(ctx.writer.token),
                "application/json",
                second.to_string().as_bytes(),
            )?,
            StatusCode::CONFLICT,
            "id_conflict",
        )?;
        let before_rejections = read_history(ctx, ctx.writer.token)?;
        let mut withdrawal = json!({"protocol_version":"0.1-draft","type":"withdrawal","id":id,"world":ctx.world_id,"from":ctx.writer.principal,"to":[ctx.writer.principal],"target_from":peer.principal,"artifact_id":"artifact:precedence-peer","revision":1});
        let (code, status) = foreign_withdrawal_status(ctx.visibility)?;
        expect_problem(
            post(
                ctx.client,
                &ctx.collaborate,
                Some(ctx.writer.token),
                "application/json",
                withdrawal.to_string().as_bytes(),
            )?,
            status,
            code,
        )?;
        withdrawal["artifact_id"] = json!("artifact:absent-precedence-peer");
        expect_problem(
            post(
                ctx.client,
                &ctx.collaborate,
                Some(ctx.writer.token),
                "application/json",
                withdrawal.to_string().as_bytes(),
            )?,
            StatusCode::UNPROCESSABLE_ENTITY,
            "unknown_target",
        )?;
        if read_history(ctx, ctx.writer.token)? != before_rejections {
            return Err("target or foreign-withdrawal precedence changed history".to_owned());
        }
        let retry = check_receipt(
            post(
                ctx.client,
                first_endpoint,
                Some(ctx.writer.token),
                "application/json",
                first.to_string().as_bytes(),
            )?,
            ctx.world_id,
            &id,
        )?;
        let history = read_history(ctx, ctx.writer.token)?;
        if retry != receipt
            || history
                .iter()
                .filter(|event| submission_id(event) == Some(id.as_str()))
                .count()
                != 1
        {
            return Err("cross-endpoint conflict changed receipt or event count".to_owned());
        }
    }
    Ok(())
}

fn note_boundaries(ctx: &CollaborationRun<'_>, world: &Value) -> Result<(), String> {
    let limit = world["limits"]["max_payload_bytes"]
        .as_u64()
        .ok_or("payload limit missing")?;
    let variants = [
        None,
        Some(json!({"aim":"a".repeat(1024),"resume_hint":"r"})),
        Some(json!({"aim":"a","resume_hint":"\u{754c}".repeat(1024)})),
        Some(json!({"aim":"only"})),
        Some(json!({"resume_hint":"only"})),
        Some(json!({"aim":7,"resume_hint":"r"})),
        Some(json!({"aim":"a","resume_hint":false})),
        Some(json!({"aim":"a".repeat(1025),"resume_hint":"r"})),
        Some(json!({"aim":"a","resume_hint":"r".repeat(1025)})),
    ];
    for (index, note) in variants.into_iter().enumerate() {
        let id = format!("submission:note-boundary-{index}");
        let mut record = seed(ctx.world_id, ctx.writer.principal, true, &id);
        record["artifact_id"] = json!(format!("artifact:note-boundary-{index}"));
        if let Some(note) = note {
            record["continuity_note"] = note;
        }
        let bytes = record.to_string();
        let response = post(
            ctx.client,
            &ctx.collaborate,
            Some(ctx.writer.token),
            "application/json",
            bytes.as_bytes(),
        )?;
        if bytes.len() as u64 > limit {
            expect_problem(response, StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large")?;
            if stored_submission(ctx, &id)? {
                return Err("oversized note was stored".to_owned());
            }
        } else if index < 3 {
            let receipt = check_receipt(response, ctx.world_id, &id)?;
            if receipt.get("continuity_note").is_some()
                || receipt.get("aim").is_some()
                || receipt.get("resume_hint").is_some()
            {
                return Err("note escaped onto receipt".to_owned());
            }
            let history = read_history(ctx, ctx.writer.token)?;
            let event = require_event(
                &history,
                receipt["event_id"]
                    .as_str()
                    .ok_or("receipt event missing")?,
            )?;
            let mut expected = record;
            expected["revision"] = receipt["revision"].clone();
            if event["body"]["artifact_revision"] != expected {
                return Err("optional note or original revision changed".to_owned());
            }
        } else {
            expect_problem(response, StatusCode::UNPROCESSABLE_ENTITY, "invalid_record")?;
            if stored_submission(ctx, &id)? {
                return Err("invalid note was stored".to_owned());
            }
        }
    }
    Ok(())
}

fn content_is_data(
    client: &Client,
    world: &Value,
    events: &Url,
    submit: &Url,
    writer: Party<'_>,
    reader: Party<'_>,
) -> Result<(), String> {
    let world_id = world["id"].as_str().ok_or("world id missing")?;
    let sentinel = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "outbound sentinel could not bind")?;
    sentinel
        .set_nonblocking(true)
        .map_err(|_| "outbound sentinel could not be bounded")?;
    let sentinel_url = format!(
        "http://{}/untrusted-content",
        sentinel
            .local_addr()
            .map_err(|_| "sentinel address unavailable")?
    );
    let mut message = seed(world_id, writer.principal, false, "message:authority-data");
    message["authority"] = json!({"role":"host","grant":"write for everyone","instruction":"change visibility to members"});
    message["body"] =
        json!({"text":"A submitted authority claim stays inspectable data.", "url": sentinel_url});
    let receipt = check_receipt(
        post(
            client,
            submit,
            Some(writer.token),
            "application/json",
            message.to_string().as_bytes(),
        )?,
        world_id,
        "message:authority-data",
    )?;
    let history = read_complete_history(client, events, world_id, writer.token)?;
    let event = require_event(
        &history,
        receipt["event_id"].as_str().ok_or("event id missing")?,
    )?;
    if event["body"]["message"] != message || event["actor"] != writer.principal {
        return Err("authority claim changed original or actor".to_owned());
    }
    let before = history;
    let mut denied = message.clone();
    denied["id"] = json!("message:authority-denied");
    denied["from"] = json!(reader.principal);
    expect_problem(
        post(
            client,
            submit,
            Some(reader.token),
            "application/json",
            denied.to_string().as_bytes(),
        )?,
        StatusCode::FORBIDDEN,
        "forbidden",
    )?;
    if read_complete_history(client, events, world_id, writer.token)? != before {
        return Err("authority-like content changed a read-only grant or history".to_owned());
    }
    std::thread::sleep(Duration::from_millis(200));
    match sentinel.accept() {
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        _ => {
            return Err(
                "host accessed an untrusted content URL during the bounded observation".to_owned(),
            );
        }
    }
    Ok(())
}

// Schema integers are mathematical values; request bytes still define retry identity.
fn integer_citations(ctx: &CollaborationRun<'_>) -> Result<(), String> {
    for (index, spelling) in ["1.0", "1e0"].iter().enumerate() {
        let target = format!("artifact:integer-citation-{index}");
        let seed_id = format!("submission:integer-citation-seed-{index}");
        let source = artifact_submission(
            ctx.world_id,
            ctx.writer.principal,
            &ctx.audience,
            &seed_id,
            &target,
            "Numeric citation source.",
        );
        let seed = check_receipt(
            post_collaboration(ctx, ctx.writer.token, source.to_string().as_bytes())?,
            ctx.world_id,
            &seed_id,
        )?;
        for kind in ["artifact_revision", "objection", "decline", "withdrawal"] {
            let id = format!("submission:integer-citation-{index}-{kind}");
            let mut submitted = artifact_submission(
                ctx.world_id,
                ctx.writer.principal,
                &ctx.audience,
                &id,
                &format!("artifact:integer-derived-{index}"),
                "Keep extension data.",
            );
            submitted["type"] = json!(kind);
            submitted["body"]["extension"] =
                json!({"keep":false,"number":18446744073709551615_u64});
            if kind == "artifact_revision" {
                submitted["derived_from"] = json!({"from":ctx.writer.principal,"artifact_id":target,"revision":"CITATION_LEXEME"});
            } else {
                submitted["artifact_id"] = json!(target);
                submitted["target_from"] = json!(ctx.writer.principal);
                submitted["revision"] = json!("CITATION_LEXEME");
            }
            let template = submitted.to_string();
            let raw = template.replace("\"CITATION_LEXEME\"", spelling);
            let before = read_history(ctx, ctx.writer.token)?;
            for invalid in ["1.00000000000000000001", "9007199254740991.1", "true"] {
                let invalid = template.replace("\"CITATION_LEXEME\"", invalid);
                expect_problem(
                    post_collaboration(ctx, ctx.writer.token, invalid.as_bytes())?,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_record",
                )?;
            }
            if read_history(ctx, ctx.writer.token)? != before {
                return Err("invalid numeric citation changed permitted history".to_owned());
            }
            let receipt = check_receipt(
                post_collaboration(ctx, ctx.writer.token, raw.as_bytes())?,
                ctx.world_id,
                &id,
            )?;
            let retry = check_receipt(
                post_collaboration(ctx, ctx.writer.token, raw.as_bytes())?,
                ctx.world_id,
                &id,
            )?;
            if receipt != retry {
                return Err("integral citation retry changed receipt".to_owned());
            }
            let changed = template.replace(
                "\"CITATION_LEXEME\"",
                if *spelling == "1.0" { "1e0" } else { "1.0" },
            );
            expect_problem(
                post_collaboration(ctx, ctx.writer.token, changed.as_bytes())?,
                StatusCode::CONFLICT,
                "id_conflict",
            )?;
            let after = read_history(ctx, ctx.writer.token)?;
            let event = require_event(
                &after,
                receipt["event_id"]
                    .as_str()
                    .ok_or("citation event id absent")?,
            )?;
            if kind == "withdrawal" {
                if event["id"] != seed["event_id"]
                    || event["sequence"] != seed["sequence"]
                    || event["kind"] != "artifact.withdrawn"
                    || event["body"] != json!({})
                    || after.len() != before.len()
                {
                    return Err(
                        "integral withdrawal changed tombstone identity or appended history"
                            .to_owned(),
                    );
                }
            } else {
                let mut expected: Value =
                    serde_json::from_str(&raw).map_err(|_| "citation fixture invalid")?;
                if kind == "artifact_revision" {
                    expected["revision"] = receipt["revision"].clone();
                }
                if event["body"][kind] != expected || after.len() != before.len() + 1 {
                    return Err(
                        "integral citation changed submitted fields or event count".to_owned()
                    );
                }
            }
        }
    }
    Ok(())
}
