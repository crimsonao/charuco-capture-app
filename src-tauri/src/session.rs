//! Autosave unmarked JPEG frames; write accept artifacts only when session is accepted.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::calib::{CalibResult, Shot};
use crate::detect::sharpness as frame_sharpness;
use crate::jpeg::encode_jpeg_bytes;
use crate::score::{preview_percent, session_percent};

pub const SAVE_COOLDOWN: Duration = Duration::from_millis(700);
pub const SAVE_JPEG_QUALITY: i32 = 95;
pub const DEFAULT_COUNT_TARGET: usize = 15;
pub const MIN_COUNT_TARGET: usize = 3;
pub const DEFAULT_SCORE_TARGET: f64 = 80.0;
pub const DEFAULT_SQUARE_MM: f64 = 20.0;
pub const DEFAULT_MARKER_MM: f64 = 15.0;

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub count_target: usize,
    pub score_target: f64,
    pub square_mm: f64,
    pub marker_mm: f64,
    pub out_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CapturedImage {
    pub path: String,
    pub n_corners: usize,
    pub quality: f64,
    pub diversity: f64,
    pub percent: f64,
    pub reprojection_error: Option<f64>,
    pub feature: [f32; 6],
}

impl SessionConfig {
    pub fn default_capture() -> Self {
        Self {
            count_target: DEFAULT_COUNT_TARGET,
            score_target: DEFAULT_SCORE_TARGET,
            square_mm: DEFAULT_SQUARE_MM,
            marker_mm: DEFAULT_MARKER_MM,
            out_root: default_output_root(),
        }
    }

    pub fn clamp_count(n: usize) -> usize {
        n.max(MIN_COUNT_TARGET)
    }

    pub fn from_setup(
        count_target: usize,
        score_target: f64,
        square_mm: f64,
        marker_mm: f64,
        out_root: PathBuf,
    ) -> Self {
        Self {
            count_target: Self::clamp_count(count_target),
            score_target,
            square_mm,
            marker_mm,
            out_root,
        }
    }
}

pub fn default_output_root() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let desktop = home.join("Desktop");
    if desktop.is_dir() {
        return desktop.join("ChArUcoCapture");
    }
    let onedrive = home.join("OneDrive").join("Desktop");
    if onedrive.is_dir() {
        return onedrive.join("ChArUcoCapture");
    }
    home.join("ChArUcoCapture")
}

/// Use a non-empty user path, otherwise Desktop/ChArUcoCapture.
pub fn resolve_out_root(raw: Option<&str>) -> PathBuf {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => default_output_root(),
    }
}

#[tauri::command]
pub fn default_output_dir() -> String {
    default_output_root().to_string_lossy().into_owned()
}

pub fn session_dir_name(stamp: &str) -> String {
    format!("session_{stamp}")
}

pub fn local_session_stamp() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

pub fn start_session_dir(root: &Path) -> Result<PathBuf, String> {
    start_session_dir_named(root, &local_session_stamp())
}

pub fn start_session_dir_named(root: &Path, stamp: &str) -> Result<PathBuf, String> {
    let dir = root.join(session_dir_name(stamp));
    std::fs::create_dir_all(&dir).map_err(|err| format!("create session dir: {err}"))?;
    Ok(dir)
}

/// User-visible hint when autosave cannot run because the session directory failed to open.
pub fn session_save_error_hint(err: &str) -> String {
    format!("无法保存：{err}。请检查目录权限")
}

