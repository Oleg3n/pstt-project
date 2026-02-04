# How to Suppress Whisper Debug Output

## Method 1: Parameter Settings (What we already did)

```rust
// Set up parameters - suppress ALL output
let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
params.set_print_progress(false);
params.set_print_special(false);
params.set_print_realtime(false);
params.set_print_timestamps(false);
params.set_suppress_blank(true);
params.set_suppress_non_speech_tokens(true);
```

This should work for most cases.

## Method 2: Nuclear Option - Redirect stderr (if Method 1 fails)

If you're still seeing the verbose output, it's because whisper.cpp writes directly to stderr. Here's how to suppress it:

### Option A: Suppress in your Rust code

Add this helper function to your whisper.rs:

```rust
use std::fs::OpenOptions;
use std::os::unix::io::{AsRawFd, RawFd};

#[cfg(unix)]
fn suppress_stderr<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    use libc::{dup, dup2, STDERR_FILENO};
    use std::os::unix::io::FromRawFd;
    
    unsafe {
        // Save the original stderr
        let stderr_backup = dup(STDERR_FILENO);
        
        // Open /dev/null
        let null_file = OpenOptions::new()
            .write(true)
            .open("/dev/null")
            .expect("Failed to open /dev/null");
        
        // Redirect stderr to /dev/null
        dup2(null_file.as_raw_fd(), STDERR_FILENO);
        
        // Run the function
        let result = f();
        
        // Restore stderr
        dup2(stderr_backup, STDERR_FILENO);
        libc::close(stderr_backup);
        
        result
    }
}

#[cfg(not(unix))]
fn suppress_stderr<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    // On Windows, just run the function (or implement Windows-specific version)
    f()
}
```

Then wrap the transcription call:

```rust
log::info!("Transcribing with Whisper...");

// Suppress stderr during transcription
let result = suppress_stderr(|| {
    let mut state = ctx.create_state().unwrap();
    state.full(params, &samples).unwrap();
    state
});

let state = result;
let num_segments = state.full_n_segments();
```

### Option B: Set environment variable before running

You can also suppress it at the OS level:

```bash
# Linux/Mac
WHISPER_LOG_LEVEL=0 cargo run --release

# Or in your shell startup
export WHISPER_LOG_LEVEL=0
```

### Option C: Redirect stderr when running the program

```bash
# Redirect stderr to /dev/null (Linux/Mac)
cargo run --release 2>/dev/null

# Or only suppress Whisper output by filtering
cargo run --release 2>&1 | grep -v "whisper_full_with_state"
```

## Recommended Approach

1. **First, try the parameters** (already in whisper_complete.rs)
2. **If that doesn't work**, use Method 2, Option A (suppress_stderr wrapper)
3. **For quick testing**, use Option C (redirect when running)

## Why This Happens

The verbose output comes from the underlying C++ whisper.cpp library, which:
- Writes debug info directly to stderr (not through Rust logging)
- Ignores some of the parameter settings
- Was designed for command-line tools, not library use

The parameters *should* suppress it, but sometimes the library version or build settings cause it to still print. The stderr redirection is a guaranteed solution.
