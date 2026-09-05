//! Native DirectShow capture via Sample Grabber (qedit).

use crate::camera::{fps_from_avg_time_per_frame, is_com_init_success, normalize_fourcc};
use crate::camera_open::{frame_has_image, OpenRequest, PreviewFrame};
use crate::frame_convert::{mjpg_to_bgr, yuy2_to_bgr};
use crate::native_msmf_capture::subtype_guid_for_fourcc;

/// qedit.h CLSID_SampleGrabber
const CLSID_SAMPLE_GRABBER: windows::core::GUID =
    windows::core::GUID::from_u128(0x0579_154a_2b53_4994_b0d0_e773_148e_ff85);
/// qedit.h CLSID_NullRenderer
const CLSID_NULL_RENDERER: windows::core::GUID =
    windows::core::GUID::from_u128(0xc1f4_00a4_3f08_11d3_9f0b_0060_0803_9e37);

windows::core::imp::define_interface!(
    ISampleGrabber,
    ISampleGrabber_Vtbl,
    0x6b652fff_11fe_4fce_92ad_0266b5d7c78f
);
windows::core::imp::interface_hierarchy!(ISampleGrabber, windows::core::IUnknown);

#[repr(C)]
#[allow(non_snake_case)]
pub struct ISampleGrabber_Vtbl {
    pub base__: windows::core::IUnknown_Vtbl,
    pub SetOneShot: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows::core::BOOL,
    ) -> windows::core::HRESULT,
    pub SetMediaType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const windows::Win32::Media::MediaFoundation::AM_MEDIA_TYPE,
    ) -> windows::core::HRESULT,
    pub GetConnectedMediaType: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows::Win32::Media::MediaFoundation::AM_MEDIA_TYPE,
    ) -> windows::core::HRESULT,
    pub SetBufferSamples: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows::core::BOOL,
    ) -> windows::core::HRESULT,
    pub GetCurrentBuffer: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut i32,
        *mut u8,
    ) -> windows::core::HRESULT,
    pub GetCurrentSample:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> windows::core::HRESULT,
    pub SetCallback: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
    ) -> windows::core::HRESULT,
}

impl ISampleGrabber {
    pub unsafe fn set_one_shot(&self, one_shot: windows::core::BOOL) -> windows::core::Result<()> {
        unsafe {
            (windows::core::Interface::vtable(self).SetOneShot)(
                windows::core::Interface::as_raw(self),
                one_shot,
            )
            .ok()
        }
    }

    pub unsafe fn set_buffer_samples(
        &self,
        buffer_them: windows::core::BOOL,
    ) -> windows::core::Result<()> {
        unsafe {
            (windows::core::Interface::vtable(self).SetBufferSamples)(
                windows::core::Interface::as_raw(self),
                buffer_them,
            )
            .ok()
        }
    }

    pub unsafe fn get_current_buffer(
        &self,
        buffer_size: *mut i32,
        buffer: *mut u8,
    ) -> windows::core::Result<()> {
        unsafe {
            (windows::core::Interface::vtable(self).GetCurrentBuffer)(
                windows::core::Interface::as_raw(self),
                buffer_size,
                buffer,
            )
            .ok()
        }
    }
}

pub struct NativeDshowCam {
    control: windows::Win32::Media::DirectShow::IMediaControl,
    grabber: ISampleGrabber,
    width: i32,
    height: i32,
    fourcc: String,
    _graph: windows::Win32::Media::DirectShow::IGraphBuilder,
    _builder: windows::Win32::Media::DirectShow::ICaptureGraphBuilder2,
    _capture: windows::Win32::Media::DirectShow::IBaseFilter,
    _grabber_filter: windows::Win32::Media::DirectShow::IBaseFilter,
    _null: windows::Win32::Media::DirectShow::IBaseFilter,
}

impl NativeDshowCam {
    pub fn open(req: &OpenRequest) -> Result<Self, String> {
        #[cfg(not(windows))]
        {
            let _ = req;
            return Err("native DirectShow requires Windows".into());
        }
        #[cfg(windows)]
        {
            unsafe { open_dshow_com(req) }
        }
    }

