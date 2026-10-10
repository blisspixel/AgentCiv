//! Public HTTP routes. Storage work runs on a blocking thread and never across an await.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::store::{CollaborateError, ReadError, Store, SubmitError};
use crate::validate::{collaboration_error, exact_integers, message_error, world_matches};
use crate::{Credential, Visibility};

#[derive(Clone)]
pub struct App {
    pub world_id: String,
    pub title: String,
    pub visibility: Visibility,
    pub retention_seconds: i64,
    pub max_payload: usize,
    pub origin: String,
    pub credentials: Arc<[Credential]>,
    pub store: Store,
}

pub fn router(app: App) -> Router {
    Router::new()
        .route("/.well-known/agentciv", get(discover))
        .route("/submit", post(submit))
        .route("/collaborate", post(collaborate))
        .route("/events", get(events))
        .with_state(Arc::new(app))
}

async fn discover(State(app): State<Arc<App>>) -> Response {
    Json(json!({
        "protocol_version": "0.1-draft",
        "profile": "http-commons/0.1-draft",
        "type": "world",
        "id": app.world_id,
        "title": app.title,
        "capabilities": ["events.read", "messages.submit", "collaboration.submit"],
        "endpoints": {
            "events": format!("{}/events", app.origin),
            "submit": format!("{}/submit", app.origin),
            "collaborate": format!("{}/collaborate", app.origin)
        },
        "history": {
            "visibility": app.visibility.as_str(),
            "retention_seconds": app.retention_seconds
        },
        "authentication": {"events": "bearer", "submit": "bearer", "collaborate": "bearer"},
        "limits": {"max_payload_bytes": app.max_payload}
    }))
    .into_response()
}

async fn submit(State(app): State<Arc<App>>, headers: HeaderMap, body: Body) -> Response {
    let Some(credential) = authenticate(&headers, &app.credentials) else {
        return problem(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Authentication required",
        );
    };
    if !credential.write {
        return problem(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Write access is required",
        );
    }
    let bytes = match read_body(body, app.max_payload).await {
        Ok(bytes) => bytes,
        Err(response) => return *response,
    };
    if !json_content_type(headers.get(header::CONTENT_TYPE)) {
        return problem(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "JSON is required",
        );
    }
    let principal = credential.principal.clone();
    let world_id = app.world_id.clone();
    let store = app.store.clone();
    let submitted = tokio::task::spawn_blocking(move || {
        accept_submission(&store, &principal, &world_id, &bytes)
    })
    .await;
    match submitted {
        Ok(Ok(receipt)) => json_no_store(StatusCode::OK, receipt),
        Ok(Err(SubmitRejection::Status {
            status,
            code,
            title,
        })) => problem(status, code, title),
        Ok(Err(SubmitRejection::Storage)) | Err(_) => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            "The message was not recorded",
        ),
    }
}

async fn collaborate(State(app): State<Arc<App>>, headers: HeaderMap, body: Body) -> Response {
    let Some(credential) = authenticate(&headers, &app.credentials) else {
        return problem(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Authentication required",
        );
    };
    if !credential.write {
        return problem(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Write access is required",
        );
    }
    let bytes = match read_body(body, app.max_payload).await {
        Ok(bytes) => bytes,
        Err(response) => return *response,
    };
    if !json_content_type(headers.get(header::CONTENT_TYPE)) {
        return problem(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "JSON is required",
        );
    }
    let principal = credential.principal.clone();
    let can_read = credential.read;
    let world_id = app.world_id.clone();
    let store = app.store.clone();
    let submitted = tokio::task::spawn_blocking(move || {
        accept_collaboration(&store, &principal, can_read, &world_id, &bytes)
    })
    .await;
    match submitted {
        Ok(Ok(receipt)) => json_no_store(StatusCode::OK, receipt),
        Ok(Err(SubmitRejection::Status {
            status,
            code,
            title,
        })) => problem(status, code, title),
        Ok(Err(SubmitRejection::Storage)) | Err(_) => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            "The record was not recorded",
        ),
    }
}

async fn events(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    query: Result<Query<EventQuery>, QueryRejection>,
) -> Response {
    let Some(credential) = authenticate(&headers, &app.credentials) else {
        return problem(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Authentication required",
        );
    };
    let principal = credential.principal.clone();
    let can_read = credential.read;
    let world_id = app.world_id.clone();
    let store = app.store.clone();
    // A malformed query, such as a repeated `after`, is an invalid cursor. It is reported
    // after the credential and read grant, in the profile's check order.
    let after = match query {
        Ok(Query(query)) => query.after,
        Err(_) => Some(String::new()),
    };
    let page = tokio::task::spawn_blocking(move || {
        store.read_page(&principal, &world_id, after.as_deref(), can_read)
    })
    .await;
    match page {
        Ok(Ok(body)) => json_no_store(StatusCode::OK, body),
        Ok(Err(ReadError::InvalidCursor)) => problem(
            StatusCode::BAD_REQUEST,
            "invalid_cursor",
            "Cursor is malformed",
        ),
        Ok(Err(ReadError::Forbidden)) => problem(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Read access is required",
        ),
        Ok(Err(ReadError::Expired)) => problem(
            StatusCode::GONE,
            "cursor_expired",
            "Cursor can no longer resume",
        ),
        Ok(Err(ReadError::Storage)) | Err(_) => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            "Events could not be read",
        ),
    }
}

