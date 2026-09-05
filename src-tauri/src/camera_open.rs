//! Open a camera by friendly name and stream JPEG preview.
//!
//! Never call `VideoCapture(dshow_index, CAP_MSMF)`. DirectShow order is not
//! the MSMF order; that mistake opens the laptop camera when ocal4 is selected.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use tauri::{AppHandle, Emitter};

use crate::calib::session_score_of_shots;
use crate::camera::normalize_fourcc;
use crate::capture_flow::{CaptureFlow, CapturePhase, SessionDone};
use crate::detect::{draw_detected, frame_hint, make_board_from_mm};
use crate::jpeg::encode_jpeg_bytes;
use crate::session::{resolve_out_root, SessionConfig};

const BLACK_MEAN: f64 = 4.0;
const MIN_FRAME_EDGE: i32 = 16;
const PREVIEW_INTERVAL: Duration = Duration::from_millis(50);
/// Shared budget for the whole open_capture attempt chain (all backends × codecs).
const OPEN_BUDGET: Duration = Duration::from_secs(10);
/// Cap a single backend/codec try so a slow MSMF open cannot burn the whole budget
/// before DirectShow is attempted (deadline is checked around blocking OpenCV calls).
const ATTEMPT_BUDGET: Duration = Duration::from_secs(3);
const JPEG_QUALITY: i32 = 80;

/// Last backend + FOURCC that successfully opened a device (keyed by lowercased name).
static LAST_SUCCESS: Mutex<Option<HashMap<String, SuccessfulOpen>>> = Mutex::new(None);
/// MSMF friendly names cached after list/prefetch so open need not re-enumerate.
static MSMF_NAMES_CACHE: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// User-visible hint when consecutive read failures stop the preview loop.
pub const CAMERA_DISCONNECT_HINT: &str = "摄像头断开";

const CAP_MSMF: i32 = 1400;
const CAP_DSHOW: i32 = 700;
const CAP_PROP_FRAME_WIDTH: i32 = 3;
const CAP_PROP_FRAME_HEIGHT: i32 = 4;
const CAP_PROP_FPS: i32 = 5;
const CAP_PROP_FOURCC: i32 = 6;
const CAP_PROP_BUFFERSIZE: i32 = 38;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OpenRequest {
    pub device_name: String,
    pub dshow_index: i32,
    pub width: i32,
    pub height: i32,
    pub fourcc: String,
    pub fps: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAttempt {
    pub backend: &'static str,
    pub index: i32,
}

/// Remembered open path for a device (backend + FOURCC that worked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuccessfulOpen {
    pub backend: String,
    pub fourcc: String,
}

/// Total time budget for opening (shared across all backend/codec tries).
pub fn open_budget() -> Duration {
    OPEN_BUDGET
}

/// Warmup `read` attempts before giving up on a backend/codec try.
pub fn warmup_read_tries(backend: &str, width: i32, height: i32) -> u32 {
    let _ = backend;
    let mut tries = 2;
    if width.saturating_mul(height) >= 1920 * 1080 {
        tries += 1;
    }
    tries
}

/// Cold open prefers DirectShow (UI modes come from DSHOW enum). Cached success wins.
pub fn preferred_backend_for_open(cached: Option<&SuccessfulOpen>) -> &str {
    cached
        .map(|success| success.backend.as_str())
        .filter(|backend| !backend.trim().is_empty())
        .unwrap_or("DSHOW")
}

fn device_cache_key(device_name: &str) -> String {
    device_name.trim().to_ascii_lowercase()
}

pub fn remember_successful_open(device_name: &str, success: &SuccessfulOpen) {
    let key = device_cache_key(device_name);
    if key.is_empty() {
        return;
    }
    let mut guard = LAST_SUCCESS.lock().unwrap_or_else(|err| err.into_inner());
    guard
        .get_or_insert_with(HashMap::new)
        .insert(key, success.clone());
}