    pub fn read_bgr(&mut self) -> Result<PreviewFrame, String> {
        let bytes = unsafe { grab_current_buffer(&self.grabber)? };
        let fourcc = self.fourcc.as_str();
        let (width, height, data) = match fourcc {
            "YUY2" => {
                let bgr = yuy2_to_bgr(self.width, self.height, &bytes)?;
                (self.width, self.height, bgr)
            }
            "MJPG" => mjpg_to_bgr(&bytes)?,
            other => return Err(format!("unsupported native DSHOW fourcc {other}")),
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

    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }
}

impl Drop for NativeDshowCam {
    fn drop(&mut self) {
        unsafe {
            let _ = self.control.Stop();
        }
    }
}

fn bgr_mean(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let sum: u64 = data.iter().map(|&b| u64::from(b)).sum();
    sum as f64 / data.len() as f64
}

#[cfg(windows)]
unsafe fn grab_current_buffer(grabber: &ISampleGrabber) -> Result<Vec<u8>, String> {
    let mut size = 0i32;
    // First call with null buffer returns required size (may fail with VFW_E_WRONG_STATE briefly).
    let _ = grabber.get_current_buffer(&mut size, std::ptr::null_mut());
    if size <= 0 {
        size = 0;
        grabber
            .get_current_buffer(&mut size, std::ptr::null_mut())
            .map_err(|err| format!("GetCurrentBuffer size: {err}"))?;
    }
    if size <= 0 {
        return Err("GetCurrentBuffer returned empty size".into());
    }
    let mut buf = vec![0u8; size as usize];
    let mut got = size;
    grabber
        .get_current_buffer(&mut got, buf.as_mut_ptr())
        .map_err(|err| format!("GetCurrentBuffer data: {err}"))?;
    if got > 0 && (got as usize) < buf.len() {
        buf.truncate(got as usize);
    }
    Ok(buf)
}

#[cfg(windows)]
unsafe fn open_dshow_com(req: &OpenRequest) -> Result<NativeDshowCam, String> {
    use windows::core::{Interface, w};
    use windows::Win32::Media::DirectShow::{
        IBaseFilter, ICaptureGraphBuilder2, IGraphBuilder, IMediaControl,
    };
    use windows::Win32::Media::MediaFoundation::{
        CLSID_CaptureGraphBuilder2, CLSID_FilterGraph, MEDIATYPE_Video, PIN_CATEGORY_CAPTURE,
        PIN_CATEGORY_PREVIEW,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };

    let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let com_owned = hr.0 == 0; // S_OK
    if !is_com_init_success(hr.0) {
        return Err(format!("CoInitializeEx: {hr:?}"));
    }

    let result = (|| {
        let graph: IGraphBuilder =
            CoCreateInstance(&CLSID_FilterGraph, None, CLSCTX_INPROC_SERVER)
                .map_err(|err| format!("CLSID_FilterGraph: {err}"))?;
        let builder: ICaptureGraphBuilder2 =
            CoCreateInstance(&CLSID_CaptureGraphBuilder2, None, CLSCTX_INPROC_SERVER)
                .map_err(|err| format!("CLSID_CaptureGraphBuilder2: {err}"))?;
        builder
            .SetFiltergraph(&graph)
            .map_err(|err| format!("SetFiltergraph: {err}"))?;

        let capture = bind_capture_filter(req.dshow_index)?;
        graph
            .AddFilter(&capture, w!("Capture"))
            .map_err(|err| format!("AddFilter capture: {err}"))?;

        let grabber_filter: IBaseFilter =
            CoCreateInstance(&CLSID_SAMPLE_GRABBER, None, CLSCTX_INPROC_SERVER)
                .map_err(|err| format!("CLSID_SampleGrabber: {err}"))?;
        let null_filter: IBaseFilter =
            CoCreateInstance(&CLSID_NULL_RENDERER, None, CLSCTX_INPROC_SERVER)
                .map_err(|err| format!("CLSID_NullRenderer: {err}"))?;
        graph
            .AddFilter(&grabber_filter, w!("Grabber"))
            .map_err(|err| format!("AddFilter grabber: {err}"))?;
        graph
            .AddFilter(&null_filter, w!("Null"))
            .map_err(|err| format!("AddFilter null: {err}"))?;

        configure_capture_format(&capture, req)?;

        let grabber: ISampleGrabber = grabber_filter
            .cast()
            .map_err(|err| format!("ISampleGrabber cast: {err}"))?;
        grabber
            .set_one_shot(false.into())
            .map_err(|err| format!("SetOneShot: {err}"))?;
        grabber
            .set_buffer_samples(true.into())
            .map_err(|err| format!("SetBufferSamples: {err}"))?;

        let render = builder.RenderStream(
            Some(&PIN_CATEGORY_CAPTURE),
            &MEDIATYPE_Video,
            &capture,
            &grabber_filter,
            &null_filter,
        );
        if render.is_err() {
            builder
                .RenderStream(
                    Some(&PIN_CATEGORY_PREVIEW),
                    &MEDIATYPE_Video,
                    &capture,
                    &grabber_filter,
                    &null_filter,
                )
                .map_err(|err| format!("RenderStream: {err}"))?;
        }

        let control: IMediaControl = graph
            .cast()
            .map_err(|err| format!("IMediaControl: {err}"))?;
        control.Run().map_err(|err| format!("IMediaControl::Run: {err}"))?;

        let fourcc = normalize_fourcc(&req.fourcc);
        let mut last_err = "no frame".to_string();
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(30));
            match grab_and_convert(&grabber, &fourcc, req.width, req.height) {
                Ok(frame) if frame_has_image(frame.mean, frame.width, frame.height) => {
                    return Ok(NativeDshowCam {
                        control,
                        grabber,
                        width: frame.width,
                        height: frame.height,
                        fourcc,
                        _graph: graph,
                        _builder: builder,
                        _capture: capture,
                        _grabber_filter: grabber_filter,
                        _null: null_filter,
                    });
                }
                Ok(_) => last_err = "black or tiny frame".into(),
                Err(err) => last_err = err,
            }
        }
        let _ = control.Stop();
        Err(format!("DSHOW warmup failed: {last_err}"))
    })();

    if result.is_err() && com_owned {
        CoUninitialize();
    }
    result
}