enum SubmitRejection {
    Status {
        status: StatusCode,
        code: &'static str,
        title: &'static str,
    },
    Storage,
}

fn reject(status: StatusCode, code: &'static str, title: &'static str) -> SubmitRejection {
    SubmitRejection::Status {
        status,
        code,
        title,
    }
}

/// Parse a request body, refusing integers serde_json would round so stored fields stay exact.
fn parse_request(bytes: &[u8]) -> Result<Value, SubmitRejection> {
    let malformed = || {
        reject(
            StatusCode::BAD_REQUEST,
            "malformed_json",
            "JSON could not be parsed",
        )
    };
    let record: Value = serde_json::from_slice(bytes).map_err(|_| malformed())?;
    if !exact_integers(bytes) {
        return Err(malformed());
    }
    Ok(record)
}

fn accept_submission(
    store: &Store,
    principal: &str,
    world_id: &str,
    bytes: &[u8],
) -> Result<Value, SubmitRejection> {
    let record = parse_request(bytes)?;
    if let Some(code) = message_error(&record) {
        let title = match code {
            "unsupported_version" => "Unsupported protocol version",
            "unsupported_record_type" => "Unsupported record type",
            _ => "Invalid record",
        };
        return Err(reject(StatusCode::UNPROCESSABLE_ENTITY, code, title));
    }
    if !world_matches(&record, world_id) {
        return Err(reject(
            StatusCode::UNPROCESSABLE_ENTITY,
            "wrong_world",
            "Message world does not match this host",
        ));
    }
    if record["from"].as_str() != Some(principal) {
        return Err(reject(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Message sender does not match the credential",
        ));
    }
    match store.submit(principal, world_id, bytes) {
        Ok(receipt) => Ok(receipt),
        Err(SubmitError::Conflict) => Err(reject(
            StatusCode::CONFLICT,
            "id_conflict",
            "Message id was already used for different bytes",
        )),
        Err(SubmitError::Storage) => Err(SubmitRejection::Storage),
    }
}

fn accept_collaboration(
    store: &Store,
    principal: &str,
    can_read: bool,
    world_id: &str,
    bytes: &[u8],
) -> Result<Value, SubmitRejection> {
    let record = parse_request(bytes)?;
    if let Some(code) = collaboration_error(&record) {
        let title = match code {
            "unsupported_version" => "Unsupported protocol version",
            "unsupported_record_type" => "Unsupported record type",
            _ => "Invalid record",
        };
        return Err(reject(StatusCode::UNPROCESSABLE_ENTITY, code, title));
    }
    if !world_matches(&record, world_id) {
        return Err(reject(
            StatusCode::UNPROCESSABLE_ENTITY,
            "wrong_world",
            "Record world does not match this host",
        ));
    }
    if record["from"].as_str() != Some(principal) {
        return Err(reject(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Record sender does not match the credential",
        ));
    }
    match store.collaborate(principal, can_read, world_id, bytes) {
        Ok(receipt) => Ok(receipt),
        Err(CollaborateError::Conflict) => Err(reject(
            StatusCode::CONFLICT,
            "id_conflict",
            "Record id was already used for different bytes",
        )),
        Err(CollaborateError::UnknownTarget) => Err(reject(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unknown_target",
            "Target revision is not available",
        )),
        Err(CollaborateError::Forbidden) => Err(reject(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Only the author can withdraw this revision",
        )),
        Err(CollaborateError::Storage) => Err(SubmitRejection::Storage),
    }
}

#[derive(Deserialize)]
pub struct EventQuery {
    after: Option<String>,
}

async fn read_body(body: Body, limit: usize) -> Result<Vec<u8>, Box<Response>> {
    match axum::body::to_bytes(body, limit).await {
        Ok(bytes) => Ok(bytes.to_vec()),
        Err(_) => Err(Box::new(problem(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Body exceeds the published limit",
        ))),
    }
}

fn json_content_type(value: Option<&HeaderValue>) -> bool {
    let Some(value) = value.and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let media = value.split(';').next().unwrap_or("").trim();
    media.eq_ignore_ascii_case("application/json")
}

fn authenticate<'a>(headers: &HeaderMap, credentials: &'a [Credential]) -> Option<&'a Credential> {
    let header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    credentials
        .iter()
        .find(|credential| tokens_equal(&credential.token, token))
}

fn tokens_equal(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

fn problem(status: StatusCode, code: &str, title: &str) -> Response {
    let mut response = (
        status,
        Json(json!({
            "type": format!("https://agentciv.io/problems/{code}"),
            "title": title,
            "status": status.as_u16(),
            "code": code
        })),
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if status == StatusCode::UNAUTHORIZED {
        headers.insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"agentciv\""),
        );
    }
    response
}

fn json_no_store(status: StatusCode, body: Value) -> Response {
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
