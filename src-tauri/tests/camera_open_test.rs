use std::time::{Duration, Instant};

use charuco_capture_app_lib::camera::{
    cache_msmf_names, clear_msmf_names_cache, clear_open_success_cache, fourcc_attempts,
    fourcc_u32, frame_has_image, last_successful_open, list_msmf_names, match_device_index,
    msmf_names_for_open, open_attempt_timed_out, open_attempts_for_mode, open_budget,
    open_plan_steps, preferred_backend_for_open, prioritize_backend_attempts,
    prioritize_fourcc_attempts, remember_successful_open, warmup_read_tries, OpenRequest,
    OpenStep, SuccessfulOpen,
};

fn ocal4_720p_yuy2() -> OpenRequest {
    OpenRequest {
        device_name: "ocal4".into(),
        dshow_index: 1,
        width: 1280,
        height: 720,
        fourcc: "YUY2".into(),
        fps: 30,
    }
}

#[test]
fn match_device_index_exact_is_case_insensitive() {
    let names = vec!["Integrated Webcam".into(), "ocal4".into()];
    assert_eq!(match_device_index("ocal4", &names), Some(1));
    assert_eq!(match_device_index("OCAL4", &names), Some(1));
    assert_eq!(match_device_index("Integrated Webcam", &names), Some(0));
}

#[test]
fn match_device_index_uses_msmf_order_not_dshow_index() {
    // Production lesson: ocal4 is MSMF 0 / DSHOW 1. Never treat DSHOW index as MSMF.
    let msmf_names = vec!["ocal4".into(), "Integrated Webcam".into()];
    assert_eq!(match_device_index("ocal4", &msmf_names), Some(0));
    assert_ne!(match_device_index("ocal4", &msmf_names), Some(1));
}

#[test]
fn match_device_index_rejects_empty_and_unknown() {
    assert_eq!(match_device_index("", &["ocal4".into()]), None);
    assert_eq!(match_device_index("   ", &["ocal4".into()]), None);
    assert_eq!(match_device_index("no-such-cam", &["ocal4".into()]), None);
}

#[test]
fn match_device_index_substring_fallback() {
    let names = vec!["USB ocal4 Camera".into()];
    assert_eq!(match_device_index("ocal4", &names), Some(0));
}

#[test]
fn open_attempts_msmf_by_friendly_name_then_dshow_index() {
    let req = ocal4_720p_yuy2();
    let msmf_names = vec!["ocal4".into(), "Integrated Webcam".into()];
    let attempts = open_attempts_for_mode(&req, &msmf_names);
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].backend, "DSHOW");
    assert_eq!(attempts[0].index, 1);
    assert_eq!(attempts[1].backend, "MSMF");
    assert_eq!(attempts[1].index, 0);
    assert!(
        attempts
            .iter()
            .all(|attempt| !(attempt.backend == "MSMF" && attempt.index == req.dshow_index)),
        "must never VideoCapture(dshow_index, CAP_MSMF)"
    );
}

#[test]
fn open_attempts_skips_msmf_when_name_is_missing() {
    let req = ocal4_720p_yuy2();
    let attempts = open_attempts_for_mode(&req, &["Integrated Webcam".into()]);
    assert!(
        attempts.iter().all(|attempt| attempt.backend != "MSMF"),
        "name miss must not fall back to MSMF with DirectShow index"
    );
    assert_eq!(attempts[0].backend, "DSHOW");
    assert_eq!(attempts[0].index, 1);
}

#[test]
fn yuy2_retries_mjpg() {
    assert_eq!(
        fourcc_attempts("YUY2"),
        vec!["YUY2".to_string(), "MJPG".to_string()]
    );
    assert_eq!(fourcc_attempts("MJPG"), vec!["MJPG".to_string()]);
}

#[test]
fn black_frame_mean_below_four_is_rejected() {
    assert!(!frame_has_image(3.9, 1280, 720));
    assert!(frame_has_image(4.0, 1280, 720));
    assert!(!frame_has_image(100.0, 8, 8));
    assert!(!frame_has_image(50.0, 0, 720));
}

#[test]
fn list_msmf_names_returns_ok() {
    let names = list_msmf_names().expect("MSMF enum should return Ok");
    for (index, name) in names.iter().enumerate() {
        println!("msmf {index}: {name}");
        assert!(!name.trim().is_empty());
    }
}

#[test]
fn fourcc_u32_is_little_endian_fourcc() {
    assert_eq!(fourcc_u32("MJPG"), Some(0x4750_4A4D));
    assert_eq!(fourcc_u32("YUY2"), Some(0x3259_5559));
    assert_eq!(fourcc_u32("auto"), None);
    assert_eq!(fourcc_u32("MJP"), None);
}

