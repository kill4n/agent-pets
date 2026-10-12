use crate::config::{config_path, Config, SpriteSize};
use crate::installer;
use crate::packs::{list_pack_infos, PackInfo, list_packs, load_pack, Manifest};
use crate::state::store::AgentView;
use crate::state::agent_state::provisional_name;
use crate::windows::manager::{collapsed_origin, effective_manifest, label_for, resize_all, save_position};
use crate::windows::positions::Pos;
use crate::App;
use base64::Engine;
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, EventTarget, Manager, State};

#[derive(serde::Serialize)]
pub struct Status {
    pub hooks_installed: bool, pub poll_error: Option<String>, pub server_error: Option<String>,
    pub pack_warning: Option<String>, pub hook_url: String, pub token: String, pub pack: String,
    pub size: SpriteSize,
}

#[tauri::command]
pub fn get_manifest(app: State<Arc<App>>) -> Option<Manifest> { app.manifest.lock().unwrap().clone() }

/// True if `s` is exactly one plain path component (no separators, `..`, root or prefix).
fn is_single_normal(s: &str) -> bool {
    let mut c = Path::new(s).components();
    matches!((c.next(), c.next()), (Some(Component::Normal(n)), None) if n == s)
}

#[tauri::command]
pub fn get_pack_url(app: State<Arc<App>>, sheet: String) -> Result<String, String> {
    if !is_single_normal(&sheet) { return Err(format!("invalid sheet name: {sheet}")); }
    let in_pack = app.manifest.lock().unwrap().as_ref().is_some_and(|m| m.animations.values().any(|a| a.sheet == sheet));
    if !in_pack {
        return Err(format!("sheet not in current pack: {sheet}"));
    }
    let pack = app.config.lock().unwrap().pack.clone();
    let path = app.packs_root.join(&pack).join(&sheet);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[tauri::command(async)]
pub fn report_position(app: State<Arc<App>>, key: String, x: i32, y: i32) {
    let view = app.store.lock().unwrap().views(Instant::now()).into_iter().find(|v| v.key == key);
    let Some(view) = view else { return };
    // the provisional session_id[:8] name is throwaway: saving under it would never be found again
    if view.name == provisional_name(&view.session_id) { return; }
    // saved positions always describe the collapsed (sprite-only) rect
    let label = label_for(&key);
    let open = app.placement.lock().unwrap().is_open(&label);
    let manifest = app.manifest.lock().unwrap().clone();
    let origin = match manifest {
        Some(m) => collapsed_origin(open, Pos { x, y }, &m),
        None => Pos { x, y },
    };
    save_position(&mut app.positions.lock().unwrap(), &view.host, &view.name, origin.x, origin.y);
    app.placement.lock().unwrap().mark_dragged(&label);
}

/// Push a new effective manifest to every sprite window and resize them around their sprites.
fn apply_manifest(handle: &AppHandle, app: &App, old: Option<&Manifest>, new: &Manifest) {
    if let Some(old) = old { resize_all(handle, old, new, &app.placement); }
    for label in handle.webview_windows().into_keys() {
        if label.starts_with("sprite-") { let _ = handle.emit_to(EventTarget::webview_window(label), "manifest", new); }
    }
}

/// Change the on-screen sprite size (small / normal / large) and resize every window in place.
#[tauri::command(async)]
pub fn set_size(handle: AppHandle, app: State<Arc<App>>, size: String) -> Result<(), String> {
    let size = SpriteSize::parse(&size).ok_or_else(|| format!("unknown size: {size}"))?;
    let pack = {
        let mut cfg = app.config.lock().unwrap();
        cfg.size = size;
        if let Err(e) = cfg.save(&config_path()) { log::warn!("cannot save config: {e}"); }
        cfg.pack.clone()
    };
    let loaded = load_pack(&app.packs_root.join(&pack)).map_err(|e| e.to_string())?;
    let m = effective_manifest(&loaded, size);
    let old = app.manifest.lock().unwrap().replace(m.clone());
    apply_manifest(&handle, &app, old.as_ref(), &m);
    Ok(())
}

/// Bring the terminal hosting this agent's session to the front (double-click on the sprite).
#[tauri::command(async)]
pub fn focus_terminal(app: State<Arc<App>>, key: String) -> Result<(), String> {
    let view = app.store.lock().unwrap().views(Instant::now()).into_iter().find(|v| v.key == key)
        .ok_or("sesión desconocida")?;
    if view.pid.is_none() && view.job_id.is_none() { return Err("esta sesión no tiene un terminal asociado".into()); }
    let r = crate::focus::focus_session(view.pid, view.job_id.as_deref());
    if let Err(e) = &r { log::warn!("focus_terminal {}: {e}", view.name); }
    r
}

/// Open or close the speech bubble: the window grows/shrinks around the sprite.
#[tauri::command(async)]
pub fn set_bubble(handle: AppHandle, app: State<Arc<App>>, key: String, open: bool) -> Result<(), String> {
    let manifest = app.manifest.lock().unwrap().clone().ok_or("no pack loaded")?;
    crate::windows::manager::set_bubble(&handle, &label_for(&key), &manifest, open, &app.placement)
}

#[tauri::command]
pub fn list_agents(app: State<Arc<App>>) -> Vec<AgentView> { app.store.lock().unwrap().views(Instant::now()) }

#[tauri::command]
pub fn get_status(app: State<Arc<App>>) -> Status {
    let cfg = app.config.lock().unwrap().clone();
    let settings = std::fs::read_to_string(installer::default_settings_path()).ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).unwrap_or(serde_json::json!({}));
    Status { hooks_installed: installer::is_installed(&settings, &cfg.hook_url()),
             poll_error: app.poll_error.borrow().clone(), server_error: app.server_error.borrow().clone(),
             pack_warning: app.pack_warning.borrow().clone(), hook_url: cfg.hook_url(), token: cfg.token.clone(), pack: cfg.pack, size: cfg.size }
}

#[tauri::command]
pub fn install_hooks(app: State<Arc<App>>) -> Result<(), String> {
    let cfg = app.config.lock().unwrap().clone();
    installer::install(&installer::default_settings_path(), &cfg.hook_url(), &cfg.token).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn uninstall_hooks(app: State<Arc<App>>) -> Result<(), String> {
    let cfg = app.config.lock().unwrap().clone();
    installer::uninstall(&installer::default_settings_path(), &cfg.hook_url()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_pack_names(app: State<Arc<App>>) -> Vec<PackInfo> { list_pack_infos(&app.packs_root) }

#[tauri::command]
pub fn set_pack(handle: AppHandle, app: State<Arc<App>>, name: String) -> Result<(), String> {
    if !is_single_normal(&name) || !list_packs(&app.packs_root).contains(&name) {
        return Err(format!("unknown pack: {name}"));
    }
    let loaded = load_pack(&app.packs_root.join(&name)).map_err(|e| e.to_string())?;
    let size = {
        let mut cfg = app.config.lock().unwrap();
        cfg.pack = name;
        if let Err(e) = cfg.save(&config_path()) { log::warn!("cannot save config: {e}"); }
        cfg.size
    };
    let m = effective_manifest(&loaded, size);
    let old = app.manifest.lock().unwrap().replace(m.clone());
    app.pack_warning.send_replace(None);
    apply_manifest(&handle, &app, old.as_ref(), &m);
    Ok(())
}

#[tauri::command]
pub fn get_config(app: State<Arc<App>>) -> Config { app.config.lock().unwrap().clone() }
