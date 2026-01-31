# Private Speech-to-Text (PSTT)

A terminal-based voice recorder with real-time speech-to-text transcription using Vosk, written in Rust.

## Features

- 🎙️ Select from available microphones
- ⏺️ Record audio with keyboard controls (Enter/Esc)
- 🔄 Real-time audio resampling to configurable sample rate
- 💾 Save recordings as WAV files with timestamp filenames
- 🤖 Real-time speech recognition using Vosk
- 📝 Save transcriptions to text files
- 🎯 Optional accurate transcription with Whisper (feature flag)
- ⚡ Multi-threaded architecture for optimal performance
- 🔒 Privacy-focused: All processing happens locally

## Architecture

```
Thread 1: Mic Capture → Ring Buffer A
Thread 2: Resampler (Ring Buffer A → Ring Buffer B)
Thread 3: WAV Writer (Ring Buffer B → disk)
Thread 4: Vosk Recognition (Ring Buffer B → text queue)
Thread 5: Text Writer (text queue → disk)
```

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

### Vosk Model

Download a Vosk model from [https://alphacephei.com/vosk/models](https://alphacephei.com/vosk/models)

Recommended for English:
- Small: `vosk-model-small-en-us-0.15` (~40 MB)
- Large: `vosk-model-en-us-0.22` (~1.8 GB)

```bash
# Create models directory
mkdir -p models

# Download and extract (example for small English model)
cd models
wget https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip
unzip vosk-model-small-en-us-0.15.zip
cd ..
```

## Installation

1. Clone or extract this project
2. Update `config.toml` with your settings
3. Build the project:

```bash
# Standard build (without Whisper)
cargo build --release

# With Whisper support (optional)
cargo build --release --features whisper
```

## Configuration

Edit `config.toml`:

```toml
# Audio sample rate for processing (Hz)
sample_rate = 16000

# Directory where recordings will be saved
output_directory = "./recordings"

# Path to Vosk model directory
vosk_model_path = "./models/vosk-model-small-en-us-0.15"

# Path to Whisper model file (only needed if using --features whisper)
whisper_model_path = "./models/ggml-base.en.bin"

# Enable accurate recognition with Whisper after recording
enable_accurate_recognition = false
```

## Usage

### Interactive Mode

```bash
# Run the application
cargo run --release

# Or if built:
./target/release/pstt
```

**Controls:**
- **Enter** - Start recording
- **Esc** - Stop recording
- **Ctrl+C** - Exit application

### Accurate Transcription Mode

Run accurate transcription on an existing WAV file:

```bash
# Using full path
cargo run --release -- accurate /path/to/recording.wav

# Or just filename if in output directory
cargo run --release -- accurate 31-01-2026_14-30-45.wav
```

**Note:** Requires building with `--features whisper`

## Output Files

When you record, the following files are created in the `output_directory`:

- `DD-MM-YYYY_HH-MI-SS.wav` - Audio recording
- `DD-MM-YYYY_HH-MI-SS_real-time.txt` - Real-time Vosk transcription
- `DD-MM-YYYY_HH-MI-SS_accurate.txt` - Accurate Whisper transcription (if enabled)

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

### "Failed to load Vosk model"
- Verify the model path in `config.toml` is correct
- Ensure you've extracted the model (not just downloaded the zip)
- Check the directory structure: `models/vosk-model-small-en-us-0.15/`

### "Ring buffer overflow" warnings
- Your CPU may be overloaded
- Try closing other applications
- Consider using a smaller Vosk model

### Audio quality issues
- Adjust `sample_rate` in config (16000 Hz is standard for speech)
- Check your microphone settings in OS
- Ensure microphone is not too far away

## Performance

Typical CPU usage during recording:
- Microphone capture: ~2%
- Resampler: ~15%
- WAV writer: ~3%
- Vosk recognition: ~30%
- **Total: ~50% of one CPU core**

Memory usage:
- ~200 MB with small Vosk model
- ~2 GB with large Vosk model

## Building for Production

```bash
# Optimized release build
cargo build --release

# Strip symbols for smaller binary
strip target/release/pstt

# The binary is now at: target/release/pstt
```

## License

This project is provided as-is for educational and personal use.

## Credits

- **Vosk** - Offline speech recognition
- **Whisper** - Accurate transcription (optional)
- **cpal** - Cross-platform audio I/O
- **Rubato** - Audio resampling
- Rust community for excellent crates

## TODO

- [ ] GUI version
- [ ] More language support
- [ ] VAD (Voice Activity Detection)
- [ ] Punctuation restoration
- [ ] Speaker diarization
- [ ] Cloud backup integration
