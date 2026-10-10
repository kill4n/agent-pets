use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Ajustes…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("icon"))
        .icon_as_template(true)
        .tooltip("Buddy")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "settings" => open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    log::info!("tray ready");
    Ok(())
}

/// Tooltip text: the first active warning, if any.
pub fn tooltip_text(warning: Option<&str>) -> String {
    match warning { Some(w) => format!("Buddy — ⚠ {w}"), None => "Buddy".into() }
}

/// Reflect the current warnings (hooks server, poller, pack) in the tray tooltip.
pub fn refresh_warning(app: &AppHandle, st: &crate::App) {
    let text = tooltip_text(st.first_warning().as_deref());
    if let Some(tray) = app.tray_by_id("main") {
        if let Err(e) = tray.set_tooltip(Some(text)) { log::warn!("cannot set tray tooltip: {e}"); }
    }
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
    #[test] fn tooltip_shows_first_warning() {
        assert_eq!(tooltip_text(None), "Buddy");
        assert_eq!(tooltip_text(Some("cannot run claude")), "Buddy — ⚠ cannot run claude");
    }
}
