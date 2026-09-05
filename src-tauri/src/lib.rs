pub mod calib;
pub mod camera;
pub mod camera_open;
pub mod capture_flow;
pub mod detect;
pub mod frame_convert;
pub mod jpeg;
pub mod native_dshow_capture;
pub mod native_msmf_capture;
pub mod score;
pub mod session;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            camera::list_cameras,
            camera_open::start_preview,
            camera_open::stop_session,
            camera_open::open_session_folder,
            session::default_output_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
