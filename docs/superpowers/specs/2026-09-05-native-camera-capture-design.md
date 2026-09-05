# Native camera capture (DSHOW + MSMF) design

Date: 2026-09-05  
Branch context: `perf/faster-camera-open`  
Status: approved for planning

## Problem

Opening preview still spends too long on “正在打开相机…”. OpenCV `VideoCapture` blocks during opaque backend/format negotiation. Setup already knows the exact DirectShow mode (device, index, size, FOURCC); that information is underused at open time.

## Goals

- Cold-start time to first usable preview frame **≤ ~2 seconds** on a typical USB camera with a user-selected MJPG or YUY2 mode.
- Prefer **native** Windows capture over OpenCV negotiation.
- Keep Charuco detect / JPEG preview pipeline unchanged (BGR8 frames in).
- Never open the wrong device (no `VideoCapture(dshow_index, CAP_MSMF)`).

## Non-goals

- Progress-event UI (“正在试 MSMF/MJPG…”) in this phase.
- Removing OpenCV from the app (detect still needs it).
- Non-Windows platforms.
- Replacing the entire media stack beyond open + grab.

## Decisions (from brainstorm)

| Topic | Choice |
|-------|--------|
| Scope | Native **DSHOW + MSMF**, OpenCV as fallback |
| Fallback policy | Native fails first, then OpenCV (success cache may reorder later opens) |
| Success metric | Cold-start first frame ≤ ~2s (common USB + selected MJPG/YUY2) |
| Implementation | **Rust `windows` crate** native capture (Approach A) |
| FPS | Enumerate **and** request FPS on open |

## Architecture

Split **capture** from **vision**:

| Layer | Responsibility |
|-------|----------------|
| `native_dshow_capture` | Open by DirectShow index; set MEDIA TYPE (size, FOURCC, frame interval); grab → BGR8 |
| `native_msmf_capture` | Open by friendly-name MSMF index; `IMFSourceReader`; set subtype/size/rate; grab → BGR8 |
| `frame_convert` | YUY2→BGR, MJPG→BGR (MJPG may use OpenCV `imdecode` only, not `VideoCapture`) |
| `OpenCvCapture` (existing) | Last-resort open via current `VideoCapture` attempt chain |
| `OpenedCam` | Unified `read() → PreviewFrame`; `backend` label `DSHOW` / `MSMF` / `OPENCV-*` |
| detect / JPEG / `run_preview_loop` | Unchanged consumers of BGR8 `PreviewFrame` |

### Open order (cold start)

1. Native DirectShow (matches Setup enumeration)
2. Native Media Foundation (friendly-name index; never DSHOW index as MSMF index)
3. Existing OpenCV `VideoCapture` chain

With a success cache, prefer the last successful backend (+ fourcc, optionally fps) first, then the remaining native/OpenCV path.

### Timeouts

- Per native attempt: short budget (~2–3s), drop handle on failure/timeout, try next.
- Overall open budget retained so OpenCV fallback cannot run unbounded.
- Note: blocking COM/`VideoCapture` constructors still cannot be preempted mid-call; ordering and short attempts limit damage.

## Data flow

1. **Enumerate (Setup)** — existing DirectShow enum, extended with FPS from `VIDEOINFOHEADER.AvgTimePerFrame` (e.g. `10_000_000 / AvgTimePerFrame`). Dedup key: `(width, height, fourcc, fps)`.
2. **UI** — Setup table shows FPS; selected row passes `fps` into `OpenRequest` / `start_preview`.
3. **Open** — native path sets size + FOURCC + frame interval/rate; convert first frame(s) to BGR; require non-black usable frame (same spirit as today’s warmup, but minimal).
4. **Preview loop** — existing detect + JPEG emit path.

### Errors

- Failures accumulate per attempt; **do not switch to another camera**.
- All attempts failed → existing user-facing error plus attempt detail.
- Mid-session disconnect → existing “摄像头断开” behavior.

### FPS behavior

- Display in Setup.
- Apply on open for native DSHOW, native MSMF, and OpenCV fallback (`CAP_PROP_FPS` or equivalent media type).
- Drivers that ignore rate still succeed if frames appear; do not fail solely on FPS mismatch.

## Components (file-level intent)

- Extend `CameraMode` / `OpenRequest` with `fps`.
- Update `camera.rs` discrete-mode collection and tests for FPS + dedup.
- New modules under `src-tauri/src/` for DSHOW grab, MSMF reader, and pixel conversion.
- Refactor `camera_open.rs` to orchestrate native → OpenCV behind one `OpenedCam`.
- Update `SetupPage.vue` (and any invoke typings) for FPS column and payload.
- Optional: extend `examples/open-by-name` to pass fps for harness timing.

## Testing

- Unit: AvgTimePerFrame → FPS; dedup includes fps; open-order helpers without hardware.
- Unit: YUY2/MJPG conversion on fixed sample buffers.
- Manual / harness: cold open to first frame on real camera; confirm backend label; confirm wrong-device rule still holds when multiple cameras present.

## Risks

- Sample Grabber / Source Reader threading (STA) vs preview thread model.
- Some devices expose many FPS rows → larger Setup table; still correct.
- MJPG decode latency vs YUY2 convert — prefer selected mode as enumerated.
- OpenCV fallback path may still be slow; success depends on native path winning cold start.

## Success criteria

- [ ] Cold start, selected discrete mode: first preview frame within ~2s on typical USB cam (MJPG or YUY2).
- [ ] Setup lists FPS; open requests that FPS.
- [ ] Native failure falls back to OpenCV without opening a different device.
- [ ] Existing Charuco capture / session flow still works.
