//! JPEG encode via the OpenCV C++ FFI.

#[repr(C)]
struct CvFrame {
    data: *mut u8,
    width: i32,
    height: i32,
    channels: i32,
}

#[link(name = "opencv_capture")]
extern "C" {
    fn cvcam_imencode_jpeg(
        frame: *const CvFrame,
        quality: i32,
        out: *mut *mut u8,
        out_len: *mut i32,
    ) -> i32;
    fn cvcam_bytes_free(ptr: *mut u8);
}

pub fn encode_jpeg_bytes(
    data: &[u8],
    width: i32,
    height: i32,
    channels: i32,
    quality: i32,
) -> Result<Vec<u8>, String> {
    let raw = CvFrame {
        data: data.as_ptr() as *mut u8,
        width,
        height,
        channels,
    };
    let mut out = std::ptr::null_mut();
    let mut out_len = 0i32;
    let ok = unsafe { cvcam_imencode_jpeg(&raw, quality, &mut out, &mut out_len) };
    if ok == 0 || out.is_null() || out_len <= 0 {
        return Err("imencode failed".into());
    }
    let bytes = unsafe { std::slice::from_raw_parts(out, out_len as usize) }.to_vec();
    unsafe { cvcam_bytes_free(out) };
    Ok(bytes)
}
