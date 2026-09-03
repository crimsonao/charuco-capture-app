//! ChArUco 8×6 / `DICT_4X4_50` detection via the OpenCV 4.12 C++ FFI.
//!
//! The official `opencv_world4120` build already ships ArUco in `objdetect`
//! (not contrib). `interpolateCornersCharuco` is gone from the C++ headers;
//! `CharucoDetector::detectBoard` after `ArucoDetector::detectMarkers` is the
//! 4.12 equivalent.

use std::ptr;

pub const BOARD_SQUARES_X: i32 = 6;
pub const BOARD_SQUARES_Y: i32 = 8;
pub const MIN_CORNERS: usize = 6;
pub const DEFAULT_SQUARE_M: f32 = 0.02;
pub const DEFAULT_MARKER_M: f32 = 0.015;
pub const NO_BOARD_HINT: &str = "请把标定板放进画面";

#[derive(Debug, Clone, PartialEq)]
pub struct Detected {
    pub corners: Vec<[f32; 2]>,
    pub ids: Vec<i32>,
    pub marker_corners: Vec<Vec<[f32; 2]>>,
}

pub struct CharucoBoard {
    ptr: *mut CvCharuco,
}

impl Drop for CharucoBoard {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { cvcharuco_free(self.ptr) }
            self.ptr = ptr::null_mut();
        }
    }
}

#[repr(C)]
struct CvCharuco {
    _private: [u8; 0],
}

#[repr(C)]
struct CvFrame {
    data: *mut u8,
    width: i32,
    height: i32,
    channels: i32,
}

#[repr(C)]
struct CvCharucoDetect {
    corners: *mut f32,
    ids: *mut i32,
    n_corners: i32,
    marker_corners: *mut f32,
    n_markers: i32,
}

#[link(name = "opencv_capture")]
extern "C" {
    fn cvcharuco_create(square_m: f32, marker_m: f32) -> *mut CvCharuco;
    fn cvcharuco_free(board: *mut CvCharuco);
    fn cvcharuco_detect(
        board: *mut CvCharuco,
        frame: *const CvFrame,
        out: *mut CvCharucoDetect,
    ) -> i32;
    fn cvcharuco_detect_free(out: *mut CvCharucoDetect);
    fn cvcharuco_draw(frame: *mut CvFrame, det: *const CvCharucoDetect, enough: i32) -> i32;
    fn cvcharuco_generate(
        board: *mut CvCharuco,
        width: i32,
        height: i32,
        out: *mut CvFrame,
    ) -> i32;
    fn cvcam_frame_free(frame: *mut CvFrame);
}

/// Fixed 8×6 / `DICT_4X4_50` board. `square_m` / `marker_m` are metres.
pub fn make_board(square_m: f32, marker_m: f32) -> Result<CharucoBoard, String> {
    if square_m <= 0.0 || marker_m <= 0.0 || marker_m >= square_m {
        return Err("square_m must be > marker_m > 0".into());
    }
    let ptr = unsafe { cvcharuco_create(square_m, marker_m) };
    if ptr.is_null() {
        return Err("cvcharuco_create failed".into());
    }
    Ok(CharucoBoard { ptr })
}

pub fn has_enough_corners(detected: &Detected) -> bool {
    detected.corners.len() >= MIN_CORNERS
}

pub fn frame_hint(n_corners: usize) -> String {
    if n_corners == 0 {
        NO_BOARD_HINT.to_string()
    } else {
        "detected".into()
    }
}

/// Detect ChArUco corners. `None` when no markers and no interpolated corners.
pub fn detect_charuco(
    data: &[u8],
    width: i32,
    height: i32,
    channels: i32,
    board: &CharucoBoard,
) -> Result<Option<Detected>, String> {
    let raw = CvFrame {
        data: data.as_ptr() as *mut u8,
        width,
        height,
        channels,
    };
    let mut out = empty_detect();
    let ok = unsafe { cvcharuco_detect(board.ptr, &raw, &mut out) };
    if ok == 0 {
        unsafe { cvcharuco_detect_free(&mut out) };
        return Err("cvcharuco_detect failed".into());
    }
    let detected = take_detected(&out);
    unsafe { cvcharuco_detect_free(&mut out) };
    Ok(detected)
}

