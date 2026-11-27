//! Loopback HTTP server (`mls-relay/0.1-draft`) for Stage 2B of #209.
//!
//! Exposes the untrusted zero-plaintext PQ-MLS Delivery Service core (`mls_relay`)
//! under `/mls/v1/*`, separate from `http-commons/0.1-draft`. Storage work runs on
//! blocking threads via `tokio::task::spawn_blocking` and never across an `.await`.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::mls_relay::{
    self, EnvelopeRecord, MAX_ENVELOPES_PER_GROUP, MAX_OPAQUE_BLOB_BYTES, MlsRelayStore,
    PROFILE_ID, RelayError, SUPPORTED_CIPHERSUITES, WelcomeSubmission,
};
use crate::validate::parse_unique_json;

/// Maximum JSON request body size for non-commit POST routes (128 KiB).
pub const MAX_JSON_BODY_BYTES: usize = 128 * 1024;

/// Maximum JSON request body size for `POST /mls/v1/groups/{group_id}/commits` (256 KiB).
pub const MAX_COMMIT_BODY_BYTES: usize = 256 * 1024;

/// Default pagination limit for HTTP envelope and welcome queries.
pub const DEFAULT_HTTP_FETCH_LIMIT: usize = 50;

/// Maximum pagination limit for HTTP envelope and welcome queries.
pub const MAX_HTTP_FETCH_LIMIT: usize = 200;

/// Shared state for the `/mls/v1/*` loopback relay router.
#[derive(Clone)]
pub struct MlsRelayState {
    store: Arc<Mutex<MlsRelayStore>>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl MlsRelayState {
    /// Create relay state using the system Unix epoch clock.
    pub fn new(store: MlsRelayStore) -> Self {
        Self::with_clock(store, mls_relay::current_unix_seconds)
    }

    /// Create relay state with a caller-supplied clock (useful for deterministic expiry tests).
    pub fn with_clock<F>(store: MlsRelayStore, clock: F) -> Self
    where
        F: Fn() -> i64 + Send + Sync + 'static,
    {
        Self {
            store: Arc::new(Mutex::new(store)),
            clock: Arc::new(clock),
        }
    }

    /// Return the current timestamp from the configured clock.
    pub fn now(&self) -> i64 {
        (self.clock)()
    }

    async fn run_store<T, F>(&self, op: F) -> Result<T, RelayError>
    where
        T: Send + 'static,
        F: FnOnce(&mut MlsRelayStore) -> Result<T, RelayError> + Send + 'static,
    {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || {
            let mut guard = store
                .lock()
                .map_err(|_| RelayError::Storage("relay store lock poisoned".to_owned()))?;
            op(&mut guard)
        })
        .await
        .map_err(|_| RelayError::Storage("relay storage worker task failed".to_owned()))?
    }
}