pub fn last_successful_open(device_name: &str) -> Option<SuccessfulOpen> {
    let key = device_cache_key(device_name);
    if key.is_empty() {
        return None;
    }
    let guard = LAST_SUCCESS.lock().unwrap_or_else(|err| err.into_inner());
    guard.as_ref()?.get(&key).cloned()
}

pub fn clear_open_success_cache() {
    let mut guard = LAST_SUCCESS.lock().unwrap_or_else(|err| err.into_inner());
    *guard = None;
}

pub fn cache_msmf_names(names: Vec<String>) {
    let mut guard = MSMF_NAMES_CACHE.lock().unwrap_or_else(|err| err.into_inner());
    *guard = Some(names);
}

pub fn clear_msmf_names_cache() {
    let mut guard = MSMF_NAMES_CACHE.lock().unwrap_or_else(|err| err.into_inner());
    *guard = None;
}

/// Prefer cached MSMF names; on miss, enumerate and store.
pub fn msmf_names_for_open() -> Vec<String> {
    {
        let guard = MSMF_NAMES_CACHE.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(names) = guard.as_ref() {
            return names.clone();
        }
    }
    match list_msmf_names() {
        Ok(names) => {
            cache_msmf_names(names.clone());
            names
        }
        Err(_) => Vec::new(),
    }
}

/// Move a preferred backend to the front when present.
pub fn prioritize_backend_attempts(
    mut attempts: Vec<OpenAttempt>,
    preferred_backend: Option<&str>,
) -> Vec<OpenAttempt> {
    let Some(preferred) = preferred_backend.map(str::trim).filter(|s| !s.is_empty()) else {
        return attempts;
    };
    if let Some(index) = attempts
        .iter()
        .position(|attempt| attempt.backend.eq_ignore_ascii_case(preferred))
    {
        let chosen = attempts.remove(index);
        attempts.insert(0, chosen);
    }
    attempts
}

/// Move a preferred FOURCC to the front when present.
pub fn prioritize_fourcc_attempts(
    mut codecs: Vec<String>,
    preferred_fourcc: Option<&str>,
) -> Vec<String> {
    let Some(preferred) = preferred_fourcc.map(|s| normalize_fourcc(s)) else {
        return codecs;
    };
    if preferred == "auto" {
        return codecs;
    }
    if let Some(index) = codecs
        .iter()
        .position(|codec| codec.eq_ignore_ascii_case(&preferred))
    {
        let chosen = codecs.remove(index);
        codecs.insert(0, chosen);
    }
    codecs
}

pub struct OpenedCam {
    pub width: i32,
    pub height: i32,
    pub backend: String,
    capture: CameraHandle,
}

pub struct PreviewFrame {
    pub width: i32,
    pub height: i32,
    pub channels: i32,
    pub mean: f64,
    pub data: Vec<u8>,
}

struct CameraHandle {
    ptr: *mut CvCam,
}

impl Drop for CameraHandle {
    fn drop(&mut self) {
        unsafe { cvcam_release(self.ptr) }
        self.ptr = std::ptr::null_mut();
    }
}

#[repr(C)]
struct CvCam {
    _private: [u8; 0],
}

#[repr(C)]
struct CvFrame {
    data: *mut u8,
    width: i32,
    height: i32,
    channels: i32,
}

