use crate::config::{config_path, Config};
use crate::installer;
use crate::packs::{list_packs, load_pack, Manifest};
use crate::state::store::AgentView;
use crate::windows::manager::save_position;
use crate::App;
use base64::Engine;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(serde::Serialize)]
pub struct Status { pub hooks_installed: bool, pub poll_error: Option<String>, pub hook_url: String, pub token: String, pub pack: String }

#[tauri::command]
pub fn get_manifest(app: State<Arc<App>>) -> Manifest { app.manifest.lock().unwrap().clone() }

#[tauri::command]
pub fn get_pack_url(app: State<Arc<App>>, sheet: String) -> Result<String, String> {
    let pack = app.config.lock().unwrap().pack.clone();
    let path = app.packs_root.join(&pack).join(&sheet);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[tauri::command]
pub fn report_position(app: State<Arc<App>>, key: String, x: i32, y: i32) {
    let store = app.store.lock().unwrap();
    let Some(view) = store.views(Instant::now()).into_iter().find(|v| v.key == key) else { return };
    save_position(&mut app.positions.lock().unwrap(), &view.host, &view.name, x, y);
}

#[tauri::command]
pub fn list_agents(app: State<Arc<App>>) -> Vec<AgentView> { app.store.lock().unwrap().views(Instant::now()) }

#[tauri::command]
pub fn get_status(app: State<Arc<App>>) -> Status {
    let cfg = app.config.lock().unwrap().clone();
    let settings = std::fs::read_to_string(installer::default_settings_path()).ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).unwrap_or(serde_json::json!({}));
    Status { hooks_installed: installer::is_installed(&settings, &cfg.hook_url()),
             poll_error: app.poll_error.borrow().clone(), hook_url: cfg.hook_url(), token: cfg.token.clone(), pack: cfg.pack }
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
pub fn list_pack_names(app: State<Arc<App>>) -> Vec<String> { list_packs(&app.packs_root) }

#[tauri::command]
pub fn set_pack(handle: AppHandle, app: State<Arc<App>>, name: String) -> Result<(), String> {
    let m = load_pack(&app.packs_root.join(&name)).map_err(|e| e.to_string())?;
    { let mut cfg = app.config.lock().unwrap(); cfg.pack = name; let _ = cfg.save(&config_path()); }
    *app.manifest.lock().unwrap() = m.clone();
    for (label, w) in handle.webview_windows() {
        if label.starts_with("sprite-") { let _ = w.emit("manifest", &m); }
    }
    Ok(())
}

#[tauri::command]
pub fn get_config(app: State<Arc<App>>) -> Config { app.config.lock().unwrap().clone() }
