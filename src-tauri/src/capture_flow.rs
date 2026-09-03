//! Collect → calibrate → cull ≥1 px → trial last-place replace.

use std::path::PathBuf;
use std::time::Instant;

use crate::calib::{
    apply_replace, calibrate, decide_replace, evaluate_after_calib, session_score_of_shots,
    trial_peer_features, AfterCalib, CalibResult, Shot,
};
use crate::detect::{detect_charuco, sharpness as frame_sharpness, CharucoBoard, Detected};
use crate::score::{evaluate_frame, SavedFeature};
use crate::session::{
    can_autosave, delete_image_file, format_grade, image_path_for_json, save_jpeg,
    session_payload, session_save_error_hint, start_session_dir, write_accepted_json,
    write_session_json, SessionConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturePhase {
    Collecting,
    Improve,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionDone {
    pub image_count: usize,
    pub average_percent: f64,
    pub mean_reprojection_error: Option<f64>,
    pub camera_matrix: [[f64; 3]; 3],
    pub session_dir: String,
}

pub struct CaptureFlow {
    pub config: SessionConfig,
    pub dir: Option<PathBuf>,
    pub dir_error: Option<String>,
    pub shots: Vec<Shot>,
    pub phase: CapturePhase,
    pub last_save: Option<Instant>,
    pub last_calib: Option<CalibResult>,
    pub image_size: Option<(i32, i32)>,
    pub accepted: bool,
    allow_overfill: bool,
    next_id: u32,
}

impl CaptureFlow {
    pub fn start(config: SessionConfig) -> Self {
        let (dir, dir_error) = match start_session_dir(&config.out_root) {
            Ok(dir) => (Some(dir), None),
            Err(err) => (None, Some(err)),
        };
        let flow = Self {
            config,
            dir,
            dir_error,
            shots: Vec::new(),
            phase: CapturePhase::Collecting,
            last_save: None,
            last_calib: None,
            image_size: None,
            accepted: false,
            allow_overfill: false,
            next_id: 1,
        };
        flow.persist(false);
        flow
    }

    pub fn hint_prefix(&self) -> Option<String> {
        self.dir_error.as_ref().map(|err| session_save_error_hint(err))
    }

    pub fn persist_stop(&self) {
        if self.accepted {
            return;
        }
        self.persist(false);
    }

    pub fn process_detected(
        &mut self,
        bgr: &[u8],
        width: i32,
        height: i32,
        channels: i32,
        detected: &Detected,
        board: &CharucoBoard,
        now: Instant,
    ) -> (String, Option<SessionDone>) {
        self.image_size = Some((width, height));
        let sharp = frame_sharpness(bgr, width, height, channels).unwrap_or(0.0);
        match self.phase {
            CapturePhase::Collecting => {
                self.collect_frame(bgr, width, height, channels, detected, board, sharp, now)
            }
            CapturePhase::Improve => {
                self.trial_frame(bgr, width, height, channels, detected, board, sharp, now)
            }
        }
    }

    fn collect_frame(
        &mut self,
        bgr: &[u8],
        width: i32,
        height: i32,
        channels: i32,
        detected: &Detected,
        board: &CharucoBoard,
        sharp: f64,
        now: Instant,
    ) -> (String, Option<SessionDone>) {
        let saved: Vec<SavedFeature> = self
            .shots
            .iter()
            .map(|shot| SavedFeature {
                feature: shot.feature,
            })
            .collect();
        let eval = evaluate_frame(&detected.corners, width, height, sharp, &saved);
        let mut hint = eval.hint.clone();
        let elapsed = self.last_save.map(|at| now.duration_since(at));
        if let Some(dir) = self.dir.clone() {
            if can_autosave(
                eval.saveable,
                elapsed,
                self.shots.len(),
                if self.allow_overfill {
                    usize::MAX
                } else {
                    self.config.count_target
                },
            ) {
                match save_jpeg(&dir, self.next_id as usize, bgr, width, height, channels) {
                    Ok(path) => {
                        self.shots.push(Shot {
                            id: self.next_id,
                            quality: eval.quality,
                            diversity: eval.diversity,
                            reproj: None,
                            feature: eval.feature,
                            corners: detected.corners.clone(),
                            ids: detected.ids.clone(),
                            path: image_path_for_json(&path),
                        });
                        self.next_id += 1;
                        self.last_save = Some(now);
                        self.persist(false);
                        hint = format!(
                            "已保存 {}/{}  {}",
                            self.shots.len(),
                            self.config.count_target,
                            format_grade(eval.percent)
                        );
                        if self.shots.len() >= self.config.count_target {
                            return self.run_calibration(board, hint);
                        }
                    }
                    Err(err) => hint = err,
                }
            }
        }
        (hint, None)
    }

    fn trial_frame(
        &mut self,
        bgr: &[u8],
        width: i32,
        height: i32,
        channels: i32,
        detected: &Detected,
        board: &CharucoBoard,
        sharp: f64,
        now: Instant,
    ) -> (String, Option<SessionDone>) {
        let peers: Vec<SavedFeature> = trial_peer_features(&self.shots)
            .into_iter()
            .map(|feature| SavedFeature { feature })
            .collect();
        let eval = evaluate_frame(&detected.corners, width, height, sharp, &peers);
        if !eval.saveable {
            return (eval.hint, None);
        }
        let elapsed = self.last_save.map(|at| now.duration_since(at));
        if !can_autosave(true, elapsed, 0, usize::MAX) {
            return (eval.hint, None);
        }
        let Some(dir) = self.dir.clone() else {
            return (eval.hint, None);
        };
        let path = match save_jpeg(&dir, self.next_id as usize, bgr, width, height, channels) {
            Ok(path) => path,
            Err(err) => return (err, None),
        };
        let candidate = Shot {
            id: self.next_id,
            quality: eval.quality,
            diversity: eval.diversity,
            reproj: None,
            feature: eval.feature,
            corners: detected.corners.clone(),
            ids: detected.ids.clone(),
            path: image_path_for_json(&path),
        };
        self.next_id += 1;
        self.last_save = Some(now);

        let worst = crate::calib::worst_id(&crate::calib::scored_from_shots(&self.shots));
        let mut trial_shots: Vec<Shot> = self
            .shots
            .iter()
            .filter(|shot| Some(shot.id) != worst)
            .cloned()
            .collect();
        trial_shots.push(candidate.clone());
        let size = self.image_size.unwrap_or((width, height));
        let trial = match calibrate(&trial_shots, board, size) {
            Ok(result) => result,
            Err(err) => {
                delete_image_file(&candidate.path);
                return (format!("试标定失败，请继续换角度：{err}"), None);
            }
        };
        if !decide_replace(&self.shots, candidate.quality, candidate.diversity, Some(&trial)) {
            delete_image_file(&candidate.path);
            return (
                "试探分未提升，已丢弃本帧，请继续换角度".into(),
                None,
            );
        }
        if let Some(dropped) = apply_replace(&mut self.shots, candidate) {
            delete_image_file(&dropped.path);
        }
        let before: Vec<String> = self.shots.iter().map(|shot| shot.path.clone()).collect();
        let outcome = evaluate_after_calib(
            &mut self.shots,
            &trial,
            self.config.count_target,
            self.config.score_target,
        );
        self.last_calib = Some(trial);
        let kept: std::collections::HashSet<&str> =
            self.shots.iter().map(|shot| shot.path.as_str()).collect();
        for path in before {
            if !kept.contains(path.as_str()) {
                delete_image_file(&path);
            }
        }
        match outcome {
            AfterCalib::NeedMore { culled } => {
                self.phase = CapturePhase::Collecting;
                self.persist(false);
                (
                    format!("已替换末位，淘汰 {culled} 张误差≥1 的图，请继续补拍"),
                    None,
                )
            }
            AfterCalib::Improve => {
                self.phase = CapturePhase::Improve;
                self.persist(false);
                let score = session_score_of_shots(&self.shots);
                (
                    format!(
                        "已替换末位，平均 {} / 目标 {}，继续补拍",
                        format_grade(score),
                        format_grade(self.config.score_target)
                    ),
                    None,
                )
            }
            AfterCalib::Accepted => self.accept_session("末位替换后已达标".into()),
        }
    }

    fn run_calibration(
        &mut self,
        board: &CharucoBoard,
        waiting_hint: String,
    ) -> (String, Option<SessionDone>) {
        let size = match self.image_size {
            Some(size) => size,
            None => return (waiting_hint, None),
        };
        let result = match calibrate(&self.shots, board, size) {
            Ok(result) => {
                self.allow_overfill = false;
                result
            }
            Err(err) => {
                self.allow_overfill = true;
                return (format!("标定失败，请继续换角度拍摄：{err}"), None);
            }
        };
        let before: Vec<String> = self.shots.iter().map(|shot| shot.path.clone()).collect();
        let outcome = evaluate_after_calib(
            &mut self.shots,
            &result,
            self.config.count_target,
            self.config.score_target,
        );
        self.last_calib = Some(result);
        let kept: std::collections::HashSet<&str> =
            self.shots.iter().map(|shot| shot.path.as_str()).collect();
        for path in before {
            if !kept.contains(path.as_str()) {
                delete_image_file(&path);
            }
        }
        match outcome {
            AfterCalib::NeedMore { culled } => {
                self.phase = CapturePhase::Collecting;
                self.persist(false);
                (
                    format!("已淘汰 {culled} 张误差≥1 的图，请继续补拍"),
                    None,
                )
            }
            AfterCalib::Improve => {
                self.phase = CapturePhase::Improve;
                self.persist(false);
                let score = session_score_of_shots(&self.shots);
                (
                    format!(
                        "未达标（{} / 目标 {}），保留全部可用图，请换角度试探替换末位",
                        format_grade(score),
                        format_grade(self.config.score_target)
                    ),
                    None,
                )
            }
            AfterCalib::Accepted => self.accept_session("已达标".into()),
        }
    }

    fn accept_session(&mut self, hint: String) -> (String, Option<SessionDone>) {
        self.accepted = true;
        self.phase = CapturePhase::Collecting;
        let payload = session_payload(&self.config, &self.shots, true, self.last_calib.as_ref());
        if let Some(dir) = self.dir.as_ref() {
            let _ = write_session_json(dir, &payload);
            let _ = write_accepted_json(dir, &payload);
        }
        let score = session_score_of_shots(&self.shots);
        let mean = crate::calib::mean_reproj_of_shots(&self.shots);
        let matrix = self
            .last_calib
            .as_ref()
            .map(|calib| calib.camera_matrix)
            .unwrap_or([[0.0; 3]; 3]);
        let done = SessionDone {
            image_count: self.shots.len(),
            average_percent: score,
            mean_reprojection_error: mean,
            camera_matrix: matrix,
            session_dir: self
                .dir
                .as_ref()
                .map(|dir| dir.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        (format!("{hint} {}", format_grade(score)), Some(done))
    }

    fn persist(&self, accepted: bool) {
        let Some(dir) = self.dir.as_ref() else {
            return;
        };
        let payload = session_payload(
            &self.config,
            &self.shots,
            accepted,
            self.last_calib.as_ref(),
        );
        let _ = write_session_json(dir, &payload);
    }
}

pub fn detect_or_none(
    bgr: &[u8],
    width: i32,
    height: i32,
    channels: i32,
    board: &CharucoBoard,
) -> Option<Detected> {
    detect_charuco(bgr, width, height, channels, board)
        .ok()
        .flatten()
}
