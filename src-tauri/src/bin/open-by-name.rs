//! Hardware harness: open by friendly name, read a few frames, log backend/mean/size.

use charuco_capture_app_lib::camera::{frame_mean, list_msmf_names, open_capture, OpenRequest};

fn main() {
    let device_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ocal4".to_string());
    let width: i32 = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1280);
    let height: i32 = std::env::args()
        .nth(3)
        .and_then(|value| value.parse().ok())
        .unwrap_or(720);
    let fourcc = std::env::args()
        .nth(4)
        .unwrap_or_else(|| "MJPG".to_string());
    let dshow_index: i32 = std::env::args()
        .nth(5)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);

    match list_msmf_names() {
        Ok(names) => {
            eprintln!("MSMF names:");
            for (index, name) in names.iter().enumerate() {
                eprintln!("  {index}: {name}");
            }
        }
        Err(err) => eprintln!("MSMF enum failed: {err}"),
    }

    let req = OpenRequest {
        device_name: device_name.clone(),
        dshow_index,
        width,
        height,
        fourcc,
    };
    eprintln!(
        "Opening {} via name (DSHOW fallback index {}) {}x{} {}...",
        req.device_name, req.dshow_index, req.width, req.height, req.fourcc
    );

    let mut opened = match open_capture(&req) {
        Ok(opened) => opened,
        Err(err) => {
            eprintln!("OPEN_FAIL {err}");
            std::process::exit(1);
        }
    };

    eprintln!(
        "opened backend={} requested={}x{} actual={}x{}",
        opened.backend, req.width, req.height, opened.width, opened.height
    );

    for index in 0..5 {
        match opened.read_frame() {
            Ok(frame) => {
                let mean = frame_mean(&frame);
                eprintln!(
                    "frame {index}: backend={} device={} size={}x{} mean={mean:.2}",
                    opened.backend, req.device_name, frame.width, frame.height
                );
            }
            Err(_) => eprintln!("frame {index}: empty"),
        }
    }
    opened.release();
}
