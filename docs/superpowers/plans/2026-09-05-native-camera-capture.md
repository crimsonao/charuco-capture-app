# Native DSHOW/MSMF Camera Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open preview via native DirectShow then Media Foundation (BGR frames), with OpenCV `VideoCapture` only as fallback, enumerate/apply FPS, and target cold-start first frame ≤ ~2s.

**Architecture:** Keep detect/JPEG on BGR8 `PreviewFrame`. Add `frame_convert`, `native_msmf_capture`, and `native_dshow_capture` modules. Refactor `OpenedCam` to an enum (`Native` | `OpenCv`). Orchestrate open as native DSHOW → native MSMF → existing OpenCV chain. Extend enum/`OpenRequest` with integer FPS from `AvgTimePerFrame`.

**Tech Stack:** Tauri 2, Rust, `windows` 0.61 (DirectShow + Media Foundation), existing OpenCV C ABI for detect/JPEG and OpenCV open fallback, Vue 3 Setup page.

**Spec:** `docs/superpowers/specs/2026-09-05-native-camera-capture-design.md`

## Global Constraints

- Windows only for capture; never `VideoCapture(dshow_index, CAP_MSMF)`.
- Do not switch to another camera on failure.
- Detection/JPEG path stays BGR8; do not remove OpenCV from the app.
- Cold-start success target: first usable frame ≤ ~2s on typical USB + selected MJPG/YUY2.
- Open order (cold): native DSHOW → native MSMF → OpenCV; success cache may reorder later.
- FPS: show in Setup and request on open; ignore FPS mismatch if frames appear.
- Dedup modes by `(width, height, fourcc, fps)`.
- Prefer small focused modules; TDD; commit after each task.

## File map

| Path | Responsibility |
|------|----------------|
| `src-tauri/src/camera.rs` | Enumerate modes + FPS; `CameraMode.fps` |
| `src-tauri/src/camera_open.rs` | Orchestrate open; `OpenedCam` enum; OpenCV fallback + `CAP_PROP_FPS` |
| `src-tauri/src/frame_convert.rs` | YUY2→BGR, MJPG→BGR helpers |
| `src-tauri/src/native_msmf_capture.rs` | MF Source Reader open/read |
| `src-tauri/src/native_dshow_capture.rs` | DSHOW graph + Sample Grabber open/read |
| `src-tauri/src/lib.rs` | `mod` declarations |
| `src-tauri/Cargo.toml` | Extra `windows` features if needed |
| `src-tauri/tests/camera_modes_test.rs` | FPS enum/dedup tests |
| `src-tauri/tests/frame_convert_test.rs` | Conversion unit tests |
| `src-tauri/tests/camera_open_test.rs` | Open-order / request shape tests |
| `src/pages/SetupPage.vue` | FPS column + row key |
| `src/pages/CapturePage.vue` | Pass `fps` in mode type |
| `src-tauri/examples/open-by-name.rs` | Optional fps arg (if present on branch) |

---

### Task 0: Commit pending open-path perf (if dirty)

**Files:**
- Existing dirty: `src-tauri/src/camera.rs`, `src-tauri/src/camera_open.rs`, `src-tauri/tests/camera_open_test.rs` (DSHOW-first / attempt budget) if still uncommitted

- [ ] **Step 1: Check status**

Run: `git status --short`

- [ ] **Step 2: If those three files are modified, commit them alone**

```bash
git add src-tauri/src/camera.rs src-tauri/src/camera_open.rs src-tauri/tests/camera_open_test.rs
git commit -m "perf(camera): prefer DSHOW and cap per-attempt open budget"
```

If already clean for those files, skip.

---

### Task 1: FPS on discrete mode enumeration

**Files:**
- Modify: `src-tauri/src/camera.rs`
- Modify: `src-tauri/tests/camera_modes_test.rs`
- Modify: `src/pages/SetupPage.vue`
- Modify: `src/pages/CapturePage.vue` (CameraMode interface)

