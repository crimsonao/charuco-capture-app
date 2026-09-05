use charuco_capture_app_lib::frame_convert::{mjpg_to_bgr, yuy2_to_bgr};

#[test]
fn yuy2_to_bgr_rejects_odd_width_or_short_buffer() {
    assert!(yuy2_to_bgr(3, 2, &[0u8; 12]).is_err());
    assert!(yuy2_to_bgr(2, 2, &[0u8; 7]).is_err());
}

#[test]
fn yuy2_to_bgr_outputs_bgr_len() {
    let yuy2 = [16u8, 128, 16, 128, 16, 128, 16, 128];
    let bgr = yuy2_to_bgr(2, 2, &yuy2).expect("convert");
    assert_eq!(bgr.len(), 2 * 2 * 3);
}

#[test]
fn mjpg_to_bgr_rejects_invalid_bytes() {
    let err = mjpg_to_bgr(&[0u8, 1, 2, 3]).expect_err("invalid jpeg");
    assert!(err.contains("decode") || err.contains("MJPG"), "{err}");
}
