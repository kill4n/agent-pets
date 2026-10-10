use crate::packs::Manifest;
use crate::state::store::{AgentView, Change};
use crate::windows::positions::{Pos, Positions};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn label_for(key: &str) -> String { format!("sprite-{}", key.replace([':', '/', '.'], "_")) }

fn window_size(m: &Manifest) -> (f64, f64) {
    let [w, h] = m.frame_size;
    ((w * m.scale) as f64, (h * m.scale) as f64 + 56.0) // 56px for tooltip below the sprite
}

pub fn apply_changes(app: &AppHandle, changes: &[Change], manifest: &Manifest, positions: &mut Positions) {
    for ch in changes {
        match ch {
            Change::Added(v) => { ensure_window(app, v, manifest, positions); push_state(app, v); }
            Change::Updated(v) => { ensure_window(app, v, manifest, positions); push_state(app, v); }
            Change::Removed(key) => {
                if let Some(w) = app.get_webview_window(&label_for(key)) { let _ = w.close(); }
            }
        }
    }
}

fn push_state(app: &AppHandle, v: &AgentView) {
    if let Some(w) = app.get_webview_window(&label_for(&v.key)) { let _ = w.emit("agent-state", v); }
}

fn ensure_window(app: &AppHandle, v: &AgentView, manifest: &Manifest, positions: &mut Positions) {
    let label = label_for(&v.key);
    if app.get_webview_window(&label).is_some() { return; }
    let (w, h) = window_size(manifest);
    let pos = positions.get(&v.host, &v.name).unwrap_or_else(|| {
        let (sw, sh) = app.primary_monitor().ok().flatten()
            .map(|m| { let s = m.size(); let f = m.scale_factor(); ((s.width as f64 / f) as i32, (s.height as f64 / f) as i32) })
            .unwrap_or((1440, 900));
        let index = app.webview_windows().keys().filter(|k| k.starts_with("sprite-")).count();
        positions.cascade(index, sw, sh, w as i32, h as i32)
    });
    let url = WebviewUrl::App(format!("sprite/index.html?key={}", urlencoding(&v.key)).into());
    let built = WebviewWindowBuilder::new(app, &label, url)
        .title(&v.name).inner_size(w, h).position(pos.x as f64, pos.y as f64)
        .transparent(true).decorations(false).always_on_top(true).skip_taskbar(true)
        .resizable(false).shadow(false).visible_on_all_workspaces(true)
        .build();
    if let Err(e) = built { log::error!("cannot create sprite window: {e}"); }
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
