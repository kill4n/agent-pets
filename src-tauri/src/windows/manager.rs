use crate::config::SpriteSize;
use crate::packs::Manifest;
use crate::state::agent_state::provisional_name;
use crate::state::store::{AgentView, Change};
use crate::state::Visible;
use crate::windows::positions::{Pos, Positions};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn label_for(key: &str) -> String { format!("sprite-{}", key.replace([':', '/', '.'], "_")) }

/// Minimum window width so the speech bubble (name, folder, state + reason/peer + time) fits.
pub const MIN_WINDOW_W: f64 = 220.0;
/// Room above the sprite for the speech bubble and its tail.
pub const BUBBLE_H: f64 = 84.0;

/// The pack manifest with the user's size preference applied to its `scale`.
pub fn effective_manifest(m: &Manifest, size: SpriteSize) -> Manifest {
    let mut e = m.clone();
    e.scale = size.apply(m.scale);
    e
}

/// Window size while the bubble is closed: exactly the sprite, so clicks around it fall through.
pub fn collapsed_size(m: &Manifest) -> (f64, f64) {
    let [w, h] = m.frame_size;
    ((w * m.scale) as f64, (h * m.scale) as f64)
}

/// Window size while the bubble is open: wide enough for the text, taller by the bubble area.
pub fn window_size(m: &Manifest) -> (f64, f64) {
    let (w, h) = collapsed_size(m);
    (w.max(MIN_WINDOW_W), h + BUBBLE_H)
}

/// How far the open window's top-left sits from the collapsed one's, so the sprite stays put on screen.
pub fn bubble_offset(m: &Manifest) -> (f64, f64) {
    let (cw, _) = collapsed_size(m);
    let (ow, _) = window_size(m);
    (((ow - cw) / 2.0).round(), BUBBLE_H)
}

/// Outer position + size for a window whose collapsed (sprite) rect starts at `collapsed`.
pub fn rect_for(open: bool, collapsed: Pos, m: &Manifest) -> (Pos, (f64, f64)) {
    if !open { return (collapsed, collapsed_size(m)); }
    let (dx, dy) = bubble_offset(m);
    (Pos { x: collapsed.x - dx as i32, y: collapsed.y - dy as i32 }, window_size(m))
}

/// Given the window's current outer position and whether its bubble is open, the collapsed rect origin.
pub fn collapsed_origin(open: bool, current: Pos, m: &Manifest) -> Pos {
    if !open { return current; }
    let (dx, dy) = bubble_offset(m);
    Pos { x: current.x + dx as i32, y: current.y + dy as i32 }
}

/// Per-window bookkeeping for applying the saved `host+name` position once the real name is known.
#[derive(Default)]
pub struct Placement {
    /// labels whose window the user has moved (after the real name was known)
    dragged: HashSet<String>,
    /// last name each window was mirrored with
    names: HashMap<String, String>,
    /// labels whose speech bubble is open (window expanded)
    open: HashSet<String>,
}

