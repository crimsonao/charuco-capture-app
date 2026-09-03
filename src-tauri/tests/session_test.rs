use std::time::Duration;

use charuco_capture_app_lib::detect::{
    detect_charuco, generate_board_image, make_board, DEFAULT_MARKER_M, DEFAULT_SQUARE_M,
    MIN_CORNERS,
};
use charuco_capture_app_lib::score::{evaluate_frame, SavedFeature, MIN_SHARPNESS};
use charuco_capture_app_lib::session::{
    can_autosave, default_output_root, save_jpeg, session_json_value, sharpness, start_session_dir,
    start_session_dir_named, write_session_json, CapturedImage, SessionConfig, SAVE_COOLDOWN,
    SAVE_JPEG_QUALITY,
};
use serde_json::Value;

fn temp_root(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "charuco-session-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp root");
    dir
}

fn rect(cx: f32, cy: f32, w: f32, h: f32, shear: f32) -> Vec<[f32; 2]> {
    let hw = w / 2.0;
    let hh = h / 2.0;
    let base = [
        [cx - hw, cy - hh],
        [cx + hw, cy - hh],
        [cx + hw + shear, cy + hh],
        [cx - hw + shear, cy + hh],
    ];
    let mut pts = Vec::with_capacity(12);
    pts.extend_from_slice(&base);
    pts.extend_from_slice(&base);
    pts.extend_from_slice(&base);
    pts
}

#[test]
fn start_session_dir_named_uses_session_prefix_and_stamp() {
    let root = temp_root("named");
    let dir = start_session_dir_named(&root, "20260903_124500").expect("mkdir");
    assert_eq!(
        dir.file_name().and_then(|n| n.to_str()),
        Some("session_20260903_124500")
    );
    assert!(dir.is_dir());
}

#[test]
fn start_session_dir_name_matches_python_pattern() {
    let root = temp_root("now");
    let dir = start_session_dir(&root).expect("mkdir");
    let name = dir.file_name().and_then(|n| n.to_str()).expect("utf8 name");
    assert!(
        name.starts_with("session_"),
        "expected session_ prefix, got {name}"
    );
    let stamp = &name["session_".len()..];
    assert_eq!(stamp.len(), 15, "{name}");
    assert_eq!(&stamp[8..9], "_");
    assert!(stamp[..8].chars().all(|c| c.is_ascii_digit()), "{name}");
    assert!(stamp[9..].chars().all(|c| c.is_ascii_digit()), "{name}");
    assert!(dir.is_dir());
}

#[test]
fn default_output_root_is_charuco_capture_under_desktop() {
    let root = default_output_root();
    assert_eq!(
        root.file_name().and_then(|n| n.to_str()),
        Some("ChArUcoCapture")
    );
}

#[test]
fn session_json_uses_python_field_names() {
    let config = SessionConfig {
        count_target: 15,
        score_target: 80.0,
        square_mm: 20.0,
        marker_mm: 15.0,
        out_root: std::path::PathBuf::from("unused"),
    };
    let images = vec![CapturedImage {
        path: "img_001.jpg".into(),
        n_corners: 20,
        quality: 0.9123,
        diversity: 1.0,
        percent: 87.65,
        reprojection_error: None,
        feature: [0.0; 6],
    }];
    let value = session_json_value(&config, &images, false);
    assert_eq!(value["countTarget"], 15);
    assert_eq!(value["scoreTarget"], 80.0);
    assert!(value.get("averagePercent").is_some());
    assert!(value["meanReprojectionError"].is_null());
    assert_eq!(value["accepted"], false);
    let img = &value["images"][0];
    assert!(img.get("quality").is_some());
    assert!(img.get("diversity").is_some());
    assert!(img.get("percent").is_some());
    assert!(img["reprojectionError"].is_null());
}

#[test]
fn cooldown_blocks_save_under_700ms() {
    assert!(can_autosave(true, None, 0, 15));
    assert!(!can_autosave(
        true,
        Some(Duration::from_millis(699)),
        1,
        15
    ));
    assert!(can_autosave(
        true,
        Some(Duration::from_millis(700)),
        1,
        15
    ));
    assert_eq!(SAVE_COOLDOWN, Duration::from_millis(700));
}

