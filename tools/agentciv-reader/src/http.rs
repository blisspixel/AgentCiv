//! Replaceable loopback-only, read-only adapter. No proxy, redirect, or implicit restart.

use crate::{Budgets, Error, ReadResult, Result, Traversal, collect_until, reflected, validators};
use agentciv_archive::parse_unique;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE};
use reqwest::{Url, redirect};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const MAX_CONFIG_BYTES: usize = 65_536;

/// Trusted private operator input, never supplied by a record or model decision.
/// The credential authorizes reading only as configured by the host.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub origin: String,
    pub token: String,
    pub world: String,
    #[serde(default)]
    pub traversal: Traversal,
    #[serde(default)]
    pub budgets: Budgets,
    #[serde(default)]
    pub prefix_validation: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrefixResume {
    pub origin: String,
    pub world: String,
    pub cursor: String,
    pub prefix_sha256: String,
}

impl Config {
    pub fn parse(input: &str) -> Result<Self> {
        if input.len() > MAX_CONFIG_BYTES {
            return Err(Error::InputLimit);
        }
        let value = parse_unique(input).map_err(|_| Error::InvalidJson)?;
        let config: Self = serde_json::from_value(value).map_err(|_| Error::Configuration)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<Url> {
        self.budgets.validate()?;
        if self.prefix_validation && self.traversal != Traversal::All {
            return Err(Error::Configuration);
        }
        if self.world.is_empty()
            || self.world.len() > 1024
            || self.token.is_empty()
            || self.token.len() > 4096
            || !self
                .token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
            || self.world.contains(&self.token)
        {
            return Err(Error::Configuration);
        }
        let origin = loopback_url(&self.origin).map_err(|_| Error::InvalidOrigin)?;
        if origin.path() != "/" || origin.query().is_some() {
            return Err(Error::InvalidOrigin);
        }
        Ok(origin)
    }
}

fn loopback_url(input: &str) -> Result<Url> {
    if input.len() > 8192 {
        return Err(Error::InvalidEndpoint);
    }
    let authority = input
        .strip_prefix("http://")
        .ok_or(Error::InvalidEndpoint)?
        .split(['/', '?', '#'])
        .next()
        .ok_or(Error::InvalidEndpoint)?;
    let (literal, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (literal, suffix) = bracketed.split_once(']').ok_or(Error::InvalidEndpoint)?;
        (
            literal,
            if suffix.is_empty() {
                None
            } else {
                Some(suffix.strip_prefix(':').ok_or(Error::InvalidEndpoint)?)
            },
        )
    } else {
        authority
            .rsplit_once(':')
            .map_or((authority, None), |(literal, port)| (literal, Some(port)))
    };
    let ip: IpAddr = literal.parse().map_err(|_| Error::InvalidEndpoint)?;
    if !ip.is_loopback()
        || port.is_some_and(|port| !port.parse::<u16>().is_ok_and(|number| number > 0))
    {
        return Err(Error::InvalidEndpoint);
    }
    let url = Url::parse(input).map_err(|_| Error::InvalidEndpoint)?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(Error::InvalidEndpoint);
    }
    Ok(url)
}

fn endpoint(input: &str, origin: &Url) -> Result<Url> {
    let url = loopback_url(input)?;
    if url.origin() != origin.origin() {
        return Err(Error::InvalidEndpoint);
    }
    Ok(url)
}

fn status_is_gone(text: &str) -> bool {
    #[derive(Deserialize)]
    struct Status {
        status: Box<serde_json::value::RawValue>,
    }
    let Ok(status) = serde_json::from_str::<Status>(text) else {
        return false;
    };
    let (coefficient, exponent) = status
        .status
        .get()
        .split_once(['e', 'E'])
        .unwrap_or((status.status.get(), "0"));
    let Ok(exponent) = exponent.parse::<i64>() else {
        return false;
    };
    let (whole, fraction) = coefficient.split_once('.').unwrap_or((coefficient, ""));
    let digits = format!("{whole}{fraction}");
    let significant = digits.trim_start_matches('0').trim_end_matches('0');
    let trailing = digits.len() - digits.trim_end_matches('0').len();
    significant == "41"
        && exponent
            .checked_sub(fraction.len() as i64)
            .and_then(|power| power.checked_add(trailing as i64))
            == Some(1)
}