#[test]
fn open_attempt_times_out_on_past_deadline() {
    assert!(open_attempt_timed_out(
        Instant::now() - Duration::from_secs(1)
    ));
    assert!(!open_attempt_timed_out(
        Instant::now() + Duration::from_secs(8)
    ));
}

#[test]
fn camera_disconnect_hint_is_user_visible() {
    use charuco_capture_app_lib::camera_open::CAMERA_DISCONNECT_HINT;
    assert_eq!(CAMERA_DISCONNECT_HINT, "摄像头断开");
}

#[test]
fn prepare_preview_session_rejects_invalid_board_params() {
    use charuco_capture_app_lib::camera_open::{prepare_preview_session, SessionParams};
    let err = prepare_preview_session(Some(SessionParams {
        count_target: Some(15),
        score_target: Some(80.0),
        square_mm: Some(15.0),
        marker_mm: Some(20.0),
        out_root: None,
    }))
    .unwrap_err();
    assert!(err.contains("无法创建标定板"), "{err}");
    assert!(err.contains("Marker"), "{err}");
}

#[test]
fn prepare_preview_session_accepts_default_board() {
    use charuco_capture_app_lib::camera_open::prepare_preview_session;
    let config = prepare_preview_session(None).expect("default 20/15 mm board");
    assert_eq!(config.square_mm, 20.0);
    assert_eq!(config.marker_mm, 15.0);
}

#[test]
fn open_budget_is_shared_across_attempts() {
    assert_eq!(open_budget(), Duration::from_secs(10));
}

#[test]
fn preferred_backend_defaults_to_dshow() {
    assert_eq!(preferred_backend_for_open(None), "DSHOW");
    assert_eq!(
        preferred_backend_for_open(Some(&SuccessfulOpen {
            backend: "MSMF".into(),
            fourcc: "MJPG".into(),
        })),
        "MSMF"
    );
    assert_eq!(
        preferred_backend_for_open(Some(&SuccessfulOpen {
            backend: "OPENCV-MSMF".into(),
            fourcc: "MJPG".into(),
        })),
        "MSMF"
    );
}

#[test]
fn cold_open_plan_is_native_dshow_msmf_then_opencv() {
    assert_eq!(
        open_plan_steps("DSHOW"),
        vec![
            OpenStep::NativeDshow,
            OpenStep::NativeMsmf,
            OpenStep::OpenCv
        ]
    );
    assert_eq!(
        open_plan_steps("MSMF"),
        vec![
            OpenStep::NativeMsmf,
            OpenStep::NativeDshow,
            OpenStep::OpenCv
        ]
    );
}

#[test]
fn warmup_read_tries_are_reduced() {
    assert_eq!(warmup_read_tries("MSMF", 1280, 720), 2);
    assert_eq!(warmup_read_tries("DSHOW", 1280, 720), 2);
    assert_eq!(warmup_read_tries("MSMF", 1920, 1080), 3);
    assert_eq!(warmup_read_tries("DSHOW", 1920, 1080), 3);
}

#[test]
fn prioritize_backend_puts_preferred_first() {
    let req = ocal4_720p_yuy2();
    let msmf_names = vec!["ocal4".into(), "Integrated Webcam".into()];
    let attempts = open_attempts_for_mode(&req, &msmf_names);
    let ordered = prioritize_backend_attempts(attempts, Some("MSMF"));
    assert_eq!(ordered[0].backend, "MSMF");
    assert_eq!(ordered[1].backend, "DSHOW");
}

#[test]
fn prioritize_fourcc_puts_preferred_first() {
    let ordered = prioritize_fourcc_attempts(fourcc_attempts("YUY2"), Some("MJPG"));
    assert_eq!(ordered, vec!["MJPG".to_string(), "YUY2".to_string()]);
}

#[test]
fn remember_successful_open_is_recalled_by_device_name() {
    clear_open_success_cache();
    remember_successful_open(
        "ocal4",
        &SuccessfulOpen {
            backend: "DSHOW".into(),
            fourcc: "MJPG".into(),
        },
    );
    let recalled = last_successful_open("OCAL4").expect("cached success");
    assert_eq!(recalled.backend, "DSHOW");
    assert_eq!(recalled.fourcc, "MJPG");
    clear_open_success_cache();
}

#[test]
fn msmf_names_for_open_reuses_cache() {
    clear_msmf_names_cache();
    cache_msmf_names(vec!["cached-cam".into()]);
    let names = msmf_names_for_open();
    assert_eq!(names, vec!["cached-cam".to_string()]);
    clear_msmf_names_cache();
}
