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

pub fn claude_bin() -> String {
    std::env::var("BUDDY_CLAUDE_BIN").unwrap_or_else(|_| "claude".into())
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

pub fn spawn_poller(host: String, tx: mpsc::Sender<AgentEvent>, errors: watch::Sender<Option<String>>)
    -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let bin = claude_bin();
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