fn gone_error(body: &[u8]) -> Error {
    static PROBLEM: OnceLock<jsonschema::Validator> = OnceLock::new();
    let validator = PROBLEM.get_or_init(|| {
        let schema = serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../schemas/problem.schema.json"
        ))
        .expect("bundled problem schema");
        jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .expect("bundled problem validator")
    });
    let text = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return Error::CursorExpired,
    };
    let value = match parse_unique(text) {
        Ok(value) if validator.is_valid(&value) && status_is_gone(text) => value,
        _ => return Error::CursorExpired,
    };
    match value["code"].as_str() {
        Some("prefix_changed") => Error::PrefixChanged,
        Some("prefix_unavailable") => Error::PrefixUnavailable,
        _ => Error::CursorExpired,
    }
}

fn get(
    client: &Client,
    url: Url,
    token: Option<&str>,
    deadline: Instant,
    allowance: usize,
    restricted: bool,
) -> Result<Vec<u8>> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(Error::Deadline)?;
    let mut request = client
        .get(url)
        .header(ACCEPT, "application/json")
        .timeout(remaining);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let mut response = request.send().map_err(|_| {
        if Instant::now() >= deadline {
            Error::Deadline
        } else {
            Error::Transport
        }
    })?;
    let gone = match response.status().as_u16() {
        200 => false,
        410 => true,
        401 => return Err(Error::Authentication),
        403 => return Err(Error::Forbidden),
        _ => return Err(Error::Http),
    };
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next());
    let expected_type = if gone {
        "application/problem+json"
    } else {
        "application/json"
    };
    if !content_type.is_some_and(|value| value.trim().eq_ignore_ascii_case(expected_type)) {
        return Err(if gone {
            Error::CursorExpired
        } else {
            Error::ContentType
        });
    }
    if restricted
        && !response
            .headers()
            .get_all(CACHE_CONTROL)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .any(|value| value.trim().eq_ignore_ascii_case("no-store"))
    {
        return Err(Error::CacheControl);
    }
    if response
        .content_length()
        .is_some_and(|length| length > allowance as u64)
    {
        return Err(Error::ResponseLimit);
    }
    let mut body = Vec::new();
    (&mut response)
        .take((allowance + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| {
            if Instant::now() >= deadline {
                Error::Deadline
            } else {
                Error::Transport
            }
        })?;
    if Instant::now() >= deadline {
        return Err(Error::Deadline);
    }
    if body.len() > allowance {
        return Err(Error::ResponseLimit);
    }
    if gone {
        Err(gone_error(&body))
    } else {
        Ok(body)
    }
}

/// Read exactly the configured caller's view. No credential is sent during discovery.
/// Output remains read data; its report explicitly grants no copying permission.
pub fn read(config: &Config) -> Result<ReadResult> {
    read_inner(config, None)
}

pub fn sync(config: &Config, previous: &str) -> Result<ReadResult> {
    if !config.prefix_validation || config.traversal != Traversal::All {
        return Err(Error::Configuration);
    }
    let validated = crate::offers::validated(previous)?;
    let value = parse_unique(previous).map_err(|_| Error::InvalidResume)?;
    let resume: PrefixResume =
        serde_json::from_value(value["resume"].clone()).map_err(|_| Error::InvalidResume)?;
    if resume.world != config.world
        || validated.world != config.world
        || resume.origin != config.validate()?.to_string()
        || resume.cursor.is_empty()
        || resume.cursor.len() > 8192
        || resume.prefix_sha256 != prefix::digest(&validated.records)
        || validated.records.len() > config.budgets.max_events
        || reflected(previous, &value, Some(&config.token))
    {
        return Err(Error::InvalidResume);
    }
    read_inner(config, Some((resume, validated.records)))
}