**Interfaces:**
- Produces: `CameraMode { ..., fps: i32 }`, `fps_from_avg_time_per_frame(avg_time_per_frame_hns: i64) -> i32`, `collect_discrete_modes` → `Vec<(i32, i32, String, i32)>` `(w,h,fourcc,fps)`, `StreamCapSlot.avg_time_per_frame: i64`

- [ ] **Step 1: Write failing tests**

In `src-tauri/tests/camera_modes_test.rs` add:

```rust
#[test]
fn fps_from_avg_time_per_frame_rounds_common_rates() {
    use charuco_capture_app_lib::camera::fps_from_avg_time_per_frame;
    // 10000000 / 333333 ≈ 30
    assert_eq!(fps_from_avg_time_per_frame(333_333), 30);
    assert_eq!(fps_from_avg_time_per_frame(400_000), 25);
    assert_eq!(fps_from_avg_time_per_frame(0), 0);
    assert_eq!(fps_from_avg_time_per_frame(-1), 0);
}

#[test]
fn collect_discrete_keeps_same_size_fourcc_different_fps() {
    fn slot(w: i32, h: i32, label: &str, avg: i64) -> StreamCapSlot {
        StreamCapSlot {
            format_is_video_info: true,
            header_width: w,
            header_height: h,
            subtype_label: label.into(),
            avg_time_per_frame: avg,
            range_min_width: 0,
            range_min_height: 0,
            range_max_width: 0,
            range_max_height: 0,
        }
    }
    let modes = collect_discrete_modes(&[
        slot(1280, 720, "MJPG", 333_333),
        slot(1280, 720, "MJPG", 666_667),
    ]);
    assert_eq!(
        modes,
        vec![
            (1280, 720, "MJPG".into(), 30),
            (1280, 720, "MJPG".into(), 15),
        ]
    );
}
```

Update every existing `StreamCapSlot { ... }` literal in this file to include `avg_time_per_frame: 333_333` (or `0` where irrelevant). Update assertions that compare `(w,h,fourcc)` tuples to include fps.

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test --test camera_modes_test --manifest-path src-tauri/Cargo.toml fps_from_avg_time_per_frame -- --nocapture`

Expected: compile error / missing `fps_from_avg_time_per_frame` or missing field.

- [ ] **Step 3: Implement**

Add to `camera.rs`:

```rust
pub fn fps_from_avg_time_per_frame(avg_time_per_frame_hns: i64) -> i32 {
    if avg_time_per_frame_hns <= 0 {
        return 0;
    }
    let fps = 10_000_000.0 / (avg_time_per_frame_hns as f64);
    fps.round() as i32
}
```

- Add `pub avg_time_per_frame: i64` to `StreamCapSlot`.
- Change `CameraMode` to include `pub fps: i32`.
- Change `collect_discrete_modes` / `prefer_larger_modes` to use `(i32, i32, String, i32)`.
- In `parse_videoinfo_slot`, set `avg_time_per_frame: header.AvgTimePerFrame` (i64).
- When pushing `CameraMode`, set `fps: fps_from_avg_time_per_frame(...)`.

SetupPage / CapturePage:

```ts
interface CameraMode {
  device_name: string
  dshow_index: number
  width: number
  height: number
  fourcc: string
  fps: number
}
```

Row key includes fps; table header + cell for FPS; `aria-label` includes fps.

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test --test camera_modes_test --manifest-path src-tauri/Cargo.toml`

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/camera.rs src-tauri/tests/camera_modes_test.rs src/pages/SetupPage.vue src/pages/CapturePage.vue
git commit -m "feat(camera): enumerate and display mode FPS"
```

---

### Task 2: Frame convert helpers (YUY2 / MJPG → BGR)

**Files:**
- Create: `src-tauri/src/frame_convert.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod frame_convert;`)
- Create: `src-tauri/tests/frame_convert_test.rs`
- Possibly extend: `src-tauri/native/opencv_capture.cpp` + header with `cvcam_imdecode_bgr` if MJPG uses OpenCV decode

**Interfaces:**
- Produces:
  - `pub fn yuy2_to_bgr(width: i32, height: i32, yuy2: &[u8]) -> Result<Vec<u8>, String>`
  - `pub fn mjpg_to_bgr(mjpg: &[u8]) -> Result<(i32, i32, Vec<u8>), String>`
- BGR output length = `width * height * 3`

- [ ] **Step 1: Write failing YUY2 test**

```rust
use charuco_capture_app_lib::frame_convert::yuy2_to_bgr;

