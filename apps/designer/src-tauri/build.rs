#![allow(clippy::unwrap_used, clippy::expect_used)]

use tauri_build::{Attributes, WindowsAttributes};

fn main() {
    // Replace the manifest tauri-build embeds by default with our own copy below, so the
    // same manifest is linked into both the app binary and any cargo test binaries.
    tauri_build::try_build(
        Attributes::new().windows_attributes(WindowsAttributes::new_without_app_manifest()),
    )
    .expect("failed to run tauri-build");

    // Embed the Windows app manifest via linker args so the Common-Controls v6 dependency is
    // present in test binaries too. Without this, `cargo test` on Windows MSVC fails with
    // STATUS_ENTRYPOINT_NOT_FOUND when libtest spawns the test executable. See
    // https://github.com/tauri-apps/tauri/issues/13419.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").ok();
    if target_os == "windows" && target_env.as_deref() == Some("msvc") {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let manifest = std::path::Path::new(&manifest_dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
