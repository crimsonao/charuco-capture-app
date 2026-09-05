//! Convert packed camera buffers to BGR8 for the detect/JPEG pipeline.

#[repr(C)]
struct CvFrame {
    data: *mut u8,
    width: i32,
    height: i32,
    channels: i32,
}

#[link(name = "opencv_capture")]
extern "C" {
    fn cvcam_imdecode_bgr(data: *const u8, nbytes: i32, out: *mut CvFrame) -> i32;
    fn cvcam_frame_free(frame: *mut CvFrame);
}

/// Convert packed YUY2 (YUYV) to BGR8. Width must be even.
pub fn yuy2_to_bgr(width: i32, height: i32, yuy2: &[u8]) -> Result<Vec<u8>, String> {
    if width <= 0 || height <= 0 {
        return Err("invalid YUY2 size".into());
    }
    if width % 2 != 0 {
        return Err("YUY2 width must be even".into());
    }
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(2);
    if yuy2.len() < expected {
        return Err(format!(
            "YUY2 buffer too short: need {expected}, got {}",
            yuy2.len()
        ));
    }
    let mut bgr = vec![0u8; (width as usize) * (height as usize) * 3];
    let row_bytes = (width as usize) * 2;
    for y in 0..height as usize {
        let src_row = &yuy2[y * row_bytes..y * row_bytes + row_bytes];
        let dst_row = &mut bgr[y * (width as usize) * 3..(y + 1) * (width as usize) * 3];
        let mut x = 0usize;
        while x < width as usize {
            let i = x * 2;
            let y0 = src_row[i] as f64;
            let u = src_row[i + 1] as f64;
            let y1 = src_row[i + 2] as f64;
            let v = src_row[i + 3] as f64;
            let (b0, g0, r0) = yuv_to_bgr(y0, u, v);
            let (b1, g1, r1) = yuv_to_bgr(y1, u, v);
            let o = x * 3;
            dst_row[o] = b0;
            dst_row[o + 1] = g0;
            dst_row[o + 2] = r0;
            dst_row[o + 3] = b1;
            dst_row[o + 4] = g1;
            dst_row[o + 5] = r1;
            x += 2;
        }
    }
    Ok(bgr)
}

fn yuv_to_bgr(y: f64, u: f64, v: f64) -> (u8, u8, u8) {
    let c = y - 16.0;
    let d = u - 128.0;
    let e = v - 128.0;
    let r = (298.0 * c + 409.0 * e + 128.0) / 256.0;
    let g = (298.0 * c - 100.0 * d - 208.0 * e + 128.0) / 256.0;
    let b = (298.0 * c + 516.0 * d + 128.0) / 256.0;
    (clamp_u8(b), clamp_u8(g), clamp_u8(r))
}

fn clamp_u8(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

/// Decode MJPG bytes to BGR8 via OpenCV `imdecode`.
pub fn mjpg_to_bgr(mjpg: &[u8]) -> Result<(i32, i32, Vec<u8>), String> {
    if mjpg.is_empty() {
        return Err("empty MJPG buffer".into());
    }
    let mut raw = CvFrame {
        data: std::ptr::null_mut(),
        width: 0,
        height: 0,
        channels: 0,
    };
    let ok = unsafe { cvcam_imdecode_bgr(mjpg.as_ptr(), mjpg.len() as i32, &mut raw) };
    if ok == 0 || raw.data.is_null() || raw.width <= 0 || raw.height <= 0 {
        unsafe { cvcam_frame_free(&mut raw) };
        return Err("MJPG decode failed".into());
    }
    let channels = raw.channels.max(1) as usize;
    let width = raw.width;
    let height = raw.height;
    let nbytes = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(channels);
    let mut data = vec![0u8; nbytes];
    unsafe {
        std::ptr::copy_nonoverlapping(raw.data, data.as_mut_ptr(), nbytes);
        cvcam_frame_free(&mut raw);
    }
    if channels != 3 {
        return Err(format!("expected BGR3, got {channels} channels"));
    }
    Ok((width, height, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuy2_rejects_odd_width() {
        assert!(yuy2_to_bgr(3, 2, &[0u8; 12]).is_err());
    }
}
