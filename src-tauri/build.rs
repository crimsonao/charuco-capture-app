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

    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let mut dest = std::path::PathBuf::from(out_dir);
        dest.pop();
        dest.pop();
        dest.pop();
        let src = format!("{opencv_dir}\\x64\\vc16\\bin\\opencv_world4120.dll");
        let dest_dll = dest.join("opencv_world4120.dll");
        if std::path::Path::new(&src).exists() {
            let _ = std::fs::copy(&src, dest_dll);
        }
    }
}
