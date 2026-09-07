# src-tauri OpenCV link

ChArUco detection and camera I/O use a small C++ FFI (`native/opencv_capture.cpp`) compiled by `build.rs` (`cc` crate) and linked against **OpenCV 4.12 `opencv_world4120`**. The Rust `opencv` crate is **not** used: it needs libclang, which is optional and not required for this layout.

## Why not `opencv4[contrib]` / the Rust crate

Official OpenCV **4.7+** moved ArUco / ChArUco from contrib into **`objdetect`**. A matching world DLL already exports:

- `cv::aruco::ArucoDetector::detectMarkers`
- `cv::aruco::CharucoDetector::detectBoard` (4.12 replacement for `interpolateCornersCharuco`)
- `cv::aruco::CharucoBoard` (`Size(6, 8)` squares, `DICT_4X4_50`)

No extra `opencv_contrib` or `opencv_aruco*.lib` is required for this official 4.12 world build.

If you rebuild against an **older** OpenCV (≤ 4.6) whose world DLL has **no** objdetect ArUco:

1. Install LLVM (libclang) and vcpkg `opencv4[contrib]`.
2. Point the Rust `opencv` crate at it with `OPENCV_INCLUDE_PATHS` / `OPENCV_LINK_PATHS` / `OPENCV_LINK_LIBS` (typically `opencv_world4` plus contrib, or the split `opencv_aruco4` lib).
3. Or keep this FFI and add the contrib include + `opencv_aruco` / contrib world lib in `build.rs`.

## Configuration

`OPENCV_DIR` is **required** (no hardcoded machine path in the repo).

Resolution order:

1. Environment variable `OPENCV_DIR`
2. Repo-root gitignored `.env` line `OPENCV_DIR=...` (see `.env.example`)

```bat
set OPENCV_DIR=C:\path\to\opencv\build
```

| Step | Value |
| --- | --- |
| Include | `%OPENCV_DIR%\include` (`opencv2/objdetect.hpp`, `charuco_detector.hpp`) |
| Lib search | `%OPENCV_DIR%\x64\vc16\lib` |
| Link lib | `opencv_world4120` (no separate aruco / contrib lib) |
| C++ | `native/opencv_capture.cpp` → static lib `opencv_capture` |
| Runtime DLL | `%OPENCV_DIR%\x64\vc16\bin\opencv_world4120.dll` copied next to the cargo target exe |

`OPENCV_INCLUDE_PATHS` / `OPENCV_LINK_LIBS` are **not** read; this project does not compile the `opencv` crate.

## Runtime

`pnpm tauri build` stages `opencv_world*.dll` and `opencv_videoio*.dll` (if
present) into `opencv-runtime/` and NSIS copies them **next to the exe**. See
the repo-root `README.md` for `OPENCV_DIR` and the exact copy commands.

For `cargo test` / `tauri dev`, `build.rs` also copies those DLLs into the
Cargo target dir. Optional PATH helper:

```bat
set PATH=%OPENCV_DIR%\x64\vc16\bin;%PATH%
cd src-tauri
cargo test --test detect_test
```

Board is fixed **8×6 squares** (`Size(6, 8)`), **`DICT_4X4_50`**, `MIN_CORNERS = 6`. Default geometry matches the Python tool: square 20 mm, marker 15 mm.