/// Absolute path for report `images[].path` (matches Python `str(session.out_dir / name)`).
pub fn image_path_for_json(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

pub fn sharpness(data: &[u8], width: i32, height: i32, channels: i32) -> Result<f64, String> {
    frame_sharpness(data, width, height, channels)
}

pub fn save_jpeg(
    dir: &Path,
    index: usize,
    bgr: &[u8],
    width: i32,
    height: i32,
    channels: i32,
) -> Result<PathBuf, String> {
    let path = dir.join(format!("img_{index:03}.jpg"));
    let jpeg = encode_jpeg_bytes(bgr, width, height, channels, SAVE_JPEG_QUALITY)?;
    std::fs::write(&path, &jpeg).map_err(|err| format!("write {}: {err}", path.display()))?;
    Ok(path)
}

fn write_pretty_json(dir: &Path, name: &str, payload: &Value) -> Result<(), String> {
    let path = dir.join(name);
    let text = serde_json::to_string_pretty(payload).map_err(|err| err.to_string())?;
    std::fs::write(&path, text).map_err(|err| format!("write {name}: {err}"))
}

fn remove_accept_artifacts(dir: &Path) {
    for name in ["camera.json", "camera.txt", "report.json"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

pub fn camera_json_value(
    config: &SessionConfig,
    images: &[CapturedImage],
    calib: &CalibResult,
) -> Value {
    let average = average_percent(images);
    json!({
        "accepted": true,
        "averageGrade": letter_grade(average),
        "averagePercent": round_places(average, 1),
        "cameraCalibrationData": {
            "cameraMatrix": calib.camera_matrix,
            "distCoeffs": calib.dist_coeffs,
        },
        "markerMm": config.marker_mm,
        "meanReprojectionError": mean_reprojection_error(images).map(|e| round_places(e, 4)),
        "overallReprojectionError": round_places(calib.overall_rms, 4),
        "scoreTarget": config.score_target,
        "scoreTargetGrade": letter_grade(config.score_target),
        "squareMm": config.square_mm,
    })
}

pub fn camera_txt_content(calib: &CalibResult) -> Result<String, String> {
    if calib.dist_coeffs.len() < 5 {
        return Err(format!(
            "distCoeffs need at least 5 values, got {}",
            calib.dist_coeffs.len()
        ));
    }
    let m = calib.camera_matrix;
    let d = &calib.dist_coeffs;
    Ok(format!(
        "{} 0.0 {}\n0.0 {} {}\n0.0 0.0 1.0\n{}\n{}\n{}\n{}\n{}\n",
        m[0][0], m[0][2], m[1][1], m[1][2], d[0], d[1], d[2], d[3], d[4]
    ))
}

pub fn report_json_value(config: &SessionConfig, images: &[CapturedImage]) -> Value {
    let average = average_percent(images);
    json!({
        "averagePercent": round_places(average, 1),
        "averageGrade": letter_grade(average),
        "scoreTarget": config.score_target,
        "images": images.iter().map(|image| json!({
            "path": image.path,
            "percent": round_places(image.percent, 1),
            "grade": letter_grade(image.percent),
            "reprojectionError": image.reprojection_error.map(|e| round_places(e, 4)),
        })).collect::<Vec<_>>(),
    })
}

/// Write `camera.json`, `camera.txt`, and `report.json`. On any failure, remove partial writes.
pub fn write_accept_artifacts(
    dir: &Path,
    config: &SessionConfig,
    shots: &[Shot],
    calib: &CalibResult,
) -> Result<(), String> {
    let images: Vec<CapturedImage> = shots.iter().map(captured_from_shot).collect();
    let camera_json = camera_json_value(config, &images, calib);
    let camera_txt = camera_txt_content(calib)?;
    let report_json = report_json_value(config, &images);

    if let Err(err) = write_pretty_json(dir, "camera.json", &camera_json) {
        remove_accept_artifacts(dir);
        return Err(err);
    }
    let txt_path = dir.join("camera.txt");
    if let Err(err) = std::fs::write(&txt_path, camera_txt) {
        remove_accept_artifacts(dir);
        return Err(format!("write camera.txt: {err}"));
    }
    if let Err(err) = write_pretty_json(dir, "report.json", &report_json) {
        remove_accept_artifacts(dir);
        return Err(err);
    }
    Ok(())
}

pub fn delete_image_file(path: &str) {
    if path.is_empty() {
        return;
    }
    let _ = std::fs::remove_file(path);
}

pub fn captured_from_shot(shot: &Shot) -> CapturedImage {
    let percent = match shot.reproj {
        Some(_) => session_percent(shot.quality, shot.diversity, shot.reproj),
        None => preview_percent(shot.quality, shot.diversity),
    };
    CapturedImage {
        path: shot.path.clone(),
        n_corners: shot.corners.len(),
        quality: shot.quality,
        diversity: shot.diversity,
        percent,
        reprojection_error: shot.reproj,
        feature: shot.feature,
    }
}

pub fn can_autosave(
    saveable: bool,
    elapsed_since_last: Option<Duration>,
    saved_count: usize,
    count_target: usize,
) -> bool {
    if !saveable || saved_count >= count_target {
        return false;
    }
    match elapsed_since_last {
        None => true,
        Some(elapsed) => elapsed >= SAVE_COOLDOWN,
    }
}

pub fn letter_grade(percent: f64) -> &'static str {
    if percent >= 90.0 {
        "A"
    } else if percent >= 80.0 {
        "B"
    } else if percent >= 70.0 {
        "C"
    } else if percent >= 60.0 {
        "D"
    } else {
        "F"
    }
}

pub fn format_grade(percent: f64) -> String {
    format!("{} {:.0}", letter_grade(percent), percent)
}

fn round_places(value: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (value * scale).round() / scale
}

fn mean_reprojection_error(images: &[CapturedImage]) -> Option<f64> {
    let errors: Vec<f64> = images
        .iter()
        .filter_map(|image| image.reprojection_error)
        .collect();
    if errors.is_empty() {
        None
    } else {
        Some(errors.iter().sum::<f64>() / errors.len() as f64)
    }
}

fn average_percent(images: &[CapturedImage]) -> f64 {
    if images.is_empty() {
        return 0.0;
    }
    let n = images.len() as f64;
    let quality = images.iter().map(|image| image.quality).sum::<f64>() / n;
    let diversity = images.iter().map(|image| image.diversity).sum::<f64>() / n;
    session_percent(quality, diversity, mean_reprojection_error(images))
}
