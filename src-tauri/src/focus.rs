//! Bring the terminal that hosts a Claude Code session to the front.
//!
//! The session registry gives us the `claude` process pid. We walk up the process tree until we
//! reach the GUI application that owns it (iTerm, Terminal, VS Code, Warp, Cursor, Ghostty, ...)
//! and activate that application. No Apple Events / Automation permission is needed on macOS
//! because we use `open` on the `.app` bundle path.

use std::path::PathBuf;
use std::process::Command;

/// One `ps -o ppid=,comm=` line → (parent pid, command path). `comm` may contain spaces.
pub fn parse_ps_line(line: &str) -> Option<(u32, String)> {
    let t = line.trim_start();
    let (ppid, comm) = t.split_once(' ')?;
    Some((ppid.trim().parse().ok()?, comm.trim().to_string()))
}

/// `/Applications/iTerm.app/Contents/MacOS/iTerm2` → `/Applications/iTerm.app`.
pub fn bundle_from_comm(comm: &str) -> Option<PathBuf> {
    let idx = comm.find(".app/")?;
    Some(PathBuf::from(&comm[..idx + 4]))
}

/// Walk ancestors of `pid` using `lookup(pid) -> (ppid, comm)` and return the first `.app` bundle.
pub fn find_bundle<F: FnMut(u32) -> Option<(u32, String)>>(mut pid: u32, mut lookup: F) -> Option<PathBuf> {
    for _ in 0..32 {
        let (ppid, comm) = lookup(pid)?;
        if let Some(b) = bundle_from_comm(&comm) { return Some(b); }
        if ppid <= 1 || ppid == pid { return None; }
        pid = ppid;
    }
    None
}

/// Claude Code's session registry directory (`$CLAUDE_CONFIG_DIR` or `~/.claude`, then `sessions/`).
pub fn sessions_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().join(".claude")))
        .unwrap_or_else(|| PathBuf::from(".claude"))
        .join("sessions")
}

/// A daemon-hosted (background) session is shown by a separate interactive `claude` client that the
/// registry marks with `parkedJobId == <job id>`. That client is the process living inside the terminal.
pub fn parked_client_pid(dir: &std::path::Path, job_id: &str) -> Option<u32> {
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        if v.get("parkedJobId").and_then(|j| j.as_str()) == Some(job_id) {
            if let Some(pid) = v.get("pid").and_then(|x| x.as_u64()) { return Some(pid as u32); }
        }
    }
    None
}

/// Resolve the terminal app for a session: walk from its own pid, else from its parked client's pid.
pub fn terminal_bundle(pid: Option<u32>, job_id: Option<&str>, sessions: &std::path::Path) -> Option<PathBuf> {
    if let Some(b) = pid.and_then(|p| find_bundle(p, ps_lookup)) { return Some(b); }
    let client = job_id.and_then(|j| parked_client_pid(sessions, j))?;
    find_bundle(client, ps_lookup)
}

fn ps_lookup(pid: u32) -> Option<(u32, String)> {
    let out = Command::new("ps").args(["-o", "ppid=,comm=", "-p", &pid.to_string()]).output().ok()?;
    if !out.status.success() { return None; }
    parse_ps_line(&String::from_utf8_lossy(&out.stdout))
}