/// Draw marker outlines + Charuco corner boxes onto a BGR frame.
pub fn draw_detected(
    data: &mut [u8],
    width: i32,
    height: i32,
    channels: i32,
    detected: &Detected,
) -> Result<(), String> {
    let (mut corners, mut ids, mut marker_flat) = pack_detected(detected);
    let c_det = CvCharucoDetect {
        corners: if corners.is_empty() {
            ptr::null_mut()
        } else {
            corners.as_mut_ptr()
        },
        ids: if ids.is_empty() {
            ptr::null_mut()
        } else {
            ids.as_mut_ptr()
        },
        n_corners: detected.corners.len() as i32,
        marker_corners: if marker_flat.is_empty() {
            ptr::null_mut()
        } else {
            marker_flat.as_mut_ptr()
        },
        n_markers: detected.marker_corners.len() as i32,
    };
    let mut raw = CvFrame {
        data: data.as_mut_ptr(),
        width,
        height,
        channels,
    };
    let enough = i32::from(has_enough_corners(detected));
    let ok = unsafe { cvcharuco_draw(&mut raw, &c_det, enough) };
    if ok == 0 {
        return Err("cvcharuco_draw failed".into());
    }
    Ok(())
}

/// Synthetic 8×6 board image for tests (`generateImage`).
pub fn generate_board_image(
    board: &CharucoBoard,
    width: i32,
    height: i32,
) -> Result<(Vec<u8>, i32, i32, i32), String> {
    let mut raw = CvFrame {
        data: ptr::null_mut(),
        width: 0,
        height: 0,
        channels: 0,
    };
    let ok = unsafe { cvcharuco_generate(board.ptr, width, height, &mut raw) };
    if ok == 0 || raw.data.is_null() {
        return Err("cvcharuco_generate failed".into());
    }
    let nbytes = (raw.width as usize)
        .saturating_mul(raw.height as usize)
        .saturating_mul(raw.channels.max(1) as usize);
    let mut data = vec![0u8; nbytes];
    if nbytes > 0 {
        unsafe {
            ptr::copy_nonoverlapping(raw.data, data.as_mut_ptr(), nbytes);
        }
    }
    let result = (data, raw.width, raw.height, raw.channels);
    unsafe { cvcam_frame_free(&mut raw) };
    Ok(result)
}

fn empty_detect() -> CvCharucoDetect {
    CvCharucoDetect {
        corners: ptr::null_mut(),
        ids: ptr::null_mut(),
        n_corners: 0,
        marker_corners: ptr::null_mut(),
        n_markers: 0,
    }
}

fn take_detected(out: &CvCharucoDetect) -> Option<Detected> {
    if out.n_corners <= 0 && out.n_markers <= 0 {
        return None;
    }
    let mut corners = Vec::new();
    let mut ids = Vec::new();
    if out.n_corners > 0 && !out.corners.is_null() {
        let n = out.n_corners as usize;
        let xy = unsafe { std::slice::from_raw_parts(out.corners, n * 2) };
        corners.reserve(n);
        for i in 0..n {
            corners.push([xy[i * 2], xy[i * 2 + 1]]);
        }
        if !out.ids.is_null() {
            ids.extend_from_slice(unsafe { std::slice::from_raw_parts(out.ids, n) });
        }
    }
    let mut marker_corners = Vec::new();
    if out.n_markers > 0 && !out.marker_corners.is_null() {
        let n = out.n_markers as usize;
        let xy = unsafe { std::slice::from_raw_parts(out.marker_corners, n * 8) };
        marker_corners.reserve(n);
        for i in 0..n {
            let base = i * 8;
            marker_corners.push(vec![
                [xy[base], xy[base + 1]],
                [xy[base + 2], xy[base + 3]],
                [xy[base + 4], xy[base + 5]],
                [xy[base + 6], xy[base + 7]],
            ]);
        }
    }
    Some(Detected {
        corners,
        ids,
        marker_corners,
    })
}

fn pack_detected(detected: &Detected) -> (Vec<f32>, Vec<i32>, Vec<f32>) {
    let mut corners = Vec::with_capacity(detected.corners.len() * 2);
    for [x, y] in &detected.corners {
        corners.push(*x);
        corners.push(*y);
    }
    let ids = detected.ids.clone();
    let mut marker_flat = Vec::with_capacity(detected.marker_corners.len() * 8);
    for marker in &detected.marker_corners {
        for i in 0..4 {
            let [x, y] = marker.get(i).copied().unwrap_or([0.0, 0.0]);
            marker_flat.push(x);
            marker_flat.push(y);
        }
    }
    (corners, ids, marker_flat)
}

#[cfg(test)]
mod hint_tests {
    use super::*;

    #[test]
    fn no_board_hint_is_chinese() {
        assert_eq!(frame_hint(0), NO_BOARD_HINT);
        assert_eq!(frame_hint(6), "detected");
    }

    #[test]
    fn min_corners_is_six() {
        assert_eq!(MIN_CORNERS, 6);
        assert!(!has_enough_corners(&Detected {
            corners: vec![[0.0, 0.0]; 5],
            ids: vec![0; 5],
            marker_corners: vec![],
        }));
        assert!(has_enough_corners(&Detected {
            corners: vec![[0.0, 0.0]; 6],
            ids: vec![0; 6],
            marker_corners: vec![],
        }));
    }
}
