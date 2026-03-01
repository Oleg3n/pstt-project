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

    // ------------------------------------------------------------------
    // Build number handling
    // ------------------------------------------------------------------
    // Bump persistent build counter stored in `build_number.txt` (committed in repo).
    // The file contains a single integer; if missing we start from 0.
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let build_file = Path::new(&manifest_dir).join("build_number.txt");
    let mut build = 0u64;
    if let Ok(contents) = fs::read_to_string(&build_file) {
        if let Ok(n) = contents.trim().parse::<u64>() {
            build = n;
        }
    }
    build += 1;
    let _ = fs::write(&build_file, build.to_string());

    // Make the build number available to the code
    println!("cargo:rustc-env=BUILD_NUMBER={}", build);

    // Re-run build script when version changes or counter file is modified
    println!("cargo:rerun-if-changed={}", build_file.display());
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
}