fn read_inner(
    config: &Config,
    previous: Option<(PrefixResume, Vec<String>)>,
) -> Result<ReadResult> {
    let origin = config.validate()?;
    let deadline = Instant::now() + Duration::from_secs(config.budgets.seconds);
    let client = Client::builder()
        .no_proxy()
        .redirect(redirect::Policy::none())
        .timeout(Duration::from_secs(config.budgets.seconds))
        .build()
        .map_err(|_| Error::Transport)?;
    let discovery_url = origin
        .join("/.well-known/agentciv")
        .map_err(|_| Error::InvalidOrigin)?;
    let allowance = config
        .budgets
        .max_response_bytes
        .min(config.budgets.max_total_bytes);
    let discovery_bytes =
        get(&client, discovery_url, None, deadline, allowance, false).map_err(|error| {
            if error == Error::ResponseLimit && allowance < config.budgets.max_response_bytes {
                Error::TotalLimit
            } else {
                error
            }
        })?;
    let raw = std::str::from_utf8(&discovery_bytes).map_err(|_| Error::InvalidUtf8)?;
    let discovery = parse_unique(raw).map_err(|_| Error::InvalidJson)?;
    if reflected(raw, &discovery, Some(&config.token)) {
        return Err(Error::CredentialReflection);
    }
    if !validators()[0].is_valid(&discovery) || discovery["id"] != config.world {
        return Err(Error::InvalidDiscovery);
    }
    if config.prefix_validation
        && !discovery["capabilities"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item == "events.prefix.validate"))
    {
        return Err(Error::PrefixUnsupported);
    }
    for value in discovery["endpoints"]
        .as_object()
        .ok_or(Error::InvalidDiscovery)?
        .values()
    {
        endpoint(value.as_str().ok_or(Error::InvalidEndpoint)?, &origin)?;
    }
    let events = endpoint(
        discovery["endpoints"]["events"]
            .as_str()
            .ok_or(Error::InvalidEndpoint)?,
        &origin,
    )?;
    if events
        .query_pairs()
        .any(|(key, _)| key == "after" || key == "prefix")
    {
        return Err(Error::InvalidEndpoint);
    }
    let (initial_cursor, cached) = previous.map_or((None, Vec::new()), |(resume, records)| {
        (Some(resume.cursor), records)
    });
    let mut prefix_records = cached.clone();
    let mut final_cursor = None;
    let mut tail_budgets = config.budgets.clone();
    tail_budgets.max_events = config
        .budgets
        .max_events
        .saturating_sub(cached.len())
        .max(1);
    let mut result = collect_until(
        &config.world,
        &tail_budgets,
        config.traversal,
        Some(&config.token),
        deadline,
        discovery_bytes.len(),
        |after, _remaining, allowance| {
            let mut url = events.clone();
            if let Some(cursor) = after.or(initial_cursor.as_deref()) {
                url.query_pairs_mut().append_pair("after", cursor);
            }
            if config.prefix_validation {
                url.query_pairs_mut().append_pair("prefix", "validate");
            }
            let bytes = get(&client, url, Some(&config.token), deadline, allowance, true).map_err(
                |error| {
                    if error == Error::ResponseLimit
                        && allowance < config.budgets.max_response_bytes
                    {
                        Error::TotalLimit
                    } else {
                        error
                    }
                },
            )?;
            if config.prefix_validation {
                final_cursor = Some(prefix::validate(
                    &bytes,
                    &mut prefix_records,
                    &config.token,
                )?);
            }
            Ok(bytes)
        },
    )?;
    if config.prefix_validation {
        if !result.report.reached_end || prefix_records.len() > config.budgets.max_events {
            return Err(Error::EventLimit);
        }
        result.report.cached_events = cached.len();
        result.report.cached_record_bytes = cached.iter().map(String::len).sum();
        result.report.prefix_validated = true;
        result.report.events = prefix_records.len();
        result.snapshot.records = prefix_records;
        result.resume = Some(PrefixResume {
            origin: origin.to_string(),
            world: config.world.clone(),
            cursor: final_cursor.ok_or(Error::PrefixMismatch)?,
            prefix_sha256: prefix::digest(&result.snapshot.records),
        });
        // Recheck identity, order and revision uniqueness across the cache/tail boundary.
        crate::offers::validated(&serde_json::to_string(&result).map_err(|_| Error::Output)?)?;
    }
    Ok(result)
}

mod prefix;

#[cfg(test)]
mod tests;
