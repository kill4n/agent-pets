pub mod collector;
pub mod commands;
pub mod config;
pub mod installer;
pub mod packs;
pub mod state;
pub mod tray;
pub mod windows;

use collector::event::AgentEvent;
use config::{config_path, positions_path, Config};
use packs::{load_pack, Manifest};
use state::store::{Change, Store};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tokio::sync::{mpsc, watch};
use windows::positions::Positions;

pub struct App {
    pub config: Mutex<Config>,
    pub store: Mutex<Store>,
    pub positions: Mutex<Positions>,
    pub manifest: Mutex<Manifest>,
    pub packs_root: PathBuf,
    pub host: String,
    pub poll_error: watch::Receiver<Option<String>>,
}

fn packs_root(handle: &tauri::AppHandle) -> PathBuf {
    // dev: repo assets; bundled: resources dir
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/packs");
    if dev.is_dir() { return dev; }
    handle.path().resource_dir().map(|r| r.join("assets/packs")).unwrap_or(dev)
}

pub fn run() {
    env_logger::init();
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let _guard = rt.enter();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(move |app| {
            let handle = app.handle().clone();
            let mut cfg = Config::load(&config_path());
            let _ = cfg.save(&config_path());
            let root = packs_root(&handle);
            let manifest = match load_pack(&root.join(&cfg.pack)) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("pack {} invalid ({e}), falling back to default", cfg.pack);
                    cfg.pack = "default".to_string();
                    load_pack(&root.join("default")).expect("default pack must be valid")
                }
            };
            let host = hostname::get().map(|h| h.to_string_lossy().into_owned()).unwrap_or_else(|_| "local".into());

            let (tx, mut rx) = mpsc::channel::<AgentEvent>(256);
            let (err_tx, err_rx) = watch::channel(None);

            let state = Arc::new(App {
                config: Mutex::new(cfg.clone()), store: Mutex::new(Store::new()),
                positions: Mutex::new(Positions::load(positions_path())),
                manifest: Mutex::new(manifest), packs_root: root, host: host.clone(), poll_error: err_rx,
            });
            app.manage(state.clone());

            tray::build(&handle)?;

            let addr = cfg.bind_addr();
            let (h2, t2, tx2) = (host.clone(), cfg.token.clone(), tx.clone());
            tauri::async_runtime::spawn(async move {
                match collector::hooks_server::serve(addr, h2, t2, tx2).await {
                    Ok(a) => log::info!("hooks server on {a}"),
                    Err(e) => log::error!("hooks server failed to bind {addr}: {e}"),
                }
            });
            collector::poller::spawn_poller(host.clone(), tx.clone(), err_tx);

            // event loop: apply events, mirror windows, sweep gone agents
            let st = state.clone();
            tauri::async_runtime::spawn(async move {
                let mut sweep = tokio::time::interval(Duration::from_secs(1));
                loop {
                    tokio::select! {
                        Some(ev) = rx.recv() => {
                            let changes = st.store.lock().unwrap().apply(&ev);
                            mirror(&handle, &st, changes);
                        }
                        _ = sweep.tick() => {
                            let changes = st.store.lock().unwrap().sweep(Instant::now());
                            mirror(&handle, &st, changes);
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_manifest, commands::get_pack_url, commands::report_position, commands::list_agents,
            commands::get_status, commands::install_hooks, commands::uninstall_hooks, commands::list_pack_names,
            commands::set_pack, commands::get_config
        ])
        .build(tauri::generate_context!())
        .expect("error while building buddy")
        .run(|_app, event| {
            // keep running with no windows (tray app); an explicit app.exit(code) still quits
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() { api.prevent_exit(); }
            }
        });
}

/// Mirror store changes into sprite windows and refresh the settings window.
fn mirror(handle: &tauri::AppHandle, st: &Arc<App>, changes: Vec<Change>) {
    if changes.is_empty() { return; }
    let m = st.manifest.lock().unwrap().clone();
    windows::manager::apply_changes(handle, &changes, &m, &st.positions);
    notify_settings(handle, st);
}

fn notify_settings(handle: &tauri::AppHandle, st: &Arc<App>) {
    if let Some(w) = handle.get_webview_window("settings") {
        let views = st.store.lock().unwrap().views(Instant::now());
        let _ = w.emit("agents", views);
    }
}
