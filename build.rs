use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var_os("CARGO_FEATURE_DESKTOP").is_none()
    {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        Ok("gnu"),
        "build the Windows desktop in MSYS2 UCRT64"
    );
    println!("cargo:rerun-if-changed=packaging/windows/spoolcut.rc");
    println!("cargo:rerun-if-changed=packaging/windows/spoolcut.manifest");
    println!("cargo:rerun-if-changed=assets/io.github.bugrakoc.SpoolCut.png");
    println!("cargo:rerun-if-env-changed=WINDRES");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::copy(
        "packaging/windows/spoolcut.manifest",
        out.join("spoolcut.manifest"),
    )
    .unwrap();

    // ICO supports embedding the existing 256px PNG. Zero encodes 256 in the directory.
    let png = include_bytes!("assets/io.github.bugrakoc.SpoolCut.png");
    assert_eq!(
        &png[16..24],
        &[0, 0, 1, 0, 0, 0, 1, 0],
        "expected a 256px icon"
    );
    let mut icon = vec![0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0];
    icon.extend_from_slice(&(png.len() as u32).to_le_bytes());
    icon.extend_from_slice(&22u32.to_le_bytes());
    icon.extend_from_slice(png);
    fs::write(out.join("spoolcut.ico"), icon).unwrap();

    let resources = out.join("spoolcut-resources.o");
    let status = Command::new(env::var_os("WINDRES").unwrap_or_else(|| "windres".into()))
        .args(["-O", "coff", "-I"])
        .arg(&out)
        .args([
            "-D",
            concat!("APP_MAJOR=", env!("CARGO_PKG_VERSION_MAJOR")),
            "-D",
            concat!("APP_MINOR=", env!("CARGO_PKG_VERSION_MINOR")),
            "-D",
            concat!("APP_PATCH=", env!("CARGO_PKG_VERSION_PATCH")),
            "-D",
            concat!("APP_VERSION=\"", env!("CARGO_PKG_VERSION"), "\""),
            "packaging/windows/spoolcut.rc",
        ])
        .arg(&resources)
        .status()
        .expect("run windres from MSYS2 UCRT64");
    assert!(status.success(), "windres failed");
    println!(
        "cargo:rustc-link-arg-bin=spoolcut-desktop={}",
        resources.display()
    );

    // libmpv2-sys declares -lmpv. GTK's pkg-config crates add this directory too,
    // but passing mpv's own directory makes the native import library explicit.
    let mpv = Command::new("pkg-config")
        .args(["--variable=libdir", "mpv"])
        .output()
        .expect("find the MSYS2 libmpv import library");
    assert!(mpv.status.success(), "pkg-config could not find mpv");
    let library = String::from_utf8(mpv.stdout).unwrap();
    assert!(!library.trim().is_empty(), "mpv has no pkg-config libdir");
    println!("cargo:rustc-link-search=native={}", library.trim());
}
