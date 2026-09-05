//! Native Media Foundation capture via `IMFSourceReader`.

use crate::camera::normalize_fourcc;
use crate::camera_open::{frame_has_image, OpenRequest, PreviewFrame};
use crate::frame_convert::{mjpg_to_bgr, yuy2_to_bgr};

/// GUID fourcc subtype for MF media types (same packing as DirectShow FOURCC GUIDs).
pub fn subtype_guid_for_fourcc(fourcc: &str) -> Option<windows::core::GUID> {
    let label = normalize_fourcc(fourcc);
    let bytes = label.as_bytes();
    if bytes.len() != 4 {
        return None;
    }
    Some(windows::core::GUID {
        data1: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        data2: 0x0000,
        data3: 0x0010,
        data4: [0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71],
    })
}

#[cfg(not(windows))]
pub struct NativeMsmfCam;

#[cfg(not(windows))]
impl NativeMsmfCam {
    pub fn open(_req: &OpenRequest, _msmf_index: i32) -> Result<Self, String> {
        Err("native MSMF requires Windows".into())
    }

    pub fn read_bgr(&mut self) -> Result<PreviewFrame, String> {
        Err("native MSMF requires Windows".into())
    }

    pub fn size(&self) -> (i32, i32) {
        (0, 0)
    }
}

#[cfg(windows)]
pub struct NativeMsmfCam {
    reader: windows::Win32::Media::MediaFoundation::IMFSourceReader,
    width: i32,
    height: i32,
    fourcc: String,
}

#[cfg(windows)]
impl NativeMsmfCam {
    pub fn open(req: &OpenRequest, msmf_index: i32) -> Result<Self, String> {
        if msmf_index < 0 {
            return Err("invalid MSMF index".into());
        }
        unsafe { open_msmf_com(req, msmf_index as u32) }
    }

    pub fn read_bgr(&mut self) -> Result<PreviewFrame, String> {
        read_msmf_frame(&self.reader, &self.fourcc, self.width, self.height)
    }

    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }
}

#[cfg(windows)]
unsafe fn open_msmf_com(req: &OpenRequest, msmf_index: u32) -> Result<NativeMsmfCam, String> {
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, IMFMediaSource, MFCreateAttributes, MFCreateMediaType,
        MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFShutdown, MFStartup,
        MFSTARTUP_NOSOCKET, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
        MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
        MFMediaType_Video,
    };
    use windows::Win32::System::Com::CoTaskMemFree;

    MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).map_err(|err| format!("MFStartup: {err}"))?;

    let opened = (|| {
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
        if devices.is_null() || count == 0 {
            if !devices.is_null() {
                CoTaskMemFree(Some(devices as *const _));
            }
            return Err("no MSMF video devices".into());
        }
        if msmf_index >= count {
            CoTaskMemFree(Some(devices as *const _));
            return Err(format!("MSMF index {msmf_index} out of range ({count})"));
        }

        let items = std::slice::from_raw_parts_mut(devices, count as usize);
        let activate = items[msmf_index as usize]
            .take()
            .ok_or_else(|| "MSMF activate null".to_string())?;
        // Drop remaining activates to avoid leaks.
        for slot in items.iter_mut() {
            let _ = slot.take();
        }
        CoTaskMemFree(Some(devices as *const _));

        let source: IMFMediaSource = activate
            .ActivateObject()
            .map_err(|err| format!("ActivateObject: {err}"))?;

        let reader = MFCreateSourceReaderFromMediaSource(&source, None)
            .map_err(|err| format!("MFCreateSourceReaderFromMediaSource: {err}"))?;

        let subtype = subtype_guid_for_fourcc(&req.fourcc)
            .ok_or_else(|| format!("unsupported fourcc {}", req.fourcc))?;

        let media_type = MFCreateMediaType().map_err(|err| format!("MFCreateMediaType: {err}"))?;
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|err| format!("SetGUID major: {err}"))?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &subtype)
            .map_err(|err| format!("SetGUID subtype: {err}"))?;
        // MF_MT_FRAME_SIZE packs width/height into UINT64 (high=width, low=height).
        let frame_size = ((req.width as u64) << 32) | (req.height as u64 & 0xffff_ffff);
        media_type
            .SetUINT64(&MF_MT_FRAME_SIZE, frame_size)
            .map_err(|err| format!("SetUINT64 frame size: {err}"))?;
        if req.fps > 0 {
            let frame_rate = ((req.fps as u64) << 32) | 1u64;
            media_type
                .SetUINT64(&MF_MT_FRAME_RATE, frame_rate)
                .map_err(|err| format!("SetUINT64 frame rate: {err}"))?;
        }

        reader
            .SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &media_type,
            )
            .map_err(|err| format!("SetCurrentMediaType: {err}"))?;

        // Warm first frame so open returns a working stream.
        let mut last_err = "no frame".to_string();
        for _ in 0..8 {
            match read_msmf_frame(&reader, &req.fourcc, req.width, req.height) {
                Ok(frame) if frame_has_image(frame.mean, frame.width, frame.height) => {
                    return Ok(NativeMsmfCam {
                        reader,
                        width: frame.width,
                        height: frame.height,
                        fourcc: normalize_fourcc(&req.fourcc),
                    });
                }
                Ok(_) => last_err = "black or tiny frame".into(),
                Err(err) => last_err = err,
            }
        }
        Err(format!("MSMF warmup failed: {last_err}"))
    })();

    if opened.is_err() {
        let _ = MFShutdown();
    }
    // Successful open keeps MF started (refcounted). Reader Drop releases the device.
    opened
}