/// Activate the terminal application showing this session.
pub fn focus_session(pid: Option<u32>, job_id: Option<&str>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let bundle = terminal_bundle(pid, job_id, &sessions_dir()).ok_or("no se encontró la app del terminal")?;
        let st = Command::new("open").arg(&bundle).status().map_err(|e| e.to_string())?;
        return if st.success() { Ok(()) } else { Err(format!("open {} falló", bundle.display())) };
    }
    #[cfg(target_os = "linux")]
    {
        // best effort: find the first ancestor that owns an X window and activate it
        let start = pid.or_else(|| job_id.and_then(|j| parked_client_pid(&sessions_dir(), j)))
            .ok_or("esta sesión no tiene un terminal asociado")?;
        let mut p = start;
        for _ in 0..32 {
            let win = Command::new("xdotool").args(["search", "--pid", &p.to_string(), "--limit", "1"]).output()
                .map_err(|e| format!("xdotool no disponible: {e}"))?;
            let id = String::from_utf8_lossy(&win.stdout).trim().to_string();
            if !id.is_empty() {
                let st = Command::new("xdotool").args(["windowactivate", &id]).status().map_err(|e| e.to_string())?;
                return if st.success() { Ok(()) } else { Err("xdotool windowactivate falló".into()) };
            }
            let Some((pp, _)) = ps_lookup(p) else { break };
            if pp <= 1 { break; }
            p = pp;
        }
        return Err("no se encontró la ventana del terminal".into());
    }
    #[allow(unreachable_code)]
    { let _ = (pid, job_id); Err("enfocar el terminal no está soportado en este sistema".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn parses_ps_lines_with_spaces_in_comm() {
        assert_eq!(parse_ps_line("55880 /Applications/iTerm.app/Contents/MacOS/iTerm2\n"),
                   Some((55880, "/Applications/iTerm.app/Contents/MacOS/iTerm2".into())));
        assert_eq!(parse_ps_line("  55883 /Users/me/Library/Application Support/iTerm2/iTermServer-3.7.3"),
                   Some((55883, "/Users/me/Library/Application Support/iTerm2/iTermServer-3.7.3".into())));
        assert_eq!(parse_ps_line("garbage"), None);
    }

    #[test] fn bundle_is_cut_at_dot_app() {
        assert_eq!(bundle_from_comm("/Applications/iTerm.app/Contents/MacOS/iTerm2"), Some(PathBuf::from("/Applications/iTerm.app")));
        assert_eq!(bundle_from_comm("/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper"),
                   Some(PathBuf::from("/Applications/Visual Studio Code.app")), "outermost bundle wins");
        assert_eq!(bundle_from_comm("-zsh"), None);
        assert_eq!(bundle_from_comm("/usr/bin/login"), None);
    }

    #[test] fn walks_ancestors_until_an_app_bundle() {
        let tree = |pid: u32| match pid {
            64361 => Some((55885, "claude".to_string())),
            55885 => Some((55884, "-zsh".to_string())),
            55884 => Some((55883, "/usr/bin/login".to_string())),
            55883 => Some((55880, "/Users/me/Library/Application Support/iTerm2/iTermServer".to_string())),
            55880 => Some((1, "/Applications/iTerm.app/Contents/MacOS/iTerm2".to_string())),
            _ => None,
        };
        assert_eq!(find_bundle(64361, tree), Some(PathBuf::from("/Applications/iTerm.app")));
    }

    #[test] fn parked_client_is_found_by_job_id() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("64361.json"), r#"{"pid":64361,"kind":"interactive","parkedJobId":"7b09e06b"}"#).unwrap();
        std::fs::write(d.path().join("69075.json"), r#"{"pid":69075,"kind":"bg","jobId":"7b09e06b"}"#).unwrap();
        std::fs::write(d.path().join("junk.json"), "not json").unwrap();
        std::fs::write(d.path().join("x.key"), r#"{"pid":1,"parkedJobId":"7b09e06b"}"#).unwrap();
        assert_eq!(parked_client_pid(d.path(), "7b09e06b"), Some(64361));
        assert_eq!(parked_client_pid(d.path(), "nope"), None);
        assert_eq!(parked_client_pid(&d.path().join("missing"), "7b09e06b"), None);
    }

    #[test] fn gives_up_at_init_or_unknown_pid() {
        let no_app = |pid: u32| match pid { 10 => Some((5, "bash".to_string())), 5 => Some((1, "launchd".to_string())), _ => None };
        assert_eq!(find_bundle(10, no_app), None);
        assert_eq!(find_bundle(999, |_| None), None);
        let cycle = |_pid: u32| Some((7u32, "x".to_string()));
        assert_eq!(find_bundle(7, cycle), None, "self-parent loop terminates");
    }
}
