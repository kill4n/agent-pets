//! Menu-bar icon with every setting embedded in its menu. The menu is rebuilt from the current
//! state whenever something it shows changes (config, agents, warnings).

use crate::commands::{self, Status};
use crate::config::SpriteSize;
use crate::packs::list_pack_infos;
use crate::state::store::AgentView;
use crate::state::Visible;
use crate::App;
use std::sync::Arc;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;

pub const WANDER_AFTER_CHOICES: [u32; 6] = [10, 20, 30, 60, 120, 300];

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::with_items(app, &[&MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("icon"))
        .icon_as_template(true)
        .tooltip("Buddy")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, ev| on_menu(app, ev.id.as_ref()))
        .build(app)?;
    log::info!("tray ready");
    Ok(())
}

// ---------- labels (pure, tested) ----------

pub fn status_line(s: &Status) -> String {
    let hooks = if s.hooks_installed { "instalados" } else { "no instalados" };
    let poll = match (&s.server_error, &s.poll_error) {
        (Some(e), _) => format!("servidor ⚠ {e}"),
        (None, Some(e)) => format!("poll ⚠ {e}"),
        (None, None) => "poll ok".into(),
    };
    format!("Hooks: {hooks} · {poll}")
}

pub fn visible_label(v: Visible) -> &'static str {
    match v {
        Visible::Working => "trabajando", Visible::Idle => "idle", Visible::NeedsInput => "necesita respuesta",
        Visible::Delegating => "delegando", Visible::WaitingPeer => "esperando a otro agente", Visible::Gone => "terminó",
    }
}

pub fn agent_label(v: &AgentView) -> String { format!("{} · {}", v.name, visible_label(v.visible)) }

pub fn wander_after_label(secs: u32) -> String {
    if secs % 60 == 0 { format!("{} min", secs / 60) } else { format!("{secs} s") }
}

pub fn size_label(s: SpriteSize) -> &'static str {
    match s { SpriteSize::Small => "Pequeño", SpriteSize::Normal => "Normal", SpriteSize::Large => "Grande" }
}

// ---------- menu ----------