#[link(name = "opencv_capture")]
extern "C" {
    fn cvcam_open(index: i32, api_preference: i32) -> *mut CvCam;
    fn cvcam_is_opened(cam: *mut CvCam) -> i32;
    fn cvcam_set(cam: *mut CvCam, prop_id: i32, value: f64) -> i32;
    fn cvcam_read(cam: *mut CvCam, out: *mut CvFrame) -> i32;
    fn cvcam_frame_free(frame: *mut CvFrame);
    fn cvcam_frame_mean(frame: *const CvFrame) -> f64;
    fn cvcam_release(cam: *mut CvCam);
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FrameEvent {
    pub jpeg_base64: String,
    pub hint: String,
    pub n_corners: usize,
    pub corners: Vec<[f32; 2]>,
    pub image_count: usize,
    pub count_target: usize,
    pub phase: String,
    pub average_percent: f64,
    pub mean_reprojection_error: Option<f64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionParams {
    pub count_target: Option<usize>,
    pub score_target: Option<f64>,
    pub square_mm: Option<f64>,
    pub marker_mm: Option<f64>,
    pub out_root: Option<String>,
}

impl SessionParams {
    pub fn into_config(self) -> SessionConfig {
        SessionConfig::from_setup(
            self.count_target.unwrap_or(crate::session::DEFAULT_COUNT_TARGET),
            self.score_target.unwrap_or(crate::session::DEFAULT_SCORE_TARGET),
            self.square_mm.unwrap_or(crate::session::DEFAULT_SQUARE_MM),
            self.marker_mm.unwrap_or(crate::session::DEFAULT_MARKER_MM),
            resolve_out_root(self.out_root.as_deref()),
        )
    }
}

/// Validate board millimetres (and OpenCV create) before opening a camera.
pub fn prepare_preview_session(session: Option<SessionParams>) -> Result<SessionConfig, String> {
    let config = session
        .map(SessionParams::into_config)
        .unwrap_or_else(SessionConfig::default_capture);
    let _board = make_board_from_mm(config.square_mm, config.marker_mm)?;
    Ok(config)
}

struct PreviewSession {
    stop: std::sync::Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

static SESSION: Mutex<Option<PreviewSession>> = Mutex::new(None);

/// Match `device_name` against an MSMF/PnP name list. Exact (case-insensitive)
/// first, then substring. Empty name is never a match.
pub fn match_device_index(device_name: &str, names: &[String]) -> Option<i32> {
    let wanted = device_name.trim().to_ascii_lowercase();
    if wanted.is_empty() {
        return None;
    }
    for (index, name) in names.iter().enumerate() {
        if name.trim().eq_ignore_ascii_case(wanted.as_str()) {
            return Some(index as i32);
        }
    }
    for (index, name) in names.iter().enumerate() {
        let lowered = name.trim().to_ascii_lowercase();
        if wanted.contains(&lowered) || lowered.contains(&wanted) {
            return Some(index as i32);
        }
    }
    None
}

/// DirectShow by `dshow_index` first (matches Setup enumeration), then MSMF by
/// friendly-name index. Never pairs MSMF with the DirectShow index.
pub fn open_attempts_for_mode(req: &OpenRequest, msmf_names: &[String]) -> Vec<OpenAttempt> {
    let mut attempts = Vec::new();
    attempts.push(OpenAttempt {
        backend: "DSHOW",
        index: req.dshow_index,
    });
    if let Some(msmf_index) = match_device_index(&req.device_name, msmf_names) {
        attempts.push(OpenAttempt {
            backend: "MSMF",
            index: msmf_index,
        });
    }
    attempts
}

/// Requested FOURCC first; YUY2 (and any non-MJPG) retries MJPG.
pub fn fourcc_attempts(fourcc: &str) -> Vec<String> {
    let mut values = Vec::new();
    let requested = normalize_fourcc(fourcc);
    if requested != "auto" {
        values.push(requested);
    }
    if !values.iter().any(|value| value == "MJPG") {
        values.push("MJPG".into());
    }
    values
}

/// True when a warmup frame has usable size and mean luminance ≥ 4.
pub fn frame_has_image(mean: f64, width: i32, height: i32) -> bool {
    width >= MIN_FRAME_EDGE && height >= MIN_FRAME_EDGE && mean >= BLACK_MEAN
}

/// `VideoWriter_fourcc` little-endian packing. Rejects `"auto"` and short labels.
pub fn fourcc_u32(label: &str) -> Option<i32> {
    let bytes = label.as_bytes();
    if bytes.len() != 4 || label.eq_ignore_ascii_case("auto") {
        return None;
    }
    Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// MSMF device order as used by OpenCV `CAP_MSMF` (MFEnumDeviceSources).
pub fn list_msmf_names() -> Result<Vec<String>, String> {
    #[cfg(not(windows))]
    {
        Err("MSMF enumeration requires Windows".into())
    }
    #[cfg(windows)]
    {
        list_msmf_names_sta()
    }
}

#[cfg(windows)]
fn list_msmf_names_sta() -> Result<Vec<String>, String> {
    let (tx, rx) = mpsc::channel();
    let worker = thread::Builder::new()
        .name("msmf-enum".into())
        .spawn(move || {
            let result = unsafe { list_msmf_names_com() };
            let _ = tx.send(result);
        })
        .map_err(|err| format!("spawn msmf-enum thread: {err}"))?;
    let result = rx
        .recv()
        .map_err(|err| format!("msmf-enum thread: {err}"))?;
    let _ = worker.join();
    result
}

#[cfg(windows)]
unsafe fn list_msmf_names_com() -> Result<Vec<String>, String> {
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, MFCreateAttributes, MFEnumDeviceSources, MFShutdown, MFStartup,
        MFSTARTUP_NOSOCKET, MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        MF_VERSION,
    };
    use windows::Win32::System::Com::CoTaskMemFree;

    MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).map_err(|err| format!("MFStartup: {err}"))?;

    let result = (|| {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 1).map_err(|err| format!("MFCreateAttributes: {err}"))?;
        let attrs = attrs.ok_or_else(|| "MFCreateAttributes returned null".to_string())?;
        attrs
            .SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .map_err(|err| format!("SetGUID: {err}"))?;

        let mut devices: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFEnumDeviceSources(&attrs, &mut devices, &mut count)
            .map_err(|err| format!("MFEnumDeviceSources: {err}"))?;
        if devices.is_null() {
            return Ok(Vec::new());
        }
        if count == 0 {
            CoTaskMemFree(Some(devices as *const _));
            return Ok(Vec::new());
        }

        let mut names = Vec::with_capacity(count as usize);
        let items = std::slice::from_raw_parts_mut(devices, count as usize);
        for slot in items.iter_mut() {
            let Some(activate) = slot.take() else {
                continue;
            };
            let mut raw = windows::core::PWSTR::null();
            let mut chars = 0u32;
            if activate
                .GetAllocatedString(
                    &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                    &mut raw,
                    &mut chars,
                )
                .is_ok()
                && !raw.is_null()
            {
                if let Ok(text) = raw.to_string() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        names.push(trimmed.to_string());
                    }
                }
                CoTaskMemFree(Some(raw.0 as *const _));
            }
        }
        CoTaskMemFree(Some(devices as *const _));
        Ok(names)
    })();

    let _ = MFShutdown();
    result
}

