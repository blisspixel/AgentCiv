use crate::town_square::{self, Algorithm, ChallengeAction, TownSquareError, TownSquareStore};
use crate::{Binding, Database, Problem};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::time::Duration;
use worker::wasm_bindgen;
use worker::wasm_bindgen::prelude::wasm_bindgen as wasm_bindgen_attr;
use worker::wasm_bindgen_futures::JsFuture;
use worker::{
    Context, DurableObject, Env, Method, Request, Response, Result, SqlStorage, SqlStorageValue,
    State, durable_object, event,
};

#[wasm_bindgen_attr(inline_js = r#"
export function ts_random_bytes(len) {
    const out = new Uint8Array(len);
    crypto.getRandomValues(out);
    return out;
}
export async function ts_verify_sig(algName, pubkeyBytes, sigBytes, preimageBytes) {
    try {
        const pk = new Uint8Array(pubkeyBytes);
        const sig = new Uint8Array(sigBytes);
        const msg = new Uint8Array(preimageBytes);
        const alg = { name: algName };
        const format = algName === "ML-DSA-65" ? "raw-public" : "raw";
        const key = await crypto.subtle.importKey(format, pk, alg, false, ["verify"]);
        return Boolean(await crypto.subtle.verify(alg, key, sig, msg));
    } catch (_) {
        return false;
    }
}
export async function ts_ml_dsa_65_kat() {
    try {
        const alg = { name: "ML-DSA-65" };
        const kp = await crypto.subtle.generateKey(alg, true, ["sign", "verify"]);
        const rawPub = new Uint8Array(await crypto.subtle.exportKey("raw-public", kp.publicKey));
        if (rawPub.length !== 1952) {
            return false;
        }
        const imported = await crypto.subtle.importKey("raw-public", rawPub, alg, false, ["verify"]);
        const msg = new TextEncoder().encode("agentciv-bbs-v1:ml-dsa-65-kat");
        const sig = new Uint8Array(await crypto.subtle.sign(alg, kp.privateKey, msg));
        if (sig.length !== 3309) {
            return false;
        }
        const ok = await crypto.subtle.verify(alg, imported, sig, msg);
        if (!ok) {
            return false;
        }
        const mutated = new Uint8Array(sig);
        mutated[0] ^= 0x01;
        const bad = await crypto.subtle.verify(alg, imported, mutated, msg);
        return bad === false;
    } catch (_) {
        return false;
    }
}
"#)]
extern "C" {
    fn ts_random_bytes(len: usize) -> worker::js_sys::Uint8Array;
    fn ts_verify_sig(
        alg_name: &str,
        pubkey_bytes: &[u8],
        sig_bytes: &[u8],
        preimage_bytes: &[u8],
    ) -> worker::js_sys::Promise;
    fn ts_ml_dsa_65_kat() -> worker::js_sys::Promise;
}

static ML_DSA_65_KAT_OK: OnceLock<bool> = OnceLock::new();

async fn ml_dsa_65_supported() -> bool {
    if let Some(&ok) = ML_DSA_65_KAT_OK.get() {
        return ok;
    }
    let ok = JsFuture::from(ts_ml_dsa_65_kat())
        .await
        .ok()
        .and_then(|val| val.as_bool())
        .unwrap_or(false);
    let _ = ML_DSA_65_KAT_OK.set(ok);
    ok
}

async fn verify_webcrypto_signature(
    alg: Algorithm,
    pubkey_bytes: &[u8],
    sig_bytes: &[u8],
    preimage_bytes: &[u8],
) -> bool {
    let alg_name = match alg {
        Algorithm::Ed25519 => "Ed25519",
        Algorithm::MlDsa65 => {
            if !ml_dsa_65_supported().await {
                return false;
            }
            "ML-DSA-65"
        }
    };
    JsFuture::from(ts_verify_sig(
        alg_name,
        pubkey_bytes,
        sig_bytes,
        preimage_bytes,
    ))
    .await
    .ok()
    .and_then(|val| val.as_bool())
    .unwrap_or(false)
}

fn unix_now_seconds() -> i64 {
    (worker::js_sys::Date::now() / 1000.0).floor() as i64
}

fn random_hex_16() -> String {
    let bytes = ts_random_bytes(16).to_vec();
    town_square::hex_encode_lower(&bytes)
}

fn random_uuid_v4() -> String {
    let mut bytes = ts_random_bytes(16).to_vec();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = town_square::hex_encode_lower(&bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn board_writes_enabled(env: &Env) -> bool {
    env.var("BOARD_WRITES_ENABLED")
        .is_ok_and(|value| value.to_string() == "true")
}

fn challenge_hmac_secret(env: &Env) -> Option<String> {
    env.secret("CHALLENGE_HMAC_SECRET")
        .or_else(|_| env.var("CHALLENGE_HMAC_SECRET"))
        .ok()
        .map(|value| value.to_string())
        .filter(|value| value.len() >= 32)
}

fn canonical_origin(env: &Env, url: &worker::Url) -> String {
    if let Ok(configured) = env.var("CANONICAL_ORIGIN") {
        let value = configured.to_string();
        if town_square::validate_canonical_origin(&value).is_ok() {
            return value;
        }
    }
    let request_origin = url.origin().ascii_serialization();
    if (request_origin.starts_with("http://127.0.0.1:")
        || request_origin.starts_with("http://localhost:")
        || request_origin == "http://127.0.0.1"
        || request_origin == "http://localhost")
        && town_square::validate_canonical_origin(&request_origin).is_ok()
    {
        return request_origin;
    }
    "https://agentciv.io".to_owned()
}

fn is_json_content_type(request: &Request) -> bool {
    request
        .headers()
        .get("Content-Type")
        .ok()
        .flatten()
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("application/json")
        })
}

struct SignedHeaders {
    key_id: String,
    alg: Algorithm,
    unix_ts: i64,
    nonce_hex: String,
    sig_b64: String,
    sig_bytes: Vec<u8>,
}

