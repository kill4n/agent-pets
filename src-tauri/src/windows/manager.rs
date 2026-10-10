use crate::packs::Manifest;
use crate::state::agent_state::provisional_name;
use crate::state::store::{AgentView, Change};
use crate::state::Visible;
use crate::windows::positions::{Pos, Positions};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, EventTarget, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn label_for(key: &str) -> String { format!("sprite-{}", key.replace([':', '/', '.'], "_")) }

/// Minimum window width so the tooltip (name, folder, state + reason/peer + time) fits.
pub const MIN_WINDOW_W: f64 = 220.0;
/// Room below the sprite for the (wrapping) tooltip.
pub const TOOLTIP_H: f64 = 84.0;

pub fn window_size(m: &Manifest) -> (f64, f64) {
    let [w, h] = m.frame_size;
    (((w * m.scale) as f64).max(MIN_WINDOW_W), (h * m.scale) as f64 + TOOLTIP_H)
}

/// Per-window bookkeeping for applying the saved `host+name` position once the real name is known.
#[derive(Default)]
pub struct Placement {
    /// labels whose window the user has moved (after the real name was known)
    dragged: HashSet<String>,
    /// last name each window was mirrored with
    names: HashMap<String, String>,
}

impl Placement {
    pub fn created(&mut self, label: &str, name: &str) { self.names.insert(label.into(), name.into()); }
    pub fn mark_dragged(&mut self, label: &str) { self.dragged.insert(label.into()); }
    pub fn removed(&mut self, label: &str) { self.dragged.remove(label); self.names.remove(label); }
    /// Record `v.name` for `label`; true when the window should now move to the saved position of
    /// the real name: the name just changed from the provisional one and the user has not dragged it.
    pub fn renamed(&mut self, label: &str, v: &AgentView) -> bool {
        let provisional = provisional_name(&v.session_id);
        let Some(prev) = self.names.insert(label.into(), v.name.clone()) else { return false };
        prev != v.name && prev == provisional && v.name != provisional && !self.dragged.contains(label)
    }
}

pub fn apply_changes(app: &AppHandle, changes: &[Change], manifest: Option<&Manifest>,
                     positions: &Mutex<Positions>, placement: &Mutex<Placement>) {
    for ch in changes {
        match ch {
            Change::Added(v) | Change::Updated(v) => {
                let label = label_for(&v.key);
                if app.get_webview_window(&label).is_some() {
                    let should_move = placement.lock().unwrap().renamed(&label, v);
                    if should_move { move_to_saved(app, &label, v, positions); }
                } else if let Some(m) = manifest {
                    ensure_window(app, v, m, positions, placement);
                }
                push_state(app, v);
            }
            Change::Removed(key) => {
                let label = label_for(key);
                placement.lock().unwrap().removed(&label);
                if let Some(w) = app.get_webview_window(&label) { let _ = w.close(); }
            }
        }
    }
}

/// Emit only to this agent's window: `emit` on a window broadcasts to every webview.
fn push_state(app: &AppHandle, v: &AgentView) {
    let label = label_for(&v.key);
    if app.get_webview_window(&label).is_some() {
        let _ = app.emit_to(EventTarget::webview_window(label), "agent-state", v);
    }
}

fn move_to_saved(app: &AppHandle, label: &str, v: &AgentView, positions: &Mutex<Positions>) {
    let saved = positions.lock().unwrap().get(&v.host, &v.name);
    let (Some(p), Some(w)) = (saved, app.get_webview_window(label)) else { return };
    if let Err(e) = w.set_position(LogicalPosition::new(p.x as f64, p.y as f64)) {
        log::warn!("cannot move {label} to saved position: {e}");
    }
}