/// Open the selected device only. Does not fall through to another camera.
pub fn open_capture(req: &OpenRequest) -> Result<OpenedCam, String> {
    let msmf_names = msmf_names_for_open();
    let preferred = last_successful_open(&req.device_name);
    let attempts = prioritize_backend_attempts(
        open_attempts_for_mode(req, &msmf_names),
        Some(preferred_backend_for_open(preferred.as_ref())),
    );
    let codecs = prioritize_fourcc_attempts(
        fourcc_attempts(&req.fourcc),
        preferred.as_ref().map(|p| p.fourcc.as_str()),
    );
    let deadline = Instant::now() + OPEN_BUDGET;
    let mut errors = Vec::new();
    'open: for attempt in attempts {
        for codec in &codecs {
            if open_attempt_timed_out(deadline) {
                errors.push(format!(
                    "{}/{}: open budget exhausted",
                    attempt.backend, codec
                ));
                break 'open;
            }
            let attempt_deadline = {
                let slice = Instant::now() + ATTEMPT_BUDGET;
                if slice < deadline {
                    slice
                } else {
                    deadline
                }
            };
            match try_open_backend_until(
                attempt.backend,
                attempt.index,
                req,
                codec,
                attempt_deadline,
            ) {
                Ok(opened) => {
                    remember_successful_open(
                        &req.device_name,
                        &SuccessfulOpen {
                            backend: opened.backend.clone(),
                            fourcc: codec.clone(),
                        },
                    );
                    return Ok(opened);
                }
                Err(err) => errors.push(err),
            }
        }
    }
    let detail = if errors.is_empty() {
        "no backend".into()
    } else {
        errors.join("; ")
    };
    Err(format!(
        "Failed to open '{}'. Try another resolution or format. Did not switch device. {detail}",
        req.device_name
    ))
}

