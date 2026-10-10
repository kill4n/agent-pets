pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running buddy");
}