// Never hold the positions/placement locks across calls that wait on the main thread
// (primary_monitor, build): main-thread commands may be waiting on those locks.
fn ensure_window(app: &AppHandle, v: &AgentView, manifest: &Manifest, positions: &Mutex<Positions>,
                 placement: &Mutex<Placement>) {
    // a gone agent never gets a new window (e.g. a SessionEnd for an unknown session)
    if v.visible == Visible::Gone { return; }
    let label = label_for(&v.key);
    if app.get_webview_window(&label).is_some() { return; }
    let (w, h) = window_size(manifest);
    let saved = positions.lock().unwrap().get(&v.host, &v.name);
    let pos = saved.unwrap_or_else(|| {
        let (sw, sh) = app.primary_monitor().ok().flatten()
            .map(|m| { let s = m.size(); let f = m.scale_factor(); ((s.width as f64 / f) as i32, (s.height as f64 / f) as i32) })
            .unwrap_or((1440, 900));
        let index = app.webview_windows().keys().filter(|k| k.starts_with("sprite-")).count();
        positions.lock().unwrap().cascade(index, sw, sh, w as i32, h as i32)
    });
    let url = WebviewUrl::App(format!("sprite/index.html?key={}", urlencoding(&v.key)).into());
    let built = WebviewWindowBuilder::new(app, &label, url)
        .title(&v.name).inner_size(w, h).position(pos.x as f64, pos.y as f64)
        .transparent(true).decorations(false).always_on_top(true).skip_taskbar(true)
        .resizable(false).shadow(false).visible_on_all_workspaces(true)
        .build();
    match built {
        Ok(_) => placement.lock().unwrap().created(&label, &v.name),
        Err(e) => log::error!("cannot create sprite window: {e}"),
    }
}

pub fn save_position(positions: &mut Positions, host: &str, name: &str, x: i32, y: i32) {
    positions.set(host, name, Pos { x, y });
}

fn urlencoding(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(name: &str) -> AgentView {
        AgentView { key: "h:abcdef123456".into(), host: "h".into(), session_id: "abcdef123456".into(),
            name: name.into(), cwd: None, visible: Visible::Working, waiting_for: None, peer: None, since_secs: 0 }
    }

    #[test] fn rename_from_provisional_triggers_move_once() {
        let mut p = Placement::default();
        p.created("w", "abcdef12");
        assert!(!p.renamed("w", &view("abcdef12")), "no rename yet");
        assert!(p.renamed("w", &view("buddy-7")), "provisional -> real name");
        assert!(!p.renamed("w", &view("buddy-7")), "only once");
        assert!(!p.renamed("w", &view("buddy-8")), "real -> real rename does not move");
    }

    #[test] fn dragged_window_is_not_moved_on_rename() {
        let mut p = Placement::default();
        p.created("w", "abcdef12");
        p.mark_dragged("w");
        assert!(!p.renamed("w", &view("buddy-7")));
    }

    #[test] fn unknown_or_removed_window_is_not_moved() {
        let mut p = Placement::default();
        assert!(!p.renamed("w", &view("buddy-7")));
        p.created("x", "abcdef12");
        p.mark_dragged("x");
        p.removed("x");
        p.created("x", "abcdef12");
        assert!(p.renamed("x", &view("buddy-7")), "removal clears the dragged flag");
    }

    #[test] fn renamed_agent_finds_position_saved_under_real_name() {
        let d = tempfile::tempdir().unwrap();
        let mut ps = Positions::load(d.path().join("p.json"));
        save_position(&mut ps, "h", "buddy-7", 300, 200);
        let mut p = Placement::default();
        p.created("w", "abcdef12");
        assert_eq!(ps.get("h", "abcdef12"), None, "provisional name has no saved position");
        let v = view("buddy-7");
        assert!(p.renamed("w", &v));
        assert_eq!(ps.get(&v.host, &v.name), Some(Pos { x: 300, y: 200 }));
    }

    fn manifest(frame: u32, scale: u32) -> Manifest {
        serde_json::from_value(serde_json::json!({
            "name": "t", "frameSize": [frame, frame], "scale": scale, "animations": {}
        })).unwrap()
    }

    #[test] fn window_is_at_least_tooltip_wide() {
        assert_eq!(window_size(&manifest(32, 4)).0, MIN_WINDOW_W);
        assert_eq!(window_size(&manifest(64, 4)).0, 256.0);
        assert_eq!(window_size(&manifest(32, 4)).1, 128.0 + TOOLTIP_H);
    }
}