#[test]
fn yuy2_to_bgr_rejects_odd_width_or_short_buffer() {
    assert!(yuy2_to_bgr(3, 2, &[0u8; 12]).is_err());
    assert!(yuy2_to_bgr(2, 2, &[0u8; 7]).is_err());
}

#[test]
fn yuy2_to_bgr_outputs_bgr_len() {
    // 2x2 YUY2 = 8 bytes
    let yuy2 = [16u8, 128, 16, 128, 16, 128, 16, 128];
    let bgr = yuy2_to_bgr(2, 2, &yuy2).expect("convert");
    assert_eq!(bgr.len(), 2 * 2 * 3);
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test --test frame_convert_test --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 3: Implement `yuy2_to_bgr`**

Standard BT.601 YUY2 packed convert into BGR8 (clamp 0..255). Pure Rust; no COM.

- [ ] **Step 4: Add MJPG decode via thin OpenCV helper**

In C++ (`opencv_capture.cpp` / `.h`):

```cpp
// returns 1 on success; out owns malloc'd BGR buffer
int cvcam_imdecode_bgr(const unsigned char *data, int nbytes,
                       CvFrame *out);
```

Rust:

```rust
pub fn mjpg_to_bgr(mjpg: &[u8]) -> Result<(i32, i32, Vec<u8>), String> { /* FFI + copy */ }
```

Test: tiny invalid JPEG returns Err; optional fixture pass if you add a 1x1 jpeg bytes constant.

- [ ] **Step 5: Run tests — PASS; commit**

```bash
git add src-tauri/src/frame_convert.rs src-tauri/src/lib.rs src-tauri/tests/frame_convert_test.rs src-tauri/native/opencv_capture.cpp src-tauri/native/opencv_capture.h
git commit -m "feat(camera): convert YUY2 and MJPG buffers to BGR"
```

---

### Task 3: `OpenRequest.fps` + OpenCV fallback sets FPS

**Files:**
- Modify: `src-tauri/src/camera_open.rs` (`OpenRequest`, OpenCV `try_open_backend_until`)
- Modify: `src-tauri/tests/camera_open_test.rs`
- Modify: helpers/`ocal4_720p_yuy2()` to include `fps: 30`

**Interfaces:**
- Produces: `OpenRequest { ..., fps: i32 }`
- OpenCV path: `cvcam_set(..., CAP_PROP_FPS, fps as f64)` when `fps > 0`

- [ ] **Step 1: Failing compile/tests after adding `fps` to struct — update all `OpenRequest { ... }` literals in tests, examples, and app.

- [ ] **Step 2: In `try_open_backend_until`, after size props:**

```rust
if req.fps > 0 {
    unsafe { cvcam_set(capture.ptr, CAP_PROP_FPS, f64::from(req.fps)); }
}
```

(`CAP_PROP_FPS` = 5 in OpenCV)

- [ ] **Step 3: `cargo test --test camera_open_test --manifest-path src-tauri/Cargo.toml` PASS**

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(camera): pass FPS into OpenRequest and OpenCV open"
```

(Prefer explicit `git add` of touched files.)

---

### Task 4: Capture backend trait + `OpenedCam` enum scaffold

**Files:**
- Modify: `src-tauri/src/camera_open.rs`

**Interfaces:**
- Produces:

```rust
pub trait FrameSource {
    fn read_bgr(&mut self) -> Result<PreviewFrame, String>;
    fn backend_name(&self) -> &str;
    fn size(&self) -> (i32, i32);
}

enum CaptureBackend {
    OpenCv(CameraHandle),
    // NativeMsmf / NativeDshow filled in later tasks
}

pub struct OpenedCam {
    pub width: i32,
    pub height: i32,
    pub backend: String,
    backend_impl: CaptureBackend,
}
```

- Keep `OpenedCam::read_frame` calling through enum.
- OpenCV-only behavior must still pass existing unit tests (`timeout_tests`, prepare_preview).

- [ ] **Step 1: Refactor without behavior change (OpenCV only in enum)**

- [ ] **Step 2: `cargo test --manifest-path src-tauri/Cargo.toml --lib timeout_tests` and `cargo test --test camera_open_test --manifest-path src-tauri/Cargo.toml` PASS**

- [ ] **Step 3: Commit**

```bash
git commit -am "refactor(camera): OpenedCam enum for multiple capture backends"
```

---

### Task 5: Native MSMF Source Reader

**Files:**
- Create: `src-tauri/src/native_msmf_capture.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml` — add windows features as needed, e.g. `Win32_Media_MediaFoundation` already present; may need `Win32_System_Com_StructuredStorage` (have), and GUID helpers
- Modify: `src-tauri/src/camera_open.rs` — call native MSMF before OpenCV
- Test: unit tests for media-type builder / fourcc→GUID without device if possible; hardware test optional via `#[ignore]`

**Interfaces:**
- Consumes: `OpenRequest`, `frame_convert`, `match_device_index` / MSMF name list
- Produces:

```rust
pub struct NativeMsmfCam { /* IMFSourceReader + size + fourcc */ }
impl NativeMsmfCam {
    pub fn open(req: &OpenRequest, msmf_index: i32) -> Result<Self, String>;
    pub fn read_bgr(&mut self) -> Result<PreviewFrame, String>;
}
```

**Open steps (implementation sketch):**
1. STA thread or document that preview thread owns COM (match existing MSMF enum pattern: open on preview thread after CoInitialize if needed).
2. `MFCreateDeviceSource` / activate from enumerated index matching `msmf_index`.
3. `MFCreateSourceReaderFromMediaSource`.
4. Set current media type: subtype from FOURCC GUID, frame size, frame rate (`MF_MT_FRAME_RATE` numerator=`fps`, denominator=1 when `fps > 0`).
5. `ReadSample` → lock buffer → if MJPG/YUY2 convert to BGR; if already RGB24/BGR24 map carefully.
6. Fill `PreviewFrame` including `mean` (reuse `frame_mean` logic or compute on BGR).

**Wire in `open_capture`:**
After preferred ordering, for each plan entry:
- `NativeDshow` (Task 6) / `NativeMsmf` / `OpenCv`
For this task: try native MSMF when attempt backend is MSMF **before** OpenCV MSMF, or insert explicit native MSMF attempt ahead of entire OpenCV chain:

Recommended orchestration for Tasks 5–6:

```text
1) native DSHOW (dshow_index, fourcc, size, fps)
2) native MSMF (friendly-name index, fourcc, size, fps)
3) existing OpenCV attempts (DSHOW/MSMF × codecs) with shared budgets
```

- [ ] **Step 1: Implement `NativeMsmfCam::open` + `read_bgr`; compile**

- [ ] **Step 2: Unit-test fourcc GUID packing if extracted as `fn subtype_guid_for_fourcc(fourcc: &str) -> Option<GUID>`**

- [ ] **Step 3: Integrate into `open_capture` ahead of OpenCV; remember success as backend `"MSMF"`**

- [ ] **Step 4: Manual check with camera if available; otherwise `cargo test` suite green**

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(camera): native MSMF Source Reader capture path"
```

---

### Task 6: Native DirectShow Sample Grabber

**Files:**
- Create: `src-tauri/src/native_dshow_capture.rs`
- Modify: `src-tauri/src/lib.rs`, `camera_open.rs`
- May define Sample Grabber GUIDs manually (qedit) if missing from `windows` crate:

```rust
// CLSID_SampleGrabber / IID_ISampleGrabber — document source (qedit.h)
```

**Interfaces:**
- Produces:

```rust
pub struct NativeDshowCam { /* graph + grabber + size */ }
impl NativeDshowCam {
    pub fn open(req: &OpenRequest) -> Result<Self, String>;
    pub fn read_bgr(&mut self) -> Result<PreviewFrame, String>;
}
```

**Open steps:**
1. Build filter graph: enum moniker at `dshow_index`, bind `IBaseFilter`.
2. Find capture output pin with `IAMStreamConfig`; `SetFormat` matching width/height/FOURCC/`AvgTimePerFrame` from `fps` (`10_000_000 / fps` when fps > 0).
3. Add Sample Grabber + Null Renderer; connect.
4. Sample Grabber: one-shot or buffer mode; prefer **callback or GetCurrentBuffer** after `IMediaControl::Run`.
5. Convert buffer with `frame_convert`; return BGR `PreviewFrame`.
6. Drop tears down graph.

**Wire:** native DSHOW is **first** cold-start attempt in `open_capture`.

Timeout: if first BGR frame not ready within ~2–3s, destroy graph and return Err so MSMF/OpenCV can run.

- [ ] **Step 1: Implement + compile**

- [ ] **Step 2: Integrate as first open path**

- [ ] **Step 3: `cargo test` relevant suites PASS**

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(camera): native DirectShow Sample Grabber capture path"
```

---

### Task 7: Open orchestration polish + success cache

**Files:**
- Modify: `src-tauri/src/camera_open.rs`
- Modify: `src-tauri/tests/camera_open_test.rs`

**Behavior checklist:**
- Cold order: native DSHOW → native MSMF → OpenCV chain.
- `preferred_backend_for_open` still defaults to `"DSHOW"`; cache may prefer `"MSMF"` or `"OPENCV-DSHOW"` etc.
- Backend strings: `"DSHOW"`, `"MSMF"`, `"OPENCV-MSMF"`, `"OPENCV-DSHOW"` (OpenCV paths prefixed so cache does not confuse native vs OpenCV).
- Shared `OPEN_BUDGET` + per-attempt slice for native and OpenCV.
- Minimal warmup (1–2 non-black frames) on native success.

- [ ] **Step 1: Tests for open plan order helper**

```rust
#[test]
fn cold_open_plan_is_native_dshow_msmf_then_opencv() {
    // assert a pure function open_plan_steps(req, msmf_names, preferred) order
}
```

Extract `pub fn open_plan_steps(...) -> Vec<OpenStep>` if needed for testability.

- [ ] **Step 2: Implement / adjust; tests PASS**

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(camera): orchestrate native-first open with OpenCV fallback"
```

---

### Task 8: Harness + README note + verification

**Files:**
- Modify: `src-tauri/examples/open-by-name.rs` (if on branch) — accept fps arg, print elapsed ms to first frame
- Modify: `README.md` — one short note that open uses native DSHOW/MSMF then OpenCV; modes include FPS

- [ ] **Step 1: Harness prints `elapsed_ms` until first non-black frame and `backend=`**

- [ ] **Step 2: On a machine with camera:**

```bat
cd src-tauri
cargo run --example open-by-name -- ocal4 1280 720 MJPG 1 30
```

Expected: backend `DSHOW` or `MSMF`, elapsed ideally ≤ ~2000ms.

- [ ] **Step 3: `pnpm tauri dev` — Setup shows FPS; Start capture shows backend; preview works**

- [ ] **Step 4: Commit**

```bash
git commit -am "docs(camera): note native open path and harness FPS"
```

---

## Spec coverage (self-review)

| Spec item | Task |
|-----------|------|
| Native DSHOW + MSMF + OpenCV fallback | 5, 6, 7 |
| Fallback only after native fail | 7 |
| ≤ ~2s cold start target | 6 timeout + 8 verify |
| Rust `windows` crate | 5, 6 |
| FPS enum + open | 1, 3, 5, 6 |
| No wrong-device MSMF index | 5 (friendly name), Global Constraints |
| BGR preview unchanged | 2, 4 |
| Non-goals (progress UI, drop OpenCV) | not scheduled |

## Placeholder scan

No TBD/TODO left in task steps; Sample Grabber GUIDs must be concrete constants in Task 6 implementation (from qedit.h), not left blank.

## Type consistency

- `fps: i32` on `CameraMode` and `OpenRequest`
- `collect_discrete_modes` → `(w,h,fourcc,fps)`
- Backends: `DSHOW` / `MSMF` / `OPENCV-*`
- `PreviewFrame` unchanged shape
