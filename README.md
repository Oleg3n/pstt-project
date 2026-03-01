# Private Speech-to-Text (PSTT)

A terminal-based voice recorder with real-time speech-to-text transcription using Whisper, written in Rust.

## Features

- 🎙️ Select from available microphones
- ▶️ Record audio with keyboard controls (Enter/Esc)
- 🔄 Real-time audio resampling to configurable sample rate
- 💾 Save recordings as WAV files with timestamp filenames
- 🤖 **Dual-model system**: Fast model for real-time, accurate model for post-processing
- 📝 Save transcriptions to text files
- 🎯 Configurable chunk size for real-time transcription
- ⚡ Multi-threaded architecture for optimal performance
- 🔒 Privacy-focused: All processing happens locally

## Architecture

```
Thread 1: Mic Capture → Ring Buffer A
Thread 2: Resampler (Ring Buffer A → Ring Buffer B & C)
Thread 3: WAV Writer (Ring Buffer B → disk)
Thread 4: Whisper Real-Time Recognition (Ring Buffer C → text queue)
Thread 5: Text Writer (text queue → disk)
Post-Recording: Whisper Accurate Recognition (WAV file → accurate text)
```

## Dual-Model System

PSTT uses **two separate Whisper models** for optimal performance:

1. **Real-Time Model** (Fast & Responsive)
   - Used during recording for live transcription
   - Processes audio in configurable chunks (default: 3 seconds)
   - **Recommended**: Tiny or Base model
   - Trade-off: Speed over accuracy

2. **Accurate Model** (Slow & Precise)
   - Used after recording stops for post-processing
   - Processes the entire audio file at once
   - **Recommended**: Small, Medium, or Large model
   - Trade-off: Accuracy over speed

This gives you **the best of both worlds**: immediate feedback during recording and high-quality transcription afterwards!

## Prerequisites

### System Dependencies

#### Linux (Ubuntu/Debian)
```bash
sudo apt-get update
sudo apt-get install -y \
    libasound2-dev \
    pkg-config \
    build-essential
```

#### macOS
```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install Homebrew if needed
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

#### Windows
- Install [Microsoft Visual C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- Or install Visual Studio 2019 or later with C++ development tools

### Rust
```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Whisper Models

