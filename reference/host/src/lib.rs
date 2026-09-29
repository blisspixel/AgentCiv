//! Loopback HTTP Commons host. It records messages for one world and does not claim
//! the profile is fully proven until the public conformance cases pass.

mod http;
mod store;
mod validate;

#[cfg(test)]
mod tests;

use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::http::{App, router};
use crate::store::{Store, StoreError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    SenderOnly,
    Addressed,
    Members,
}

impl Visibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SenderOnly => "sender_only",
            Self::Addressed => "addressed",
            Self::Members => "members",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "sender_only" => Some(Self::SenderOnly),
            "addressed" => Some(Self::Addressed),
            "members" => Some(Self::Members),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Credential {
    pub principal: String,
    pub token: String,
    pub read: bool,
    pub write: bool,
}

#[derive(Clone)]
pub struct HostConfig {
    pub world_id: String,
    pub title: String,
    pub database_path: PathBuf,
    pub listen: SocketAddr,
    pub visibility: Visibility,
    pub retention_seconds: i64,
    pub max_payload_bytes: usize,
    pub credentials: Vec<Credential>,
}

#[derive(Debug)]
pub enum HostError {
    Config(String),
    Storage,
    Bind(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(detail) => write!(formatter, "invalid configuration: {detail}"),
            Self::Storage => write!(formatter, "storage could not be prepared"),
            Self::Bind(detail) => write!(formatter, "could not listen: {detail}"),
        }
    }
}

#[derive(Deserialize)]
struct ConfigFile {
    world_id: String,
    title: String,
    database_path: PathBuf,
    listen: String,
    visibility: String,
    retention_seconds: i64,
    max_payload_bytes: u64,
    credentials: Vec<CredentialFile>,
}

#[derive(Deserialize)]
struct CredentialFile {
    principal: String,
    token: String,
    read: bool,
    write: bool,
}

pub fn load_config(path: &Path) -> Result<HostConfig, HostError> {
    let text = fs::read_to_string(path)
        .map_err(|_| HostError::Config("configuration file could not be read".to_owned()))?;
    let file: ConfigFile = serde_json::from_str(&text)
        .map_err(|_| HostError::Config("configuration file is not valid JSON".to_owned()))?;
    let listen = file
        .listen
        .parse::<SocketAddr>()
        .map_err(|_| HostError::Config("listen address is invalid".to_owned()))?;
    let visibility = Visibility::parse(&file.visibility)
        .ok_or_else(|| HostError::Config("visibility is invalid".to_owned()))?;
    let credentials = file
        .credentials
        .into_iter()
        .map(|credential| Credential {
            principal: credential.principal,
            token: credential.token,
            read: credential.read,
            write: credential.write,
        })
        .collect();
    checked_config(HostConfig {
        world_id: file.world_id,
        title: file.title,
        database_path: file.database_path,
        listen,
        visibility,
        retention_seconds: file.retention_seconds,
        max_payload_bytes: usize::try_from(file.max_payload_bytes)
            .map_err(|_| HostError::Config("payload limit is too large".to_owned()))?,
        credentials,
    })
}

pub fn checked_config(config: HostConfig) -> Result<HostConfig, HostError> {
    if config.world_id.is_empty() || config.title.is_empty() {
        return Err(HostError::Config(
            "world id and title are required".to_owned(),
        ));
    }
    if !is_loopback(config.listen.ip()) {
        return Err(HostError::Config(
            "listen address must be loopback".to_owned(),
        ));
    }
    if config.retention_seconds < 1 || config.max_payload_bytes < 1024 {
        return Err(HostError::Config(
            "retention and payload limit are below the profile minimum".to_owned(),
        ));
    }
    if config.credentials.is_empty() {
        return Err(HostError::Config(
            "at least one credential is required".to_owned(),
        ));
    }
    let mut principals = Vec::new();
    let mut tokens = Vec::new();
    for credential in &config.credentials {
        if credential.principal.is_empty()
            || credential.token.is_empty()
            || (!credential.read && !credential.write)
        {
            return Err(HostError::Config(
                "each credential needs a principal, token, and a read or write grant".to_owned(),
            ));
        }
        if principals.contains(&credential.principal) || tokens.contains(&credential.token) {
            return Err(HostError::Config(
                "credentials must use distinct principals and tokens".to_owned(),
            ));
        }
        principals.push(credential.principal.clone());
        tokens.push(credential.token.clone());
    }
    Ok(config)
}

pub async fn serve(config: HostConfig) -> Result<(), HostError> {
    let (listener, app) = bind(config).await?;
    println!("discovery {}/.well-known/agentciv", app.origin);
    let _ = std::io::Write::flush(&mut std::io::stdout());
    axum::serve(listener, router(app))
        .await
        .map_err(|error| HostError::Bind(error.to_string()))
}

pub struct TestHost {
    pub discovery: String,
    shutdown: Option<oneshot::Sender<()>>,
}

impl Drop for TestHost {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

pub async fn start_test_host(config: HostConfig) -> Result<TestHost, HostError> {
    let (listener, app) = bind(config).await?;
    let discovery = format!("{}/.well-known/agentciv", app.origin);
    let (shutdown, signal) = oneshot::channel();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router(app))
            .with_graceful_shutdown(async move {
                let _ = signal.await;
            })
            .await;
    });
    Ok(TestHost {
        discovery,
        shutdown: Some(shutdown),
    })
}

async fn bind(config: HostConfig) -> Result<(TcpListener, App), HostError> {
    let config = checked_config(config)?;
    let grants = config
        .credentials
        .iter()
        .map(|credential| {
            (
                credential.principal.clone(),
                credential.read,
                credential.write,
            )
        })
        .collect::<Vec<_>>();
    let store = Store::open(
        &config.database_path,
        &config.world_id,
        config.visibility,
        config.retention_seconds,
        &grants,
    )
    .map_err(|error| match error {
        StoreError::WorldMismatch => {
            HostError::Config("database belongs to a different world".to_owned())
        }
        StoreError::Storage => HostError::Storage,
    })?;
    let listener = TcpListener::bind(config.listen)
        .await
        .map_err(|error| HostError::Bind(error.to_string()))?;
    let address = listener
        .local_addr()
        .map_err(|error| HostError::Bind(error.to_string()))?;
    if !is_loopback(address.ip()) {
        return Err(HostError::Config(
            "bound address is not loopback".to_owned(),
        ));
    }
    let origin = origin_for(address);
    let app = App {
        world_id: config.world_id,
        title: config.title,
        visibility: config.visibility,
        retention_seconds: config.retention_seconds,
        max_payload: config.max_payload_bytes,
        origin,
        credentials: Arc::from(config.credentials),
        store,
    };
    Ok((listener, app))
}

fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
}

fn origin_for(address: SocketAddr) -> String {
    match address {
        SocketAddr::V4(address) => format!("http://{address}"),
        SocketAddr::V6(address) => format!("http://[{}]:{}", address.ip(), address.port()),
    }
}