#[cfg(windows)]
unsafe fn grab_and_convert(
    grabber: &ISampleGrabber,
    fourcc: &str,
    expect_w: i32,
    expect_h: i32,
) -> Result<PreviewFrame, String> {
    let bytes = grab_current_buffer(grabber)?;
    let (width, height, data) = match fourcc {
        "YUY2" => {
            let bgr = yuy2_to_bgr(expect_w, expect_h, &bytes)?;
            (expect_w, expect_h, bgr)
        }
        "MJPG" => mjpg_to_bgr(&bytes)?,
        other => return Err(format!("unsupported fourcc {other}")),
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

#[cfg(windows)]
unsafe fn bind_capture_filter(
    dshow_index: i32,
) -> Result<windows::Win32::Media::DirectShow::IBaseFilter, String> {
    use windows::Win32::Media::DirectShow::{IBaseFilter, ICreateDevEnum};
    use windows::Win32::Media::MediaFoundation::{
        CLSID_SystemDeviceEnum, CLSID_VideoInputDeviceCategory,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, IEnumMoniker, CLSCTX_INPROC_SERVER,
    };

    if dshow_index < 0 {
        return Err("invalid dshow_index".into());
    }
    let dev_enum: ICreateDevEnum =
        CoCreateInstance(&CLSID_SystemDeviceEnum, None, CLSCTX_INPROC_SERVER)
            .map_err(|err| format!("CLSID_SystemDeviceEnum: {err}"))?;
    let mut enum_moniker: Option<IEnumMoniker> = None;
    dev_enum
        .CreateClassEnumerator(&CLSID_VideoInputDeviceCategory, &mut enum_moniker, 0)
        .map_err(|err| format!("CreateClassEnumerator: {err}"))?;
    let enum_moniker = enum_moniker.ok_or_else(|| "no video devices".to_string())?;

    let mut index = 0i32;
    loop {
        let mut fetched = 0u32;
        let mut slot: [Option<windows::Win32::System::Com::IMoniker>; 1] = [None];
        let hr = enum_moniker.Next(&mut slot, Some(&mut fetched));
        if hr.is_err() || fetched == 0 {
            break;
        }
        let Some(moniker) = slot[0].take() else {
            break;
        };
        if index == dshow_index {
            return moniker
                .BindToObject::<_, _, IBaseFilter>(None, None)
                .map_err(|err| format!("BindToObject: {err}"));
        }
        index += 1;
    }
    Err(format!("dshow_index {dshow_index} not found"))
}

#[cfg(windows)]
unsafe fn configure_capture_format(
    filter: &windows::Win32::Media::DirectShow::IBaseFilter,
    req: &OpenRequest,
) -> Result<(), String> {
    use windows::core::Interface;
    use windows::Win32::Media::DirectShow::{IAMStreamConfig, IPin, PINDIR_OUTPUT};
    use windows::Win32::Media::MediaFoundation::AM_MEDIA_TYPE;

    let wanted = normalize_fourcc(&req.fourcc);
    let wanted_fps = req.fps;
    let enum_pins = filter
        .EnumPins()
        .map_err(|err| format!("EnumPins: {err}"))?;
    let mut configured = false;
    loop {
        let mut fetched = 0u32;
        let mut pin_slot: [Option<IPin>; 1] = [None];
        let hr = enum_pins.Next(&mut pin_slot, Some(&mut fetched));
        if hr.is_err() || fetched == 0 {
            break;
        }
        let Some(pin) = pin_slot[0].take() else {
            break;
        };
        let Ok(direction) = pin.QueryDirection() else {
            continue;
        };
        if direction != PINDIR_OUTPUT {
            continue;
        }
        let Ok(config) = pin.cast::<IAMStreamConfig>() else {
            continue;
        };
        let mut count = 0i32;
        let mut cap_size = 0i32;
        if config
            .GetNumberOfCapabilities(&mut count, &mut cap_size)
            .is_err()
            || count <= 0
        {
            continue;
        }
        let buf_len = cap_size.max(1) as usize;
        for index in 0..count {
            let mut scc = vec![0u8; buf_len];
            let mut media_type: *mut AM_MEDIA_TYPE = std::ptr::null_mut();
            if config
                .GetStreamCaps(index, &mut media_type, scc.as_mut_ptr())
                .is_err()
                || media_type.is_null()
            {
                continue;
            }
            let matched = match_media_type(media_type, req.width, req.height, &wanted, wanted_fps);
            if matched {
                let set = config.SetFormat(media_type);
                delete_media_type(media_type);
                set.map_err(|err| format!("SetFormat: {err}"))?;
                configured = true;
                break;
            }
            delete_media_type(media_type);
        }
        if configured {
            break;
        }
    }
    if !configured {
        // Still try to run with default pin format.
        return Ok(());
    }
    Ok(())
}

#[cfg(windows)]
unsafe fn match_media_type(
    media_type: *mut windows::Win32::Media::MediaFoundation::AM_MEDIA_TYPE,
    width: i32,
    height: i32,
    fourcc: &str,
    fps: i32,
) -> bool {
    use windows::Win32::Media::MediaFoundation::{FORMAT_VideoInfo, VIDEOINFOHEADER};

    let mt = &*media_type;
    if mt.formattype != FORMAT_VideoInfo
        || mt.pbFormat.is_null()
        || (mt.cbFormat as usize) < std::mem::size_of::<VIDEOINFOHEADER>()
    {
        return false;
    }
    let header = &*(mt.pbFormat as *const VIDEOINFOHEADER);
    let w = header.bmiHeader.biWidth;
    let h = header.bmiHeader.biHeight.abs();
    if w != width || h != height {
        return false;
    }
    let Some(wanted_guid) = subtype_guid_for_fourcc(fourcc) else {
        return false;
    };
    if mt.subtype != wanted_guid {
        return false;
    }
    if fps > 0 {
        let got = fps_from_avg_time_per_frame(header.AvgTimePerFrame);
        if got > 0 && (got - fps).abs() > 1 {
            return false;
        }
    }
    true
}

#[cfg(windows)]
unsafe fn delete_media_type(
    media_type: *mut windows::Win32::Media::MediaFoundation::AM_MEDIA_TYPE,
) {
    use windows::Win32::System::Com::CoTaskMemFree;

    if media_type.is_null() {
        return;
    }
    let mt = &mut *media_type;
    if !mt.pbFormat.is_null() {
        CoTaskMemFree(Some(mt.pbFormat as *const _));
        mt.pbFormat = std::ptr::null_mut();
    }
    CoTaskMemFree(Some(media_type as *const _));
}

#[cfg(test)]
mod tests {
    #[test]
    fn sample_grabber_clsid_matches_qedit() {
        assert_eq!(
            super::CLSID_SAMPLE_GRABBER.data1,
            0x0579_154a
        );
    }
}
