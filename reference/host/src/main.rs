use std::env;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use agentciv_host::mls_relay::{DEFAULT_RETENTION_SECONDS, MlsRelayStore, PROFILE_ID, RelayConfig};
use agentciv_host::mls_relay_http::{self, MlsRelayState};
use agentciv_host::{load_config, serve};
use serde_json::json;
use tokio::net::TcpListener;

const USAGE: &str = "usage: agentciv-host --config CONFIG\n\
A host configuration file (--config) is required and is produced by the local room wrapper.\n\
For local room setup, see docs/LOCAL_ENCOUNTER.md in a repository checkout.";

const MLS_RELAY_USAGE: &str = "usage: agentciv-host mls-relay --db PATH [OPTIONS]\n\
Run the opt-in loopback PQ-MLS Delivery Service relay (mls-relay/0.1-draft).\n\
Options:\n\
  --db PATH                    Required SQLite database path\n\
  --host ADDR                  Loopback bind IP (127.0.0.1 or ::1; default: 127.0.0.1)\n\
  --port PORT                  TCP port (0..=65535, 0 for ephemeral; default: 8788)\n\
  --retention-seconds SECONDS  Hard TTL in seconds (60..=86400; default: 86400)\n\
  --writes-enabled             Enable mutating relay routes (default: disabled / 503)\n\
  --help, -h                   Show this help message";

const MIN_RETENTION_SECONDS: i64 = 60;
const DEFAULT_MLS_RELAY_PORT: u16 = 8788;

#[derive(Debug, Eq, PartialEq)]
struct MlsRelayCliArgs {
    db: PathBuf,
    host: IpAddr,
    port: u16,
    retention_seconds: i64,
    writes_enabled: bool,
}

#[derive(Debug, Eq, PartialEq)]
enum MlsRelayParseOutcome {
    Help,
    Run(MlsRelayCliArgs),
}

fn parse_mls_relay_args<I>(mut args: I) -> Result<MlsRelayParseOutcome, String>
where
    I: Iterator<Item = String>,
{
    let mut db: Option<PathBuf> = None;
    let mut host: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut port: u16 = DEFAULT_MLS_RELAY_PORT;
    let mut retention_seconds: i64 = DEFAULT_RETENTION_SECONDS;
    let mut writes_enabled = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(MlsRelayParseOutcome::Help),
            "--db" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --db".to_owned())?;
                if value.is_empty() {
                    return Err("--db path must not be empty".to_owned());
                }
                if db.is_some() {
                    return Err("duplicate --db flag".to_owned());
                }
                db = Some(PathBuf::from(value));
            }
            "--host" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --host".to_owned())?;
                let parsed = value
                    .parse::<IpAddr>()
                    .map_err(|_| format!("invalid --host IP address: {value}"))?;
                if parsed != IpAddr::V4(Ipv4Addr::LOCALHOST)
                    && parsed != IpAddr::V6(Ipv6Addr::LOCALHOST)
                {
                    return Err(format!(
                        "--host must be a loopback IP (127.0.0.1 or ::1), got: {value}"
                    ));
                }
                host = parsed;
            }
            "--port" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --port".to_owned())?;
                port = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --port value (expected 0..=65535): {value}"))?;
            }
            "--retention-seconds" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --retention-seconds".to_owned())?;
                let parsed = value.parse::<i64>().map_err(|_| {
                    format!("invalid --retention-seconds value (expected integer): {value}")
                })?;
                if !(MIN_RETENTION_SECONDS..=DEFAULT_RETENTION_SECONDS).contains(&parsed) {
                    return Err(format!(
                        "--retention-seconds must be within {MIN_RETENTION_SECONDS}..={DEFAULT_RETENTION_SECONDS}, got: {parsed}"
                    ));
                }
                retention_seconds = parsed;
            }
            "--writes-enabled" => {
                writes_enabled = true;
            }
            other => {
                return Err(format!("unknown argument for mls-relay: {other}"));
            }
        }
    }

    let Some(db) = db else {
        return Err("missing required --db PATH".to_owned());
    };

    Ok(MlsRelayParseOutcome::Run(MlsRelayCliArgs {
        db,
        host,
        port,
        retention_seconds,
        writes_enabled,
    }))
}

fn origin_for_socket(addr: SocketAddr) -> String {
    match addr {
        SocketAddr::V4(v4) => format!("http://{v4}"),
        SocketAddr::V6(v6) => format!("http://[{}]:{}", v6.ip(), v6.port()),
    }
}