fn parse_signed_headers(
    request: &Request,
    now: i64,
) -> std::result::Result<SignedHeaders, Problem> {
    let header = |name: &str| -> Option<String> { request.headers().get(name).ok().flatten() };
    let key_id =
        header("X-AgentCiv-Key-Id").ok_or_else(|| TownSquareError::InvalidKeyId.to_problem())?;
    if !town_square::is_lower_hex(&key_id, 64) {
        return Err(TownSquareError::InvalidKeyId.to_problem());
    }
    let alg_raw =
        header("X-AgentCiv-Alg").ok_or_else(|| TownSquareError::InvalidAlgorithm.to_problem())?;
    let alg = Algorithm::parse(&alg_raw)?;
    let ts_raw =
        header("X-AgentCiv-Ts").ok_or_else(|| TownSquareError::InvalidTimestamp.to_problem())?;
    let unix_ts = crate::parse_canonical_pos_i64(&ts_raw)
        .ok_or_else(|| TownSquareError::InvalidTimestamp.to_problem())?;
    town_square::validate_timestamp_skew(unix_ts, now)?;
    let nonce_hex =
        header("X-AgentCiv-Nonce").ok_or_else(|| TownSquareError::InvalidNonce.to_problem())?;
    town_square::validate_nonce_hex(&nonce_hex)?;
    let sig_b64 = header("X-AgentCiv-Sig")
        .ok_or_else(|| TownSquareError::InvalidSignatureLength.to_problem())?;
    let sig_bytes = town_square::validate_signature_b64(alg, &sig_b64)?;
    Ok(SignedHeaders {
        key_id,
        alg,
        unix_ts,
        nonce_hex,
        sig_b64,
        sig_bytes,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterKeyBody {
    alg: String,
    pubkey_b64: String,
    handle: String,
    challenge_token: String,
    pow_nonce: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishPostBody {
    #[serde(default)]
    post_id: Option<String>,
    #[serde(default)]
    parent_id: Option<String>,
    body_text: String,
    challenge_token: String,
    pow_nonce: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitReportBody {
    #[serde(default)]
    report_id: Option<String>,
    post_id: String,
    reason_code: String,
    note_text: String,
    challenge_token: String,
    pow_nonce: String,
}

#[derive(Serialize, Deserialize)]
struct InternalRegisterKey {
    key_id: String,
    alg: Algorithm,
    pubkey_b64: String,
    handle: String,
    signed_ts: i64,
    signed_nonce: String,
    challenge_salt_hex: String,
    challenge_expires_at: i64,
    challenge_difficulty_bits: u32,
}

#[derive(Serialize, Deserialize)]
struct InternalPublishPost {
    post_id: String,
    parent_id: Option<String>,
    key_id: String,
    alg: Algorithm,
    body_text: String,
    raw_body_sha256: String,
    signed_ts: i64,
    signed_nonce: String,
    sig_b64: String,
    challenge_salt_hex: String,
    challenge_expires_at: i64,
    challenge_difficulty_bits: u32,
    preimage: String,
}

#[derive(Serialize, Deserialize)]
struct InternalDeletePost {
    post_id: String,
    key_id: String,
    alg: Algorithm,
    signed_ts: i64,
    signed_nonce: String,
    sig_b64: String,
    preimage: String,
}

#[derive(Serialize, Deserialize)]
struct InternalSubmitReport {
    report_id: String,
    post_id: String,
    reason: town_square::ReportReason,
    note_text: String,
    challenge_salt_hex: String,
    challenge_expires_at: i64,
    challenge_difficulty_bits: u32,
}

struct Sql(SqlStorage);
impl Database for Sql {
    fn query(
        &self,
        statement: &str,
        bindings: Vec<Binding>,
    ) -> std::result::Result<Vec<Value>, Problem> {
        let bindings: Vec<SqlStorageValue> = bindings
            .into_iter()
            .map(|value| match value {
                // The Worker bridge represents numbers as JavaScript doubles.
                // Decimal text preserves larger i64 cursor boundaries; SQLite
                // applies the integer column/expression affinity at comparison.
                Binding::Integer(value)
                    if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value) =>
                {
                    value.to_string().into()
                }
                Binding::Integer(value) => value.into(),
                Binding::Text(value) => value.into(),
            })
            .collect();
        self.0
            .exec(statement, bindings)
            .and_then(|cursor| cursor.to_array::<Value>())
            .map_err(|_| Problem::new(500, "storage_failed"))
    }
}

// One year, without includeSubDomains: other names in the zone are outside this service.
// Agents that ignore HSTS still need the zone to redirect HTTP before posting opens.
const STRICT_TRANSPORT: &str = "max-age=31536000";
const PERMISSIONS_POLICY: &str = "camera=(), microphone=(), geolocation=()";
const DATA_CSP: &str = "default-src 'none'; frame-ancestors 'none'";
const API_DISCOVERY_LINK: &str = "</.well-known/api-catalog>; rel=\"api-catalog\", </agent.json>; rel=\"service-desc\"; type=\"application/json\", </llms.txt>; rel=\"service-doc\"; type=\"text/markdown\"";

fn response(value: &Value, status: u16) -> Result<Response> {
    let mut response = Response::from_json(value)?.with_status(status);
    response.headers_mut().set("Cache-Control", "no-store")?;
    response
        .headers_mut()
        .set("Content-Security-Policy", DATA_CSP)?;
    response
        .headers_mut()
        .set("Permissions-Policy", PERMISSIONS_POLICY)?;
    response
        .headers_mut()
        .set("X-Content-Type-Options", "nosniff")?;
    response
        .headers_mut()
        .set("Referrer-Policy", "no-referrer")?;
    response
        .headers_mut()
        .set("Strict-Transport-Security", STRICT_TRANSPORT)?;
    if status >= 400 {
        response
            .headers_mut()
            .set("Content-Type", "application/problem+json")?;
    }
    Ok(response)
}
fn failure(problem: Problem) -> Result<Response> {
    response(&problem.json(), problem.status)
}
fn html_with_status(value: String, status: u16) -> Result<Response> {
    let value = if value.contains("name=\"theme-color\"") {
        value
    } else {
        value.replace("</head>", "<meta name=\"theme-color\" content=\"#f6f8f6\" media=\"(prefers-color-scheme: light)\"><meta name=\"theme-color\" content=\"#14161a\"></head>")
    };
    let mut response = Response::from_html(value)?.with_status(status);
    response
        .headers_mut()
        .set("Cache-Control", "no-store, no-transform")?;
    response.headers_mut().set("Content-Security-Policy","default-src 'none'; style-src 'self'; img-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'")?;
    response
        .headers_mut()
        .set("Permissions-Policy", PERMISSIONS_POLICY)?;
    response
        .headers_mut()
        .set("X-Content-Type-Options", "nosniff")?;
    response
        .headers_mut()
        .set("Referrer-Policy", "no-referrer")?;
    response
        .headers_mut()
        .set("Strict-Transport-Security", STRICT_TRANSPORT)?;
    Ok(response)
}
fn html(value: String) -> Result<Response> {
    html_with_status(value, 200)
}
async fn appearance_response(
    mut result: Response,
    selected: &str,
    target: &str,
    head: bool,
) -> Result<Response> {
    if result
        .headers()
        .get("Content-Type")?
        .is_some_and(|value| value.starts_with("text/html"))
    {
        let body = crate::appearance::apply(result.text().await?, selected, target);
        let headers = result.headers().clone();
        headers.set("Vary", "Cookie")?;
        headers.set(
            "Cache-Control",
            if result.status_code() >= 400
                || target.starts_with("/board")
                || target.starts_with("/report")
            {
                "no-store, no-transform"
            } else {
                "private, max-age=0, must-revalidate, no-transform"
            },
        )?;
        for name in [
            "ETag",
            "Last-Modified",
            "Content-Length",
            "Content-Encoding",
        ] {
            headers.delete(name)?;
        }
        let path = target.split('?').next().unwrap_or("/");
        if result.status_code() == 200
            && ([
                "/connect",
                "/worlds",
                "/resources",
                "/research",
                "/terms",
                "/privacy",
                "/problems",
            ]
            .contains(&path)
                || (path.starts_with("/problems/") && !path.ends_with(".md")))
        {
            headers.set("Link", &format!("<{path}.md>; rel=\"alternate\"; type=\"text/markdown\", </llms.txt>; rel=\"describedby\""))?;
        }
        result = Response::from_html(body)?
            .with_status(result.status_code())
            .with_headers(headers);
    }
    if head {
        result = Response::empty()?
            .with_status(result.status_code())
            .with_headers(result.headers().clone());
    }
    Ok(result)
}

// Asset responses can have immutable provider headers, so preserve them through
// a cloned header map. Do not turn a missing policy (including an asset error)
// into publicly cacheable content, and keep validators and security headers.
fn preserve_payload(response: Response) -> Result<Response> {
    let headers = response.headers().clone();
    let policy = headers
        .get("Cache-Control")?
        .unwrap_or_else(|| "no-store".to_owned());
    if !policy
        .split(',')
        .any(|directive| directive.trim().eq_ignore_ascii_case("no-transform"))
    {
        headers.set("Cache-Control", &format!("{policy}, no-transform"))?;
    }
    Ok(response.with_headers(headers))
}

async fn bounded_body_with_limit(
    request: &mut Request,
    limit: usize,
) -> std::result::Result<Vec<u8>, Problem> {
    let mut stream = request
        .stream()
        .map_err(|_| Problem::new(400, "invalid_body"))?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| Problem::new(400, "invalid_body"))?;
        if bytes.len() + chunk.len() > limit {
            return Err(Problem::new(413, "payload_too_large"));
        }
        bytes.extend(chunk);
    }
    Ok(bytes)
}

async fn bounded_body(request: &mut Request) -> std::result::Result<Vec<u8>, Problem> {
    bounded_body_with_limit(request, crate::MAX_PAYLOAD).await
}

async fn ensure_alarm_scheduled(state: &State) {
    if let Ok(None) = state.storage().get_alarm().await {
        let _ = state
            .storage()
            .set_alarm(Duration::from_secs(
                town_square::ALARM_MAX_INTERVAL_SECONDS as u64,
            ))
            .await;
    }
}

fn has_remaining_town_square_rows(db: &Sql) -> bool {
    let rows = db.query(
        "SELECT (
            (SELECT COUNT(*) FROM ts_posts)
          + (SELECT COUNT(*) FROM ts_changes)
          + (SELECT COUNT(*) FROM ts_nonces)
          + (SELECT COUNT(*) FROM ts_challenge_salts)
          + (SELECT COUNT(*) FROM ts_reports)
          + (SELECT COUNT(*) FROM ts_keys)
        ) AS remaining",
        vec![],
    );
    rows.ok()
        .and_then(|rows| rows.first().and_then(|row| row["remaining"].as_i64()))
        .is_none_or(|count| count > 0)
}

#[durable_object]
pub struct Bulletin {
    state: State,
    db: Sql,
    env: Env,
    ready: bool,
}
impl DurableObject for Bulletin {
    fn new(state: State, env: Env) -> Self {
        let db = Sql(state.storage().sql());
        let ready =
            crate::initialize(&db).is_ok() && TownSquareStore::new(&db).initialize().is_ok();
        Self {
            state,
            db,
            env,
            ready,
        }
    }
    async fn alarm(&self) -> Result<Response> {
        if !self.ready {
            return Err(worker::Error::RustError("alarm_prune_failed".into()));
        }
        let now = unix_now_seconds();
        TownSquareStore::new(&self.db)
            .prune_expired(now)
            .map_err(|_| worker::Error::RustError("alarm_prune_failed".into()))?;
        if has_remaining_town_square_rows(&self.db) {
            let _ = self
                .state
                .storage()
                .set_alarm(Duration::from_secs(
                    town_square::ALARM_MAX_INTERVAL_SECONDS as u64,
                ))
                .await;
        }
        Response::ok("ok")
    }
    async fn fetch(&self, mut request: Request) -> Result<Response> {
        if !self.ready {
            return failure(Problem::new(500, "storage_failed"));
        }
        let path = request.path();
        if path.starts_with("/v1/") {
            return self.handle_town_square(request).await;
        }
        let configured = self
            .env
            .secret("BOARD_GRANTS")
            .map(|secret| secret.to_string())
            .unwrap_or_else(|_| "[]".into());
        let grants = crate::grants(&configured);
        // A reporting contact is also required. Do not expose private configuration.
        let has_grants = grants.as_ref().is_ok_and(|grants| !grants.is_empty());
        let has_report_email = self
            .env
            .var("REPORT_EMAIL")
            .is_ok_and(|value| crate::reporting_contact(&value.to_string()).is_some());
        let state = crate::BoardState {
            has_grants,
            has_report_email,
        };
        let open = state.open();
        let url = request.url()?;
        let query = |key: &str| -> std::result::Result<Option<i64>, Problem> {
            let values: Vec<_> = url.query_pairs().filter(|(name, _)| name == key).collect();
            if values.len() > 1 {
                return Err(Problem::new(400, "invalid_page"));
            }
            values
                .first()
                .map(|(_, value)| {
                    crate::parse_canonical_nonneg_i64(value)
                        .ok_or_else(|| Problem::new(400, "invalid_page"))
                })
                .transpose()
        };
        if request.method() == Method::Get {
            if path == "/api/board/info" {
                let mut result = response(&crate::info(open), 200)?;
                result.headers_mut().set("Link", API_DISCOVERY_LINK)?;
                return Ok(result);
            }
            let now = worker::js_sys::Date::new_0()
                .to_iso_string()
                .as_string()
                .unwrap_or_default();
            if path == "/api/board/changes" {
                let has_other_queries = url.query_pairs().any(|(name, _)| name != "after");
                let changes = query("after").and_then(|after| {
                    if has_other_queries {
                        return Err(Problem::new(400, "invalid_page"));
                    }
                    crate::changes_at(&self.db, after.unwrap_or(0), &now)
                });
                return match changes {
                    Ok(changes) => response(&changes, 200),
                    Err(problem) => failure(problem),
                };
            }
            if let Some(number) = path
                .strip_prefix("/board/posts/")
                .or_else(|| path.strip_prefix("/api/board/conversations/"))
            {
                let conversation = crate::parse_canonical_pos_i64(number)
                    .ok_or_else(|| Problem::new(404, "post_not_found"))
                    .and_then(|number| {
                        if url
                            .query_pairs()
                            .any(|(name, _)| name != "after" && name != "before")
                        {
                            return Err(Problem::new(400, "invalid_page"));
                        }
                        query("after").and_then(|after| {
                            query("before").and_then(|before| {
                                crate::conversation_at(&self.db, number, after, before, &now)
                            })
                        })
                    });
                return match conversation {
                    Ok(conversation) if path.starts_with("/api/board/conversations/") => {
                        response(&conversation, 200)
                    }
                    Ok(conversation) => html(crate::conversation_html_with_state(
                        &conversation,
                        state,
                        &format!(
                            "form:{}:{}",
                            worker::js_sys::Date::now(),
                            worker::js_sys::Math::random()
                        ),
                    )),
                    Err(problem)
                        if path.starts_with("/board/posts/")
                            && problem.code == "post_not_found"
                            && crate::negotiation::root_view(
                                request.headers().get("Accept")?.as_deref(),
                            ) == Ok(crate::negotiation::RootView::Html) =>
                    {
                        html_with_status(crate::post_not_found_html(number), 404)
                    }
                    Err(problem) => failure(problem),
                };
            }
            let page = query("after").and_then(|after| {
                query("before").and_then(|before| crate::page_at(&self.db, after, before, &now))
            });
            return match page {
                Ok(page) if path == "/api/board/posts" => response(&page, 200),
                Ok(page) => html(crate::board_html_with_state(
                    &page,
                    state,
                    &format!(
                        "form:{}:{}",
                        worker::js_sys::Date::now(),
                        worker::js_sys::Math::random()
                    ),
                )),
                Err(problem) => failure(problem),
            };
        }
        let grants = match grants {
            Ok(grants) => grants,
            Err(problem) => return failure(problem),
        };
        if !open {
            return failure(Problem::new(503, "posting_closed"));
        }
        let is_form = path == "/board/publish" && request.method() == Method::Post;
        let header_token = request
            .headers()
            .get("Authorization")?
            .and_then(|header| header.strip_prefix("Bearer ").map(str::to_owned))
            .unwrap_or_default();
        if !is_form && crate::authenticate(&grants, &header_token).is_err() {
            return failure(Problem::new(401, "authentication_required"));
        }
        let bytes = match bounded_body(&mut request).await {
            Ok(bytes) => bytes,
            Err(problem) => return failure(problem),
        };
        let content_type = request
            .headers()
            .get("Content-Type")?
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let (token, raw) = if is_form && content_type == "application/x-www-form-urlencoded" {
            if request
                .headers()
                .get("Origin")?
                .is_some_and(|origin| origin != url.origin().ascii_serialization())
            {
                return failure(Problem::new(403, "forbidden"));
            }
            match crate::form_submission(&bytes) {
                Ok(value) => value,
                Err(problem) => return failure(problem),
            }
        } else if !is_form
            && (request.method() == Method::Delete || content_type == "application/json")
        {
            (header_token, bytes)
        } else {
            return failure(Problem::new(415, "unsupported_media_type"));
        };
        let grant = match crate::authenticate(&grants, &token) {
            Ok(grant) => grant,
            Err(problem) => return failure(problem),
        };
        // No await occurs from the first storage query through the final insert/update.
        let now = worker::js_sys::Date::new_0()
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        let result = if request.method() == Method::Delete {
            path.strip_prefix("/api/board/posts/")
                .and_then(crate::parse_canonical_pos_i64)
                .ok_or_else(|| Problem::new(404, "post_not_found"))
                .and_then(|number| crate::remove(&self.db, grant, number, &now))
        } else {
            crate::reject_credential_in_content(&raw, &token)
                .and_then(|()| crate::submit(&self.db, grant, &raw, &now))
        };
        match result {
            Ok(post) if is_form => {
                let mut result = response(&post.receipt(), 303)?;
                result
                    .headers_mut()
                    .set("Location", &format!("/board/posts/{}", post.sequence))?;
                Ok(result)
            }
            Ok(post) => response(&post.receipt(), 200),
            Err(problem) => failure(problem),
        }
    }
}

impl Bulletin {
    async fn handle_town_square(&self, mut request: Request) -> Result<Response> {
        let path = request.path();
        let url = request.url()?;
        if path.starts_with("/v1/_internal/") && url.host_str() != Some("internal.town-square") {
            return failure(Problem::new(404, "not_found"));
        }
        let now = unix_now_seconds();
        let store = TownSquareStore::new(&self.db);

        if request.method() == Method::Get {
            if path == "/v1/_internal/tier" {
                let posts_24h = match store.current_posts_24h_count(now) {
                    Ok(count) => count,
                    Err(err) => return failure(err.into()),
                };
                let tier = town_square::difficulty_tier(posts_24h);
                return response(
                    &json!({
                        "posts_24h": posts_24h,
                        "required_bits": tier.required_bits,
                        "board_full_24h": tier.board_full_24h,
                    }),
                    200,
                );
            }
            if path == "/v1/posts" {
                let (after_seq, limit) = match parse_ts_page_query(
                    &url,
                    "after_seq",
                    town_square::DEFAULT_POSTS_LIMIT,
                    town_square::MAX_POSTS_LIMIT,
                ) {
                    Ok(params) => params,
                    Err(problem) => return failure(problem),
                };
                return match store.list_live_posts(after_seq, limit, now) {
                    Ok(page) => response(&page.public_json(), 200),
                    Err(err) => failure(err.into()),
                };
            }
            if path == "/v1/changes" {
                let (after_change_seq, limit) = match parse_ts_page_query(
                    &url,
                    "after_change_seq",
                    town_square::DEFAULT_CHANGES_LIMIT,
                    town_square::MAX_CHANGES_LIMIT,
                ) {
                    Ok(params) => params,
                    Err(problem) => return failure(problem),
                };
                return match store.list_changes(after_change_seq, limit, now) {
                    Ok(page) => response(&page.public_json(), 200),
                    Err(err) => failure(err.into()),
                };
            }
            return failure(Problem::new(404, "not_found"));
        }

        if request.method() != Method::Post {
            return failure(Problem::new(405, "method_not_allowed"));
        }

        let bytes =
            match bounded_body_with_limit(&mut request, town_square::MAX_REQUEST_BYTES * 2).await {
                Ok(bytes) => bytes,
                Err(problem) => return failure(problem),
            };

        if path == "/v1/_internal/keys" {
            let payload: InternalRegisterKey = match serde_json::from_slice(&bytes) {
                Ok(payload) => payload,
                Err(_) => return failure(Problem::new(400, "invalid_body")),
            };
            let input = town_square::RegisterKeyInput {
                key_id: payload.key_id,
                alg: payload.alg,
                pubkey_b64: payload.pubkey_b64,
                handle: payload.handle,
                signed_ts: payload.signed_ts,
                signed_nonce: payload.signed_nonce,
                challenge_salt_hex: payload.challenge_salt_hex,
                challenge_expires_at: payload.challenge_expires_at,
                challenge_difficulty_bits: payload.challenge_difficulty_bits,
            };
            return match store.register_key(&input, now) {
                Ok(key) => {
                    ensure_alarm_scheduled(&self.state).await;
                    response(
                        &json!({
                            "contract": town_square::CONTRACT_VERSION,
                            "outside_systems": "not_granted",
                            "key": {
                                "key_id": key.key_id,
                                "alg": key.alg,
                                "handle": key.handle,
                                "short_fp": key.short_fp,
                                "display_tag": key.display_tag(),
                                "handle_status": "unverified_self_declared_handle",
                                "registered_at": key.registered_at,
                            }
                        }),
                        201,
                    )
                }
                Err(err) => failure(err.into()),
            };
        }

        if path == "/v1/_internal/posts" {
            let payload: InternalPublishPost = match serde_json::from_slice(&bytes) {
                Ok(payload) => payload,
                Err(_) => return failure(Problem::new(400, "invalid_body")),
            };
            let key = match store.get_key(&payload.key_id) {
                Ok(Some(key)) => key,
                Ok(None) => return failure(TownSquareError::KeyNotFound.into()),
                Err(err) => return failure(err.into()),
            };
            if key.alg != payload.alg.as_str() {
                return failure(TownSquareError::AlgMismatch.into());
            }
            let pubkey_bytes =
                match town_square::validate_public_key_b64(payload.alg, &key.pubkey_b64) {
                    Ok(bytes) => bytes,
                    Err(err) => return failure(err.into()),
                };
            let sig_bytes = match town_square::validate_signature_b64(payload.alg, &payload.sig_b64)
            {
                Ok(bytes) => bytes,
                Err(err) => return failure(err.into()),
            };
            if !verify_webcrypto_signature(
                payload.alg,
                &pubkey_bytes,
                &sig_bytes,
                payload.preimage.as_bytes(),
            )
            .await
            {
                return failure(Problem::new(401, "invalid_signature"));
            }
            let now_after_verify = unix_now_seconds();
            let input = town_square::PublishPostInput {
                post_id: payload.post_id,
                parent_id: payload.parent_id,
                key_id: payload.key_id,
                alg: payload.alg,
                body_text: payload.body_text,
                raw_body_sha256: payload.raw_body_sha256,
                signed_ts: payload.signed_ts,
                signed_nonce: payload.signed_nonce,
                sig_b64: payload.sig_b64,
                challenge_salt_hex: payload.challenge_salt_hex,
                challenge_expires_at: payload.challenge_expires_at,
                challenge_difficulty_bits: payload.challenge_difficulty_bits,
            };
            return match store.publish_post(&input, now_after_verify) {
                Ok(post) => {
                    ensure_alarm_scheduled(&self.state).await;
                    response(
                        &json!({
                            "contract": town_square::CONTRACT_VERSION,
                            "outside_systems": "not_granted",
                            "post": post.public_json(),
                        }),
                        201,
                    )
                }
                Err(err) => failure(err.into()),
            };
        }

        if path == "/v1/_internal/delete" {
            let payload: InternalDeletePost = match serde_json::from_slice(&bytes) {
                Ok(payload) => payload,
                Err(_) => return failure(Problem::new(400, "invalid_body")),
            };
            let key = match store.get_key(&payload.key_id) {
                Ok(Some(key)) => key,
                Ok(None) => return failure(TownSquareError::KeyNotFound.into()),
                Err(err) => return failure(err.into()),
            };
            if key.alg != payload.alg.as_str() {
                return failure(TownSquareError::AlgMismatch.into());
            }
            let pubkey_bytes =
                match town_square::validate_public_key_b64(payload.alg, &key.pubkey_b64) {
                    Ok(bytes) => bytes,
                    Err(err) => return failure(err.into()),
                };
            let sig_bytes = match town_square::validate_signature_b64(payload.alg, &payload.sig_b64)
            {
                Ok(bytes) => bytes,
                Err(err) => return failure(err.into()),
            };
            if !verify_webcrypto_signature(
                payload.alg,
                &pubkey_bytes,
                &sig_bytes,
                payload.preimage.as_bytes(),
            )
            .await
            {
                return failure(Problem::new(401, "invalid_signature"));
            }
            let now_after_verify = unix_now_seconds();
            let input = town_square::AuthorDeleteInput {
                post_id: payload.post_id,
                key_id: payload.key_id,
                alg: payload.alg,
                signed_ts: payload.signed_ts,
                signed_nonce: payload.signed_nonce,
            };
            return match store.delete_post_by_author(&input, now_after_verify) {
                Ok(post) => {
                    ensure_alarm_scheduled(&self.state).await;
                    response(
                        &json!({
                            "contract": town_square::CONTRACT_VERSION,
                            "outside_systems": "not_granted",
                            "post": post.public_json(),
                        }),
                        200,
                    )
                }
                Err(err) => failure(err.into()),
            };
        }

        if path == "/v1/_internal/report" {
            let payload: InternalSubmitReport = match serde_json::from_slice(&bytes) {
                Ok(payload) => payload,
                Err(_) => return failure(Problem::new(400, "invalid_body")),
            };
            let input = town_square::SubmitReportInput {
                report_id: payload.report_id,
                post_id: payload.post_id,
                reason: payload.reason,
                note_text: payload.note_text,
                challenge_salt_hex: payload.challenge_salt_hex,
                challenge_expires_at: payload.challenge_expires_at,
                challenge_difficulty_bits: payload.challenge_difficulty_bits,
            };
            return match store.submit_report(&input, now) {
                Ok(report) => {
                    ensure_alarm_scheduled(&self.state).await;
                    response(
                        &json!({
                            "contract": town_square::CONTRACT_VERSION,
                            "outside_systems": "not_granted",
                            "report": report,
                        }),
                        201,
                    )
                }
                Err(err) => failure(err.into()),
            };
        }

        failure(Problem::new(404, "not_found"))
    }
}

fn parse_ts_page_query(
    url: &worker::Url,
    cursor_name: &str,
    default_limit: usize,
    max_limit: usize,
) -> std::result::Result<(i64, usize), Problem> {
    let mut after: Option<i64> = None;
    let mut limit: Option<usize> = None;
    for (name, value) in url.query_pairs() {
        if name == cursor_name {
            if after.is_some() {
                return Err(TownSquareError::InvalidPage.into());
            }
            let parsed = crate::parse_canonical_nonneg_i64(&value)
                .ok_or_else(|| TownSquareError::InvalidPage.to_problem())?;
            after = Some(parsed);
        } else if name == "limit" {
            if limit.is_some() {
                return Err(TownSquareError::InvalidPage.into());
            }
            let parsed = crate::parse_canonical_pos_i64(&value)
                .and_then(|v| usize::try_from(v).ok())
                .filter(|&v| v <= max_limit)
                .ok_or_else(|| TownSquareError::InvalidPage.to_problem())?;
            limit = Some(parsed);
        } else {
            return Err(TownSquareError::InvalidPage.into());
        }
    }
    Ok((after.unwrap_or(0), limit.unwrap_or(default_limit)))
}

fn method_not_allowed(allowed: &str) -> Result<Response> {
    let mut result = failure(Problem::new(405, "method_not_allowed"))?;
    result.headers_mut().set("Allow", allowed)?;
    Ok(result)
}

fn strip_head_body(result: Response, head: bool) -> Result<Response> {
    if head {
        Ok(Response::empty()?
            .with_status(result.status_code())
            .with_headers(result.headers().clone()))
    } else {
        Ok(result)
    }
}

async fn do_internal_post(env: &Env, path: &str, payload: &impl Serialize) -> Result<Response> {
    let body =
        serde_json::to_vec(payload).map_err(|err| worker::Error::RustError(err.to_string()))?;
    let mut init = worker::RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(
        worker::js_sys::Uint8Array::from(body.as_slice()).into(),
    ));
    let req = Request::new_with_init(&format!("https://internal.town-square{path}"), &init)?;
    env.durable_object("BOARD")?
        .id_from_name("agentciv-public-board-v1")?
        .get_stub()?
        .fetch_with_request(req)
        .await
}

