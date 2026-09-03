//! DirectShow camera enumeration.
//!
//! Reads discrete `VIDEOINFOHEADER` sizes + FOURCC from `IAMStreamConfig`.
//! Never expands `VIDEO_STREAM_CONFIG_CAPS` min/max ranges (that path caused
//! OpenCV heap corruption in the Python capture tool).

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CameraMode {
    pub device_name: String,
    pub dshow_index: i32,
    pub width: i32,
    pub height: i32,
    pub fourcc: String,
}

/// One `IAMStreamConfig::GetStreamCaps` slot.
///
/// Range fields are accepted so callers/tests can prove they are ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamCapSlot {
    pub format_is_video_info: bool,
    pub header_width: i32,
    pub header_height: i32,
    pub subtype_label: String,
    pub range_min_width: i32,
    pub range_min_height: i32,
    pub range_max_width: i32,
    pub range_max_height: i32,
}

/// Matches `dshow_enum.py` `_normalize_fourcc`.
pub fn normalize_fourcc(label: &str) -> String {
    let text = label.trim().to_ascii_uppercase();
    match text.as_str() {
        "MJPG" | "MJPEG" => "MJPG".into(),
        "YUY2" | "YUYV" => "YUY2".into(),
        "NV12" => "NV12".into(),
        "RGB24" | "RGB32" | "ARGB32" | "RGB555" | "RGB565" => "auto".into(),
        other
            if other.len() == 4
                && other
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == ' ') =>
        {
            other.replace(' ', "")
        }
        _ => "auto".into(),
    }
}

/// Matches `dshow_enum.py` `_prefer_larger_modes`.
pub fn prefer_larger_modes(modes: Vec<(i32, i32, String)>) -> Vec<(i32, i32, String)> {
    let large: Vec<(i32, i32, String)> = modes
        .iter()
        .filter(|(width, height, _)| *width >= 640 && *height >= 480)
        .cloned()
        .collect();
    if large.is_empty() {
        modes
    } else {
        large
    }
}

/// Keep only discrete VIDEOINFOHEADER width/height + FOURCC.
/// `range_*` on each slot is intentionally unused.
pub fn collect_discrete_modes(slots: &[StreamCapSlot]) -> Vec<(i32, i32, String)> {
    let mut found: Vec<(i32, i32, String)> = Vec::new();
    let mut seen: HashSet<(i32, i32, String)> = HashSet::new();
    for slot in slots {
        let _ = (
            slot.range_min_width,
            slot.range_min_height,
            slot.range_max_width,
            slot.range_max_height,
        );
        if !slot.format_is_video_info {
            continue;
        }
        let width = slot.header_width;
        let height = slot.header_height.abs();
        if width <= 0 || height <= 0 {
            continue;
        }
        let fourcc = normalize_fourcc(&slot.subtype_label);
        let key = (width, height, fourcc);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key.clone());
        found.push(key);
    }
    prefer_larger_modes(found)
}

#[tauri::command]
pub fn list_cameras() -> Result<Vec<CameraMode>, String> {
    list_dshow_modes()
}

pub fn list_dshow_modes() -> Result<Vec<CameraMode>, String> {
    #[cfg(not(windows))]
    {
        Err("DirectShow enumeration requires Windows".into())
    }
    #[cfg(windows)]
    {
        list_dshow_modes_sta()
    }
}

#[cfg(windows)]
fn list_dshow_modes_sta() -> Result<Vec<CameraMode>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("dshow-enum".into())
        .spawn(move || {
            let result = unsafe { list_dshow_modes_com() };
            let _ = tx.send(result);
        })
        .map_err(|err| format!("spawn dshow-enum thread: {err}"))?;
    let result = rx
        .recv()
        .map_err(|err| format!("dshow-enum thread: {err}"))?;
    let _ = worker.join();
    result
}

#[cfg(windows)]
unsafe fn list_dshow_modes_com() -> Result<Vec<CameraMode>, String> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    if hr.is_err() {
        return Err(format!("CoInitializeEx: {hr:?}"));
    }
    let result = enumerate_video_devices();
    CoUninitialize();
    result
}

