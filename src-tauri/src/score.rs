use crate::detect::MIN_CORNERS;

pub const REPROJ_GOOD: f64 = 0.40;
pub const REPROJ_LIMIT: f64 = 1.00;
pub const MIN_SHARPNESS: f64 = 30.0;

const CORNERS_FOR_FULL_QUALITY: f64 = 20.0;
const SHARPNESS_GOOD: f64 = 100.0;
const FEATURE_WEIGHTS: [f32; 6] = [0.8, 0.8, 1.0, 1.4, 0.7, 1.2];

fn clip(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

pub fn quality_unit(n_corners: usize, sharp: f64) -> f64 {
    let corner_frac = clip(n_corners as f64 / CORNERS_FOR_FULL_QUALITY, 0.0, 1.0);
    let sharp_norm = clip(sharp / SHARPNESS_GOOD, 0.0, 1.0);
    0.60 * corner_frac + 0.40 * sharp_norm
}

pub fn diversity_unit(min_dist: f64, has_others: bool) -> f64 {
    if !has_others {
        return 1.0;
    }
    clip(min_dist / 0.28, 0.0, 1.0)
}

pub fn diversity_threshold(n_saved: usize) -> f64 {
    if n_saved == 0 {
        return 0.0;
    }
    (0.13 - 0.003 * (n_saved as f64 - 1.0)).max(0.07)
}

pub fn reproj_unit(error: f64) -> f64 {
    if error <= REPROJ_GOOD {
        1.0
    } else if error >= REPROJ_LIMIT {
        0.0
    } else {
        (REPROJ_LIMIT - error) / (REPROJ_LIMIT - REPROJ_GOOD)
    }
}

pub fn preview_percent(quality: f64, diversity: f64) -> f64 {
    let mixed = 100.0 * (0.85 * quality + 0.15 * diversity);
    clip(mixed, 0.0, 100.0)
}

pub fn session_percent(quality: f64, diversity: f64, mean_reproj: Option<f64>) -> f64 {
    let mixed = match mean_reproj {
        None => 0.85 * quality + 0.15 * diversity,
        Some(e) => 0.40 * quality + 0.10 * diversity + 0.50 * reproj_unit(e),
    };
    clip(100.0 * mixed, 0.0, 100.0)
}

fn covariance_2d(points: &[[f32; 2]], cx: f32, cy: f32) -> [[f64; 2]; 2] {
    let n = points.len() as f64;
    if n < 2.0 {
        return [[1e-3, 0.0], [0.0, 1e-3]];
    }
    let mut cov = [[0.0_f64; 2]; 2];
    for [x, y] in points {
        let dx = f64::from(*x - cx);
        let dy = f64::from(*y - cy);
        cov[0][0] += dx * dx;
        cov[0][1] += dx * dy;
        cov[1][0] += dy * dx;
        cov[1][1] += dy * dy;
    }
    let scale = 1.0 / (n - 1.0);
    cov[0][0] *= scale;
    cov[0][1] *= scale;
    cov[1][0] *= scale;
    cov[1][1] *= scale;
    if cov.iter().flatten().all(|v| v.is_finite()) {
        cov
    } else {
        [[1e-3, 0.0], [0.0, 1e-3]]
    }
}

fn eigh_symmetric_2x2(cov: [[f64; 2]; 2]) -> ([f64; 2], [[f64; 2]; 2]) {
    let a = cov[0][0];
    let b = cov[0][1];
    let c = cov[1][1];
    let trace = a + c;
    let det = a * c - b * b;
    let disc = (trace * trace / 4.0 - det).max(0.0).sqrt();
    let eval0 = trace / 2.0 - disc;
    let eval1 = trace / 2.0 + disc;

    let mut axis0 = [b, eval0 - a];
    let mut axis1 = [b, eval1 - a];
    let len0 = (axis0[0] * axis0[0] + axis0[1] * axis0[1]).sqrt();
    let len1 = (axis1[0] * axis1[0] + axis1[1] * axis1[1]).sqrt();
    if len0 < 1e-12 {
        axis0 = [1.0, 0.0];
    } else {
        axis0 = [axis0[0] / len0, axis0[1] / len0];
    }
    if len1 < 1e-12 {
        axis1 = [0.0, 1.0];
    } else {
        axis1 = [axis1[0] / len1, axis1[1] / len1];
    }

    ([eval0, eval1], [axis0, axis1])
}

pub fn board_feature(corners_xy: &[[f32; 2]], width: i32, height: i32) -> [f32; 6] {
    let width = width.max(1);
    let height = height.max(1);
    let img_scale = ((width * width + height * height) as f32).sqrt();
    let n = corners_xy.len();
    if n == 0 {
        return [0.5, 0.5, 0.2, 1.0, 0.5, 0.5];
    }

    let (cx, cy) = corners_xy.iter().fold((0.0_f32, 0.0_f32), |(sx, sy), [x, y]| {
        (sx + x, sy + y)
    });
    let cx = cx / n as f32;
    let cy = cy / n as f32;
    let center = [cx / width as f32, cy / height as f32];

    if n < 3 {
        return [center[0], center[1], 0.2, 1.0, 0.5, 0.5];
    }

    let cov = covariance_2d(corners_xy, cx, cy);
    let (_evals, evecs) = eigh_symmetric_2x2(cov);
    let mut axis0 = evecs[0];
    let mut axis1 = evecs[1];

    let centered: Vec<[f32; 2]> = corners_xy
        .iter()
        .map(|[x, y]| [*x - cx, *y - cy])
        .collect();

    let mut proj0: Vec<f32> = centered
        .iter()
        .map(|[dx, dy]| (f64::from(*dx) * axis0[0] + f64::from(*dy) * axis0[1]) as f32)
        .collect();
    let mut proj1: Vec<f32> = centered
        .iter()
        .map(|[dx, dy]| (f64::from(*dx) * axis1[0] + f64::from(*dy) * axis1[1]) as f32)
        .collect();

    let mut ext0 = (proj0.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        - proj0.iter().cloned().fold(f32::INFINITY, f32::min))
        / img_scale;
    let mut ext1 = (proj1.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        - proj1.iter().cloned().fold(f32::INFINITY, f32::min))
        / img_scale;

    if ext0 > ext1 {
        std::mem::swap(&mut ext0, &mut ext1);
        std::mem::swap(&mut axis0, &mut axis1);
        std::mem::swap(&mut proj0, &mut proj1);
    }

    let aspect = ext0 / ext1.max(1e-6);
    let angle = (axis1[1].atan2(axis1[0]) / std::f64::consts::PI) as f32;

    let left: Vec<[f32; 2]> = centered
        .iter()
        .zip(proj1.iter())
        .filter(|(_, p1)| **p1 < 0.0)
        .map(|(pt, _)| *pt)
        .collect();
    let right: Vec<[f32; 2]> = centered
        .iter()
        .zip(proj1.iter())
        .filter(|(_, p1)| **p1 >= 0.0)
        .map(|(pt, _)| *pt)
        .collect();

    let trap = if left.len() >= 2 && right.len() >= 2 {
        let w_left = ptp_along_axis(&left, axis0);
        let w_right = ptp_along_axis(&right, axis0);
        (w_left - w_right) / (w_left + w_right).max(1e-6)
    } else {
        0.0
    };

    [
        center[0],
        center[1],
        ext1,
        aspect,
        angle * 0.5 + 0.5,
        trap * 0.5 + 0.5,
    ]
}

fn ptp_along_axis(points: &[[f32; 2]], axis: [f64; 2]) -> f32 {
    let projections: Vec<f32> = points
        .iter()
        .map(|[dx, dy]| (f64::from(*dx) * axis[0] + f64::from(*dy) * axis[1]) as f32)
        .collect();
    let min = projections.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = projections.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    max - min
}

pub fn feature_distance(a: &[f32; 6], b: &[f32; 6]) -> f64 {
    let mut sum = 0.0_f64;
    for i in 0..6 {
        let delta = f64::from((a[i] - b[i]) * FEATURE_WEIGHTS[i]);
        sum += delta * delta;
    }
    sum.sqrt()
}

#[derive(Debug, Clone, PartialEq)]
pub struct SavedFeature {
    pub feature: [f32; 6],
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameEval {
    pub saveable: bool,
    pub quality: f64,
    pub diversity: f64,
    pub percent: f64,
    pub feature: [f32; 6],
    pub hint: String,
}

fn min_feature_distance(feature: &[f32; 6], others: &[SavedFeature]) -> f64 {
    if others.is_empty() {
        return 1.0;
    }
    others
        .iter()
        .map(|item| feature_distance(feature, &item.feature))
        .fold(f64::INFINITY, f64::min)
}

fn rejected(hint: impl Into<String>, feature: [f32; 6]) -> FrameEval {
    FrameEval {
        saveable: false,
        quality: 0.0,
        diversity: 0.0,
        percent: 0.0,
        feature,
        hint: hint.into(),
    }
}

/// Python `evaluate_frame`: corners / sharpness / pose-diversity gate.
pub fn evaluate_frame(
    corners: &[[f32; 2]],
    width: i32,
    height: i32,
    sharp: f64,
    saved: &[SavedFeature],
) -> FrameEval {
    let n_corners = corners.len();
    if n_corners < MIN_CORNERS {
        return rejected("角点不足，靠近或摆正标定板", [0.0; 6]);
    }
    if sharp < MIN_SHARPNESS {
        return rejected("画面过糊，请持稳或改善光线", [0.0; 6]);
    }
    let feature = board_feature(corners, width, height);
    let min_dist = min_feature_distance(&feature, saved);
    let threshold = diversity_threshold(saved.len());
    if !saved.is_empty() && min_dist < threshold {
        return rejected(
            format!("姿态太接近已存图（差异 {min_dist:.2} / 需 {threshold:.2}），请加大倾角或换远近"),
            feature,
        );
    }
    let quality = quality_unit(n_corners, sharp);
    let diversity = diversity_unit(min_dist, !saved.is_empty());
    let percent = preview_percent(quality, diversity);
    FrameEval {
        saveable: true,
        quality,
        diversity,
        percent,
        feature,
        hint: format!("可采集 {percent:.0} 分"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diversity_threshold_cases() {
        assert_eq!(diversity_threshold(0), 0.0);
        assert!((diversity_threshold(1) - 0.13).abs() < 1e-9);
        assert!((diversity_threshold(21) - 0.07).abs() < 1e-9);
    }

    #[test]
    fn preview_percent_mix() {
        assert!((preview_percent(1.0, 0.0) - 85.0).abs() < 1e-6);
        assert!((preview_percent(0.0, 1.0) - 15.0).abs() < 1e-6);
    }

    #[test]
    fn quality_unit_blend() {
        assert!((quality_unit(20, 100.0) - 1.0).abs() < 1e-9);
        assert!((quality_unit(0, 0.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn preview_percent_clips_to_hundred() {
        assert!((preview_percent(1.2, 1.2) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn session_percent_clips_to_hundred() {
        assert!((session_percent(1.2, 1.2, Some(0.30)) - 100.0).abs() < 1e-6);
    }
}