/// True when an open/warmup attempt should abort and drop the capture.
///
/// Open and `read()` stay on the caller thread (preview / harness). A deadline
/// replaces the old `cam-open` worker so VideoCapture is never moved across
/// threads, and a timed-out attempt drops the handle before the next FOURCC
/// or DSHOW try.
pub fn open_attempt_timed_out(deadline: Instant) -> bool {
    Instant::now() >= deadline
}

fn try_open_backend_until(
    backend: &str,
    index: i32,
    req: &OpenRequest,
    codec: &str,
    deadline: Instant,
) -> Result<OpenedCam, String> {
    if open_attempt_timed_out(deadline) {
        return Err(format!("{backend}#{index}/{codec}: timeout"));
    }
    let api = match backend {
        "MSMF" => CAP_MSMF,
        "DSHOW" => CAP_DSHOW,
        other => return Err(format!("unknown backend {other}")),
    };
    let ptr = unsafe { cvcam_open(index, api) };
    if ptr.is_null() {
        return Err(format!("{backend}#{index}/{codec}: not opened"));
    }
    let capture = CameraHandle { ptr };
    if unsafe { cvcam_is_opened(capture.ptr) } == 0 {
        return Err(format!("{backend}#{index}/{codec}: not opened"));
    }
    unsafe {
        cvcam_set(capture.ptr, CAP_PROP_BUFFERSIZE, 1.0);
        if let Some(fourcc) = fourcc_u32(codec) {
            cvcam_set(capture.ptr, CAP_PROP_FOURCC, f64::from(fourcc));
        }
        cvcam_set(capture.ptr, CAP_PROP_FRAME_WIDTH, f64::from(req.width));
        cvcam_set(capture.ptr, CAP_PROP_FRAME_HEIGHT, f64::from(req.height));
        if req.fps > 0 {
            cvcam_set(capture.ptr, CAP_PROP_FPS, f64::from(req.fps));
        }
    }

    let tries = warmup_read_tries(backend, req.width, req.height);

    let mut last_width = 0i32;
    let mut last_height = 0i32;
    let mut last_mean = 0.0f64;
    for _ in 0..tries {
        if open_attempt_timed_out(deadline) {
            return Err(format!("{backend}#{index}/{codec}: timeout"));
        }
        match read_preview_frame(&capture) {
            Ok(frame) => {
                last_width = frame.width;
                last_height = frame.height;
                last_mean = frame.mean;
                if frame_has_image(last_mean, last_width, last_height) {
                    return Ok(OpenedCam {
                        width: last_width,
                        height: last_height,
                        backend: backend.to_string(),
                        capture,
                    });
                }
            }
            Err(_) => continue,
        }
    }
    Err(format!(
        "{backend}#{index}/{codec}: no image (last {last_width}x{last_height} mean={last_mean:.1})"
    ))
}

fn read_preview_frame(capture: &CameraHandle) -> Result<PreviewFrame, String> {
    let mut raw = CvFrame {
        data: std::ptr::null_mut(),
        width: 0,
        height: 0,
        channels: 0,
    };
    let ok = unsafe { cvcam_read(capture.ptr, &mut raw) };
    if ok == 0 || raw.data.is_null() {
        return Err("empty frame".into());
    }
    let mean = unsafe { cvcam_frame_mean(&raw) };
    let nbytes = (raw.width as usize)
        .saturating_mul(raw.height as usize)
        .saturating_mul(raw.channels.max(1) as usize);
    let mut data = vec![0u8; nbytes];
    if nbytes > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(raw.data, data.as_mut_ptr(), nbytes);
        }
    }
    let frame = PreviewFrame {
        width: raw.width,
        height: raw.height,
        channels: raw.channels,
        mean,
        data,
    };
    unsafe { cvcam_frame_free(&mut raw) };
    Ok(frame)
}