#[cfg(windows)]
fn enumerate_video_devices() -> Result<Vec<CameraMode>, String> {
    use windows::Win32::Media::DirectShow::{IBaseFilter, ICreateDevEnum};
    use windows::Win32::Media::MediaFoundation::{
        CLSID_SystemDeviceEnum, CLSID_VideoInputDeviceCategory,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, IEnumMoniker, IMoniker, CLSCTX_INPROC_SERVER,
    };

    let dev_enum: ICreateDevEnum = unsafe {
        CoCreateInstance(&CLSID_SystemDeviceEnum, None, CLSCTX_INPROC_SERVER)
            .map_err(|err| format!("CLSID_SystemDeviceEnum: {err}"))?
    };

    let mut enum_moniker: Option<IEnumMoniker> = None;
    if let Err(err) = unsafe {
        dev_enum.CreateClassEnumerator(&CLSID_VideoInputDeviceCategory, &mut enum_moniker, 0)
    } {
        return Err(format!("CreateClassEnumerator: {err}"));
    }
    let Some(enum_moniker) = enum_moniker else {
        return Ok(Vec::new());
    };

    let mut modes = Vec::new();
    let mut dshow_index: i32 = 0;
    loop {
        let mut fetched = 0u32;
        let mut slot: [Option<IMoniker>; 1] = [None];
        let hr = unsafe { enum_moniker.Next(&mut slot, Some(&mut fetched)) };
        if hr.is_err() || fetched == 0 {
            break;
        }
        let Some(moniker) = slot[0].take() else {
            break;
        };

        let device_name =
            friendly_name(&moniker).unwrap_or_else(|| format!("Camera {dshow_index}"));
        let device_modes = match unsafe { moniker.BindToObject::<_, _, IBaseFilter>(None, None) } {
            Ok(filter) => read_filter_modes(&filter),
            Err(_) => Vec::new(),
        };
        for (width, height, fourcc) in device_modes {
            modes.push(CameraMode {
                device_name: device_name.clone(),
                dshow_index,
                width,
                height,
                fourcc,
            });
        }
        dshow_index += 1;
    }
    Ok(modes)
}

#[cfg(windows)]
fn friendly_name(moniker: &windows::Win32::System::Com::IMoniker) -> Option<String> {
    use windows::core::w;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::System::Com::StructuredStorage::IPropertyBag;
    use windows::Win32::System::Variant::{VariantClear, VariantToStringAlloc, VARIANT};

    let bag: IPropertyBag = unsafe { moniker.BindToStorage(None, None).ok()? };
    let mut var = VARIANT::default();
    unsafe {
        bag.Read(w!("FriendlyName"), &mut var, None).ok()?;
    }
    let name = unsafe {
        let pwstr = VariantToStringAlloc(&var).ok()?;
        let text = pwstr.to_string().unwrap_or_default();
        CoTaskMemFree(Some(pwstr.0 as *const _));
        let _ = VariantClear(&mut var);
        text
    };
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(windows)]
fn read_filter_modes(
    filter: &windows::Win32::Media::DirectShow::IBaseFilter,
) -> Vec<(i32, i32, String)> {
    use windows::core::Interface;
    use windows::Win32::Media::DirectShow::{IAMStreamConfig, IPin, PINDIR_OUTPUT};

    let Ok(enum_pins) = (unsafe { filter.EnumPins() }) else {
        return Vec::new();
    };
    let mut slots = Vec::new();
    loop {
        let mut fetched = 0u32;
        let mut pin_slot: [Option<IPin>; 1] = [None];
        let hr = unsafe { enum_pins.Next(&mut pin_slot, Some(&mut fetched)) };
        if hr.is_err() || fetched == 0 {
            break;
        }
        let Some(pin) = pin_slot[0].take() else {
            break;
        };
        let Ok(direction) = (unsafe { pin.QueryDirection() }) else {
            continue;
        };
        if direction != PINDIR_OUTPUT {
            continue;
        }
        let Ok(config) = pin.cast::<IAMStreamConfig>() else {
            continue;
        };
        slots.extend(read_stream_caps(&config));
    }
    collect_discrete_modes(&slots)
}