fn build_menu(app: &AppHandle, st: &App) -> tauri::Result<Menu<tauri::Wry>> {
    let status = commands::status_of(st);
    let agents = st.store.lock().unwrap().views(std::time::Instant::now());
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);

    let status_item = MenuItem::with_id(app, "status", status_line(&status), false, None::<&str>)?;
    let hooks_item = MenuItem::with_id(app, "hooks:toggle",
        if status.hooks_installed { "Desconectar de Claude Code" } else { "Conectar con Claude Code" }, true, None::<&str>)?;

    let agents_menu = Submenu::with_id(app, "agents", format!("Agentes ({})", agents.len()), true)?;
    if agents.is_empty() {
        agents_menu.append(&MenuItem::with_id(app, "agents:none", "ninguno", false, None::<&str>)?)?;
    }
    for v in &agents {
        let focusable = v.pid.is_some() || v.job_id.is_some();
        agents_menu.append(&MenuItem::with_id(app, format!("agent:{}", v.key), agent_label(v), focusable, None::<&str>)?)?;
    }

    let pack_menu = Submenu::with_id(app, "pack", "Pack", true)?;
    for p in list_pack_infos(&st.packs_root) {
        pack_menu.append(&CheckMenuItem::with_id(app, format!("pack:{}", p.dir), &p.name, true, p.dir == status.pack, None::<&str>)?)?;
    }

    let size_menu = Submenu::with_id(app, "size", "Tamaño", true)?;
    for s in [SpriteSize::Small, SpriteSize::Normal, SpriteSize::Large] {
        let id = format!("size:{}", serde_json::to_value(s).unwrap().as_str().unwrap());
        size_menu.append(&CheckMenuItem::with_id(app, id, size_label(s), true, s == status.size, None::<&str>)?)?;
    }

    let wander_item = CheckMenuItem::with_id(app, "wander:toggle", "Pasear cuando están inactivos", true, status.wander, None::<&str>)?;
    let wander_after = Submenu::with_id(app, "wander_after", "Pasear tras", true)?;
    let mut choices: Vec<u32> = WANDER_AFTER_CHOICES.to_vec();
    if !choices.contains(&status.wander_after_secs) { choices.push(status.wander_after_secs); choices.sort(); }
    for secs in choices {
        wander_after.append(&CheckMenuItem::with_id(app, format!("wander_after:{secs}"), wander_after_label(secs), true,
            secs == status.wander_after_secs, None::<&str>)?)?;
    }

    let dev_build = cfg!(debug_assertions);
    let autostart_item = CheckMenuItem::with_id(app, "autostart:toggle",
        if dev_build { "Arrancar al iniciar sesión (solo en la app instalada)" } else { "Arrancar al iniciar sesión" },
        !dev_build, autostart && !dev_build, None::<&str>)?;
    let advanced = MenuItem::with_id(app, "advanced", "Avanzado…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = vec![
        Box::new(status_item), Box::new(hooks_item), Box::new(PredefinedMenuItem::separator(app)?),
        Box::new(agents_menu), Box::new(PredefinedMenuItem::separator(app)?),
        Box::new(pack_menu), Box::new(size_menu), Box::new(wander_item), Box::new(wander_after), Box::new(autostart_item),
        Box::new(PredefinedMenuItem::separator(app)?), Box::new(advanced), Box::new(quit),
    ];
    if let Some(w) = status.pack_warning.as_deref() {
        items.insert(1, Box::new(MenuItem::with_id(app, "pack_warning", format!("⚠ {w}"), false, None::<&str>)?));
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// Rebuild the tray menu from the current state (settings, agents, warnings) and refresh the tooltip.
pub fn refresh_menu(app: &AppHandle, st: &App) {
    let Some(tray) = app.tray_by_id("main") else { return };
    match build_menu(app, st) {
        Ok(menu) => { if let Err(e) = tray.set_menu(Some(menu)) { log::warn!("cannot set tray menu: {e}"); } }
        Err(e) => log::warn!("cannot build tray menu: {e}"),
    }
    if let Err(e) = tray.set_tooltip(Some(tooltip_text(st.first_warning().as_deref()))) { log::warn!("cannot set tray tooltip: {e}"); }
}

/// Reflect the current warnings (hooks server, poller, pack) in the tray.
pub fn refresh_warning(app: &AppHandle, st: &App) { refresh_menu(app, st); }

fn on_menu(app: &AppHandle, id: &str) {
    let st: Arc<App> = app.state::<Arc<App>>().inner().clone();
    let result: Result<(), String> = match id {
        "quit" => { app.exit(0); Ok(()) }
        "advanced" => { open_settings(app); Ok(()) }
        "hooks:toggle" => {
            let installed = commands::status_of(&st).hooks_installed;
            commands::do_set_hooks(&st, !installed)
        }
        "wander:toggle" => {
            let (enabled, secs) = { let c = st.config.lock().unwrap(); (c.wander, c.wander_after_secs) };
            commands::do_set_wander(&st, !enabled, secs)
        }
        "autostart:toggle" if cfg!(debug_assertions) => Err("la build de desarrollo no puede arrancar sola: instala Buddy.app".into()),
        "autostart:toggle" => {
            let al = app.autolaunch();
            let r = if al.is_enabled().unwrap_or(false) { al.disable() } else { al.enable() };
            r.map_err(|e| e.to_string())
        }
        _ => {
            if let Some(key) = id.strip_prefix("agent:") { commands::do_focus_terminal(&st, key) }
            else if let Some(dir) = id.strip_prefix("pack:") { commands::do_set_pack(app, &st, dir.to_string()) }
            else if let Some(s) = id.strip_prefix("size:") {
                SpriteSize::parse(s).ok_or_else(|| format!("tamaño desconocido {s}")).and_then(|sz| commands::do_set_size(app, &st, sz))
            }
            else if let Some(s) = id.strip_prefix("wander_after:") {
                let enabled = st.config.lock().unwrap().wander;
                s.parse::<u32>().map_err(|e| e.to_string()).and_then(|secs| commands::do_set_wander(&st, enabled, secs))
            }
            else { Ok(()) }
        }
    };
    if let Err(e) = result { log::warn!("tray action {id}: {e}"); }
    refresh_menu(app, &st);
}

/// Tooltip text: the first active warning, if any.
pub fn tooltip_text(warning: Option<&str>) -> String {
    match warning { Some(w) => format!("Buddy — ⚠ {w}"), None => "Buddy".into() }
}

pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings/index.html".into()))
        .title("Buddy")
        .inner_size(420.0, 520.0)
        .resizable(true)
        .build();
}

#[cfg(test)]
mod tests {
    use super::*;
    fn status(hooks: bool, poll: Option<&str>, server: Option<&str>) -> Status {
        Status { hooks_installed: hooks, poll_error: poll.map(Into::into), server_error: server.map(Into::into), pack_warning: None,
            hook_url: "u".into(), token: "t".into(), pack: "default".into(), size: SpriteSize::Normal, wander: true, wander_after_secs: 20 }
    }
    #[test] fn tooltip_shows_first_warning() {
        assert_eq!(tooltip_text(None), "Buddy");
        assert_eq!(tooltip_text(Some("cannot run claude")), "Buddy — ⚠ cannot run claude");
    }
    #[test] fn status_line_reports_hooks_and_the_most_severe_collector_error() {
        assert_eq!(status_line(&status(true, None, None)), "Hooks: instalados · poll ok");
        assert_eq!(status_line(&status(false, Some("no claude"), None)), "Hooks: no instalados · poll ⚠ no claude");
        assert_eq!(status_line(&status(true, Some("x"), Some("puerto en uso"))), "Hooks: instalados · servidor ⚠ puerto en uso");
    }
    #[test] fn labels() {
        assert_eq!(wander_after_label(20), "20 s");
        assert_eq!(wander_after_label(120), "2 min");
        assert_eq!(size_label(SpriteSize::Large), "Grande");
        let v = AgentView { key: "h:s".into(), host: "h".into(), session_id: "s".into(), name: "buddy-1".into(), cwd: None, pid: None,
            job_id: None, visible: Visible::NeedsInput, waiting_for: None, peer: None, since_secs: 0 };
        assert_eq!(agent_label(&v), "buddy-1 · necesita respuesta");
    }
}
