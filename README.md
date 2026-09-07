# ChArUco Capture

Windows desktop client for **ChArUco** camera calibration capture.

Built with **Tauri 2**, **Vue 3**, and **Rust**. Install once and double-click the app — no Python runtime, and end users do not need OpenCV on `PATH`.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-lightgrey)](#prerequisites)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)

## Features

- Guided flow: **Start → Setup → Capture → Done**
- Choose an output folder (default: `%USERPROFILE%\Desktop\ChArUcoCapture`)
- Enumerate DirectShow camera modes (resolution / FOURCC / FPS); nothing is preselected until you click a row
- Native camera open (DirectShow → Media Foundation → OpenCV `VideoCapture` fallback)
- Live ChArUco detection preview and autosave of calibration frames
- On accept: writes `camera.json`, `camera.txt`, and `report.json`

Board defaults match the common Python tooling: **8×6** squares (`DICT_4X4_50`), square **20 mm**, marker **15 mm**.

## Download

If a [GitHub Release](https://github.com/crimsonao/charuco-capture-app/releases) is published, install the NSIS setup (`*-setup.exe`). After install, run `charuco-capture-app.exe` from the install folder.

OpenCV runtime DLLs are bundled next to the executable by the installer.

## Prerequisites

### End users (installed app)

- Windows 10/11 x64
- WebView2 (included on most Windows 11 systems)
- A USB / built-in camera and a printed ChArUco board

### Developers (build from source)

- Windows 10/11 x64
- [Rust](https://rustup.rs/) with the **MSVC** toolchain
- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/)
- OpenCV **4.12** official **world** build layout:

  ```
  %OPENCV_DIR%\
    include\
    x64\vc16\lib\          (opencv_world4120.lib)
    x64\vc16\bin\          (opencv_world4120.dll, optional opencv_videoio_ffmpeg*.dll)
  ```

- WebView2

`OPENCV_DIR` is **required** for every local build (no machine path is committed).

Priority:

1. Process environment `OPENCV_DIR`
2. Gitignored repo-root `.env` (`OPENCV_DIR=...`) — copy from [`.env.example`](./.env.example)

`vcpkg opencv4[contrib]` is **not** required for official OpenCV 4.12 (ArUco/ChArUco live in `objdetect`). Details: [`src-tauri/README.md`](./src-tauri/README.md).

## Build and run

```bat
git clone https://github.com/crimsonao/charuco-capture-app.git
cd charuco-capture-app
pnpm install

rem Option A: shell
set OPENCV_DIR=C:\path\to\opencv\build

rem Option B: copy .env.example to .env and edit OPENCV_DIR (gitignored)
pnpm tauri dev
```

Release installer (NSIS):

```bat
set OPENCV_DIR=C:\path\to\opencv\build
pnpm tauri build
```

Outputs:

| Artifact | Path |
|----------|------|
| Unpackaged app + DLLs | `src-tauri\target\release\charuco-capture-app.exe` |
| NSIS installer | `src-tauri\target\release\bundle\nsis\` |

### OpenCV DLLs next to the exe

Windows loads `opencv_world*.dll` from the same folder as the exe. Debug `*d.dll` files are skipped.

| Mode | Behavior |
|------|----------|
| Dev / `cargo test` | `src-tauri/build.rs` copies matching DLLs into the Cargo target dir |
| Installer | `scripts/stage-opencv-dlls.mjs` stages DLLs into `src-tauri/opencv-runtime/` (gitignored); Tauri bundles them via `resources` |

Manual copy (Cargo release only):

```bat
set OPENCV_DIR=C:\path\to\opencv\build
copy "%OPENCV_DIR%\x64\vc16\bin\opencv_world4120.dll" src-tauri\target\release\
copy "%OPENCV_DIR%\x64\vc16\bin\opencv_videoio_ffmpeg4120_64.dll" src-tauri\target\release\
```

Or run `node scripts/stage-opencv-dlls.mjs` and copy from `src-tauri\opencv-runtime\`.

> **Note:** Do not add extra binaries under `src-tauri/src/bin/` — Cargo may run them instead of the UI entrypoint.

## Optional camera-open harness

With a camera plugged in:

```bat
cd src-tauri
cargo run --example open-by-name -- <device-name> 1280 720 MJPG 1 30
```

Prints `backend=` and `elapsed_ms` to the first non-black frame.

## Tests

```bat
cd src-tauri
cargo test --test session_test
cargo test
```

Frontend typecheck runs as part of `pnpm build` (`vue-tsc --noEmit`).

## Release smoke checklist

On a clean Windows machine (no Python, no system OpenCV on `PATH`), with a printed 8×6 `DICT_4X4_50` board and the NSIS-installed app:

1. Camera list shows expected devices; nothing is preselected.
2. Start capture opens **only** the selected device.
3. Preview detects the board; autosave writes `img_*.jpg` only (no mid-session JSON).
4. After N frames, the last-place JPEG remains on disk.
5. A worse trial does not delete that file; a better trial replaces it.
6. Meeting the score target writes `camera.json`, `camera.txt`, and `report.json`, then opens Done; **Open folder** works.

## Project layout

```
charuco-capture-app/
├── src/                 # Vue UI (Start / Setup / Capture / Done)
├── src-tauri/           # Rust + native OpenCV FFI + Tauri
├── scripts/             # OpenCV DLL staging for the installer
└── docs/                # Design notes and plans
```

## License

[MIT](./LICENSE) © 2026 crimsonao / oz
