use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const HOOK_EVENTS: [&str; 10] = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse",
    "PostToolUse", "Stop", "PermissionRequest", "Notification", "SubagentStart", "SubagentStop"];

#[derive(Debug)]
pub enum InstallError { Io(std::io::Error), Json(serde_json::Error) }
impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for InstallError {}
impl From<std::io::Error> for InstallError { fn from(e: std::io::Error) -> Self { InstallError::Io(e) } }
impl From<serde_json::Error> for InstallError { fn from(e: serde_json::Error) -> Self { InstallError::Json(e) } }

pub fn hook_entry(url: &str, token: &str) -> Value {
    json!({ "hooks": [{ "type": "http", "url": url,
        "headers": { "Authorization": format!("Bearer {token}") }, "timeout": 2 }] })
}

fn group_has_url(group: &Value, url: &str) -> bool {
    group.get("hooks").and_then(|h| h.as_array())
        .is_some_and(|arr| arr.iter().any(|h| h.get("url").and_then(|u| u.as_str()) == Some(url)))
}

pub fn remove_hooks(settings: &mut Value, url: &str) {
    let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) else { return };
    let mut empty = vec![];
    for (ev, groups) in hooks.iter_mut() {
        if let Some(arr) = groups.as_array_mut() {
            arr.retain(|g| !group_has_url(g, url));
            if arr.is_empty() { empty.push(ev.clone()); }
        }
    }
    for ev in empty { hooks.remove(&ev); }
}

pub fn merge_hooks(settings: &mut Value, url: &str, token: &str) {
    remove_hooks(settings, url);
    if !settings.is_object() { *settings = json!({}); }
    let obj = settings.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() { *hooks = json!({}); }
    let hooks = hooks.as_object_mut().unwrap();
    for ev in HOOK_EVENTS {
        let groups = hooks.entry(ev).or_insert_with(|| json!([]));
        if !groups.is_array() { *groups = json!([]); }
        groups.as_array_mut().unwrap().push(hook_entry(url, token));
    }
}

pub fn is_installed(settings: &Value, url: &str) -> bool {
    settings.get("hooks").and_then(|h| h.as_object())
        .is_some_and(|m| m.values().any(|groups|
            groups.as_array().is_some_and(|arr| arr.iter().any(|g| group_has_url(g, url)))))
}

fn read_settings(path: &Path) -> Result<Value, InstallError> {
    if !path.exists() { return Ok(json!({})); }
    let text = std::fs::read_to_string(path)?;
    if text.trim().is_empty() { return Ok(json!({})); }
    Ok(serde_json::from_str(&text)?)
}

fn write_with_backup(path: &Path, value: &Value) -> Result<(), InstallError> {
    if path.exists() {
        let backup = path.with_file_name(format!("{}.buddy-backup",
            path.file_name().unwrap().to_string_lossy()));
        std::fs::copy(path, backup)?;
    } else if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

pub fn install(path: &Path, url: &str, token: &str) -> Result<(), InstallError> {
    let mut v = read_settings(path)?;
    merge_hooks(&mut v, url, token);
    write_with_backup(path, &v)
}

pub fn uninstall(path: &Path, url: &str) -> Result<(), InstallError> {
    let mut v = read_settings(path)?;
    remove_hooks(&mut v, url);
    write_with_backup(path, &v)
}

pub fn default_settings_path() -> PathBuf {
    let base = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().join(".claude")))
        .unwrap_or_else(|| PathBuf::from(".claude"));
    base.join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const URL: &str = "http://127.0.0.1:7433/hook";

    #[test] fn merge_into_empty_adds_all_events() {
        let mut s = json!({"model":"x"});
        merge_hooks(&mut s, URL, "tok");
        for ev in HOOK_EVENTS { assert_eq!(s["hooks"][ev].as_array().unwrap().len(), 1, "{ev}"); }
        assert_eq!(s["model"], "x");
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["type"], "http");
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["headers"]["Authorization"], "Bearer tok");
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["timeout"], 2);
    }

    #[test] fn merge_is_idempotent() {
        let mut s = json!({});
        merge_hooks(&mut s, URL, "tok");
        merge_hooks(&mut s, URL, "tok");
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    #[test] fn merge_replaces_stale_token_for_same_url() {
        let mut s = json!({});
        merge_hooks(&mut s, URL, "old");
        merge_hooks(&mut s, URL, "new");
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["headers"]["Authorization"], "Bearer new");
    }

    #[test] fn merge_keeps_foreign_entries_and_remove_only_touches_ours() {
        let mut s = json!({"hooks":{"Stop":[{"matcher":"","hooks":[{"type":"command","command":"say done"}]}],
                                    "PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"lint"}]}]}});
        merge_hooks(&mut s, URL, "tok");
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert!(is_installed(&s, URL));
        remove_hooks(&mut s, URL);
        assert!(!is_installed(&s, URL));
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
        assert_eq!(s["hooks"]["PreToolUse"][0]["matcher"], "Bash");
        assert!(s["hooks"].get("SessionStart").is_none(), "events we created are removed when empty");
    }

    #[test] fn remove_tolerates_mixed_legacy_shapes() {
        // a foreign entry whose "hooks" is not an array, and one group where hooks is a string
        let mut s = json!({"hooks":{"Stop":[{"hooks":"weird"}, 42]}});
        merge_hooks(&mut s, URL, "tok");
        remove_hooks(&mut s, URL);
        assert_eq!(s["hooks"]["Stop"], json!([{"hooks":"weird"}, 42]));
    }

    #[test] fn install_writes_backup_and_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(&p, r#"{"model":"fable"}"#).unwrap();
        install(&p, URL, "tok").unwrap();
        let backup = std::fs::read_to_string(d.path().join("settings.json.buddy-backup")).unwrap();
        assert_eq!(backup, r#"{"model":"fable"}"#);
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert!(is_installed(&v, URL));
        assert_eq!(v["model"], "fable");
        uninstall(&p, URL).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert!(!is_installed(&v, URL));
    }

    #[test] fn install_on_missing_file_creates_it() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        install(&p, URL, "tok").unwrap();
        assert!(p.is_file());
    }

    #[test] fn install_on_invalid_json_fails_without_writing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert!(matches!(install(&p, URL, "tok"), Err(InstallError::Json(_))));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json");
    }
}
