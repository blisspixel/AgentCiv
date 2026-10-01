//! Replaceable loopback-only, read-only adapter. No proxy, redirect, or implicit restart.

use crate::{Budgets, Error, ReadResult, Result, Traversal, collect_until, reflected, validators};
use agentciv_archive::parse_unique;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE};
use reqwest::{Url, redirect};
use serde::Deserialize;
use std::io::Read;
use std::net::IpAddr;
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
    if !ip.is_loopback() || port.is_some_and(|port| port.parse::<u16>().is_err() || port == "0") {
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
    match response.status().as_u16() {
        200 => (),
        401 => return Err(Error::Authentication),
        403 => return Err(Error::Forbidden),
        410 => return Err(Error::CursorExpired),
        _ => return Err(Error::Http),
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next());
    if !content_type.is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json")) {
        return Err(Error::ContentType);
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
    Ok(body)
}

/// Read exactly the configured caller's view. No credential is sent during discovery.
/// Output remains read data; its report explicitly grants no copying permission.
pub fn read(config: &Config) -> Result<ReadResult> {
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
    if events.query_pairs().any(|(key, _)| key == "after") {
        return Err(Error::InvalidEndpoint);
    }
    collect_until(
        &config.world,
        &config.budgets,
        config.traversal,
        Some(&config.token),
        deadline,
        discovery_bytes.len(),
        |after, _remaining, allowance| {
            let mut url = events.clone();
            if let Some(cursor) = after {
                url.query_pairs_mut().append_pair("after", cursor);
            }
            get(&client, url, Some(&config.token), deadline, allowance, true).map_err(|error| {
                if error == Error::ResponseLimit && allowance < config.budgets.max_response_bytes {
                    Error::TotalLimit
                } else {
                    error
                }
            })
        },
    )
}

#[cfg(test)]
mod tests;
