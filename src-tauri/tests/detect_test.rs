use charuco_capture_app_lib::detect::{
    detect_charuco, frame_hint, generate_board_image, make_board, Detected, DEFAULT_MARKER_M,
    DEFAULT_SQUARE_M, MIN_CORNERS, NO_BOARD_HINT,
};

fn default_board() -> charuco_capture_app_lib::detect::CharucoBoard {
    make_board(DEFAULT_SQUARE_M, DEFAULT_MARKER_M).expect("make_board")
}

#[test]
fn make_board_rejects_invalid_sizes() {
    assert!(make_board(0.0, DEFAULT_MARKER_M).is_err());
    assert!(make_board(DEFAULT_SQUARE_M, 0.0).is_err());
    assert!(make_board(0.01, 0.02).is_err());
}

#[test]
fn make_board_from_mm_rejects_marker_not_smaller_than_square() {
    let err = match charuco_capture_app_lib::detect::make_board_from_mm(20.0, 20.0) {
        Err(err) => err,
        Ok(_) => panic!("marker == square must fail"),
    };
    assert!(err.contains("无法创建标定板"), "{err}");
    assert!(err.contains("Marker"), "{err}");
    assert!(charuco_capture_app_lib::detect::make_board_from_mm(15.0, 20.0).is_err());
    assert!(charuco_capture_app_lib::detect::make_board_from_mm(20.0, 15.0).is_ok());
}

#[test]
fn blank_image_has_no_board() {
    let board = default_board();
    let width = 320;
    let height = 240;
    let data = vec![180u8; (width * height) as usize];
    let found = detect_charuco(&data, width, height, 1, &board).expect("detect");
    assert!(found.is_none());
    assert_eq!(frame_hint(0), NO_BOARD_HINT);
}

#[test]
fn generated_8x6_board_detects_at_least_min_corners() {
    let board = default_board();
    let (data, width, height, channels) =
        generate_board_image(&board, 960, 1280).expect("generateImage");
    assert!(width >= 32 && height >= 32);
    assert!(channels == 1 || channels == 3);

    let found = detect_charuco(&data, width, height, channels, &board)
        .expect("detect generated board")
        .expect("synthetic 8x6 DICT_4X4_50 board must be detected");

    assert!(
        found.corners.len() >= MIN_CORNERS,
        "n_corners={} markers={} (need >= {MIN_CORNERS})",
        found.corners.len(),
        found.marker_corners.len()
    );
    assert_eq!(found.ids.len(), found.corners.len());
    assert!(!found.marker_corners.is_empty());
    assert_n_corners_in_frame(&found, width, height);
    assert_eq!(frame_hint(found.corners.len()), "detected");
}

fn assert_n_corners_in_frame(detected: &Detected, width: i32, height: i32) {
    for [x, y] in &detected.corners {
        assert!(*x >= 0.0 && *x <= width as f32);
        assert!(*y >= 0.0 && *y <= height as f32);
    }
}
