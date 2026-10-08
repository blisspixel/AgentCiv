//! Explicit, bounded local gathering adapter. No HTTP runtime endpoint is added.
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use agentciv_host::runtime::{Decision, DispatchRequest, ExecutorFailure, Gate, Scope};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MAX_FILE: u64 = 65_536;
const FORMAT: &str = "agentciv-local-runtime/0.1-example";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    format: String,
    runtime_id: String,
    world_id: String,
    database_path: PathBuf,
    administrator_token: String,
    coordinator_owner: String,
    python_executable: PathBuf,
    max_child_seconds: u64,
    scopes: Vec<ConfiguredScope>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfiguredScope {
    participant: String,
    activity: String,
    control_token: String,
    child_config_path: PathBuf,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Initialize {
        token: String,
    },
    Claim {
        token: String,
    },
    Register {
        token: String,
        participant: String,
        activity: String,
    },
    Inspect {
        token: String,
        participant: String,
        activity: String,
    },
    Stop {
        token: String,
        participant: String,
        activity: String,
        command_id: String,
        expected_revision: u64,
        generation: u64,
    },
    Resume {
        token: String,
        participant: String,
        activity: String,
        command_id: String,
        expected_revision: u64,
        generation: u64,
    },
    Dispatch {
        token: String,
        participant: String,
        activity: String,
        generation: u64,
        invitation_id: String,
        history_available: bool,
    },
}

fn private_path(path: &Path, exists: bool) -> Result<PathBuf, &'static str> {
    if !path.is_absolute() {
        return Err("invalid_configuration");
    }
    let resolved = if exists {
        fs::canonicalize(path).map_err(|_| "invalid_configuration")?
    } else {
        let parent = path.parent().ok_or("invalid_configuration")?;
        let name = path.file_name().ok_or("invalid_configuration")?;
        fs::canonicalize(parent)
            .map_err(|_| "invalid_configuration")?
            .join(name)
    };
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let checkout = fs::canonicalize(checkout).map_err(|_| "invalid_configuration")?;
    if resolved.starts_with(checkout) {
        return Err("invalid_configuration");
    }
    Ok(resolved)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| "invalid_configuration")?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "invalid_configuration")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("invalid_configuration");
    }
    Ok(bytes)
}

fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn capability(value: &str) -> bool {
    (32..=256).contains(&value.len()) && !value.chars().any(char::is_control)
}

fn authorized(actual: &str, expected: &str) -> bool {
    let left = Sha256::digest(actual.as_bytes());
    let right = Sha256::digest(expected.as_bytes());
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |sum, (a, b)| sum | (a ^ b))
        == 0
}

fn checked_config(path: &Path) -> Result<Config, &'static str> {
    let path = private_path(path, true)?;
    let mut config: Config =
        serde_json::from_slice(&read_bounded(&path)?).map_err(|_| "invalid_configuration")?;
    if config.format != FORMAT
        || !identifier(&config.runtime_id)
        || !identifier(&config.world_id)
        || !identifier(&config.coordinator_owner)
        || !capability(&config.administrator_token)
        || !(1..=60).contains(&config.max_child_seconds)
        || config.scopes.is_empty()
        || config.scopes.len() > 16
    {
        return Err("invalid_configuration");
    }
    config.database_path = private_path(&config.database_path, config.database_path.exists())?;
    if !config.python_executable.is_absolute() || !config.python_executable.is_file() {
        return Err("invalid_configuration");
    }
    config.python_executable =
        fs::canonicalize(&config.python_executable).map_err(|_| "invalid_configuration")?;
    let mut keys = Vec::new();
    let mut tokens = vec![config.administrator_token.clone()];
    for scope in &mut config.scopes {
        if !identifier(&scope.participant)
            || !identifier(&scope.activity)
            || !capability(&scope.control_token)
            || keys.contains(&(scope.participant.clone(), scope.activity.clone()))
            || tokens.contains(&scope.control_token)
        {
            return Err("invalid_configuration");
        }
        keys.push((scope.participant.clone(), scope.activity.clone()));
        tokens.push(scope.control_token.clone());
        // A configured future child path may be prepared after initialization.
        scope.child_config_path =
            private_path(&scope.child_config_path, scope.child_config_path.exists())?;
    }
    let labels = [
        &config.runtime_id,
        &config.world_id,
        &config.coordinator_owner,
    ]
    .into_iter()
    .chain(
        config
            .scopes
            .iter()
            .flat_map(|scope| [&scope.participant, &scope.activity]),
    );
    if labels
        .into_iter()
        .any(|label| tokens.iter().any(|token| label.contains(token)))
    {
        return Err("invalid_configuration");
    }
    Ok(config)
}

