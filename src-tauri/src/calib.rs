use crate::score::session_percent;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredShot {
    pub id: u32,
    pub quality: f64,
    pub diversity: f64,
    pub reproj: f64,
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
