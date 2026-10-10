pub mod collector;
pub mod config;
pub mod installer;
pub mod packs;
pub mod state;
pub mod windows;

pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running buddy");
}