fn scope_config<'a>(
    config: &'a Config,
    participant: &str,
    activity: &str,
) -> Result<&'a ConfiguredScope, &'static str> {
    config
        .scopes
        .iter()
        .find(|scope| scope.participant == participant && scope.activity == activity)
        .ok_or("unauthorized")
}

fn require_admin(config: &Config, token: &str) -> Result<(), &'static str> {
    if authorized(token, &config.administrator_token) {
        Ok(())
    } else {
        Err("unauthorized")
    }
}

fn core<T>(result: Result<T, agentciv_host::runtime::Error>) -> Result<T, &'static str> {
    result.map_err(|error| error.code())
}

fn public_label(config: &Config, label: &str) -> Result<(), &'static str> {
    if !identifier(label)
        || label.contains(&config.administrator_token)
        || config
            .scopes
            .iter()
            .any(|scope| label.contains(&scope.control_token))
    {
        Err("invalid_request")
    } else {
        Ok(())
    }
}

fn dispatch(
    config: &Config,
    gate: &Gate,
    scope_config: &ConfiguredScope,
    generation: u64,
    invitation_id: String,
    history_available: bool,
) -> Result<Value, &'static str> {
    let bytes = read_bounded(&scope_config.child_config_path)?;
    let child_config: Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid_configuration")?;
    if child_config.get("principal").and_then(Value::as_str) != Some(&scope_config.participant)
        || child_config.get("runtime_world").and_then(Value::as_str) != Some(&config.world_id)
        || child_config.get("mode").and_then(Value::as_str) != Some("scripted")
        || ["attempt_journal", "private_candidates", "private_trace"]
            .iter()
            .any(|key| child_config.get(key).is_some())
    {
        return Err("invalid_configuration");
    }
    public_label(config, &invitation_id)?;
    if child_config
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .is_some_and(|token| invitation_id.contains(token))
    {
        return Err("invalid_request");
    }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/participants/gathering.py");
    let scope = Scope {
        participant: scope_config.participant.clone(),
        activity: scope_config.activity.clone(),
    };
    let request = DispatchRequest {
        generation,
        invitation_id,
        payload_identity: digest,
        history_available,
        launch_budget: 1,
    };
    let mut frozen = tempfile::NamedTempFile::new().map_err(|_| "invalid_configuration")?;
    frozen
        .write_all(&bytes)
        .map_err(|_| "invalid_configuration")?;
    frozen.flush().map_err(|_| "invalid_configuration")?;
    let outcome = core(gate.dispatch(&scope, &request, || {
        let child = Command::new(&config.python_executable)
            .arg(&script)
            .arg("--config")
            .arg(frozen.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ExecutorFailure::NotStarted)?;
        Ok((child, frozen))
    }))?;
    let child = outcome
        .executor_value
        .map(|(child, _frozen)| wait_child(child, config.max_child_seconds));
    Ok(
        json!({"outcome":"ok", "dispatch": outcome.receipt, "child": child,
        "history_authority":"operator_attested_current_permitted_history", "decision_source":"scripted"}),
    )
}

