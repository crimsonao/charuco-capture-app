//! Native DirectShow capture (Sample Grabber).
//!
//! Stub that fails fast until the Sample Grabber graph is implemented; OpenCV
//! DSHOW remains the fallback. Native MSMF covers the fast path for many devices.

use crate::camera_open::{OpenRequest, PreviewFrame};

pub struct NativeDshowCam {
    width: i32,
    height: i32,
}

impl NativeDshowCam {
    pub fn open(req: &OpenRequest) -> Result<Self, String> {
        let _ = req;
        Err("native DirectShow Sample Grabber not implemented yet".into())
    }

    pub fn read_bgr(&mut self) -> Result<PreviewFrame, String> {
        Err("native DirectShow not open".into())
    }

    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }
}
