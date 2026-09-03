use std::path::{Path, PathBuf};

use charuco_capture_app_lib::calib::{CalibResult, Shot};
use charuco_capture_app_lib::capture_flow::{CaptureFlow, CapturePhase};
use charuco_capture_app_lib::session::{image_path_for_json, SessionConfig};
use serde_json::Value;

fn temp_root(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "charuco-flow-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp root");
    dir
}

fn start_flow(label: &str) -> (CaptureFlow, PathBuf) {
    let root = temp_root(label);
    let flow = CaptureFlow::start(SessionConfig {
        count_target: 3,
        score_target: 99.0,
        square_mm: 20.0,
        marker_mm: 15.0,
        out_root: root,
    });
    let dir = flow.dir.clone().expect("session dir");
    (flow, dir)
}

fn block_as_directory(path: &Path) {
    if path.is_file() {
        std::fs::remove_file(path).expect("remove file before blocking");
    }
    if !path.exists() {
        std::fs::create_dir(path).expect("block path as directory");
    }
}

fn write_dummy_jpeg(dir: &Path, name: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, [0xFF, 0xD8, 0xFF, 0xD9]).expect("dummy jpeg");
    image_path_for_json(&path)
}

fn shot(id: u32, quality: f64, diversity: f64, reproj: f64, path: String) -> Shot {
    Shot {
        id,
        quality,
        diversity,
        reproj: Some(reproj),
        feature: [id as f32 * 0.1; 6],
        corners: vec![[0.0, 0.0]; 8],
        ids: vec![0, 1, 2, 3, 4, 5, 6, 7],
        path,
    }
}

fn calib(per_image: Vec<f64>) -> CalibResult {
    let n = per_image.len() as f64;
    let mean_reproj = if n == 0.0 {
        0.0
    } else {
        per_image.iter().sum::<f64>() / n
    };
    CalibResult {
        camera_matrix: [[600.0, 0.0, 320.0], [0.0, 600.0, 240.0], [0.0, 0.0, 1.0]],
        dist_coeffs: vec![0.0; 5],
        overall_rms: mean_reproj,
        per_image,
        mean_reproj,
    }
}

fn jpg_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("jpg"))
                .unwrap_or(false)
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn read_session_json(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("session.json")).expect("session.json"))
        .expect("parse session.json")
}

fn image_file_names(payload: &Value) -> Vec<String> {
    payload["images"]
        .as_array()
        .expect("images")
        .iter()
        .map(|image| {
            PathBuf::from(image["path"].as_str().expect("path"))
                .file_name()
                .and_then(|name| name.to_str())
                .expect("utf8 name")
                .to_string()
        })
        .collect()
}

fn seed_three_shots(flow: &mut CaptureFlow, dir: &Path) {
    let p1 = write_dummy_jpeg(dir, "img_001.jpg");
    let p2 = write_dummy_jpeg(dir, "img_002.jpg");
    let p3 = write_dummy_jpeg(dir, "img_003.jpg");
    flow.shots = vec![
        shot(1, 1.0, 1.0, 0.45, p1),
        shot(2, 1.0, 1.0, 0.45, p2),
        shot(3, 0.9, 0.8, 0.80, p3),
    ];
    flow.phase = CapturePhase::Improve;
    flow.image_size = Some((1280, 720));
    flow.last_calib = Some(calib(vec![0.45, 0.45, 0.80]));
    flow.persist(false).expect("seed persist");
}

#[test]
fn persist_write_failure_sets_hint_and_keeps_accepted_false() {
    let (mut flow, dir) = start_flow("persist-fail");
    block_as_directory(&dir.join("session.json"));
    let err = flow.persist(false).expect_err("json write must fail");
    assert!(err.contains("session.json"), "{err}");
    assert!(!flow.accepted);
    let hint = flow.hint_prefix().expect("user-visible hint");
    assert!(hint.contains("无法保存"), "{hint}");
    assert!(hint.contains("检查目录权限"), "{hint}");
    assert!(!dir.join("accepted.json").is_file());
}

#[test]
fn accept_session_json_write_failure_does_not_emit_done() {
    let (mut flow, dir) = start_flow("accept-session-fail");
    seed_three_shots(&mut flow, &dir);
    block_as_directory(&dir.join("session.json"));
    let (hint, done) = flow.accept_session("已达标".into());
    assert!(done.is_none(), "must not enter Done when json write fails");
    assert!(!flow.accepted);
    assert_eq!(flow.phase, CapturePhase::Improve);
    assert!(hint.contains("无法保存"), "{hint}");
    assert!(hint.contains("检查目录权限"), "{hint}");
    assert!(!dir.join("accepted.json").is_file());
}

#[test]
fn accept_reverts_session_json_when_accepted_json_fails() {
    let (mut flow, dir) = start_flow("accept-accepted-fail");
    seed_three_shots(&mut flow, &dir);
    block_as_directory(&dir.join("accepted.json"));
    let (hint, done) = flow.accept_session("已达标".into());
    assert!(done.is_none());
    assert!(!flow.accepted);
    assert!(hint.contains("无法保存"), "{hint}");
    let payload = read_session_json(&dir);
    assert_eq!(payload["accepted"], false);
    assert!(!dir.join("accepted.json").is_file());
}

#[test]
fn last_place_file_survives_worse_trial_then_replaced_on_better() {
    let (mut flow, dir) = start_flow("trial-disk");
    seed_three_shots(&mut flow, &dir);
    let original = jpg_names(&dir);
    assert_eq!(
        original,
        vec![
            "img_001.jpg".to_string(),
            "img_002.jpg".to_string(),
            "img_003.jpg".to_string()
        ]
    );

    let worse_path = write_dummy_jpeg(&dir, "img_004.jpg");
    let worse = shot(4, 0.9, 0.8, 0.0, worse_path);
    let (hint, done) = flow.conclude_trial(worse, Ok(calib(vec![0.95, 0.95, 0.95])));
    assert!(done.is_none());
    assert!(hint.contains("试探分未提升"), "{hint}");
    assert_eq!(jpg_names(&dir), original, "worse trial must not drop last-place files");
    assert!(!dir.join("img_004.jpg").exists());
    assert_eq!(flow.shots.len(), 3);
    assert!(flow.shots.iter().any(|item| item.id == 3));

    let better_path = write_dummy_jpeg(&dir, "img_005.jpg");
    let better = shot(5, 1.0, 1.0, 0.0, better_path);
    let (hint, done) = flow.conclude_trial(better, Ok(calib(vec![0.42, 0.42, 0.42])));
    assert!(done.is_none(), "{hint}");
    assert!(!flow.accepted);
    let names = jpg_names(&dir);
    assert!(
        names.contains(&"img_001.jpg".to_string()) && names.contains(&"img_002.jpg".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"img_005.jpg".to_string()),
        "better candidate must remain: {names:?}"
    );
    assert!(
        !names.contains(&"img_003.jpg".to_string()),
        "worst file must be gone after better replace: {names:?}"
    );
    assert!(!flow.shots.iter().any(|item| item.id == 3));
    assert!(flow.shots.iter().any(|item| item.id == 5));

    let payload = read_session_json(&dir);
    assert_eq!(payload["accepted"], false);
    let json_names = image_file_names(&payload);
    assert!(json_names.contains(&"img_005.jpg".to_string()), "{json_names:?}");
    assert!(!json_names.contains(&"img_003.jpg".to_string()), "{json_names:?}");
}
