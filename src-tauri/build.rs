use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    tauri_build::build();

    let opencv_dir = std::env::var("OPENCV_DIR").unwrap_or_else(|_| {
        r"C:\Users\50429\Desktop\mark\centerExtration\opencv\build".to_string()
    });
    let include_dir = format!("{opencv_dir}\\include");
    let lib_dir = format!("{opencv_dir}\\x64\\vc16\\lib");

    println!("cargo:rerun-if-changed=native/opencv_capture.cpp");
    println!("cargo:rerun-if-changed=native/opencv_capture.h");
    println!("cargo:rerun-if-env-changed=OPENCV_DIR");
    println!("cargo:rustc-link-search=native={lib_dir}");
    println!("cargo:rustc-link-lib=opencv_world4120");

    cc::Build::new()
        .cpp(true)
        .file("native/opencv_capture.cpp")
        .include(&include_dir)
        .include("native")
        .flag_if_supported("/std:c++17")
        .flag_if_supported("/EHsc")
        .compile("opencv_capture");

    // Copy DLLs next to the Cargo exe only. Do not write src-tauri/opencv-runtime
    // here: that directory is watched by `tauri dev` and retriggers rebuilds.
    // Production staging stays in scripts/stage-opencv-dlls.mjs (beforeBuildCommand).
    let mut dest_dirs = Vec::new();
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        if let Some(profile_dir) = cargo_profile_dir(Path::new(&out_dir)) {
            dest_dirs.push(profile_dir);
        }
    }
    stage_opencv_dlls(Path::new(&opencv_dir), &dest_dirs);
}

fn cargo_profile_dir(out_dir: &Path) -> Option<PathBuf> {
    let profile = std::env::var("PROFILE").ok()?;
    out_dir.ancestors().find_map(|ancestor| {
        if ancestor.file_name().and_then(|n| n.to_str()) == Some(profile.as_str()) {
            Some(ancestor.to_path_buf())
        } else {
            None
        }
    })
}

fn is_bundled_opencv_dll(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with(".dll") {
        return false;
    }
    if lower.starts_with("opencv_world") {
        return !lower.ends_with("d.dll");
    }
    lower.starts_with("opencv_videoio")
}

fn stage_opencv_dlls(opencv_dir: &Path, dest_dirs: &[PathBuf]) {
    let bin = opencv_dir.join("x64").join("vc16").join("bin");
    let Ok(entries) = fs::read_dir(&bin) else {
        return;
    };
    for dest in dest_dirs {
        let _ = fs::create_dir_all(dest);
    }
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !is_bundled_opencv_dll(name_str) {
            continue;
        }
        for dest in dest_dirs {
            let _ = fs::copy(entry.path(), dest.join(&name));
        }
    }
}
