use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rustc-link-lib=vosk");
    println!("cargo:rustc-link-search=native=libs");

    // Copy DLLs to target directory for runtime
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let libs_dir = Path::new(&manifest_dir).join("libs");

    // Determine target directory (debug or release)
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let target_dir = Path::new(&manifest_dir).join("target").join(&profile);

    // Create target directory if it doesn't exist
    if let Err(_) = fs::create_dir_all(&target_dir) {
        // Ignore errors
    }

    // Copy all DLL files
    if let Ok(entries) = fs::read_dir(&libs_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "dll" {
                        let file_name = path.file_name().unwrap();
                        let dest = target_dir.join(file_name);
                        let _ = fs::copy(&path, &dest); // Ignore errors in build script
                    }
                }
            }
        }
    }
}
