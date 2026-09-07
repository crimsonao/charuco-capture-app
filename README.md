# ChArUco Capture

Windows desktop client (Tauri 2 + Vue 3 + Rust) for ChArUco camera calibration.
Double-click the installed exe; no Python runtime is required.

Pages: **start → setup → capture → done**. The start page sets the output root
(default `%USERPROFILE%\Desktop\ChArUcoCapture`). Setup lists cameras and does
**not** preselect a row; Start capture stays disabled until you click one.

## Prerequisites

- Windows 10/11 x64
- [Rust](https://rustup.rs/) (MSVC toolchain) and [pnpm](https://pnpm.io/)
- OpenCV **4.12** world build (include + `x64\vc16\lib` + `x64\vc16\bin`)
- WebView2 (usually already on Windows 11)

This machine’s OpenCV (override with `OPENCV_DIR`):

```
C:\Users\50429\Desktop\mark\centerExtration\opencv\build
```

That directory must contain `include\` and `x64\vc16\`. `build.rs` links
`opencv_world4120` and compiles `src-tauri/native/opencv_capture.cpp`.

vcpkg `opencv4[contrib]` is **not** required for this official 4.12 world DLL
(ArUco/ChArUco live in `objdetect`). See `src-tauri/README.md` if you rebuild
against OpenCV ≤ 4.6.

## Copy OpenCV DLLs next to the exe

Runtime needs `opencv_world*.dll` beside the exe (Windows loader). If the
OpenCV bin folder also has `opencv_videoio*.dll` (this tree has
`opencv_videoio_ffmpeg4120_64.dll`), that is copied too. Debug
`opencv_world*d.dll` is skipped.

**Dev / `cargo test`:** `src-tauri/build.rs` copies matching DLLs into the
Cargo target directory (`src-tauri/target/debug` or `release`).

**Installer:** `pnpm tauri build` runs `scripts/stage-opencv-dlls.mjs`, which
copies the same DLLs into `src-tauri/opencv-runtime/` (gitignored).
`src-tauri/tauri.conf.json` bundles them with:

```json
"resources": {
  "opencv-runtime/*.dll": "./"
}
```

NSIS installs those files into `$INSTDIR` next to `charuco-capture-app.exe`.

Manual copy if you only have the Cargo exe:

```bat
set OPENCV_DIR=C:\Users\50429\Desktop\mark\centerExtration\opencv\build
copy "%OPENCV_DIR%\x64\vc16\bin\opencv_world4120.dll" src-tauri\target\release\
copy "%OPENCV_DIR%\x64\vc16\bin\opencv_videoio_ffmpeg4120_64.dll" src-tauri\target\release\
```

Or: `node scripts/stage-opencv-dlls.mjs` then copy from `src-tauri/opencv-runtime\`.

## Build and run

```bat
cd C:\Users\50429\company\charuco-capture-app
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts the desktop window (`src/main.rs`). Do not put camera harness binaries under `src-tauri/src/bin/` — Cargo would run them instead of the UI.

Camera open uses native DirectShow, then native Media Foundation, then OpenCV
`VideoCapture` as fallback. Setup lists discrete modes with FPS; Start capture
requests that FPS on open.

Camera-open harness (optional; camera must be plugged in). Prints `backend=` and
`elapsed_ms` to the first non-black frame:

```bat
cd src-tauri
cargo run --example open-by-name -- ocal4 1280 720 MJPG 1 30
```

Release installer (NSIS):

```bat
set OPENCV_DIR=C:\Users\50429\Desktop\mark\centerExtration\opencv\build
pnpm tauri build
```

Outputs:

- Unpackaged exe + DLLs: `src-tauri\target\release\charuco-capture-app.exe`
- Installer: `src-tauri\target\release\bundle\nsis\`

After install, open the install folder and double-click `charuco-capture-app.exe`.
Camera listing uses DirectShow; you do not need Python or a system OpenCV install
on PATH.

## Release smoke checklist

Do this on a clean Windows machine (no Python, no system OpenCV on PATH) with a
printed 8×6 `DICT_4X4_50` board and the NSIS-installed exe:

1. Camera list includes ocal4 and the laptop camera. Nothing is preselected.
2. Start capture opens **only** the selected device (never `VideoCapture(dshow_index, CAP_MSMF)`).
3. Preview detects the printed board; autosave writes `img_*.jpg` only (no mid-session JSON).
4. After N frames, the last-place JPEG is still on disk (no drop-then-reshoot).
5. A worse trial does not delete that last-place file; a better trial replaces it on disk.
6. Meeting the score target writes `camera.json`, `camera.txt`, and `report.json`, then opens the Done page; use Open folder.

Until this checklist is recorded, do not treat the app as a drop-in replacement
for the Python capture tool.

## Tests

```bat
cd src-tauri
cargo test --test session_test
cargo test
```

Frontend typecheck is part of `pnpm build` (`vue-tsc --noEmit`).
