//! Autosave unmarked frames and `session.json` before the count target.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::camera_open::encode_jpeg_bytes;
use crate::detect::sharpness as frame_sharpness;
use crate::score::session_percent;

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
    format!("无法保存：{err}")
}

/// Absolute path for `session.json` `images[].path` (matches Python `str(session.out_dir / name)`).
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

pub fn write_session_json(dir: &Path, payload: &Value) -> Result<(), String> {
    let path = dir.join("session.json");
    let text = serde_json::to_string_pretty(payload).map_err(|err| err.to_string())?;
    std::fs::write(&path, text).map_err(|err| format!("write session.json: {err}"))
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

pub fn session_json_value(
    config: &SessionConfig,
    images: &[CapturedImage],
    accepted: bool,
) -> Value {
    let average = average_percent(images);
    json!({
        "countTarget": config.count_target,
        "scoreTarget": config.score_target,
        "scoreTargetGrade": letter_grade(config.score_target),
        "squareMm": config.square_mm,
        "markerMm": config.marker_mm,
        "averagePercent": round_places(average, 1),
        "averageGrade": letter_grade(average),
        "meanReprojectionError": mean_reprojection_error(images).map(|e| round_places(e, 4)),
        "imageCount": images.len(),
        "images": images.iter().map(|image| json!({
            "path": image.path,
            "nCorners": image.n_corners,
            "quality": round_places(image.quality, 4),
            "diversity": round_places(image.diversity, 4),
            "percent": round_places(image.percent, 1),
            "grade": letter_grade(image.percent),
            "reprojectionError": image.reprojection_error.map(|e| round_places(e, 4)),
        })).collect::<Vec<_>>(),
        "accepted": accepted,
    })
}