#[cfg(windows)]
fn read_stream_caps(
    config: &windows::Win32::Media::DirectShow::IAMStreamConfig,
) -> Vec<StreamCapSlot> {
    use windows::Win32::Media::MediaFoundation::AM_MEDIA_TYPE;

    let mut count = 0i32;
    let mut cap_size = 0i32;
    if unsafe { config.GetNumberOfCapabilities(&mut count, &mut cap_size) }.is_err() {
        return Vec::new();
    }
    if count <= 0 {
        return Vec::new();
    }
    let buf_len = cap_size.max(1) as usize;
    let mut slots = Vec::new();
    for index in 0..count {
        let mut scc = vec![0u8; buf_len];
        let mut media_type: *mut AM_MEDIA_TYPE = std::ptr::null_mut();
        // `scc` is required by GetStreamCaps; never read VIDEO_STREAM_CONFIG_CAPS ranges from it.
        if unsafe { config.GetStreamCaps(index, &mut media_type, scc.as_mut_ptr()) }.is_err() {
            continue;
        }
        if media_type.is_null() {
            continue;
        }
        let slot = unsafe { parse_videoinfo_slot(media_type) };
        unsafe { delete_media_type(media_type) };
        if let Some(slot) = slot {
            slots.push(slot);
        }
    }
    slots
}

#[cfg(windows)]
unsafe fn parse_videoinfo_slot(
    media_type: *mut windows::Win32::Media::MediaFoundation::AM_MEDIA_TYPE,
) -> Option<StreamCapSlot> {
    use windows::Win32::Media::MediaFoundation::{FORMAT_VideoInfo, VIDEOINFOHEADER};

    let mt = &*media_type;
    let format_is_video_info = mt.formattype == FORMAT_VideoInfo;
    let mut header_width = 0i32;
    let mut header_height = 0i32;
    if format_is_video_info
        && !mt.pbFormat.is_null()
        && mt.cbFormat as usize >= std::mem::size_of::<VIDEOINFOHEADER>()
    {
        let header = &*(mt.pbFormat as *const VIDEOINFOHEADER);
        header_width = header.bmiHeader.biWidth;
        header_height = header.bmiHeader.biHeight;
    }
    Some(StreamCapSlot {
        format_is_video_info,
        header_width,
        header_height,
        subtype_label: fourcc_label_from_guid(mt.subtype),
        range_min_width: 0,
        range_min_height: 0,
        range_max_width: 0,
        range_max_height: 0,
    })
}

#[cfg(windows)]
fn fourcc_label_from_guid(guid: windows::core::GUID) -> String {
    const FOURCC_SUFFIX: [u8; 8] = [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71];
    if guid.data2 == 0 && guid.data3 == 0x0010 && guid.data4 == FOURCC_SUFFIX {
        let bytes = guid.data1.to_le_bytes();
        if bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b' ')
        {
            return String::from_utf8_lossy(&bytes).into_owned();
        }
    }
    String::new()
}

#[cfg(windows)]
unsafe fn delete_media_type(
    media_type: *mut windows::Win32::Media::MediaFoundation::AM_MEDIA_TYPE,
) {
    use std::mem::ManuallyDrop;
    use windows::Win32::System::Com::CoTaskMemFree;

    if media_type.is_null() {
        return;
    }
    let mt = &mut *media_type;
    if !mt.pbFormat.is_null() {
        CoTaskMemFree(Some(mt.pbFormat as *const _));
        mt.pbFormat = std::ptr::null_mut();
        mt.cbFormat = 0;
    }
    ManuallyDrop::drop(&mut mt.pUnk);
    CoTaskMemFree(Some(media_type as *const _));
}
