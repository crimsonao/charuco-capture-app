//! Hardware harness: open by friendly name, time first usable frame, log backend.
//!
//! Not a package binary — `pnpm tauri dev` must run `src/main.rs`.
//!
//! ```bat
//! cargo run --example open-by-name -- ocal4 1280 720 MJPG 1 30
//! ```

use std::time::Instant;

use charuco_capture_app_lib::camera::{
    frame_has_image, frame_mean, list_msmf_names, open_capture, OpenRequest,
};

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
    let fps: i32 = std::env::args()
        .nth(6)
        .and_then(|value| value.parse().ok())
        .unwrap_or(30);

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
        fps,
    };
    eprintln!(
        "Opening {} via name (DSHOW fallback index {}) {}x{} {} @{}fps...",
        req.device_name, req.dshow_index, req.width, req.height, req.fourcc, req.fps
    );

    let open_started = Instant::now();
    let mut opened = match open_capture(&req) {
        Ok(opened) => opened,
        Err(err) => {
            eprintln!("OPEN_FAIL {err}");
            std::process::exit(1);
        }
    };

    let mut first_usable_ms: Option<u128> = None;
    for index in 0..30 {
        match opened.read_frame() {
            Ok(frame) => {
                let mean = frame_mean(&frame);
                let usable = frame_has_image(mean, frame.width, frame.height);
                eprintln!(
                    "frame {index}: backend={} device={} size={}x{} mean={mean:.2} usable={usable}",
                    opened.backend, req.device_name, frame.width, frame.height
                );
                if usable && first_usable_ms.is_none() {
                    first_usable_ms = Some(open_started.elapsed().as_millis());
                    break;
                }
            }
            Err(_) => eprintln!("frame {index}: empty"),
        }
    }

    match first_usable_ms {
        Some(elapsed_ms) => eprintln!(
            "RESULT backend={} elapsed_ms={elapsed_ms} requested={}x{} actual={}x{}",
            opened.backend, req.width, req.height, opened.width, opened.height
        ),
        None => {
            eprintln!(
                "RESULT backend={} elapsed_ms=timeout requested={}x{} actual={}x{} (no non-black frame)",
                opened.backend, req.width, req.height, opened.width, opened.height
            );
            opened.release();
            std::process::exit(2);
        }
    }
    opened.release();
}