impl Placement {
    pub fn created(&mut self, label: &str, name: &str) { self.names.insert(label.into(), name.into()); }
    pub fn mark_dragged(&mut self, label: &str) { self.dragged.insert(label.into()); }
    pub fn removed(&mut self, label: &str) { self.dragged.remove(label); self.names.remove(label); self.open.remove(label); }
    pub fn is_open(&self, label: &str) -> bool { self.open.contains(label) }
    /// Returns the previous open state.
    pub fn set_open(&mut self, label: &str, open: bool) -> bool {
        if open { !self.open.insert(label.into()) } else { self.open.remove(label) }
    }
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
    let (w, h) = collapsed_size(manifest);
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

/// Current outer position of a window in logical pixels.
fn current_pos(w: &tauri::WebviewWindow) -> Option<Pos> {
    let p = w.outer_position().ok()?;
    let f = w.scale_factor().ok()?;
    Some(Pos { x: (p.x as f64 / f).round() as i32, y: (p.y as f64 / f).round() as i32 })
}

/// Expand (open) or shrink (closed) a sprite window around its sprite, which stays put on screen.
/// Locks are only held to flip the flag, never across the main-thread calls.
pub fn set_bubble(app: &AppHandle, label: &str, m: &Manifest, open: bool, placement: &Mutex<Placement>) -> Result<(), String> {
    let w = app.get_webview_window(label).ok_or_else(|| format!("no window {label}"))?;
    let was_open = placement.lock().unwrap().set_open(label, open);
    if was_open == open { return Ok(()); }
    let cur = current_pos(&w).ok_or("cannot read window position")?;
    let collapsed = collapsed_origin(was_open, cur, m);
    let (pos, (sw, sh)) = rect_for(open, collapsed, m);
    w.set_size(LogicalSize::new(sw, sh)).map_err(|e| e.to_string())?;
    w.set_position(LogicalPosition::new(pos.x as f64, pos.y as f64)).map_err(|e| e.to_string())?;
    Ok(())
}

/// Resize every sprite window from the `old` to the `new` effective manifest, keeping each sprite's
/// top-left corner where it is. Locks are only touched to read the open flag.
pub fn resize_all(app: &AppHandle, old: &Manifest, new: &Manifest, placement: &Mutex<Placement>) {
    for (label, w) in app.webview_windows() {
        if !label.starts_with("sprite-") { continue; }
        let open = placement.lock().unwrap().is_open(&label);
        let Some(cur) = current_pos(&w) else { continue };
        let collapsed = collapsed_origin(open, cur, old);
        let (pos, (sw, sh)) = rect_for(open, collapsed, new);
        if let Err(e) = w.set_size(LogicalSize::new(sw, sh)) { log::warn!("resize {label}: {e}"); }
        if let Err(e) = w.set_position(LogicalPosition::new(pos.x as f64, pos.y as f64)) { log::warn!("move {label}: {e}"); }
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
            name: name.into(), cwd: None, pid: None, job_id: None, visible: Visible::Working, waiting_for: None, peer: None, since_secs: 0 }
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

    #[test] fn effective_manifest_applies_size_to_scale() {
        let m = manifest(96, 2);
        assert_eq!(effective_manifest(&m, SpriteSize::Small).scale, 1);
        assert_eq!(effective_manifest(&m, SpriteSize::Large).scale, 3);
        assert_eq!(collapsed_size(&effective_manifest(&m, SpriteSize::Small)), (96.0, 96.0));
    }

    #[test] fn collapsed_window_is_exactly_the_sprite() {
        assert_eq!(collapsed_size(&manifest(96, 2)), (192.0, 192.0));
        assert_eq!(collapsed_size(&manifest(32, 4)), (128.0, 128.0));
    }

    #[test] fn open_rect_keeps_the_sprite_in_place() {
        let m = manifest(96, 2); // collapsed 192x192, open 220x276 -> offset (14, 84)
        let c = Pos { x: 500, y: 400 };
        let (p, size) = rect_for(true, c, &m);
        assert_eq!(size, (220.0, 276.0));
        assert_eq!(p, Pos { x: 486, y: 316 });
        assert_eq!(collapsed_origin(true, p, &m), c, "round trip");
        assert_eq!(rect_for(false, c, &m), (c, (192.0, 192.0)));
        assert_eq!(collapsed_origin(false, c, &m), c);
        let wide = manifest(64, 4); // 256 wide: no horizontal offset
        assert_eq!(bubble_offset(&wide), (0.0, BUBBLE_H));
    }

    #[test] fn placement_tracks_open_state() {
        let mut p = Placement::default();
        assert!(!p.is_open("w"));
        assert!(!p.set_open("w", true), "was closed");
        assert!(p.is_open("w"));
        assert!(p.set_open("w", true), "already open");
        assert!(p.set_open("w", false), "was open");
        p.set_open("w", true); p.removed("w");
        assert!(!p.is_open("w"), "removal clears open");
    }

    #[test] fn window_is_at_least_bubble_wide_and_reserves_bubble_height() {
        assert_eq!(window_size(&manifest(32, 4)).0, MIN_WINDOW_W);
        assert_eq!(window_size(&manifest(64, 4)).0, 256.0);
        assert_eq!(window_size(&manifest(32, 4)).1, 128.0 + BUBBLE_H);
        assert_eq!(window_size(&manifest(64, 2)).1, 128.0 + 84.0);
    }
}
