use charuco_capture_app_lib::calib::{should_replace, worst_id, ScoredShot};

#[test]
fn does_not_replace_when_trial_worse() {
    let set = vec![
        ScoredShot {
            id: 1,
            quality: 1.0,
            diversity: 1.0,
            reproj: 0.45,
        },
        ScoredShot {
            id: 2,
            quality: 0.9,
            diversity: 0.8,
            reproj: 0.80,
        },
    ];
    assert!(!should_replace(&set, 0.9, 0.8, Some(0.95)));
}

#[test]
fn replaces_when_trial_not_worse() {
    let set = vec![
        ScoredShot {
            id: 1,
            quality: 1.0,
            diversity: 1.0,
            reproj: 0.45,
        },
        ScoredShot {
            id: 2,
            quality: 0.9,
            diversity: 0.8,
            reproj: 0.80,
        },
    ];
    assert!(should_replace(&set, 1.0, 1.0, Some(0.42)));
}

#[test]
fn worst_is_highest_reproj_low_score() {
    let set = vec![
        ScoredShot {
            id: 1,
            quality: 1.0,
            diversity: 1.0,
            reproj: 0.40,
        },
        ScoredShot {
            id: 2,
            quality: 1.0,
            diversity: 1.0,
            reproj: 0.90,
        },
    ];
    assert_eq!(worst_id(&set), Some(2));
}
