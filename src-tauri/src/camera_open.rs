//! Open a camera by friendly name and stream JPEG preview.
//!
//! Never call `VideoCapture(dshow_index, CAP_MSMF)`. DirectShow order is not
//! the MSMF order; that mistake opens the laptop camera when ocal4 is selected.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use tauri::{AppHandle, Emitter};

use crate::camera::normalize_fourcc;

const BLACK_MEAN: f64 = 4.0;
const MIN_FRAME_EDGE: i32 = 16;
const PREVIEW_INTERVAL: Duration = Duration::from_millis(50);
const OPEN_TIMEOUT: Duration = Duration::from_secs(8);
const JPEG_QUALITY: i32 = 80;

const CAP_MSMF: i32 = 1400;
const CAP_DSHOW: i32 = 700;
const CAP_PROP_FRAME_WIDTH: i32 = 3;
const CAP_PROP_FRAME_HEIGHT: i32 = 4;
const CAP_PROP_FOURCC: i32 = 6;
const CAP_PROP_BUFFERSIZE: i32 = 38;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OpenRequest {
    pub device_name: String,
    pub dshow_index: i32,
    pub width: i32,
    pub height: i32,
    pub fourcc: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAttempt {
    pub backend: &'static str,
    pub index: i32,
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
    data: Vec<u8>,
}

struct CameraHandle {
    ptr: *mut CvCam,
}

unsafe impl Send for CameraHandle {}

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
    fn cvcam_imencode_jpeg(
        frame: *const CvFrame,
        quality: i32,
        out: *mut *mut u8,
        out_len: *mut i32,
    ) -> i32;
    fn cvcam_bytes_free(ptr: *mut u8);
    fn cvcam_release(cam: *mut CvCam);
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FrameEvent {
    pub jpeg_base64: String,
    pub hint: String,
    pub n_corners: usize,
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

/// MSMF by friendly-name index first, then DirectShow by `dshow_index`.
/// Never pairs MSMF with the DirectShow index.
pub fn open_attempts_for_mode(req: &OpenRequest, msmf_names: &[String]) -> Vec<OpenAttempt> {
    let mut attempts = Vec::new();
    if let Some(msmf_index) = match_device_index(&req.device_name, msmf_names) {
        attempts.push(OpenAttempt {
            backend: "MSMF",
            index: msmf_index,
        });
    }
    attempts.push(OpenAttempt {
        backend: "DSHOW",
        index: req.dshow_index,
    });
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
    let msmf_names = list_msmf_names().unwrap_or_default();
    let mut errors = Vec::new();
    for attempt in open_attempts_for_mode(req, &msmf_names) {
        for codec in fourcc_attempts(&req.fourcc) {
            match try_open_with_timeout(attempt.backend, attempt.index, req, &codec) {
                Ok(opened) => return Ok(opened),
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

fn try_open_with_timeout(
    backend: &'static str,
    index: i32,
    req: &OpenRequest,
    codec: &str,
) -> Result<OpenedCam, String> {
    let req = req.clone();
    let codec = codec.to_string();
    let codec_label = codec.clone();
    let (tx, rx) = mpsc::channel();
    let worker = thread::Builder::new()
        .name("cam-open".into())
        .spawn(move || {
            let result = try_open_backend(backend, index, &req, &codec);
            let _ = tx.send(result);
        })
        .map_err(|err| format!("{backend}#{index}/{codec_label}: spawn {err}"))?;
    match rx.recv_timeout(OPEN_TIMEOUT) {
        Ok(result) => {
            let _ = worker.join();
            result
        }
        Err(_) => Err(format!("{backend}#{index}/{codec_label}: timeout")),
    }
}

fn try_open_backend(
    backend: &str,
    index: i32,
    req: &OpenRequest,
    codec: &str,
) -> Result<OpenedCam, String> {
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
    }

    let mut tries = if backend == "MSMF" { 12 } else { 5 };
    if req.width.saturating_mul(req.height) >= 1920 * 1080 {
        tries += 4;
    }

    let mut last_width = 0i32;
    let mut last_height = 0i32;
    let mut last_mean = 0.0f64;
    for _ in 0..tries {
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
    let raw = CvFrame {
        data: frame.data.as_ptr() as *mut u8,
        width: frame.width,
        height: frame.height,
        channels: frame.channels,
    };
    let mut out = std::ptr::null_mut();
    let mut out_len = 0i32;
    let ok = unsafe { cvcam_imencode_jpeg(&raw, JPEG_QUALITY, &mut out, &mut out_len) };
    if ok == 0 || out.is_null() || out_len <= 0 {
        return Err("imencode failed".into());
    }
    let bytes = unsafe { std::slice::from_raw_parts(out, out_len as usize) };
    let encoded = STANDARD.encode(bytes);
    unsafe { cvcam_bytes_free(out) };
    Ok(encoded)
}

fn run_preview_loop(opened: &mut OpenedCam, app: AppHandle, stop: std::sync::Arc<AtomicBool>) {
    let mut last_emit = Instant::now()
        .checked_sub(PREVIEW_INTERVAL)
        .unwrap_or_else(Instant::now);
    while !stop.load(Ordering::SeqCst) {
        let Ok(frame) = opened.read_frame() else {
            thread::sleep(Duration::from_millis(20));
            continue;
        };
        let now = Instant::now();
        if now.duration_since(last_emit) < PREVIEW_INTERVAL {
            continue;
        }
        last_emit = now;
        let Ok(jpeg_base64) = encode_jpeg_base64(&frame) else {
            continue;
        };
        let payload = FrameEvent {
            jpeg_base64,
            hint: "preview".into(),
            n_corners: 0,
        };
        let _ = app.emit("frame", payload);
    }
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

fn start_preview_blocking(app: AppHandle, req: OpenRequest) -> Result<(i32, i32, String), String> {
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
                run_preview_loop(&mut opened, app, stop_flag);
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
) -> Result<(i32, i32, String), String> {
    tauri::async_runtime::spawn_blocking(move || start_preview_blocking(app, req))
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