Download Whisper models from [https://huggingface.co/ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp)

**Recommended Setup:**
- **Real-time**: `ggml-base.en.bin` (140 MB) - Fast and responsive
- **Accurate**: `ggml-small.en.bin` (460 MB) - High accuracy

Available models:
- **Tiny**: `ggml-tiny.en.bin` (~75 MB) - Fastest, lowest accuracy
- **Base**: `ggml-base.en.bin` (~140 MB) - Good balance ⭐
- **Small**: `ggml-small.en.bin` (~460 MB) - Better accuracy ⭐
- **Medium**: `ggml-medium.en.bin` (~1.5 GB) - High accuracy
- **Large**: `ggml-large.bin` (~2.9 GB) - Best accuracy

```bash
# Create models directory
mkdir -p models
cd models

# Download real-time model (Base - recommended)
wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin

# Download accurate model (Small - recommended)
wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin

cd ..
```

## Installation

1. Clone or extract this project
2. Update `config.toml` with your settings
3. Build the project:

```bash
cargo build --release
```

## Configuration

Edit `config.toml`:

```toml
# Audio sample rate for processing (Hz)
# Whisper works best with 16000 Hz
sample_rate = 16000

# Audio gain/amplification multiplier
# 1.0 = no change, 2.0 = double volume, 3.0 = triple volume
# Increase if recordings are too quiet (try 2.0-5.0)
# Decrease if recordings are distorted (try 0.5-0.8)
audio_gain = 3.0

# Directory where recordings will be saved
output_directory = "./recordings"

# Real-time recognition engine: "vosk" or "sherpa-onnx"
# - "vosk" is the legacy engine and requires a Vosk model
#   (see below for vosk_model_path).
# - "sherpa-onnx" offers better quality but requires
#   building with --features sherpa-engine and downloading
#   the four ONNX model files.
# The example config below uses "sherpa-onnx"; if you omit
# this field entirely the code falls back to "vosk" by default.
realtime_engine = "sherpa-onnx"

# Path to Vosk model directory (required only when
# realtime_engine = "vosk"; can be omitted otherwise)
vosk_model_path = "./models/vosk-model-small-en-us-0.15"

# Path to Whisper model file for REAL-TIME recognition
# Use a faster, smaller model (Tiny or Base recommended)
whisper_model_path_realtime = "./models/ggml-base.en.bin"

# Path to Whisper model file for ACCURATE post-processing
# Use a larger, more accurate model (Small, Medium, or Large)
whisper_model_path_accurate = "./models/ggml-small.en.bin"

# Chunk duration for real-time transcription (in seconds)
# Lower = faster response, Higher = better accuracy
# Recommended: 3-5 seconds for real-time
chunk_duration_secs = 3

# Enable accurate post-processing transcription after recording
# Set to false if you only want real-time transcription
enable_accurate_recognition = true
```
## Usage

### Interactive Mode

```bash
# Run the application
cargo run --release

# Or if built:
./target/release/pstt
```

> **Version & build**
>
> The program keeps the semver version from `Cargo.toml` and a separate
> build counter that is incremented automatically on every build. The two
> values are displayed independently in the startup banner.  The box width
> automatically expands so the right-hand border stays aligned no matter
> how long the version string or build number becomes:
>
> ```text
> ╔══════════════════════════════════════════════════════════════╗
> ║         Private Speech-to-Text (PSTT) v2.4.1                 ║
> ║         Build: 1047                                       ║
> ╚══════════════════════════════════════════════════════════════╝
> ```
>
> You can also query either value from the command line:
>
> ```bash
> # show version only
> ./pstt --version
>
> # show build number only
> ./pstt --build
> ```

**Controls:**
- **Enter** - Start recording
- **Esc** - Stop recording (triggers accurate transcription if enabled)
- **Ctrl+C** - Exit application

**What happens when you record:**
1. Press Enter → Recording starts
2. Real-time model processes audio in chunks → See transcription appear live
3. Press Esc → Recording stops
4. Accurate model processes entire recording → Better transcription saved

### Accurate Transcription Mode

Run accurate transcription on an existing WAV file:

```bash
# Using full path
cargo run --release -- accurate /path/to/recording.wav

# Or just filename if in output directory
cargo run --release -- accurate 31-01-2026_14-30-45.wav
```

## Output Files

When you record, the following files are created in the `output_directory`:

- `DD-MM-YYYY_HH-MI-SS.wav` - Audio recording
- `DD-MM-YYYY_HH-MI-SS_real-time.txt` - Real-time transcription (from fast model)
- `DD-MM-YYYY_HH-MI-SS_accurate.txt` - Accurate transcription (from accurate model)

## Model Configuration Examples

### For Maximum Speed (Low-end hardware)
```toml
whisper_model_path_realtime = "./models/ggml-tiny.en.bin"
whisper_model_path_accurate = "./models/ggml-base.en.bin"
chunk_duration_secs = 3
```

### Balanced (Recommended) ⭐
```toml
whisper_model_path_realtime = "./models/ggml-base.en.bin"
whisper_model_path_accurate = "./models/ggml-small.en.bin"
chunk_duration_secs = 3
```

### For Maximum Accuracy (Powerful hardware)
```toml
whisper_model_path_realtime = "./models/ggml-small.en.bin"
whisper_model_path_accurate = "./models/ggml-medium.en.bin"
chunk_duration_secs = 5
```

### Same Model for Both (Simplest setup)
```toml
whisper_model_path_realtime = "./models/ggml-base.en.bin"
whisper_model_path_accurate = "./models/ggml-base.en.bin"
chunk_duration_secs = 3
enable_accurate_recognition = false  # Optional: Skip redundant processing
```

## How Real-Time Transcription Works

The real-time recognizer processes audio in **configurable chunks** (default: 3 seconds):
- Audio samples accumulate in a buffer
- When chunk duration is reached, it's transcribed immediately
- Results appear with minimal delay (typically 3-7 seconds total)
- At the end, any remaining audio is transcribed

After recording stops (if enabled), the accurate model processes the complete audio file for best results.

## Logging

Set log level via environment variable:

```bash
# Debug logging
RUST_LOG=debug cargo run --release

# Info logging (default)
RUST_LOG=info cargo run --release

# Warning only
RUST_LOG=warn cargo run --release
```

## Troubleshooting

### "No input devices found"
- Check your microphone is connected and recognized by the OS
- On Linux, ensure ALSA is properly configured
- Try: `arecord -l` (Linux) or check System Preferences (macOS)

### "Failed to load Whisper model"
- Verify the model paths in `config.toml` are correct
- Ensure you've downloaded the correct model format (`.bin` file)
- Check the files aren't corrupted (re-download if needed)
- Make sure both model paths exist (realtime and accurate)

### Real-time transcription is slow
- Use Tiny or Base model for real-time
- Reduce `chunk_duration_secs` (try 2-3 seconds)
- Close other CPU-intensive applications
- Consider using a smaller model

### Accurate transcription is slow
- This is normal! Larger models take longer
- Small model: ~30 seconds for 1 minute of audio
- Medium model: ~60 seconds for 1 minute of audio
- You can disable it with `enable_accurate_recognition = false`

### "Ring buffer overflow" warnings
- Your CPU may be overloaded
- Try using a smaller/faster real-time model
- Increase chunk duration to reduce processing frequency
- Close other applications

### Audio quality issues
- Adjust `sample_rate` in config (16000 Hz is standard for speech)
- **If too quiet**: Increase `audio_gain` (try 2.0, 3.0, or even 5.0)
- **If distorted/clipping**: Decrease `audio_gain` (try 0.5 or 0.8)
- Check your microphone settings in OS

### Transcription accuracy issues
- For real-time: Try a larger model (Tiny → Base → Small)
- For accurate: Use Small, Medium, or Large model
- Increase `chunk_duration_secs` for better context
- Verify `audio_gain` isn't causing distortion
- Check background noise levels

## Performance

### Real-Time Transcription (with Base model)
- Microphone capture: ~2%
- Resampler: ~15%
- WAV writer: ~3%
- Whisper real-time: ~40-60%
- **Total: ~60-80% of one CPU core**

### Accurate Post-Processing
- Depends on model size and audio length
- Tiny: 0.5x realtime (30s audio → 15s processing)
- Base: 1x realtime (30s audio → 30s processing)
- Small: 2x realtime (30s audio → 60s processing)
- Medium: 4x realtime (30s audio → 120s processing)

### Memory Usage
- Tiny: ~300 MB
- Base: ~500 MB
- Small: ~1 GB
- Medium: ~2.5 GB
- Large: ~4 GB

## Building for Production

```bash
# Optimized release build
cargo build --release

# Strip symbols for smaller binary
strip target/release/pstt

# The binary is now at: target/release/pstt
```

## Why Dual Models?

**Single Model Approach** (old way):
- Use Base model → Fast real-time but mediocre accuracy
- Use Large model → Great accuracy but sluggish real-time

**Dual Model Approach** (new way):
- Base for real-time → Fast feedback during recording
- Small/Medium for accurate → Best quality final transcription
- **Result**: Responsive experience + accurate results!

## License

This project is provided as-is for educational and personal use.

## Credits

- **Whisper** - OpenAI's robust speech recognition
- **whisper.cpp** - C++ implementation by Georgi Gerganov
- **whisper-rs** - Rust bindings
- **cpal** - Cross-platform audio I/O
- **Rubato** - Audio resampling
- Rust community for excellent crates

## TODO

- [ ] GUI version
- [ ] VAD (Voice Activity Detection) to reduce processing
- [ ] Dynamic chunk sizing based on pauses
- [ ] More language support
- [ ] Punctuation restoration
- [ ] Speaker diarization
- [ ] Cloud backup integration
