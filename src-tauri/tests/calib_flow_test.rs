use charuco_capture_app_lib::calib::{
    apply_replace, calibrate, decide_replace, evaluate_after_calib, scored_from_shots,
    trial_peer_features, AfterCalib, CalibResult, Shot,
};
use charuco_capture_app_lib::calib::{should_replace, worst_id};
use charuco_capture_app_lib::detect::{
    detect_charuco, generate_board_image, make_board, DEFAULT_MARKER_M, DEFAULT_SQUARE_M,
};
use charuco_capture_app_lib::score::{session_percent, REPROJ_LIMIT};
use charuco_capture_app_lib::session::{SessionConfig, MIN_COUNT_TARGET};

fn shot(id: u32, quality: f64, diversity: f64, feature: [f32; 6]) -> Shot {
    Shot {
        id,
        quality,
        diversity,
        reproj: None,
        feature,
        corners: vec![[0.0, 0.0]; 8],
        ids: vec![0, 1, 2, 3, 4, 5, 6, 7],
        path: format!("img_{id:03}.jpg"),
    }
}

fn result_with(per_image: Vec<f64>) -> CalibResult {
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

#[test]
fn culls_images_with_reproj_at_or_above_limit() {
    let mut shots = vec![
        shot(1, 1.0, 1.0, [0.1; 6]),
        shot(2, 1.0, 1.0, [0.2; 6]),
        shot(3, 1.0, 1.0, [0.3; 6]),
    ];
    let outcome = evaluate_after_calib(
        &mut shots,
        &result_with(vec![0.40, REPROJ_LIMIT, 1.20]),
        3,
        80.0,
    );
    assert_eq!(outcome, AfterCalib::NeedMore { culled: 2 });
    assert_eq!(shots.len(), 1);
    assert_eq!(shots[0].id, 1);
    assert_eq!(shots[0].reproj, Some(0.40));
}

#[test]
fn keeps_all_usable_when_below_score_target() {
    let mut shots = vec![
        shot(1, 1.0, 1.0, [0.1; 6]),
        shot(2, 1.0, 1.0, [0.2; 6]),
        shot(3, 0.8, 0.8, [0.3; 6]),
    ];
    let outcome = evaluate_after_calib(
        &mut shots,
        &result_with(vec![0.70, 0.80, 0.90]),
        3,
        80.0,
    );
    assert_eq!(outcome, AfterCalib::Improve);
    assert_eq!(shots.len(), 3, "must not delete worst before reshoot");
    assert!(shots.iter().any(|item| item.id == 3));
}

#[test]
fn accepted_when_count_score_and_mean_reproj_ok() {
    let mut shots = vec![
        shot(1, 1.0, 1.0, [0.1; 6]),
        shot(2, 1.0, 1.0, [0.2; 6]),
        shot(3, 1.0, 1.0, [0.3; 6]),
    ];
    let outcome = evaluate_after_calib(
        &mut shots,
        &result_with(vec![0.40, 0.40, 0.40]),
        3,
        80.0,
    );
    assert_eq!(outcome, AfterCalib::Accepted);
    assert_eq!(shots.len(), 3);
}

#[test]
fn not_accepted_when_mean_reproj_at_limit() {
    let mut shots = vec![
        shot(1, 1.0, 1.0, [0.1; 6]),
        shot(2, 1.0, 1.0, [0.2; 6]),
        shot(3, 1.0, 1.0, [0.3; 6]),
    ];
    let outcome = evaluate_after_calib(
        &mut shots,
        &result_with(vec![0.99, 0.99, 0.99]),
        3,
        80.0,
    );
    assert_ne!(outcome, AfterCalib::Accepted);
}

#[test]
fn worst_is_lowest_post_calib_composite() {
    let mut shots = vec![
        shot(1, 1.0, 1.0, [0.1; 6]),
        shot(2, 1.0, 1.0, [0.2; 6]),
        shot(3, 1.0, 1.0, [0.3; 6]),
    ];
    evaluate_after_calib(&mut shots, &result_with(vec![0.40, 0.90, 0.45]), 3, 99.0);
    let scored = scored_from_shots(&shots);
    assert_eq!(worst_id(&scored), Some(2));
    let peers = trial_peer_features(&shots);
    assert_eq!(peers.len(), 2);
    assert!(!peers.iter().any(|feat| feat == &[0.2; 6]));
}

#[test]
fn trial_none_does_not_replace() {
    let shots = vec![
        shot(1, 1.0, 1.0, [0.1; 6]),
        shot(2, 0.9, 0.8, [0.2; 6]),
    ];
    assert!(!decide_replace(&shots, 1.0, 1.0, None));
}

#[test]
fn replace_only_when_trial_session_score_not_lower() {
    let mut current = vec![
        shot(1, 1.0, 1.0, [0.1; 6]),
        shot(2, 0.9, 0.8, [0.2; 6]),
    ];
    evaluate_after_calib(&mut current, &result_with(vec![0.45, 0.80]), 2, 99.0);
    let scored = scored_from_shots(&current);
    assert!(!should_replace(&scored, 0.9, 0.8, Some(0.95)));
    assert!(!decide_replace(&current, 0.9, 0.8, Some(&result_with(vec![0.45, 0.95]))));

    let better = result_with(vec![0.45, 0.42]);
    assert!(decide_replace(&current, 1.0, 1.0, Some(&better)));
    let dropped = apply_replace(&mut current, shot(9, 1.0, 1.0, [0.9; 6]));
    assert_eq!(dropped.map(|item| item.id), Some(2));
    assert!(current.iter().any(|item| item.id == 9));
    assert!(!current.iter().any(|item| item.id == 2));
}

#[test]
fn setup_params_clamp_count_to_min_three() {
    let config = SessionConfig::from_setup(1, 80.0, 20.0, 15.0, std::path::PathBuf::from("."));
    assert_eq!(config.count_target, MIN_COUNT_TARGET);
    assert_eq!(config.score_target, 80.0);
    assert_eq!(config.square_mm, 20.0);
    assert_eq!(config.marker_mm, 15.0);
}

#[test]
fn session_percent_weights_match_spec() {
    let good = session_percent(1.0, 1.0, Some(0.40));
    let weak = session_percent(1.0, 1.0, Some(0.90));
    assert!(good >= 80.0);
    assert!(weak < good);
}

#[test]
fn synthetic_board_calibrate_returns_matrix_and_per_image() {
    let board = make_board(DEFAULT_SQUARE_M, DEFAULT_MARKER_M).expect("board");
    let (data, width, height, channels) =
        generate_board_image(&board, 960, 1280).expect("generateImage");
    let detected = detect_charuco(&data, width, height, channels, &board)
        .expect("detect")
        .expect("synthetic board");
    let views: Vec<Shot> = (1..=3)
        .map(|id| Shot {
            id,
            quality: 1.0,
            diversity: 1.0,
            reproj: None,
            feature: [0.0; 6],
            corners: detected.corners.clone(),
            ids: detected.ids.clone(),
            path: format!("img_{id:03}.jpg"),
        })
        .collect();
    let result = calibrate(&views, &board, (width, height)).expect("calibrate");
    assert_eq!(result.camera_matrix[2][2], 1.0);
    assert_eq!(result.per_image.len(), 3);
    assert!(result.mean_reproj.is_finite());
}
