use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // libs/build/ — static libs and headers used only at compile time
    println!("cargo:rustc-link-lib=vosk");
    println!("cargo:rustc-link-search=native=libs/build");

    // libs/distribute/ — DLLs that must be shipped alongside the exe
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let distribute_dir = Path::new(&manifest_dir).join("libs").join("distribute");

    // Derive the actual profile output dir from OUT_DIR (works with custom target-dir too).
    // OUT_DIR = <target_dir>/<profile>/build/<crate>-<hash>/out  →  go up 3 levels
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir)
        .parent().unwrap() // <crate>-<hash>/out  → <crate>-<hash>
        .parent().unwrap() // build/<crate>-<hash> → build
        .parent().unwrap() // <profile>/build      → <profile>
        .to_path_buf();

    // Create target directory if it doesn't exist
    let _ = fs::create_dir_all(&target_dir);

    // Copy all DLLs from libs/distribute/ to the target directory
    if let Ok(entries) = fs::read_dir(&distribute_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "dll") {
                let dest = target_dir.join(path.file_name().unwrap());
                let _ = fs::copy(&path, &dest);
            }
        }
    }
}