fn wait_child(mut child: Child, seconds: u64) -> Value {
    let stdout = child.stdout.take();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .ok_or(())?
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        if bytes.len() as u64 > MAX_FILE {
            return Err(());
        }
        Ok(bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = reader.join().ok().and_then(Result::ok);
    let Some(status) = status else {
        return json!({"status":"timeout"});
    };
    let parsed: Option<Value> = bytes.and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let Some(parsed) = parsed else {
        return json!({"status":"invalid_result"});
    };
    let outcome = parsed.get("outcome").and_then(Value::as_str);
    if !matches!(outcome, Some("quiet" | "left" | "recorded" | "failed")) {
        return json!({"status":"invalid_result"});
    }
    let action = parsed
        .get("decision")
        .and_then(|value| value.get("action"))
        .and_then(Value::as_str)
        .filter(|action| matches!(*action, "message" | "quiet" | "leave"));
    let coherent = match outcome {
        Some("quiet") => status.success() && action == Some("quiet"),
        Some("left") => status.success() && action == Some("leave"),
        Some("recorded") => status.success() && action == Some("message"),
        Some("failed") => !status.success(),
        _ => false,
    };
    if !coherent {
        return json!({"status":"invalid_result"});
    }
    json!({"status":if status.success() {"completed"} else {"failed"}, "outcome":outcome, "action":action})
}

fn execute(config: &Config, request: Request) -> Result<Value, &'static str> {
    let decision = if matches!(&request, Request::Stop { .. }) {
        Decision::Stop
    } else {
        Decision::Resume
    };
    if let Request::Initialize { token } = request {
        require_admin(config, &token)?;
        core(Gate::initialize(
            &config.database_path,
            &config.runtime_id,
            &config.world_id,
        ))?;
        return Ok(json!({"outcome":"ok", "operation":"initialize"}));
    }
    let gate = core(Gate::open(
        &config.database_path,
        &config.runtime_id,
        &config.world_id,
    ))?;
    match request {
        Request::Initialize { .. } => unreachable!(),
        Request::Claim { token } => {
            require_admin(config, &token)?;
            let generation = core(gate.claim_coordinator(&config.coordinator_owner))?;
            Ok(json!({"outcome":"ok", "generation":generation}))
        }
        Request::Register {
            token,
            participant,
            activity,
        } => {
            require_admin(config, &token)?;
            scope_config(config, &participant, &activity)?;
            let state = core(gate.register_scope(&Scope {
                participant,
                activity,
            }))?;
            Ok(json!({"outcome":"ok", "scope":state}))
        }
        Request::Inspect {
            token,
            participant,
            activity,
        } => {
            let selected = scope_config(config, &participant, &activity)?;
            if !authorized(&token, &selected.control_token) {
                require_admin(config, &token)?;
            }
            let state = core(gate.inspect(&Scope {
                participant,
                activity,
            }))?;
            Ok(json!({"outcome":"ok", "scope":state}))
        }
        Request::Stop {
            token,
            participant,
            activity,
            command_id,
            expected_revision,
            generation,
        }
        | Request::Resume {
            token,
            participant,
            activity,
            command_id,
            expected_revision,
            generation,
        } => {
            let selected = scope_config(config, &participant, &activity)?;
            if !authorized(&token, &selected.control_token) {
                return Err("unauthorized");
            }
            public_label(config, &command_id)?;
            let receipt = core(gate.control(
                &Scope {
                    participant,
                    activity,
                },
                &command_id,
                expected_revision,
                decision,
                generation,
            ))?;
            Ok(json!({"outcome":"ok", "control":receipt}))
        }
        Request::Dispatch {
            token,
            participant,
            activity,
            generation,
            invitation_id,
            history_available,
        } => {
            require_admin(config, &token)?;
            let selected = scope_config(config, &participant, &activity)?;
            dispatch(
                config,
                &gate,
                selected,
                generation,
                invitation_id,
                history_available,
            )
        }
    }
}

fn run() -> Result<Value, &'static str> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 4 || args[0] != "--config" || args[2] != "--request" {
        return Err("invalid_arguments");
    }
    let config = checked_config(Path::new(&args[1]))?;
    let path = private_path(Path::new(&args[3]), true)?;
    let request: Request =
        serde_json::from_slice(&read_bounded(&path)?).map_err(|_| "invalid_request")?;
    execute(&config, request)
}

fn main() -> ExitCode {
    match run() {
        Ok(result) => {
            println!("{result}");
            ExitCode::SUCCESS
        }
        Err(code) => {
            println!("{}", json!({"outcome":"failed", "code":code}));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python(code: &str) -> Child {
        Command::new("python")
            .args(["-c", code])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn child_failure_and_output_limits_never_return_private_bytes() {
        for code in [
            "print('private child trace')",
            "print('{\"outcome\":\"invented-private-value\"}')",
            "print('{\"outcome\":\"quiet\"}')",
            "print('{\"outcome\":\"failed\"}')",
            "print('x' * 65537)",
        ] {
            assert_eq!(
                wait_child(python(code), 5),
                json!({"status":"invalid_result"})
            );
        }
        assert_eq!(
            wait_child(
                python("print('{\"outcome\":\"failed\"}'); raise SystemExit(1)"),
                5
            ),
            json!({"status":"failed","outcome":"failed","action":null})
        );
        let started = Instant::now();
        assert_eq!(
            wait_child(python("import time; time.sleep(10)"), 1),
            json!({"status":"timeout"})
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn private_paths_and_capabilities_reject_ambiguous_inputs() {
        assert!(private_path(Path::new("relative.json"), false).is_err());
        assert!(private_path(Path::new(env!("CARGO_MANIFEST_DIR")), true).is_err());
        assert!(!identifier(""));
        assert!(!identifier("line\nbreak"));
        assert!(!capability("short"));
        assert!(!capability(&"a".repeat(257)));
        assert!(!authorized(
            "wrong-private-capability",
            "correct-private-capability"
        ));
        assert!(authorized(
            "same-private-capability",
            "same-private-capability"
        ));
        assert!(
            serde_json::from_str::<Request>(
                "{\"op\":\"claim\",\"token\":\"one\",\"token\":\"two\"}"
            )
            .is_err()
        );
        assert!(serde_json::from_str::<Config>("{}").is_err());
    }
}
