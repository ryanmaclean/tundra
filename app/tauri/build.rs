// build.rs — ensure frontendDist exists before tauri::generate_context!() proc-macro
// runs. The real Leptos dist is produced by trunk (see ../leptos-ui/Trunk.toml);
// when it hasn't been built yet (e.g. headless CI or release-binary builds that
// don't need the GUI), we materialize a minimal stub so the tauri proc-macro
// stops panicking on a missing path. The stub is harmless if trunk later
// overwrites the directory with a real build.
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest_dir
        .join("..")
        .join("leptos-ui")
        .join("dist");
    let index = dist.join("index.html");

    if !index.exists() {
        if let Err(e) = fs::create_dir_all(&dist) {
            println!("cargo:warning=at-tauri: could not create stub dist dir {}: {e}", dist.display());
        } else if let Err(e) = fs::write(
            &index,
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>auto-tundra</title></head><body><div id=\"app\"></div></body></html>\n",
        ) {
            println!("cargo:warning=at-tauri: could not write stub index.html: {e}");
        }
    }

    println!("cargo:rerun-if-changed=../leptos-ui/dist/index.html");

    tauri_build::build()
}