#[cfg(windows)]
fn read_msmf_frame(
    reader: &windows::Win32::Media::MediaFoundation::IMFSourceReader,
    fourcc: &str,
    expect_w: i32,
    expect_h: i32,
) -> Result<PreviewFrame, String> {
    use windows::Win32::Media::MediaFoundation::{
        IMFMediaBuffer, IMFSample, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    };

    let mut flags = 0u32;
    let mut timestamp = 0i64;
    let mut sample: Option<IMFSample> = None;
    unsafe {
        reader
            .ReadSample(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                0,
                None,
                Some(&mut flags),
                Some(&mut timestamp),
                Some(&mut sample as *mut _),
            )
            .map_err(|err| format!("ReadSample: {err}"))?;
    }
    let sample = sample.ok_or_else(|| "empty MSMF sample".to_string())?;
    let buffer: IMFMediaBuffer = unsafe {
        sample
            .ConvertToContiguousBuffer()
            .map_err(|err| format!("ConvertToContiguousBuffer: {err}"))?
    };
    let mut raw_ptr: *mut u8 = std::ptr::null_mut();
    let mut max_len = 0u32;
    let mut cur_len = 0u32;
    unsafe {
        buffer
            .Lock(&mut raw_ptr, Some(&mut max_len), Some(&mut cur_len))
            .map_err(|err| format!("Lock: {err}"))?;
    }
    if raw_ptr.is_null() || cur_len == 0 {
        unsafe {
            let _ = buffer.Unlock();
        }
        return Err("empty MSMF buffer".into());
    }
    let bytes = unsafe { std::slice::from_raw_parts(raw_ptr, cur_len as usize) }.to_vec();
    unsafe {
        let _ = buffer.Unlock();
    }

    let fourcc = normalize_fourcc(fourcc);
    let (width, height, data) = match fourcc.as_str() {
        "YUY2" => {
            let w = if expect_w > 0 {
                expect_w
            } else {
                return Err("YUY2 needs width".into());
            };
            let h = if expect_h > 0 {
                expect_h
            } else {
                return Err("YUY2 needs height".into());
            };
            let bgr = yuy2_to_bgr(w, h, &bytes)?;
            (w, h, bgr)
        }
        "MJPG" => mjpg_to_bgr(&bytes)?,
        other => return Err(format!("unsupported native MSMF fourcc {other}")),
    };
    let mean = bgr_mean(&data);
    Ok(PreviewFrame {
        width,
        height,
        channels: 3,
        mean,
        data,
    })
}

fn bgr_mean(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let sum: u64 = data.iter().map(|&b| u64::from(b)).sum();
    sum as f64 / data.len() as f64
}

#[cfg(test)]
mod tests {
    use super::subtype_guid_for_fourcc;

    #[test]
    fn subtype_guid_packs_mjpg_fourcc() {
        let guid = subtype_guid_for_fourcc("MJPG").expect("MJPG");
        assert_eq!(guid.data1, u32::from_le_bytes(*b"MJPG"));
        assert_eq!(guid.data2, 0);
        assert_eq!(guid.data3, 0x0010);
    }
}
