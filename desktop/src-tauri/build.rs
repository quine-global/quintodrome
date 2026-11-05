fn main() {
    // The macOS app links Sparkle.framework (the updater), which the bundle
    // carries in Contents/Frameworks. A debug binary run outside the bundle
    // (`cargo run`) finds it where scripts/download-sparkle.sh put it instead.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" && std::env::var("PROFILE").as_deref() == Ok("debug") {
        let dir = std::env::var_os("SPARKLE_FRAMEWORK_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().expect("current dir"));
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
    }

    tauri_build::build()
}