impl OpenedCam {
    pub fn read_frame(&mut self) -> Result<PreviewFrame, String> {
        read_preview_frame(&self.capture)
    }

    pub fn release(self) {
        drop(self);
    }
}

pub fn frame_mean(frame: &PreviewFrame) -> f64 {
    frame.mean
}

fn encode_jpeg_base64(frame: &PreviewFrame) -> Result<String, String> {
    let bytes = encode_jpeg_bytes(
        &frame.data,
        frame.width,
        frame.height,
        frame.channels,
        JPEG_QUALITY,
    )?;
    Ok(STANDARD.encode(bytes))
}

fn run_preview_loop(
    opened: &mut OpenedCam,
    app: AppHandle,
    stop: std::sync::Arc<AtomicBool>,
    config: SessionConfig,
) {
    let board = match make_board_from_mm(config.square_mm, config.marker_mm) {
        Ok(board) => board,
        Err(err) => {
            let _ = app.emit(
                "frame",
                FrameEvent {
                    jpeg_base64: String::new(),
                    hint: err,
                    n_corners: 0,
                    corners: Vec::new(),
                    image_count: 0,
                    count_target: config.count_target,
                    phase: "collecting".into(),
                    average_percent: 0.0,
                    mean_reprojection_error: None,
                },
            );
            return;
        }
    };
    let mut flow = CaptureFlow::start(config);
    let mut last_emit = Instant::now()
        .checked_sub(PREVIEW_INTERVAL)
        .unwrap_or_else(Instant::now);
    let mut fail_reads = 0u32;
    let mut last_frame_event: Option<FrameEvent> = None;
    while !stop.load(Ordering::SeqCst) {
        let Ok(mut frame) = opened.read_frame() else {
            fail_reads += 1;
            if fail_reads >= 30 {
                flow.persist_stop();
                emit_disconnect_hint(&app, &flow, last_frame_event.as_mut());
                break;
            }
            thread::sleep(Duration::from_millis(20));
            continue;
        };
        fail_reads = 0;
        let now = Instant::now();
        if now.duration_since(last_emit) < PREVIEW_INTERVAL {
            continue;
        }
        last_emit = now;

        let mut n_corners = 0usize;
        let mut corners = Vec::new();
        let mut hint = frame_hint(0);
        let mut done: Option<SessionDone> = None;
        if let Some(detected) = crate::capture_flow::detect_or_none(
            &frame.data,
            frame.width,
            frame.height,
            frame.channels,
            &board,
        ) {
            n_corners = detected.corners.len();
            corners = detected.corners.clone();
            if !detected.corners.is_empty() {
                let (next_hint, next_done) = flow.process_detected(
                    &frame.data,
                    frame.width,
                    frame.height,
                    frame.channels,
                    &detected,
                    &board,
                    now,
                );
                hint = next_hint;
                done = next_done;
            }
            let _ = draw_detected(
                &mut frame.data,
                frame.width,
                frame.height,
                frame.channels,
                &detected,
            );
        }

        if let Some(save_hint) = flow.hint_prefix() {
            if !hint.starts_with(&save_hint) {
                hint = if hint == frame_hint(0) {
                    save_hint
                } else {
                    format!("{save_hint}  {hint}")
                };
            }
        }

        let Ok(jpeg_base64) = encode_jpeg_base64(&frame) else {
            continue;
        };
        let phase = match flow.phase {
            CapturePhase::Collecting => "collecting",
            CapturePhase::Improve => "improve",
        };
        let payload = FrameEvent {
            jpeg_base64,
            hint,
            n_corners,
            corners,
            image_count: flow.shots.len(),
            count_target: flow.config.count_target,
            phase: phase.into(),
            average_percent: session_score_of_shots(&flow.shots),
            mean_reprojection_error: crate::calib::mean_reproj_of_shots(&flow.shots),
        };
        last_frame_event = Some(payload.clone());
        let _ = app.emit("frame", payload);
        if let Some(done) = done {
            let _ = app.emit("session-done", done);
            break;
        }
    }
    flow.persist_stop();
}