#[test]
fn autosave_stops_at_count_target() {
    assert!(!can_autosave(true, None, 15, 15));
    assert!(!can_autosave(false, None, 0, 15));
}

#[test]
fn pose_too_close_is_not_saveable() {
    let first = rect(640.0, 360.0, 400.0, 300.0, 0.0);
    let near = rect(642.0, 361.0, 402.0, 301.0, 0.0);
    let eval1 = evaluate_frame(&first, 1280, 720, 90.0, &[]);
    assert!(eval1.saveable, "{}", eval1.hint);
    let eval2 = evaluate_frame(
        &near,
        1280,
        720,
        90.0,
        &[SavedFeature {
            feature: eval1.feature,
        }],
    );
    assert!(!eval2.saveable, "{}", eval2.hint);
    assert!(eval2.hint.contains("姿态太接近"), "{}", eval2.hint);
}

#[test]
fn too_few_corners_is_not_saveable() {
    let corners = vec![[10.0, 10.0]; MIN_CORNERS - 1];
    let eval = evaluate_frame(&corners, 1280, 720, 90.0, &[]);
    assert!(!eval.saveable);
    assert!(eval.hint.contains("角点不足"), "{}", eval.hint);
}

#[test]
fn blurry_frame_is_not_saveable() {
    let corners = rect(640.0, 360.0, 400.0, 300.0, 0.0);
    let eval = evaluate_frame(&corners, 1280, 720, MIN_SHARPNESS - 1.0, &[]);
    assert!(!eval.saveable);
    assert!(eval.hint.contains("过糊"), "{}", eval.hint);
}

#[test]
fn synthetic_board_save_writes_unmarked_jpeg_and_session_json() {
    let root = temp_root("save");
    let dir = start_session_dir_named(&root, "20260903_120000").expect("session dir");
    let board = make_board(DEFAULT_SQUARE_M, DEFAULT_MARKER_M).expect("board");
    let (data, width, height, channels) =
        generate_board_image(&board, 960, 1280).expect("generateImage");
    let detected = detect_charuco(&data, width, height, channels, &board)
        .expect("detect")
        .expect("synthetic board");
    assert!(detected.corners.len() >= MIN_CORNERS);

    let sharp = sharpness(&data, width, height, channels).expect("laplacian");
    assert!(
        sharp >= MIN_SHARPNESS,
        "synthetic board should be sharp, got {sharp}"
    );

    let eval = evaluate_frame(&detected.corners, width, height, sharp, &[]);
    assert!(eval.saveable, "{}", eval.hint);

    let jpeg_path = save_jpeg(&dir, 1, &data, width, height, channels).expect("save jpeg");
    assert_eq!(
        jpeg_path.file_name().and_then(|n| n.to_str()),
        Some("img_001.jpg")
    );
    assert!(jpeg_path.is_file());
    let bytes = std::fs::read(&jpeg_path).expect("read jpeg");
    assert!(bytes.len() > 100);
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "JPEG SOI");
    assert_eq!(SAVE_JPEG_QUALITY, 95);

    let config = SessionConfig::default_capture();
    let images = vec![CapturedImage {
        path: jpeg_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        n_corners: detected.corners.len(),
        quality: eval.quality,
        diversity: eval.diversity,
        percent: eval.percent,
        reprojection_error: None,
        feature: eval.feature,
    }];
    write_session_json(&dir, &session_json_value(&config, &images, false)).expect("json");
    let json_path = dir.join("session.json");
    assert!(json_path.is_file());
    let parsed: Value = serde_json::from_str(&std::fs::read_to_string(json_path).unwrap()).unwrap();
    assert_eq!(parsed["accepted"], false);
    assert_eq!(parsed["countTarget"], 15);
    assert_eq!(parsed["scoreTarget"], 80.0);
    assert_eq!(parsed["images"][0]["reprojectionError"], Value::Null);
}
