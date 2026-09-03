use crate::detect::CharucoBoard;
use crate::score::{session_percent, REPROJ_LIMIT};

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredShot {
    pub id: u32,
    pub quality: f64,
    pub diversity: f64,
    pub reproj: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalibResult {
    pub camera_matrix: [[f64; 3]; 3],
    pub dist_coeffs: Vec<f64>,
    pub overall_rms: f64,
    pub per_image: Vec<f64>,
    pub mean_reproj: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    pub id: u32,
    pub quality: f64,
    pub diversity: f64,
    pub reproj: Option<f64>,
    pub feature: [f32; 6],
    pub corners: Vec<[f32; 2]>,
    pub ids: Vec<i32>,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterCalib {
    NeedMore { culled: usize },
    Improve,
    Accepted,
}

fn shot_score(shot: &ScoredShot) -> f64 {
    session_percent(shot.quality, shot.diversity, Some(shot.reproj))
}

pub fn worst_id(shots: &[ScoredShot]) -> Option<u32> {
    shots
        .iter()
        .min_by(|a, b| {
            shot_score(a)
                .partial_cmp(&shot_score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|shot| shot.id)
}

fn mean_quality(shots: &[ScoredShot]) -> f64 {
    if shots.is_empty() {
        return 0.0;
    }
    shots.iter().map(|s| s.quality).sum::<f64>() / shots.len() as f64
}

fn mean_diversity(shots: &[ScoredShot]) -> f64 {
    if shots.is_empty() {
        return 0.0;
    }
    shots.iter().map(|s| s.diversity).sum::<f64>() / shots.len() as f64
}

fn mean_reproj(shots: &[ScoredShot]) -> f64 {
    if shots.is_empty() {
        return 0.0;
    }
    shots.iter().map(|s| s.reproj).sum::<f64>() / shots.len() as f64
}

pub fn should_replace(
    current: &[ScoredShot],
    new_quality: f64,
    new_diversity: f64,
    trial_mean_reproj: Option<f64>,
) -> bool {
    let Some(trial_mean_reproj) = trial_mean_reproj else {
        return false;
    };
    if current.is_empty() {
        return false;
    }

    let current_score = session_percent(
        mean_quality(current),
        mean_diversity(current),
        Some(mean_reproj(current)),
    );

    let worst = worst_id(current).expect("current is non-empty");
    let trial_shots: Vec<&ScoredShot> = current.iter().filter(|s| s.id != worst).collect();

    let n_trial = trial_shots.len() + 1;
    let trial_mean_q =
        (trial_shots.iter().map(|s| s.quality).sum::<f64>() + new_quality) / n_trial as f64;
    let trial_mean_d = (trial_shots.iter().map(|s| s.diversity).sum::<f64>() + new_diversity)
        / n_trial as f64;

    let trial_score = session_percent(trial_mean_q, trial_mean_d, Some(trial_mean_reproj));

    trial_score >= current_score
}

pub fn scored_from_shots(shots: &[Shot]) -> Vec<ScoredShot> {
    shots
        .iter()
        .filter_map(|shot| {
            shot.reproj.map(|reproj| ScoredShot {
                id: shot.id,
                quality: shot.quality,
                diversity: shot.diversity,
                reproj,
            })
        })
        .collect()
}

pub fn mean_reproj_of_shots(shots: &[Shot]) -> Option<f64> {
    let errors: Vec<f64> = shots.iter().filter_map(|shot| shot.reproj).collect();
    if errors.is_empty() {
        None
    } else {
        Some(errors.iter().sum::<f64>() / errors.len() as f64)
    }
}

pub fn session_score_of_shots(shots: &[Shot]) -> f64 {
    if shots.is_empty() {
        return 0.0;
    }
    let n = shots.len() as f64;
    let quality = shots.iter().map(|shot| shot.quality).sum::<f64>() / n;
    let diversity = shots.iter().map(|shot| shot.diversity).sum::<f64>() / n;
    session_percent(quality, diversity, mean_reproj_of_shots(shots))
}

pub fn meets_target(shots: &[Shot], count_target: usize, score_target: f64) -> bool {
    if shots.len() < count_target {
        return false;
    }
    let Some(mean) = mean_reproj_of_shots(shots) else {
        return false;
    };
    if mean >= REPROJ_LIMIT {
        return false;
    }
    if shots
        .iter()
        .any(|shot| shot.reproj.is_some_and(|err| err >= REPROJ_LIMIT))
    {
        return false;
    }
    session_score_of_shots(shots) >= score_target
}

pub fn evaluate_after_calib(
    shots: &mut Vec<Shot>,
    result: &CalibResult,
    count_target: usize,
    score_target: f64,
) -> AfterCalib {
    for (shot, err) in shots.iter_mut().zip(result.per_image.iter()) {
        shot.reproj = Some(*err);
    }
    let before = shots.len();
    shots.retain(|shot| match shot.reproj {
        Some(err) => err < REPROJ_LIMIT,
        None => true,
    });
    let culled = before - shots.len();
    if shots.len() < count_target {
        return AfterCalib::NeedMore { culled };
    }
    if meets_target(shots, count_target, score_target) {
        AfterCalib::Accepted
    } else {
        AfterCalib::Improve
    }
}

pub fn trial_peer_features(shots: &[Shot]) -> Vec<[f32; 6]> {
    let scored = scored_from_shots(shots);
    let worst = worst_id(&scored);
    shots
        .iter()
        .filter(|shot| Some(shot.id) != worst)
        .map(|shot| shot.feature)
        .collect()
}

pub fn decide_replace(
    current: &[Shot],
    new_quality: f64,
    new_diversity: f64,
    trial: Option<&CalibResult>,
) -> bool {
    should_replace(
        &scored_from_shots(current),
        new_quality,
        new_diversity,
        trial.map(|result| result.mean_reproj),
    )
}

pub fn apply_replace(shots: &mut Vec<Shot>, new_shot: Shot) -> Option<Shot> {
    let worst = worst_id(&scored_from_shots(shots))?;
    let index = shots.iter().position(|shot| shot.id == worst)?;
    let dropped = shots.remove(index);
    shots.push(new_shot);
    Some(dropped)
}

pub fn calibrate(
    shots: &[Shot],
    board: &CharucoBoard,
    size: (i32, i32),
) -> Result<CalibResult, String> {
    if shots.len() < 3 {
        return Err("有效图像不足 3 张".into());
    }
    if size.0 < 16 || size.1 < 16 {
        return Err("未知图像尺寸".into());
    }
    let views: Vec<CvCalibView> = shots
        .iter()
        .map(|shot| CvCalibView {
            corners: shot.corners.as_ptr() as *const f32,
            ids: shot.ids.as_ptr(),
            n_corners: shot.corners.len() as i32,
        })
        .collect();
    let mut raw = CvCalibResult {
        camera_matrix: [0.0; 9],
        dist_coeffs: [0.0; 8],
        n_dist: 0,
        overall_rms: 0.0,
        per_image: std::ptr::null_mut(),
        n_images: 0,
    };
    let ok = unsafe {
        cvcharuco_calibrate(
            board.as_ptr(),
            size.0,
            size.1,
            views.as_ptr(),
            views.len() as i32,
            &mut raw,
        )
    };
    if ok == 0 {
        unsafe { cvcharuco_calibrate_free(&mut raw) };
        return Err("calibrateCameraCharuco failed".into());
    }
    let per_image = if raw.per_image.is_null() || raw.n_images <= 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(raw.per_image, raw.n_images as usize) }.to_vec()
    };
    let n_dist = raw.n_dist.clamp(0, 8) as usize;
    let dist_coeffs = raw.dist_coeffs[..n_dist].to_vec();
    let camera_matrix = [
        [raw.camera_matrix[0], raw.camera_matrix[1], raw.camera_matrix[2]],
        [raw.camera_matrix[3], raw.camera_matrix[4], raw.camera_matrix[5]],
        [raw.camera_matrix[6], raw.camera_matrix[7], raw.camera_matrix[8]],
    ];
    let overall_rms = raw.overall_rms;
    let mean_reproj = if per_image.is_empty() {
        overall_rms
    } else {
        per_image.iter().sum::<f64>() / per_image.len() as f64
    };
    unsafe { cvcharuco_calibrate_free(&mut raw) };
    Ok(CalibResult {
        camera_matrix,
        dist_coeffs,
        overall_rms,
        per_image,
        mean_reproj,
    })
}

#[repr(C)]
struct CvCalibView {
    corners: *const f32,
    ids: *const i32,
    n_corners: i32,
}

#[repr(C)]
struct CvCalibResult {
    camera_matrix: [f64; 9],
    dist_coeffs: [f64; 8],
    n_dist: i32,
    overall_rms: f64,
    per_image: *mut f64,
    n_images: i32,
}

#[link(name = "opencv_capture")]
extern "C" {
    fn cvcharuco_calibrate(
        board: *mut std::ffi::c_void,
        width: i32,
        height: i32,
        views: *const CvCalibView,
        n_views: i32,
        out: *mut CvCalibResult,
    ) -> i32;
    fn cvcharuco_calibrate_free(out: *mut CvCalibResult);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worst_id_empty_returns_none() {
        assert_eq!(worst_id(&[]), None);
    }

    #[test]
    fn should_replace_empty_current_returns_false() {
        assert!(!should_replace(&[], 1.0, 1.0, Some(0.40)));
    }

    #[test]
    fn should_replace_none_trial_reproj_returns_false() {
        let set = vec![ScoredShot {
            id: 1,
            quality: 1.0,
            diversity: 1.0,
            reproj: 0.40,
        }];
        assert!(!should_replace(&set, 1.0, 1.0, None));
    }
}