fn emit_disconnect_hint(
    app: &AppHandle,
    flow: &CaptureFlow,
    last_frame_event: Option<&mut FrameEvent>,
) {
    if let Some(payload) = last_frame_event {
        payload.hint = CAMERA_DISCONNECT_HINT.into();
        let _ = app.emit("frame", payload.clone());
        return;
    }
    let phase = match flow.phase {
        CapturePhase::Collecting => "collecting",
        CapturePhase::Improve => "improve",
    };
    let _ = app.emit(
        "frame",
        FrameEvent {
            jpeg_base64: String::new(),
            hint: CAMERA_DISCONNECT_HINT.into(),
            n_corners: 0,
            corners: Vec::new(),
            image_count: flow.shots.len(),
            count_target: flow.config.count_target,
            phase: phase.into(),
            average_percent: session_score_of_shots(&flow.shots),
            mean_reprojection_error: crate::calib::mean_reproj_of_shots(&flow.shots),
        },
    );
}

fn stop_session_inner() {
    let mut guard = SESSION.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(mut session) = guard.take() {
        session.stop.store(true, Ordering::SeqCst);
        if let Some(join) = session.join.take() {
            let _ = join.join();
        }
    }
}

fn start_preview_blocking(
    app: AppHandle,
    req: OpenRequest,
    config: SessionConfig,
) -> Result<(i32, i32, String), String> {
    stop_session_inner();
    let (tx, rx) = mpsc::channel();
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let handle = thread::Builder::new()
        .name("preview".into())
        .spawn(move || match open_capture(&req) {
            Ok(mut opened) => {
                let info = (opened.width, opened.height, opened.backend.clone());
                let _ = tx.send(Ok(info));
                run_preview_loop(&mut opened, app, stop_flag, config);
            }
            Err(err) => {
                let _ = tx.send(Err(err));
            }
        })
        .map_err(|err| format!("spawn preview: {err}"))?;

    let opened = rx
        .recv()
        .map_err(|err| format!("preview thread: {err}"))??;
    let mut guard = SESSION.lock().map_err(|err| err.to_string())?;
    *guard = Some(PreviewSession {
        stop,
        join: Some(handle),
    });
    Ok(opened)
}

#[tauri::command]
pub async fn start_preview(
    app: AppHandle,
    req: OpenRequest,
    session: Option<SessionParams>,
) -> Result<(i32, i32, String), String> {
    let config = prepare_preview_session(session)?;
    tauri::async_runtime::spawn_blocking(move || start_preview_blocking(app, req, config))
        .await
        .map_err(|err| format!("start_preview join: {err}"))?
}

#[tauri::command]
pub async fn stop_session() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(stop_session_inner)
        .await
        .map_err(|err| format!("stop_session join: {err}"))?;
    Ok(())
}

#[tauri::command]
pub fn open_session_folder(path: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("empty session folder".into());
    }
    std::process::Command::new("explorer")
        .arg(&path)
        .spawn()
        .map_err(|err| format!("open folder: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    fn sample_request() -> OpenRequest {
        OpenRequest {
            device_name: "ocal4".into(),
            dshow_index: 1,
            width: 1280,
            height: 720,
            fourcc: "YUY2".into(),
            fps: 30,
        }
    }

    #[test]
    fn expired_deadline_returns_timeout_without_opening() {
        let started = Instant::now();
        let err = match try_open_backend_until(
            "MSMF",
            0,
            &sample_request(),
            "YUY2",
            Instant::now() - Duration::from_secs(1),
        ) {
            Ok(_) => panic!("expired deadline must not open"),
            Err(err) => err,
        };
        assert!(err.contains("timeout"), "{err}");
        assert!(started.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn past_deadline_is_timed_out() {
        assert!(open_attempt_timed_out(
            Instant::now() - Duration::from_secs(1)
        ));
        assert!(!open_attempt_timed_out(
            Instant::now() + Duration::from_secs(8)
        ));
    }
}