fn run_mls_relay<I>(args: I) -> ExitCode
where
    I: Iterator<Item = String>,
{
    let parsed = match parse_mls_relay_args(args) {
        Ok(MlsRelayParseOutcome::Help) => {
            println!("{MLS_RELAY_USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(MlsRelayParseOutcome::Run(parsed)) => parsed,
        Err(error) => {
            eprintln!("error: {error}\n{MLS_RELAY_USAGE}");
            return ExitCode::FAILURE;
        }
    };

    let config = RelayConfig {
        writes_enabled: parsed.writes_enabled,
        retention_seconds: parsed.retention_seconds,
    };
    let store = match MlsRelayStore::open(&parsed.db, config) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("failed to open mls-relay store: {error}");
            return ExitCode::FAILURE;
        }
    };
    let state = MlsRelayState::new(store);

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("runtime failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(serve_mls_relay(
        SocketAddr::new(parsed.host, parsed.port),
        state,
        config,
    )) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve_mls_relay(
    listen: SocketAddr,
    state: MlsRelayState,
    config: RelayConfig,
) -> Result<(), String> {
    let listener = TcpListener::bind(listen)
        .await
        .map_err(|error| format!("could not bind {listen}: {error}"))?;
    let bound = listener
        .local_addr()
        .map_err(|error| format!("could not inspect bound address: {error}"))?;
    let origin = origin_for_socket(bound);
    let readiness = json!({
        "outcome": "serving",
        "profile": PROFILE_ID,
        "origin": origin,
        "writes_enabled": config.writes_enabled,
        "retention_seconds": config.retention_seconds
    });
    println!("{readiness}");
    let _ = std::io::stdout().flush();

    axum::serve(listener, mls_relay_http::router(state))
        .with_graceful_shutdown(std::future::pending::<()>())
        .await
        .map_err(|error| format!("mls-relay server error: {error}"))
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("mls-relay") {
        return run_mls_relay(args);
    }

    let (flag, path) = (first, args.next());
    if path.is_none() {
        match flag.as_deref() {
            Some("--help" | "-h") => {
                println!(
                    "{USAGE}\nOptions: --help, --version\nSubcommands: mls-relay (use `agentciv-host mls-relay --help`)"
                );
                return ExitCode::SUCCESS;
            }
            Some("--version" | "-V") => {
                println!("agentciv-host {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            _ => {}
        }
    }
    let (Some(flag), Some(path)) = (flag, path) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    if flag != "--config" || args.next().is_some() {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    let config = match load_config(Path::new(&path)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("runtime failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(serve(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_strings(items: &[&str]) -> std::vec::IntoIter<String> {
        items
            .iter()
            .map(|item| (*item).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn parse_mls_relay_args_defaults_and_opt_in_flags() {
        let parsed = parse_mls_relay_args(to_strings(&["--db", "/tmp/relay.sqlite"]))
            .expect("valid minimal args");
        assert_eq!(
            parsed,
            MlsRelayParseOutcome::Run(MlsRelayCliArgs {
                db: PathBuf::from("/tmp/relay.sqlite"),
                host: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: DEFAULT_MLS_RELAY_PORT,
                retention_seconds: DEFAULT_RETENTION_SECONDS,
                writes_enabled: false,
            })
        );

        let custom = parse_mls_relay_args(to_strings(&[
            "--db",
            "/tmp/relay.sqlite",
            "--host",
            "::1",
            "--port",
            "0",
            "--retention-seconds",
            "3600",
            "--writes-enabled",
        ]))
        .expect("valid custom args");
        assert_eq!(
            custom,
            MlsRelayParseOutcome::Run(MlsRelayCliArgs {
                db: PathBuf::from("/tmp/relay.sqlite"),
                host: IpAddr::V6(Ipv6Addr::LOCALHOST),
                port: 0,
                retention_seconds: 3600,
                writes_enabled: true,
            })
        );

        assert_eq!(
            parse_mls_relay_args(to_strings(&["--help"])),
            Ok(MlsRelayParseOutcome::Help)
        );
        assert_eq!(
            parse_mls_relay_args(to_strings(&["-h"])),
            Ok(MlsRelayParseOutcome::Help)
        );
    }

    #[test]
    fn parse_mls_relay_args_rejects_invalid_and_non_loopback_inputs() {
        for bad in [
            &[][..],
            &["--db"][..],
            &["--db", ""][..],
            &["--db", "a.db", "--db", "b.db"][..],
            &["--db", "a.db", "--host"][..],
            &["--db", "a.db", "--host", "0.0.0.0"][..],
            &["--db", "a.db", "--host", "::"][..],
            &["--db", "a.db", "--host", "127.0.0.2"][..],
            &["--db", "a.db", "--host", "example.com"][..],
            &["--db", "a.db", "--port"][..],
            &["--db", "a.db", "--port", "70000"][..],
            &["--db", "a.db", "--retention-seconds"][..],
            &["--db", "a.db", "--retention-seconds", "59"][..],
            &["--db", "a.db", "--retention-seconds", "86401"][..],
            &["--db", "a.db", "--retention-seconds", "abc"][..],
            &["--db", "a.db", "--unknown"][..],
        ] {
            assert!(
                parse_mls_relay_args(to_strings(bad)).is_err(),
                "expected error for args {bad:?}"
            );
        }
    }

    #[test]
    fn origin_for_socket_formats_ipv4_and_ipv6() {
        let v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8788);
        let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 9000);
        assert_eq!(origin_for_socket(v4), "http://127.0.0.1:8788");
        assert_eq!(origin_for_socket(v6), "http://[::1]:9000");
    }
}
