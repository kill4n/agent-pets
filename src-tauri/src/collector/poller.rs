use super::event::{parse_snapshot, AgentEvent, ParseError};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

pub const POLL_INTERVAL: Duration = Duration::from_millis(2500);
pub const POLL_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub enum PollError { Spawn(std::io::Error), Timeout, NonZero(i32), Parse(ParseError) }
impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::Spawn(e) => write!(f, "cannot run claude: {e}"),
            PollError::Timeout => write!(f, "claude agents timed out"),
            PollError::NonZero(c) => write!(f, "claude agents exited with {c}"),
            PollError::Parse(e) => write!(f, "cannot parse claude agents output: {e}"),
        }
    }
}

const SHELL_LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);

/// Find the `claude` binary. GUI/LaunchAgent processes on macOS get a minimal PATH, so a bare
/// `claude` usually fails. Order: config override, `$BUDDY_CLAUDE_BIN`, the login shell's
/// `command -v claude`, known install locations, then plain `"claude"`.
pub fn resolve_claude_bin(config_override: Option<&str>) -> String {
    resolve_from(config_override, std::env::var("BUDDY_CLAUDE_BIN").ok(), shell_lookup, &known_locations())
}

fn resolve_from(config_override: Option<&str>, env: Option<String>, shell: impl FnOnce() -> Option<String>,
                known: &[std::path::PathBuf]) -> String {
    let non_empty = |s: &str| { let t = s.trim(); (!t.is_empty()).then(|| t.to_string()) };
    if let Some(p) = config_override.and_then(non_empty) { return p; }
    if let Some(p) = env.as_deref().and_then(non_empty) { return p; }
    if let Some(p) = shell() { return p; }
    known.iter().find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "claude".into())
}

fn known_locations() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut v = vec![];
    if let Some(h) = &home { v.push(h.join(".local/bin/claude")); }
    v.push("/opt/homebrew/bin/claude".into());
    v.push("/usr/local/bin/claude".into());
    if let Some(h) = &home {
        v.push(h.join(".claude/local/claude"));
        v.push(h.join(".npm-global/bin/claude"));
    }
    v
}

/// `$SHELL -lc 'command -v claude'`, killed after 2 s. Only absolute paths to existing files count.
fn shell_lookup() -> Option<String> {
    use std::io::Read;
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty())?;
    let mut child = std::process::Command::new(shell).args(["-lc", "command -v claude"])
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null()).spawn().ok()?;
    let deadline = std::time::Instant::now() + SHELL_LOOKUP_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill(); let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    let line = out.lines().rev().map(str::trim).find(|l| !l.is_empty())?;
    let p = std::path::Path::new(line);
    (p.is_absolute() && p.is_file()).then(|| line.to_string())
}

pub async fn poll_once(host: &str, bin: &str) -> Result<AgentEvent, PollError> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(["agents", "--json", "--all"]).kill_on_drop(true)
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    let child = cmd.spawn().map_err(PollError::Spawn)?;
    let out = tokio::time::timeout(POLL_TIMEOUT, child.wait_with_output()).await
        .map_err(|_| PollError::Timeout)?.map_err(PollError::Spawn)?;
    if !out.status.success() { return Err(PollError::NonZero(out.status.code().unwrap_or(-1))); }
    parse_snapshot(host, &String::from_utf8_lossy(&out.stdout)).map_err(PollError::Parse)
}

pub fn spawn_poller(host: String, claude_path: Option<String>, tx: mpsc::Sender<AgentEvent>,
                    errors: watch::Sender<Option<String>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let bin = tokio::task::spawn_blocking(move || resolve_claude_bin(claude_path.as_deref())).await
            .unwrap_or_else(|_| "claude".into());
        log::info!("poller: using claude binary {bin}");
        let mut tick = tokio::time::interval(POLL_INTERVAL);
        let mut last_err: Option<String> = None;
        loop {
            tick.tick().await;
            match poll_once(&host, &bin).await {
                Ok(ev) => {
                    if last_err.take().is_some() { let _ = errors.send(None); }
                    if tx.send(ev).await.is_err() { break; }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if last_err.as_deref() != Some(&msg) {
                        log::warn!("poller: {msg}");
                        let _ = errors.send(Some(msg.clone()));
                        last_err = Some(msg);
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::event::EventKind;
    use std::os::unix::fs::PermissionsExt;

    fn fake_bin(dir: &std::path::Path, body: &str) -> String {
        let p = dir.join("claude");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test] fn resolve_prefers_config_override() {
        let r = resolve_from(Some("/cfg/claude"), Some("/env/claude".into()), || panic!("no shell lookup"), &[]);
        assert_eq!(r, "/cfg/claude");
    }
    #[test] fn resolve_uses_env_when_no_override() {
        let r = resolve_from(None, Some("/env/claude".into()), || panic!("no shell lookup"), &[]);
        assert_eq!(r, "/env/claude");
        let r = resolve_from(Some("  "), Some("/env/claude".into()), || panic!("no shell lookup"), &[]);
        assert_eq!(r, "/env/claude", "blank override is ignored");
    }

    #[tokio::test] async fn poll_once_parses_output() {
        let d = tempfile::tempdir().unwrap();
        let bin = fake_bin(d.path(), r#"echo '[{"sessionId":"s1","name":"n","status":"busy"}]'"#);
        let ev = poll_once("h", &bin).await.unwrap();
        let EventKind::Snapshot(list) = ev.kind else { panic!() };
        assert_eq!(list[0].session_id, "s1");
    }
    #[tokio::test] async fn poll_once_missing_binary_is_spawn_error() {
        assert!(matches!(poll_once("h", "/definitely/not/here").await, Err(PollError::Spawn(_))));
    }
    #[tokio::test] async fn poll_once_nonzero_exit() {
        let d = tempfile::tempdir().unwrap();
        let bin = fake_bin(d.path(), "exit 3");
        assert!(matches!(poll_once("h", &bin).await, Err(PollError::NonZero(3))));
    }
    #[tokio::test] async fn poll_once_bad_json_is_parse_error() {
        let d = tempfile::tempdir().unwrap();
        let bin = fake_bin(d.path(), "echo nope");
        assert!(matches!(poll_once("h", &bin).await, Err(PollError::Parse(_))));
    }
    #[tokio::test] async fn poll_once_times_out() {
        let d = tempfile::tempdir().unwrap();
        let bin = fake_bin(d.path(), "sleep 5");
        assert!(matches!(poll_once("h", &bin).await, Err(PollError::Timeout)));
    }
}
