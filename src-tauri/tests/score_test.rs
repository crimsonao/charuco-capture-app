use charuco_capture_app_lib::score::{reproj_unit, session_percent, REPROJ_GOOD};

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
