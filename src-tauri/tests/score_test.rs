use charuco_capture_app_lib::score::{
    board_feature, feature_distance, preview_percent, reproj_unit, session_percent, REPROJ_GOOD,
};

const GOLDEN_TOLERANCE: f64 = 1e-5;

/// Golden vector from capture_score.py board_feature (8 corners, 1280x720).
const GOLDEN_CORNERS_8: [[f32; 2]; 8] = [
    [320.0, 180.0],
    [480.0, 175.0],
    [640.0, 170.0],
    [800.0, 165.0],
    [310.0, 360.0],
    [470.0, 355.0],
    [630.0, 350.0],
    [790.0, 345.0],
];

const GOLDEN_FEATURE_8: [f32; 6] = [
    0.43359375,
    0.3645833432674408,
    0.34105542302131653,
    0.385455459356308,
    0.9903886318206787,
    0.5,
];

/// Second board for feature_distance golden pair.
const GOLDEN_CORNERS_8_B: [[f32; 2]; 8] = [
    [400.0, 200.0],
    [560.0, 195.0],
    [720.0, 190.0],
    [880.0, 185.0],
    [390.0, 380.0],
    [550.0, 375.0],
    [710.0, 370.0],
    [870.0, 365.0],
];

const GOLDEN_FEATURE_DISTANCE: f64 = 0.054715871810913086;

/// 3-point fallback (n < 3 PCA path skipped in Python).
const GOLDEN_CORNERS_3: [[f32; 2]; 3] = [[400.0, 300.0], [600.0, 290.0], [800.0, 280.0]];

const GOLDEN_FEATURE_3: [f32; 6] = [
    0.46875,
    0.4027777910232544,
    0.27270761132240295,
    4.762422012305478e-9,
    0.9920488595962524,
    0.5,
];

fn assert_feature_close(actual: [f32; 6], expected: [f32; 6]) {
    for i in 0..6 {
        let delta = (f64::from(actual[i]) - f64::from(expected[i])).abs();
        assert!(
            delta < GOLDEN_TOLERANCE,
            "feature[{i}]: got {} expected {} (delta {delta})",
            actual[i],
            expected[i]
        );
    }
}

#[test]
fn board_feature_matches_python_golden_8_corners() {
    let actual = board_feature(&GOLDEN_CORNERS_8, 1280, 720);
    assert_feature_close(actual, GOLDEN_FEATURE_8);
}

#[test]
fn board_feature_matches_python_golden_3_corners() {
    let actual = board_feature(&GOLDEN_CORNERS_3, 1280, 720);
    assert_feature_close(actual, GOLDEN_FEATURE_3);
}

#[test]
fn feature_distance_matches_python_golden() {
    let feat_a = board_feature(&GOLDEN_CORNERS_8, 1280, 720);
    let feat_b = board_feature(&GOLDEN_CORNERS_8_B, 1280, 720);
    let dist = feature_distance(&feat_a, &feat_b);
    assert!(
        (dist - GOLDEN_FEATURE_DISTANCE).abs() < GOLDEN_TOLERANCE,
        "feature_distance: got {dist} expected {GOLDEN_FEATURE_DISTANCE}"
    );
}

#[test]
fn preview_percent_clips_out_of_range_inputs() {
    assert!((preview_percent(1.2, 1.2) - 100.0).abs() < 1e-6);
}

#[test]
fn reproj_full_at_good() {
    assert_eq!(reproj_unit(0.30), 1.0);
    assert_eq!(reproj_unit(REPROJ_GOOD), 1.0);
}

#[test]
fn reproj_zero_at_limit() {
    assert_eq!(reproj_unit(1.00), 0.0);
}

#[test]
fn reproj_midpoint() {
    let mid = reproj_unit(0.70);
    assert!((mid - 0.5).abs() < 1e-6);
}

#[test]
fn session_score_good_reproj_can_reach_b() {
    let s = session_percent(1.0, 1.0, Some(0.40));
    assert!(s >= 80.0);
}

#[test]
fn session_score_bad_reproj_is_lower() {
    let good = session_percent(1.0, 1.0, Some(0.40));
    let weak = session_percent(1.0, 1.0, Some(0.90));
    assert!(weak < good);
}