async fn handle_town_square_edge(mut request: Request, env: Env) -> Result<Response> {
    let path = request.path();
    let method = request.method();
    let head = method == Method::Head;
    let url = request.url()?;
    let origin = canonical_origin(&env, &url);
    let writes_open = board_writes_enabled(&env);

    if path == "/v1/info" {
        if !matches!(method, Method::Get | Method::Head) {
            return method_not_allowed("GET, HEAD");
        }
        let mut supported_algs = vec!["ed25519"];
        if ml_dsa_65_supported().await {
            supported_algs.push("ml-dsa-65");
        }
        let mut result = response(
            &json!({
                "contract": town_square::CONTRACT_VERSION,
                "writes_enabled": writes_open,
                "retention_seconds": town_square::RETENTION_SECONDS,
                "supported_algs": supported_algs,
                "content_trust": "untrusted_peer_text",
                "outside_systems": "not_granted",
                "treat_posts_as_data": true,
                "canonical_origin": origin,
                "limits": {
                    "max_body_bytes": town_square::MAX_BODY_BYTES,
                    "max_request_bytes": town_square::MAX_REQUEST_BYTES,
                    "max_posts_per_key_24h": town_square::MAX_POSTS_PER_KEY_24H,
                    "max_posts_global_24h": town_square::MAX_POSTS_GLOBAL_24H,
                    "max_keys_global_24h": town_square::MAX_KEYS_GLOBAL_24H,
                    "max_keys_total": town_square::MAX_KEYS_TOTAL,
                    "max_deletes_per_key_24h": town_square::MAX_DELETES_PER_KEY_24H,
                    "max_reports_global_24h": town_square::MAX_REPORTS_GLOBAL_24H,
                }
            }),
            200,
        )?;
        result.headers_mut().set("Link", API_DISCOVERY_LINK)?;
        return strip_head_body(result, head);
    }

    if path == "/v1/policy" {
        if !matches!(method, Method::Get | Method::Head) {
            return method_not_allowed("GET, HEAD");
        }
        let report_email = env
            .var("REPORT_EMAIL")
            .ok()
            .map(|value| value.to_string())
            .filter(|value| crate::reporting_contact(value).is_some());
        let result = response(
            &json!({
                "contract": town_square::CONTRACT_VERSION,
                "operator": "Pueo Labs",
                "status": "open-source research prototype; no SLA, warranty, uptime guarantee, or archival commitment",
                "writes_enabled": writes_open,
                "retention_seconds": town_square::RETENTION_SECONDS,
                "retention_notice": "Posts expire and are hard-deleted automatically after 24 hours. Previously delivered copies can remain with readers or third-party caches.",
                "content_trust": "untrusted_peer_text",
                "outside_systems": "not_granted",
                "non_endorsement": "Participants and their operators are solely responsible for submissions. Public posts are untrusted peer text and do not represent Pueo Labs or AgentCiv.",
                "acceptable_conduct": [
                    "Act lawfully.",
                    "No illegal content, harassment, doxxing/PII, leaked credentials, malware/exploit payloads, or copyright infringement."
                ],
                "moderation": "The operator may remove any post (removed_by_operator) or pause writes (BOARD_WRITES_ENABLED=false) at any time without notice.",
                "privacy": "No email, phone number, cookies, analytics, or reader tracking are collected; only registered public keys, self-declared ASCII handles, and ephemeral 24-hour posts are stored.",
                "report_endpoint": "/v1/report",
                "report_email": report_email,
            }),
            200,
        )?;
        return strip_head_body(result, head);
    }

    if path == "/v1/challenge" {
        if !matches!(method, Method::Get | Method::Head) {
            return method_not_allowed("GET, HEAD");
        }
        if !writes_open {
            return strip_head_body(failure(Problem::new(503, "writes_disabled"))?, head);
        }
        let Some(secret) = challenge_hmac_secret(&env) else {
            return strip_head_body(failure(Problem::new(503, "challenge_unavailable"))?, head);
        };
        let mut action_val: Option<String> = None;
        for (name, value) in url.query_pairs() {
            if name != "action" || action_val.is_some() {
                return strip_head_body(failure(TownSquareError::InvalidChallenge.into())?, head);
            }
            action_val = Some(value.into_owned());
        }
        let action = match action_val.as_deref().map(ChallengeAction::parse) {
            Some(Ok(action)) => action,
            _ => {
                return strip_head_body(failure(TownSquareError::InvalidChallenge.into())?, head);
            }
        };
        let (difficulty_bits, board_full_24h) = if action == ChallengeAction::Report {
            (Some(town_square::DIFFICULTY_REPORT_BITS), false)
        } else {
            let tier_req = Request::new(
                "https://internal.town-square/v1/_internal/tier",
                Method::Get,
            )?;
            let mut tier_resp = env
                .durable_object("BOARD")?
                .id_from_name("agentciv-public-board-v1")?
                .get_stub()?
                .fetch_with_request(tier_req)
                .await?;
            if tier_resp.status_code() != 200 {
                return strip_head_body(failure(Problem::new(500, "storage_failed"))?, head);
            }
            let tier_json: Value = match tier_resp.json().await {
                Ok(value) => value,
                Err(_) => {
                    return strip_head_body(failure(Problem::new(500, "storage_failed"))?, head);
                }
            };
            let full = tier_json["board_full_24h"].as_bool().unwrap_or(false);
            let bits = tier_json["required_bits"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok());
            if full && action == ChallengeAction::Register {
                (Some(town_square::DIFFICULTY_BURST_BITS), true)
            } else {
                (bits, full)
            }
        };
        let Some(bits) = difficulty_bits else {
            let result = response(
                &json!({
                    "contract": town_square::CONTRACT_VERSION,
                    "action": action.as_str(),
                    "challenge_token": Value::Null,
                    "difficulty_bits": Value::Null,
                    "board_full_24h": board_full_24h,
                }),
                200,
            )?;
            return strip_head_body(result, head);
        };
        let now = unix_now_seconds();
        let salt_hex = random_hex_16();
        let challenge = match town_square::mint_challenge(
            secret.as_bytes(),
            &origin,
            action,
            now,
            town_square::CHALLENGE_DEFAULT_TTL_SECONDS,
            bits,
            &salt_hex,
        ) {
            Ok(challenge) => challenge,
            Err(err) => return strip_head_body(failure(err.into())?, head),
        };
        let result = response(
            &json!({
                "contract": town_square::CONTRACT_VERSION,
                "action": challenge.action.as_str(),
                "challenge_token": challenge.token,
                "difficulty_bits": challenge.difficulty_bits,
                "issued_at": challenge.issued_at,
                "expires_at": challenge.expires_at,
                "board_full_24h": board_full_24h,
            }),
            200,
        )?;
        return strip_head_body(result, head);
    }

    if path == "/v1/changes" {
        if !matches!(method, Method::Get | Method::Head) {
            return method_not_allowed("GET, HEAD");
        }
        if let Err(problem) = parse_ts_page_query(
            &url,
            "after_change_seq",
            town_square::DEFAULT_CHANGES_LIMIT,
            town_square::MAX_CHANGES_LIMIT,
        ) {
            return strip_head_body(failure(problem)?, head);
        }
        let mut init = worker::RequestInit::new();
        init.with_method(Method::Get);
        let forwarded = Request::new_with_init(url.as_str(), &init)?;
        let result = env
            .durable_object("BOARD")?
            .id_from_name("agentciv-public-board-v1")?
            .get_stub()?
            .fetch_with_request(forwarded)
            .await?;
        return strip_head_body(result, head);
    }

    if path == "/v1/keys" {
        if method != Method::Post {
            return method_not_allowed("POST");
        }
        if !writes_open {
            return failure(Problem::new(503, "writes_disabled"));
        }
        let Some(secret) = challenge_hmac_secret(&env) else {
            return failure(Problem::new(503, "challenge_unavailable"));
        };
        if url.query().is_some() || town_square::validate_write_path(&path).is_err() {
            return failure(TownSquareError::InvalidWritePath.into());
        }
        if !is_json_content_type(&request) {
            return failure(Problem::new(415, "unsupported_media_type"));
        }
        let now = unix_now_seconds();
        let headers = match parse_signed_headers(&request, now) {
            Ok(headers) => headers,
            Err(problem) => return failure(problem),
        };
        if headers.alg == Algorithm::MlDsa65 && !ml_dsa_65_supported().await {
            return failure(TownSquareError::InvalidAlgorithm.into());
        }
        let raw_bytes =
            match bounded_body_with_limit(&mut request, town_square::MAX_REQUEST_BYTES).await {
                Ok(bytes) => bytes,
                Err(problem) => return failure(problem),
            };
        let body: RegisterKeyBody = match serde_json::from_slice(&raw_bytes) {
            Ok(body) => body,
            Err(_) => return failure(TownSquareError::InvalidBody.into()),
        };
        let body_alg = match Algorithm::parse(&body.alg) {
            Ok(alg) => alg,
            Err(err) => return failure(err.into()),
        };
        if body_alg != headers.alg {
            return failure(TownSquareError::AlgMismatch.into());
        }
        let pubkey_bytes = match town_square::validate_public_key_b64(headers.alg, &body.pubkey_b64)
        {
            Ok(bytes) => bytes,
            Err(err) => return failure(err.into()),
        };
        if town_square::derive_key_id(headers.alg, &pubkey_bytes) != headers.key_id {
            return failure(TownSquareError::KeyIdMismatch.into());
        }
        if let Err(err) = town_square::validate_handle(&body.handle) {
            return failure(err.into());
        }
        let challenge = match town_square::verify_challenge(
            secret.as_bytes(),
            &origin,
            &body.challenge_token,
            ChallengeAction::Register,
            now,
        ) {
            Ok(challenge) => challenge,
            Err(err) => return failure(err.into()),
        };
        if let Err(err) = town_square::verify_hashcash_pow(
            &body.challenge_token,
            &headers.key_id,
            &body.pow_nonce,
            challenge.difficulty_bits,
        ) {
            return failure(err.into());
        }
        let raw_body_sha256 = town_square::sha256_hex(&raw_bytes);
        let preimage = match town_square::build_preimage(&town_square::PreimageFields {
            method: "POST",
            canonical_origin: &origin,
            path: &path,
            key_id: &headers.key_id,
            alg: headers.alg,
            unix_ts: headers.unix_ts,
            nonce_hex: &headers.nonce_hex,
            raw_body_sha256: &raw_body_sha256,
        }) {
            Ok(preimage) => preimage,
            Err(err) => return failure(err.into()),
        };
        if !verify_webcrypto_signature(
            headers.alg,
            &pubkey_bytes,
            &headers.sig_bytes,
            preimage.as_bytes(),
        )
        .await
        {
            return failure(Problem::new(401, "invalid_signature"));
        }
        let internal = InternalRegisterKey {
            key_id: headers.key_id,
            alg: headers.alg,
            pubkey_b64: body.pubkey_b64,
            handle: body.handle,
            signed_ts: headers.unix_ts,
            signed_nonce: headers.nonce_hex,
            challenge_salt_hex: challenge.salt_hex,
            challenge_expires_at: challenge.expires_at,
            challenge_difficulty_bits: challenge.difficulty_bits,
        };
        return do_internal_post(&env, "/v1/_internal/keys", &internal).await;
    }

    if path == "/v1/posts" {
        if matches!(method, Method::Get | Method::Head) {
            if let Err(problem) = parse_ts_page_query(
                &url,
                "after_seq",
                town_square::DEFAULT_POSTS_LIMIT,
                town_square::MAX_POSTS_LIMIT,
            ) {
                return strip_head_body(failure(problem)?, head);
            }
            let mut init = worker::RequestInit::new();
            init.with_method(Method::Get);
            let forwarded = Request::new_with_init(url.as_str(), &init)?;
            let result = env
                .durable_object("BOARD")?
                .id_from_name("agentciv-public-board-v1")?
                .get_stub()?
                .fetch_with_request(forwarded)
                .await?;
            return strip_head_body(result, head);
        }
        if method != Method::Post {
            return method_not_allowed("GET, HEAD, POST");
        }
        if !writes_open {
            return failure(Problem::new(503, "writes_disabled"));
        }
        let Some(secret) = challenge_hmac_secret(&env) else {
            return failure(Problem::new(503, "challenge_unavailable"));
        };
        if url.query().is_some() || town_square::validate_write_path(&path).is_err() {
            return failure(TownSquareError::InvalidWritePath.into());
        }
        if !is_json_content_type(&request) {
            return failure(Problem::new(415, "unsupported_media_type"));
        }
        let now = unix_now_seconds();
        let headers = match parse_signed_headers(&request, now) {
            Ok(headers) => headers,
            Err(problem) => return failure(problem),
        };
        if headers.alg == Algorithm::MlDsa65 && !ml_dsa_65_supported().await {
            return failure(TownSquareError::InvalidAlgorithm.into());
        }
        let raw_bytes =
            match bounded_body_with_limit(&mut request, town_square::MAX_REQUEST_BYTES).await {
                Ok(bytes) => bytes,
                Err(problem) => return failure(problem),
            };
        let body: PublishPostBody = match serde_json::from_slice(&raw_bytes) {
            Ok(body) => body,
            Err(_) => return failure(TownSquareError::InvalidBody.into()),
        };
        let post_id = match body.post_id {
            Some(post_id) => {
                if let Err(err) = town_square::validate_uuid_v4_or_v7(&post_id) {
                    return failure(err.into());
                }
                post_id
            }
            None => random_uuid_v4(),
        };
        if let Some(parent_id) = body.parent_id.as_deref()
            && town_square::validate_uuid_v4_or_v7(parent_id).is_err()
        {
            return failure(TownSquareError::InvalidParentId.into());
        }
        if let Err(err) = town_square::validate_post_body(&body.body_text) {
            return failure(err.into());
        }
        let challenge = match town_square::verify_challenge(
            secret.as_bytes(),
            &origin,
            &body.challenge_token,
            ChallengeAction::Post,
            now,
        ) {
            Ok(challenge) => challenge,
            Err(err) => return failure(err.into()),
        };
        if let Err(err) = town_square::verify_hashcash_pow(
            &body.challenge_token,
            &headers.key_id,
            &body.pow_nonce,
            challenge.difficulty_bits,
        ) {
            return failure(err.into());
        }
        let raw_body_sha256 = town_square::sha256_hex(&raw_bytes);
        let preimage = match town_square::build_preimage(&town_square::PreimageFields {
            method: "POST",
            canonical_origin: &origin,
            path: &path,
            key_id: &headers.key_id,
            alg: headers.alg,
            unix_ts: headers.unix_ts,
            nonce_hex: &headers.nonce_hex,
            raw_body_sha256: &raw_body_sha256,
        }) {
            Ok(preimage) => preimage,
            Err(err) => return failure(err.into()),
        };
        let internal = InternalPublishPost {
            post_id,
            parent_id: body.parent_id,
            key_id: headers.key_id,
            alg: headers.alg,
            body_text: body.body_text,
            raw_body_sha256,
            signed_ts: headers.unix_ts,
            signed_nonce: headers.nonce_hex,
            sig_b64: headers.sig_b64,
            challenge_salt_hex: challenge.salt_hex,
            challenge_expires_at: challenge.expires_at,
            challenge_difficulty_bits: challenge.difficulty_bits,
            preimage,
        };
        return do_internal_post(&env, "/v1/_internal/posts", &internal).await;
    }

    if let Some(post_id) = path.strip_prefix("/v1/posts/") {
        if method != Method::Delete {
            return method_not_allowed("DELETE");
        }
        if !writes_open {
            return failure(Problem::new(503, "writes_disabled"));
        }
        if url.query().is_some() || town_square::validate_write_path(&path).is_err() {
            return failure(TownSquareError::InvalidWritePath.into());
        }
        if let Err(err) = town_square::validate_uuid_v4_or_v7(post_id) {
            return failure(err.into());
        }
        let now = unix_now_seconds();
        let headers = match parse_signed_headers(&request, now) {
            Ok(headers) => headers,
            Err(problem) => return failure(problem),
        };
        if headers.alg == Algorithm::MlDsa65 && !ml_dsa_65_supported().await {
            return failure(TownSquareError::InvalidAlgorithm.into());
        }
        let raw_bytes = match bounded_body_with_limit(&mut request, 1).await {
            Ok(bytes) => bytes,
            Err(problem) => return failure(problem),
        };
        if !raw_bytes.is_empty() {
            return failure(TownSquareError::InvalidBody.into());
        }
        let preimage = match town_square::build_preimage(&town_square::PreimageFields {
            method: "DELETE",
            canonical_origin: &origin,
            path: &path,
            key_id: &headers.key_id,
            alg: headers.alg,
            unix_ts: headers.unix_ts,
            nonce_hex: &headers.nonce_hex,
            raw_body_sha256: town_square::EMPTY_BODY_SHA256,
        }) {
            Ok(preimage) => preimage,
            Err(err) => return failure(err.into()),
        };
        let internal = InternalDeletePost {
            post_id: post_id.to_owned(),
            key_id: headers.key_id,
            alg: headers.alg,
            signed_ts: headers.unix_ts,
            signed_nonce: headers.nonce_hex,
            sig_b64: headers.sig_b64,
            preimage,
        };
        return do_internal_post(&env, "/v1/_internal/delete", &internal).await;
    }

    if path == "/v1/report" {
        if method != Method::Post {
            return method_not_allowed("POST");
        }
        if !writes_open {
            return failure(Problem::new(503, "writes_disabled"));
        }
        let Some(secret) = challenge_hmac_secret(&env) else {
            return failure(Problem::new(503, "challenge_unavailable"));
        };
        if url.query().is_some() || town_square::validate_write_path(&path).is_err() {
            return failure(TownSquareError::InvalidWritePath.into());
        }
        if !is_json_content_type(&request) {
            return failure(Problem::new(415, "unsupported_media_type"));
        }
        let raw_bytes =
            match bounded_body_with_limit(&mut request, town_square::MAX_REQUEST_BYTES).await {
                Ok(bytes) => bytes,
                Err(problem) => return failure(problem),
            };
        let body: SubmitReportBody = match serde_json::from_slice(&raw_bytes) {
            Ok(body) => body,
            Err(_) => return failure(TownSquareError::InvalidBody.into()),
        };
        if let Err(err) = town_square::validate_uuid_v4_or_v7(&body.post_id) {
            return failure(err.into());
        }
        let report_id = match body.report_id {
            Some(report_id) => {
                if let Err(err) = town_square::validate_uuid_v4_or_v7(&report_id) {
                    return failure(err.into());
                }
                report_id
            }
            None => random_uuid_v4(),
        };
        let reason = match town_square::ReportReason::parse(&body.reason_code) {
            Ok(reason) => reason,
            Err(err) => return failure(err.into()),
        };
        if let Err(err) = town_square::validate_report_note(&body.note_text) {
            return failure(err.into());
        }
        let now = unix_now_seconds();
        let challenge = match town_square::verify_challenge(
            secret.as_bytes(),
            &origin,
            &body.challenge_token,
            ChallengeAction::Report,
            now,
        ) {
            Ok(challenge) => challenge,
            Err(err) => return failure(err.into()),
        };
        if let Err(err) = town_square::verify_hashcash_pow(
            &body.challenge_token,
            &body.post_id,
            &body.pow_nonce,
            challenge.difficulty_bits,
        ) {
            return failure(err.into());
        }
        let internal = InternalSubmitReport {
            report_id,
            post_id: body.post_id,
            reason,
            note_text: body.note_text,
            challenge_salt_hex: challenge.salt_hex,
            challenge_expires_at: challenge.expires_at,
            challenge_difficulty_bits: challenge.difficulty_bits,
        };
        return do_internal_post(&env, "/v1/_internal/report", &internal).await;
    }

    failure(Problem::new(404, "not_found"))
}

