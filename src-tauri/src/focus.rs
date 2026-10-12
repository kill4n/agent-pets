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

fn ps_lookup(pid: u32) -> Option<(u32, String)> {
    let out = Command::new("ps").args(["-o", "ppid=,comm=", "-p", &pid.to_string()]).output().ok()?;
    if !out.status.success() { return None; }
    parse_ps_line(&String::from_utf8_lossy(&out.stdout))
}

/// Activate the terminal application hosting `pid`.
pub fn focus_pid(pid: u32) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let bundle = find_bundle(pid, ps_lookup).ok_or("no se encontró la app del terminal")?;
        let st = Command::new("open").arg(&bundle).status().map_err(|e| e.to_string())?;
        return if st.success() { Ok(()) } else { Err(format!("open {} falló", bundle.display())) };
    }
    #[cfg(target_os = "linux")]
    {
        // best effort: find the first ancestor that owns an X window and activate it
        let mut p = pid;
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
    { let _ = pid; Err("enfocar el terminal no está soportado en este sistema".into()) }
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

    #[test] fn gives_up_at_init_or_unknown_pid() {
        let no_app = |pid: u32| match pid { 10 => Some((5, "bash".to_string())), 5 => Some((1, "launchd".to_string())), _ => None };
        assert_eq!(find_bundle(10, no_app), None);
        assert_eq!(find_bundle(999, |_| None), None);
        let cycle = |_pid: u32| Some((7u32, "x".to_string()));
        assert_eq!(find_bundle(7, cycle), None, "self-parent loop terminates");
    }
}
