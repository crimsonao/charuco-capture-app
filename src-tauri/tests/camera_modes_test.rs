use charuco_capture_app_lib::camera::{
    collect_discrete_modes, discrete_fourcc, is_com_init_success, list_dshow_modes,
    normalize_fourcc, prefer_larger_modes, StreamCapSlot,
};

#[test]
fn normalize_fourcc_aliases_match_python() {
    assert_eq!(normalize_fourcc("MJPG"), "MJPG");
    assert_eq!(normalize_fourcc("mjpeg"), "MJPG");
    assert_eq!(normalize_fourcc("YUYV"), "YUY2");
    assert_eq!(normalize_fourcc("YUY2"), "YUY2");
    assert_eq!(normalize_fourcc("NV12"), "NV12");
    assert_eq!(normalize_fourcc("RGB24"), "auto");
    assert_eq!(normalize_fourcc("RGB32"), "auto");
    assert_eq!(normalize_fourcc("ARGB32"), "auto");
    assert_eq!(normalize_fourcc("RGB555"), "auto");
    assert_eq!(normalize_fourcc("RGB565"), "auto");
    assert_eq!(normalize_fourcc("I420"), "I420");
    assert_eq!(normalize_fourcc("????"), "auto");
}

#[test]
fn com_init_succeeds_on_s_ok_and_s_false() {
    assert!(is_com_init_success(0), "S_OK");
    assert!(is_com_init_success(1), "S_FALSE already initialized");
    assert!(!is_com_init_success(-2147467259)); // E_FAIL 0x80004005
    assert!(!is_com_init_success(-2147221232)); // CO_E_NOTINITIALIZED-ish failure
}

#[test]
fn discrete_fourcc_skips_unknown_guid_and_auto() {
    assert_eq!(discrete_fourcc(""), None);
    assert_eq!(discrete_fourcc("   "), None);
    assert_eq!(discrete_fourcc("RGB24"), None);
    assert_eq!(discrete_fourcc("????"), None);
    assert_eq!(discrete_fourcc("MJPG"), Some("MJPG".into()));
    assert_eq!(discrete_fourcc("YUYV"), Some("YUY2".into()));
}

#[test]
fn collect_discrete_skips_unknown_guid_and_auto_fourcc() {
    fn slot(width: i32, height: i32, subtype_label: &str) -> StreamCapSlot {
        StreamCapSlot {
            format_is_video_info: true,
            header_width: width,
            header_height: height,
            subtype_label: subtype_label.into(),
            range_min_width: 0,
            range_min_height: 0,
            range_max_width: 0,
            range_max_height: 0,
        }
    }
    let modes = collect_discrete_modes(&[
        slot(1280, 720, ""),
        slot(1920, 1080, "RGB24"),
        slot(640, 480, "????"),
        slot(1280, 720, "MJPG"),
    ]);
    assert_eq!(modes, vec![(1280, 720, "MJPG".into())]);
}

#[test]
fn collect_discrete_ignores_videoinfo2_and_invalid_sizes() {
    let slots = vec![
        StreamCapSlot {
            format_is_video_info: false,
            header_width: 1920,
            header_height: 1080,
            subtype_label: "MJPG".into(),
            range_min_width: 0,
            range_min_height: 0,
            range_max_width: 0,
            range_max_height: 0,
        },
        StreamCapSlot {
            format_is_video_info: true,
            header_width: 0,
            header_height: 720,
            subtype_label: "MJPG".into(),
            range_min_width: 0,
            range_min_height: 0,
            range_max_width: 0,
            range_max_height: 0,
        },
        StreamCapSlot {
            format_is_video_info: true,
            header_width: 1280,
            header_height: -720,
            subtype_label: "YUY2".into(),
            range_min_width: 0,
            range_min_height: 0,
            range_max_width: 0,
            range_max_height: 0,
        },
    ];
    let modes = collect_discrete_modes(&slots);
    assert_eq!(modes, vec![(1280, 720, "YUY2".into())]);
}

#[test]
fn does_not_expand_stream_caps_min_max_range_to_4k() {
    let slots = vec![StreamCapSlot {
        format_is_video_info: true,
        header_width: 1280,
        header_height: 720,
        subtype_label: "MJPG".into(),
        range_min_width: 160,
        range_min_height: 120,
        range_max_width: 3840,
        range_max_height: 2160,
    }];
    let modes = collect_discrete_modes(&slots);
    assert_eq!(modes, vec![(1280, 720, "MJPG".into())]);
    assert!(
        !modes
            .iter()
            .any(|(w, h, _)| *w >= 3840 || *h >= 2160 || (*w == 160 && *h == 120)),
        "must not synthesize sizes from VIDEO_STREAM_CONFIG_CAPS ranges"
    );
}

#[test]
fn prefer_larger_drops_qvga_when_720p_exists() {
    let modes = vec![
        (320, 240, "MJPG".into()),
        (1280, 720, "MJPG".into()),
        (640, 480, "YUY2".into()),
    ];
    let kept = prefer_larger_modes(modes);
    assert_eq!(
        kept,
        vec![(1280, 720, "MJPG".into()), (640, 480, "YUY2".into())]
    );
}

#[test]
fn prefer_larger_keeps_small_modes_when_nothing_is_vga() {
    let modes = vec![(320, 240, "MJPG".into()), (160, 120, "YUY2".into())];
    assert_eq!(prefer_larger_modes(modes.clone()), modes);
}

#[test]
fn collect_discrete_dedups_same_size_and_fourcc() {
    let slot = StreamCapSlot {
        format_is_video_info: true,
        header_width: 1280,
        header_height: 720,
        subtype_label: "MJPEG".into(),
        range_min_width: 0,
        range_min_height: 0,
        range_max_width: 0,
        range_max_height: 0,
    };
    let modes = collect_discrete_modes(&[slot.clone(), slot]);
    assert_eq!(modes, vec![(1280, 720, "MJPG".into())]);
}

#[test]
fn list_dshow_modes_returns_ok_even_without_cameras() {
    let modes = list_dshow_modes().expect("DirectShow enum should return Ok");
    for mode in &modes {
        println!(
            "dshow {} idx={} {}x{} {}",
            mode.device_name, mode.dshow_index, mode.width, mode.height, mode.fourcc
        );
        assert!(mode.width > 0);
        assert!(mode.height > 0);
        assert!(!mode.device_name.trim().is_empty());
        assert!(!mode.fourcc.is_empty());
        assert_ne!(
            mode.fourcc, "auto",
            "unknown/non-fourcc GUIDs must be skipped, not listed as auto"
        );
        assert!(mode.dshow_index >= 0);
    }
}