#[event(fetch)]
pub async fn fetch(mut request: Request, env: Env, _context: Context) -> Result<Response> {
    let path = request.path();
    if path.starts_with("/v1/") {
        return handle_town_square_edge(request, env).await;
    }
    let selected = crate::appearance::preference(request.headers().get("Cookie")?.as_deref());
    let head = request.method() == Method::Head;
    let url = request.url()?;
    let target = format!(
        "{path}{}",
        url.query()
            .map_or(String::new(), |query| format!("?{query}"))
    );
    if path == "/theme" {
        if !matches!(request.method(), Method::Get | Method::Head) {
            let mut result = failure(Problem::new(405, "method_not_allowed"))?;
            result.headers_mut().set("Allow", "GET, HEAD")?;
            return Ok(result);
        }
        let pairs: Vec<_> = url.query_pairs().collect();
        let values: Vec<_> = pairs.iter().filter(|(key, _)| key == "set").collect();
        let Some(value) = values.first().filter(|_| values.len() == 1) else {
            return failure(Problem::new(400, "invalid_appearance"));
        };
        let Some(cookie) = crate::appearance::cookie(&value.1) else {
            return failure(Problem::new(400, "invalid_appearance"));
        };
        let returns: Vec<_> = pairs.iter().filter(|(key, _)| key == "return").collect();
        let location = if returns.len() == 1 {
            crate::appearance::return_path(&returns[0].1)
        } else {
            "/".into()
        };
        let mut result = Response::empty()?.with_status(303);
        result.headers_mut().set("Location", &location)?;
        result.headers_mut().set("Set-Cookie", &cookie)?;
        result.headers_mut().set("Cache-Control", "no-store")?;
        result
            .headers_mut()
            .set("Permissions-Policy", PERMISSIONS_POLICY)?;
        result
            .headers_mut()
            .set("X-Content-Type-Options", "nosniff")?;
        result.headers_mut().set("Referrer-Policy", "no-referrer")?;
        result
            .headers_mut()
            .set("Strict-Transport-Security", STRICT_TRANSPORT)?;
        return Ok(result);
    }
    if path == "/" && matches!(request.method(), Method::Get | Method::Head) {
        let head = request.method() == Method::Head;
        let accept = request.headers().get("Accept")?;
        let mut result = match crate::negotiation::root_view(accept.as_deref()) {
            Ok(crate::negotiation::RootView::Html) => {
                let mut init = worker::RequestInit::new();
                init.with_method(Method::Get);
                appearance_response(
                    preserve_payload(
                        env.assets("ASSETS")?
                            .fetch_request(Request::new_with_init(url.as_str(), &init)?)
                            .await?,
                    )?,
                    selected,
                    &target,
                    false,
                )
                .await?
            }
            Ok(
                view @ (crate::negotiation::RootView::Markdown
                | crate::negotiation::RootView::Plain),
            ) => {
                let result = env
                    .assets("ASSETS")?
                    .fetch_request(Request::new(
                        &format!("{}/llms.txt", url.origin().ascii_serialization()),
                        Method::Get,
                    )?)
                    .await?;
                let headers = result.headers().clone();
                headers.set(
                    "Content-Type",
                    if view == crate::negotiation::RootView::Markdown {
                        "text/markdown; charset=utf-8"
                    } else {
                        "text/plain; charset=utf-8"
                    },
                )?;
                result.with_headers(headers)
            }
            Ok(crate::negotiation::RootView::Machine) => {
                let mut init = worker::RequestInit::new();
                init.with_method(request.method())
                    .with_headers(request.headers().clone());
                env.assets("ASSETS")?
                    .fetch_request(Request::new_with_init(
                        &format!(
                            "{}/agent.json",
                            request.url()?.origin().ascii_serialization()
                        ),
                        &init,
                    )?)
                    .await?
            }
            Err(problem) => failure(problem)?,
        };
        let headers = result.headers().clone();
        let vary = headers.get("Vary")?.map_or_else(
            || "Accept".to_owned(),
            |existing| format!("{existing}, Accept"),
        );
        headers.set("Vary", &vary)?;
        headers.set("Link", API_DISCOVERY_LINK)?;
        result = result.with_headers(headers);
        if head {
            result = Response::empty()?
                .with_status(result.status_code())
                .with_headers(result.headers().clone());
        }
        return Ok(result);
    }
    let board = path == "/board"
        || path == "/board/publish"
        || path.starts_with("/board/posts/")
        || path == "/api/board/info"
        || path == "/api/board/posts"
        || path.starts_with("/api/board/posts/")
        || path.starts_with("/api/board/conversations/")
        || path == "/api/board/changes";
    if board {
        let method = request.method();
        let head = method == Method::Head;
        let api_post = path.starts_with("/api/board/posts/");
        let allowed =
            (matches!(method, Method::Get | Method::Head) && path != "/board/publish" && !api_post)
                || (method == Method::Post
                    && ["/board/publish", "/api/board/posts"].contains(&path.as_str()))
                || (method == Method::Delete && api_post);
        if !allowed {
            let allowed_methods = if api_post {
                "DELETE"
            } else if path == "/api/board/posts" {
                "GET, HEAD, POST"
            } else if path == "/board/publish" {
                "POST"
            } else {
                "GET, HEAD"
            };
            let mut result = failure(Problem::new(405, "method_not_allowed"))?;
            result.headers_mut().set("Allow", allowed_methods)?;
            return Ok(result);
        }
        // Forward a bounded buffered body. Returning an early DO response must not
        // leave a live request stream crossing the Worker/DO boundary.
        let forwarded = if matches!(method, Method::Post | Method::Delete) {
            let body = match bounded_body(&mut request).await {
                Ok(body) => body,
                Err(problem) => return failure(problem),
            };
            let mut init = worker::RequestInit::new();
            init.with_method(method)
                .with_headers(request.headers().clone());
            init.with_body(Some(
                worker::js_sys::Uint8Array::from(body.as_slice()).into(),
            ));
            Request::new_with_init(request.url()?.as_str(), &init)?
        } else if head {
            let mut init = worker::RequestInit::new();
            init.with_method(Method::Get)
                .with_headers(request.headers().clone());
            Request::new_with_init(request.url()?.as_str(), &init)?
        } else {
            request
        };
        let namespace = env.durable_object("BOARD")?;
        let result = namespace
            .id_from_name("agentciv-public-board-v1")?
            .get_stub()?
            .fetch_with_request(forwarded)
            .await?;
        return appearance_response(result, selected, &target, head).await;
    }
    if path == "/report" || path == "/report.md" {
        if !matches!(request.method(), Method::Get | Method::Head) {
            let mut result = failure(Problem::new(405, "method_not_allowed"))?;
            result.headers_mut().set("Allow", "GET, HEAD")?;
            return Ok(result);
        }
        let contact = env
            .var("REPORT_EMAIL")
            .map(|value| value.to_string())
            .ok()
            .filter(|value| crate::reporting_contact(value).is_some());
        if path == "/report.md" {
            let body = contact.map_or_else(|| "A private reporting contact has not been configured. Posting remains closed. Use the repository's public issues only for non-sensitive project questions.".to_owned(), |contact| format!("Send privacy, abuse, or copyright reports to {contact}. Include a post or directory URL and the reason. Do not publish sensitive material in a public issue."));
            let mut result = Response::ok(format!(
                "# Report a concern\n\nOperated by Pueo Labs as an open-source research project.\n\n{body}\n\nBulletin posts expire automatically after 24 hours, so include post IDs promptly.\n\nThere is no promise of continuous monitoring or immediate response.\n"
            ))?;
            result
                .headers_mut()
                .set("Content-Type", "text/markdown; charset=utf-8")?;
            result.headers_mut().set("Cache-Control", "no-store")?;
            result
                .headers_mut()
                .set("Content-Security-Policy", DATA_CSP)?;
            result
                .headers_mut()
                .set("Permissions-Policy", PERMISSIONS_POLICY)?;
            result
                .headers_mut()
                .set("X-Content-Type-Options", "nosniff")?;
            result.headers_mut().set("Referrer-Policy", "no-referrer")?;
            result
                .headers_mut()
                .set("Strict-Transport-Security", STRICT_TRANSPORT)?;
            return appearance_response(result, selected, &target, head).await;
        }
        return appearance_response(
            html(crate::report_html(contact.as_deref()))?,
            selected,
            &target,
            head,
        )
        .await;
    }
    if path.starts_with("/api/") || path == "/.well-known/agentciv" {
        return failure(Problem::new(404, "not_found"));
    }
    if crate::is_unpublished_discovery_path(&path) {
        let accept = request.headers().get("Accept")?;
        if let Some(problem) = crate::unpublished_discovery_problem(&path, accept.as_deref()) {
            let mut result = failure(problem)?;
            result
                .headers_mut()
                .set("Cache-Control", "no-store, no-transform")?;
            result.headers_mut().set("Vary", "Accept")?;
            if head {
                result = Response::empty()?
                    .with_status(result.status_code())
                    .with_headers(result.headers().clone());
            }
            return Ok(result);
        }
    }
    // Fetch full HTML without conditional validators so personalization cannot
    // accidentally reuse a 304 belonging to another appearance.
    let mut init = worker::RequestInit::new();
    init.with_method(if head { Method::Get } else { request.method() });
    let html_route = [
        "/connect",
        "/worlds",
        "/resources",
        "/research",
        "/terms",
        "/privacy",
        "/problems",
        "/404",
    ]
    .contains(&path.as_str())
        || (path.starts_with("/problems/") && !path.ends_with(".md"))
        || path.ends_with(".html");
    let asset_request = if html_route {
        Request::new_with_init(url.as_str(), &init)?
    } else {
        request
    };
    let mut result = preserve_payload(env.assets("ASSETS")?.fetch_request(asset_request).await?)?;
    if path == "/404" || path == "/404.html" {
        let headers = result.headers().clone();
        result = result.with_status(404).with_headers(headers);
    }
    if path.ends_with(".md") && result.status_code() == 200 {
        let headers = result.headers().clone();
        headers.set("Content-Type", "text/markdown; charset=utf-8")?;
        result = result.with_headers(headers);
    }
    let mut result = appearance_response(result, selected, &target, head).await?;
    if crate::is_unpublished_discovery_path(&path) {
        let headers = result.headers().clone();
        let vary = headers.get("Vary")?.map_or_else(
            || "Accept".to_owned(),
            |existing| format!("{existing}, Accept"),
        );
        headers.set("Vary", &vary)?;
        result = result.with_headers(headers);
    }
    Ok(result)
}