/// Build the Axum router serving the `/mls/v1/*` PQ-MLS relay contract.
pub fn router(state: MlsRelayState) -> Router {
    Router::new()
        .route("/mls/v1/info", get(info))
        .route(
            "/mls/v1/key-packages",
            post(publish_key_package).layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES)),
        )
        .route(
            "/mls/v1/key-packages/consume",
            post(consume_key_package).layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES)),
        )
        .route(
            "/mls/v1/groups",
            post(create_group).layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES)),
        )
        .route("/mls/v1/groups/{group_id}", get(get_group))
        .route(
            "/mls/v1/groups/{group_id}/commits",
            post(submit_commit).layer(DefaultBodyLimit::max(MAX_COMMIT_BODY_BYTES)),
        )
        .route(
            "/mls/v1/groups/{group_id}/messages",
            post(submit_message).layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES)),
        )
        .route("/mls/v1/groups/{group_id}/envelopes", get(fetch_envelopes))
        .route("/mls/v1/welcomes/{recipient_id}", get(fetch_welcomes))
        .route("/mls/v1/groups/{group_id}/checkpoint", get(checkpoint))
        .route(
            "/mls/v1/prune",
            post(prune_expired).layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES)),
        )
        .fallback(not_found_fallback)
        .method_not_allowed_fallback(method_not_allowed_fallback)
        .layer(middleware::from_fn(apply_security_headers))
        .with_state(state)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishKeyPackageRequest {
    kp_id: String,
    principal_id: String,
    ciphersuite: String,
    blob_b64: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumeKeyPackageRequest {
    principal_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateGroupRequest {
    group_id: String,
    ciphersuite: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WelcomeSubmissionDto {
    recipient_id: String,
    blob_b64: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitCommitRequest {
    expected_epoch: u64,
    commit_blob_b64: String,
    #[serde(default)]
    welcomes: Vec<WelcomeSubmissionDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitMessageRequest {
    epoch: u64,
    blob_b64: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PruneRequest {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PaginationQuery {
    after_seq: Option<String>,
    limit: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointQuery {
    old_size: Option<String>,
}

#[derive(Serialize)]
struct EnvelopesResponse {
    group_id: String,
    after_seq: u64,
    limit: usize,
    envelopes: Vec<EnvelopeRecord>,
    next_after_seq: u64,
}

#[derive(Serialize)]
struct WelcomesResponse {
    recipient_id: String,
    after_seq: u64,
    limit: usize,
    welcomes: Vec<EnvelopeRecord>,
    next_after_seq: u64,
}

async fn info(State(state): State<MlsRelayState>) -> Response {
    match state.run_store(|store| Ok(store.config())).await {
        Ok(config) => json_response(
            StatusCode::OK,
            json!({
                "profile": PROFILE_ID,
                "writes_enabled": config.writes_enabled,
                "retention_seconds": config.retention_seconds,
                "supported_ciphersuites": SUPPORTED_CIPHERSUITES,
                "max_opaque_blob_bytes": MAX_OPAQUE_BLOB_BYTES,
                "max_envelopes_per_group": MAX_ENVELOPES_PER_GROUP,
                "content_trust": "untrusted_opaque_ciphertext",
                "outside_systems": "not_granted"
            }),
        ),
        Err(err) => relay_error_response(err),
    }
}

async fn publish_key_package(
    State(state): State<MlsRelayState>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let req: PublishKeyPackageRequest =
        match read_strict_json(&headers, body, MAX_JSON_BODY_BYTES).await {
            Ok(req) => req,
            Err(response) => return *response,
        };
    let now = state.now();
    let result = state
        .run_store(move |store| {
            store.publish_key_package(
                &req.kp_id,
                &req.principal_id,
                &req.ciphersuite,
                &req.blob_b64,
                now,
            )
        })
        .await;
    match result {
        Ok(record) => serializable_response(StatusCode::CREATED, &record),
        Err(err) => relay_error_response(err),
    }
}

async fn consume_key_package(
    State(state): State<MlsRelayState>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let req: ConsumeKeyPackageRequest =
        match read_strict_json(&headers, body, MAX_JSON_BODY_BYTES).await {
            Ok(req) => req,
            Err(response) => return *response,
        };
    let now = state.now();
    let principal = req.principal_id;
    let result = state
        .run_store(move |store| {
            let fetched = store.fetch_and_consume_key_package(&principal, now)?;
            fetched.ok_or_else(|| {
                RelayError::NotFound(format!("no live key package available for {principal}"))
            })
        })
        .await;
    match result {
        Ok(record) => serializable_response(StatusCode::OK, &record),
        Err(err) => relay_error_response(err),
    }
}

async fn create_group(
    State(state): State<MlsRelayState>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let req: CreateGroupRequest = match read_strict_json(&headers, body, MAX_JSON_BODY_BYTES).await
    {
        Ok(req) => req,
        Err(response) => return *response,
    };
    let now = state.now();
    let result = state
        .run_store(move |store| store.create_group(&req.group_id, &req.ciphersuite, now))
        .await;
    match result {
        Ok(record) => serializable_response(StatusCode::CREATED, &record),
        Err(err) => relay_error_response(err),
    }
}

async fn get_group(State(state): State<MlsRelayState>, Path(group_id): Path<String>) -> Response {
    let now = state.now();
    let result = state
        .run_store(move |store| {
            let group = store.get_group(&group_id, now)?;
            group.ok_or_else(|| RelayError::NotFound(format!("group not found: {group_id}")))
        })
        .await;
    match result {
        Ok(record) => serializable_response(StatusCode::OK, &record),
        Err(err) => relay_error_response(err),
    }
}

async fn submit_commit(
    State(state): State<MlsRelayState>,
    Path(group_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let req: SubmitCommitRequest =
        match read_strict_json(&headers, body, MAX_COMMIT_BODY_BYTES).await {
            Ok(req) => req,
            Err(response) => return *response,
        };
    let welcomes: Vec<WelcomeSubmission> = req
        .welcomes
        .into_iter()
        .map(|item| WelcomeSubmission {
            recipient_id: item.recipient_id,
            blob_b64: item.blob_b64,
        })
        .collect();
    let now = state.now();
    let result = state
        .run_store(move |store| {
            store.submit_commit(
                &group_id,
                req.expected_epoch,
                &req.commit_blob_b64,
                &welcomes,
                now,
            )
        })
        .await;
    match result {
        Ok(receipt) => serializable_response(StatusCode::CREATED, &receipt),
        Err(err) => relay_error_response(err),
    }
}

async fn submit_message(
    State(state): State<MlsRelayState>,
    Path(group_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let req: SubmitMessageRequest =
        match read_strict_json(&headers, body, MAX_JSON_BODY_BYTES).await {
            Ok(req) => req,
            Err(response) => return *response,
        };
    let now = state.now();
    let result = state
        .run_store(move |store| {
            store.submit_private_message(&group_id, req.epoch, &req.blob_b64, now)
        })
        .await;
    match result {
        Ok(record) => serializable_response(StatusCode::CREATED, &record),
        Err(err) => relay_error_response(err),
    }
}

async fn fetch_envelopes(
    State(state): State<MlsRelayState>,
    Path(group_id): Path<String>,
    query: Result<Query<PaginationQuery>, QueryRejection>,
) -> Response {
    let (after_seq, limit) = match parse_pagination_query(query) {
        Ok(parsed) => parsed,
        Err(response) => return *response,
    };
    let now = state.now();
    let group_id_for_store = group_id.clone();
    let result = state
        .run_store(move |store| store.fetch_envelopes(&group_id_for_store, after_seq, limit, now))
        .await;
    match result {
        Ok(envelopes) => {
            let next_after_seq = envelopes.last().map_or(after_seq, |env| env.seq);
            serializable_response(
                StatusCode::OK,
                &EnvelopesResponse {
                    group_id,
                    after_seq,
                    limit,
                    envelopes,
                    next_after_seq,
                },
            )
        }
        Err(err) => relay_error_response(err),
    }
}

async fn fetch_welcomes(
    State(state): State<MlsRelayState>,
    Path(recipient_id): Path<String>,
    query: Result<Query<PaginationQuery>, QueryRejection>,
) -> Response {
    let (after_seq, limit) = match parse_pagination_query(query) {
        Ok(parsed) => parsed,
        Err(response) => return *response,
    };
    let now = state.now();
    let recipient_for_store = recipient_id.clone();
    let result = state
        .run_store(move |store| store.fetch_welcomes(&recipient_for_store, after_seq, limit, now))
        .await;
    match result {
        Ok(welcomes) => {
            let next_after_seq = welcomes.last().map_or(after_seq, |env| env.seq);
            serializable_response(
                StatusCode::OK,
                &WelcomesResponse {
                    recipient_id,
                    after_seq,
                    limit,
                    welcomes,
                    next_after_seq,
                },
            )
        }
        Err(err) => relay_error_response(err),
    }
}

async fn checkpoint(
    State(state): State<MlsRelayState>,
    Path(group_id): Path<String>,
    query: Result<Query<CheckpointQuery>, QueryRejection>,
) -> Response {
    let old_size = match query {
        Ok(Query(CheckpointQuery { old_size: None })) => None,
        Ok(Query(CheckpointQuery {
            old_size: Some(raw),
        })) => match parse_non_negative_u64(&raw) {
            Some(value) => Some(value),
            None => {
                return problem(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "Invalid checkpoint query",
                    "old_size must be a canonical non-negative integer",
                );
            }
        },
        Err(_) => {
            return problem(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Invalid checkpoint query",
                "checkpoint query contains malformed or unknown parameters",
            );
        }
    };
    let now = state.now();
    let result = state
        .run_store(move |store| store.checkpoint(&group_id, old_size, now))
        .await;
    match result {
        Ok(cp) => serializable_response(StatusCode::OK, &cp),
        Err(err) => relay_error_response(err),
    }
}

async fn prune_expired(
    State(state): State<MlsRelayState>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if !is_json_content_type(headers.get(header::CONTENT_TYPE)) {
        return problem(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Unsupported media type",
            "Content-Type must be application/json",
        );
    }
    let bytes = match read_bounded_body(body, MAX_JSON_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(response) => return *response,
    };
    if !bytes.is_empty()
        && let Err(response) = parse_strict_json::<PruneRequest>(&bytes)
    {
        return *response;
    }
    let now = state.now();
    let result = state.run_store(move |store| store.prune_expired(now)).await;
    match result {
        Ok(report) => serializable_response(StatusCode::OK, &report),
        Err(err) => relay_error_response(err),
    }
}

async fn not_found_fallback() -> Response {
    problem(
        StatusCode::NOT_FOUND,
        "not_found",
        "Not found",
        "The requested relay route does not exist",
    )
}

async fn method_not_allowed_fallback() -> Response {
    problem(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "Method not allowed",
        "HTTP method is not allowed for this relay route",
    )
}

async fn apply_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    response
}

async fn read_strict_json<T: DeserializeOwned>(
    headers: &HeaderMap,
    body: Body,
    limit: usize,
) -> Result<T, Box<Response>> {
    if !is_json_content_type(headers.get(header::CONTENT_TYPE)) {
        return Err(Box::new(problem(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Unsupported media type",
            "Content-Type must be application/json",
        )));
    }
    let bytes = read_bounded_body(body, limit).await?;
    parse_strict_json(&bytes)
}

async fn read_bounded_body(body: Body, limit: usize) -> Result<Vec<u8>, Box<Response>> {
    match axum::body::to_bytes(body, limit).await {
        Ok(bytes) => Ok(bytes.to_vec()),
        Err(_) => Err(Box::new(problem(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Payload too large",
            &format!("Request body exceeds maximum allowed size of {limit} bytes"),
        ))),
    }
}

fn parse_strict_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Box<Response>> {
    let value = parse_unique_json(bytes).ok_or_else(|| {
        Box::new(problem(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Invalid JSON payload",
            "Request body must be valid UTF-8 JSON without duplicate object keys",
        ))
    })?;
    serde_json::from_value(value).map_err(|err| {
        Box::new(problem(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Invalid JSON payload",
            &format!("Request body did not match expected schema: {err}"),
        ))
    })
}

fn is_json_content_type(value: Option<&HeaderValue>) -> bool {
    let Some(raw) = value.and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let media = raw.split(';').next().unwrap_or("").trim();
    media.eq_ignore_ascii_case("application/json")
}

fn parse_pagination_query(
    query: Result<Query<PaginationQuery>, QueryRejection>,
) -> Result<(u64, usize), Box<Response>> {
    let Query(parsed) = query.map_err(|_| {
        Box::new(problem(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Invalid pagination query",
            "Query contains malformed or unknown parameters",
        ))
    })?;
    let after_seq = match parsed.after_seq {
        None => 0,
        Some(raw) => parse_non_negative_u64(&raw).ok_or_else(|| {
            Box::new(problem(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Invalid pagination query",
                "after_seq must be a canonical non-negative integer",
            ))
        })?,
    };
    let limit = match parsed.limit {
        None => DEFAULT_HTTP_FETCH_LIMIT,
        Some(raw) => {
            let parsed_limit = parse_non_negative_u64(&raw)
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| (1..=MAX_HTTP_FETCH_LIMIT).contains(v));
            parsed_limit.ok_or_else(|| {
                Box::new(problem(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "Invalid pagination query",
                    &format!("limit must be an integer within 1..={MAX_HTTP_FETCH_LIMIT}"),
                ))
            })?
        }
    };
    Ok((after_seq, limit))
}

fn parse_non_negative_u64(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return None;
    }
    raw.parse::<u64>().ok()
}

fn serializable_response<T: Serialize>(status: StatusCode, body: &T) -> Response {
    match serde_json::to_value(body) {
        Ok(value) => json_response(status, value),
        Err(_) => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            "Serialization error",
            "Response payload could not be serialized",
        ),
    }
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn relay_error_response(err: RelayError) -> Response {
    let status =
        StatusCode::from_u16(err.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let code = err.error_code();
    match err {
        RelayError::WritesDisabled => problem(
            status,
            code,
            "Relay writes disabled",
            "Mutating relay operations are disabled by configuration",
        ),
        RelayError::EpochConflict { current_epoch } => problem_with_body(
            status,
            json!({
                "type": format!("https://agentciv.io/problems/{code}"),
                "title": "Epoch conflict",
                "status": status.as_u16(),
                "code": code,
                "detail": format!("Submitted epoch does not match current epoch {current_epoch}"),
                "current_epoch": current_epoch
            }),
        ),
        RelayError::Conflict(detail) => problem(status, code, "Resource conflict", &detail),
        RelayError::NotFound(detail) => problem(status, code, "Not found", &detail),
        RelayError::QuotaExceeded(detail) => problem(status, code, "Quota exceeded", &detail),
        RelayError::PayloadTooLarge { size, max } => problem_with_body(
            status,
            json!({
                "type": format!("https://agentciv.io/problems/{code}"),
                "title": "Payload too large",
                "status": status.as_u16(),
                "code": code,
                "detail": format!("Opaque blob size {size} exceeds maximum {max} bytes"),
                "size": size,
                "max": max
            }),
        ),
        RelayError::InvalidInput(detail) => problem(status, code, "Invalid input", &detail),
        RelayError::UnsupportedCiphersuite(suite) => problem(
            status,
            code,
            "Unsupported ciphersuite",
            &format!("Unsupported MLS ciphersuite: {suite}"),
        ),
        RelayError::Storage(_) => problem(
            status,
            code,
            "Storage error",
            "The relay could not complete the storage operation",
        ),
    }
}

fn problem(status: StatusCode, code: &str, title: &str, detail: &str) -> Response {
    problem_with_body(
        status,
        json!({
            "type": format!("https://agentciv.io/problems/{code}"),
            "title": title,
            "status": status.as_u16(),
            "code": code,
            "detail": detail
        }),
    )
}

fn problem_with_body(status: StatusCode, body: Value) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_non_negative_u64_enforces_canonical_digits() {
        assert_eq!(parse_non_negative_u64("0"), Some(0));
        assert_eq!(parse_non_negative_u64("42"), Some(42));
        assert_eq!(parse_non_negative_u64(""), None);
        assert_eq!(parse_non_negative_u64("01"), None);
        assert_eq!(parse_non_negative_u64("-1"), None);
        assert_eq!(parse_non_negative_u64("abc"), None);
        assert_eq!(parse_non_negative_u64("1.5"), None);
    }

    #[test]
    fn json_content_type_accepts_charset_and_rejects_non_json() {
        let json_plain = HeaderValue::from_static("application/json");
        let json_utf8 = HeaderValue::from_static("application/json; charset=utf-8");
        let text_plain = HeaderValue::from_static("text/plain");
        assert!(is_json_content_type(Some(&json_plain)));
        assert!(is_json_content_type(Some(&json_utf8)));
        assert!(!is_json_content_type(Some(&text_plain)));
        assert!(!is_json_content_type(None));
    }

    #[test]
    fn state_new_uses_current_unix_clock() {
        let store = MlsRelayStore::open_in_memory(mls_relay::RelayConfig::default()).expect("open");
        let state = MlsRelayState::new(store);
        assert!(state.now() > 0);
    }
}
