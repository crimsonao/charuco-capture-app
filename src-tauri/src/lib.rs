pub mod calib;
pub mod camera;
pub mod camera_open;
pub mod detect;
pub mod score;
pub mod session;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            camera::list_cameras,
            camera_open::start_preview,
            camera_open::stop_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
