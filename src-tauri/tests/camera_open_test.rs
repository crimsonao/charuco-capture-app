use charuco_capture_app_lib::camera::{
    fourcc_attempts, fourcc_u32, frame_has_image, list_msmf_names, match_device_index,
    open_attempts_for_mode, OpenRequest,
};

fn ocal4_720p_yuy2() -> OpenRequest {
    OpenRequest {
        device_name: "ocal4".into(),
        dshow_index: 1,
        width: 1280,
        height: 720,
        fourcc: "YUY2".into(),
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
    assert_eq!(attempts[0].backend, "MSMF");
    assert_eq!(attempts[0].index, 0);
    assert_eq!(attempts[1].backend, "DSHOW");
    assert_eq!(attempts[1].index, 1);
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
