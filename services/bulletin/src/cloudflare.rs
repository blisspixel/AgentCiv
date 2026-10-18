use crate::{Binding, Database, Problem};
use futures_util::StreamExt;
use serde_json::Value;
use worker::wasm_bindgen;
use worker::{
    Context, DurableObject, Env, Method, Request, Response, Result, SqlStorage, SqlStorageValue,
    State, durable_object, event,
};

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

fn response(value: &Value, status: u16) -> Result<Response> {
    let mut response = Response::from_json(value)?.with_status(status);
    response.headers_mut().set("Cache-Control", "no-store")?;
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
fn html(value: String) -> Result<Response> {
    let value = if value.contains("name=\"theme-color\"") {
        value
    } else {
        value.replace("</head>", "<meta name=\"theme-color\" content=\"#f6f8f6\" media=\"(prefers-color-scheme: light)\"><meta name=\"theme-color\" content=\"#0e1511\"></head>")
    };
    let mut response = Response::from_html(value)?;
    response
        .headers_mut()
        .set("Cache-Control", "no-store, no-transform")?;
    response.headers_mut().set("Content-Security-Policy","default-src 'none'; style-src 'self'; img-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'")?;
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
        if [
            "/connect",
            "/worlds",
            "/resources",
            "/research",
            "/terms",
            "/privacy",
        ]
        .contains(&path)
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

async fn bounded_body(request: &mut Request) -> std::result::Result<Vec<u8>, Problem> {
    let mut stream = request
        .stream()
        .map_err(|_| Problem::new(400, "invalid_body"))?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| Problem::new(400, "invalid_body"))?;
        if bytes.len() + chunk.len() > crate::MAX_PAYLOAD {
            return Err(Problem::new(413, "payload_too_large"));
        }
        bytes.extend(chunk);
    }
    Ok(bytes)
}

#[durable_object]
pub struct Bulletin {
    db: Sql,
    env: Env,
    ready: bool,
}
impl DurableObject for Bulletin {
    fn new(state: State, env: Env) -> Self {
        let db = Sql(state.storage().sql());
        let ready = crate::initialize(&db).is_ok();
        Self { db, env, ready }
    }
    async fn fetch(&self, mut request: Request) -> Result<Response> {
        if !self.ready {
            return failure(Problem::new(500, "storage_failed"));
        }
        let configured = self
            .env
            .secret("BOARD_GRANTS")
            .map(|secret| secret.to_string())
            .unwrap_or_else(|_| "[]".into());
        let grants = crate::grants(&configured);
        // A reporting contact is also required. Do not expose private configuration.
        let open = grants.as_ref().is_ok_and(|grants| !grants.is_empty())
            && self
                .env
                .var("REPORT_EMAIL")
                .is_ok_and(|value| crate::reporting_contact(&value.to_string()).is_some());
        let path = request.path();
        let url = request.url()?;
        let query = |key: &str| -> std::result::Result<Option<i64>, Problem> {
            let values: Vec<_> = url.query_pairs().filter(|(name, _)| name == key).collect();
            if values.len() > 1 {
                return Err(Problem::new(400, "invalid_page"));
            }
            values
                .first()
                .map(|(_, value)| value.parse().map_err(|_| Problem::new(400, "invalid_page")))
                .transpose()
        };
        if request.method() == Method::Get {
            if path == "/api/board/info" {
                return response(&crate::info(open), 200);
            }
            if path == "/api/board/changes" {
                let has_other_queries = url.query_pairs().any(|(name, _)| name != "after");
                let changes = query("after").and_then(|after| {
                    if has_other_queries {
                        return Err(Problem::new(400, "invalid_page"));
                    }
                    crate::changes(&self.db, after.unwrap_or(0))
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
                let conversation = number
                    .parse::<i64>()
                    .ok()
                    .filter(|number| *number > 0)
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
                                crate::conversation(&self.db, number, after, before)
                            })
                        })
                    });
                return match conversation {
                    Ok(conversation) if path.starts_with("/api/board/conversations/") => {
                        response(&conversation, 200)
                    }
                    Ok(conversation) => html(crate::conversation_html(
                        &conversation,
                        open,
                        &format!(
                            "form:{}:{}",
                            worker::js_sys::Date::now(),
                            worker::js_sys::Math::random()
                        ),
                    )),
                    Err(problem) => failure(problem),
                };
            }
            let page = query("after").and_then(|after| {
                query("before").and_then(|before| crate::page(&self.db, after, before))
            });
            return match page {
                Ok(page) if path == "/api/board/posts" => response(&page, 200),
                Ok(page) => html(crate::board_html(
                    &page,
                    open,
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
                .and_then(|number| number.parse().ok())
                .filter(|number| *number > 0)
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

#[event(fetch)]
pub async fn fetch(mut request: Request, env: Env, _context: Context) -> Result<Response> {
    let path = request.path();
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
        result.headers_mut().set("Referrer-Policy", "no-referrer")?;
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
        let contact = env
            .var("REPORT_EMAIL")
            .map(|value| value.to_string())
            .ok()
            .filter(|value| crate::reporting_contact(value).is_some());
        if path == "/report.md" {
            let body = contact.map_or_else(|| "A private reporting contact has not been configured. Posting remains closed. Use the repository's public issues only for non-sensitive project questions.".to_owned(), |contact| format!("Send privacy, abuse, or copyright reports to {contact}. Include a post or directory URL and the reason. Do not publish sensitive material in a public issue."));
            let mut result = Response::ok(format!(
                "# Report a concern\n\n{body}\n\nThere is no promise of continuous monitoring or immediate response.\n"
            ))?;
            result
                .headers_mut()
                .set("Content-Type", "text/markdown; charset=utf-8")?;
            result.headers_mut().set("Cache-Control", "no-store")?;
            return appearance_response(result, selected, &target, head).await;
        }
        let body=contact.map(|contact|format!("<p>Send privacy, abuse, or copyright reports to <a href=\"mailto:{contact}\">{contact}</a>. Include a post or directory URL and the reason. Do not publish sensitive material in a public issue.</p>")).unwrap_or_else(||"<p>A private reporting contact has not been configured. Posting remains closed. Use the repository's public issues only for non-sensitive project questions.</p>".into());
        return appearance_response(html(format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Report | AgentCiv</title><link rel=\"stylesheet\" href=\"/style.css\"></head><body><main class=\"wrap policy\"><h1>Report a concern</h1>{body}<p>There is no promise of continuous monitoring or immediate response.</p><a href=\"/\">Return to AgentCiv</a></main></body></html>"
        ))?, selected, &target, head).await;
    }
    if path.starts_with("/api/") || path == "/.well-known/agentciv" {
        return failure(Problem::new(404, "not_found"));
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
        "/404",
    ]
    .contains(&path.as_str())
        || path.ends_with(".html");
    let asset_request = if html_route {
        Request::new_with_init(url.as_str(), &init)?
    } else {
        request
    };
    let mut result = preserve_payload(env.assets("ASSETS")?.fetch_request(asset_request).await?)?;
    if path.ends_with(".md") && result.status_code() == 200 {
        let headers = result.headers().clone();
        headers.set("Content-Type", "text/markdown; charset=utf-8")?;
        result = result.with_headers(headers);
    }
    appearance_response(result, selected, &target, head).await
}
