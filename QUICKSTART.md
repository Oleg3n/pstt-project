# Quick Start Guide

Get PSTT running in 5 minutes!

## Step 1: Install Rust (if not installed)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Step 2: Install System Dependencies

### Linux (Ubuntu/Debian)
```bash
sudo apt-get update
sudo apt-get install -y libasound2-dev pkg-config build-essential
```

### macOS
```bash
xcode-select --install
```

### Windows
Install [Visual C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)

## Step 3: Download Whisper Model

Models list: https://huggingface.co/ggerganov/whisper.cpp
```bash
# Create models directory
mkdir -p models
cd models

# Download Whisper base English model (~142 MB)
wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin

# (Optional) Download other Whisper models:
# tiny (~77 MB):
# wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin
# medium (~466 MB):
# wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin
# large-v2 (~1.5 GB):
# wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v2.bin

# Go back to project root
cd ..
```


## Step 4: Verify config.toml

The default `config.toml` should work if you followed Step 3:

```toml
sample_rate = 16000
output_directory = "./recordings"
vosk_model_path = "./models/vosk-model-small-en-us-0.15"
whisper_model_path = "./models/ggml-base.en.bin"
enable_accurate_recognition = false
```

## Step 5: Build and Run

```bash
# Build (first time takes 2-5 minutes)
cargo build --release

# Run
cargo run --release
```

## Step 6: Use the App

1. Select your microphone from the list
2. Press **Enter** to start recording
3. Speak into your microphone
4. Watch real-time transcription appear
5. Press **Esc** to stop recording
6. Press **Ctrl+C** to exit

## Output Files

Check the `recordings/` directory:
- `DD-MM-YYYY_HH-MI-SS.wav` - Your audio
- `DD-MM-YYYY_HH-MI-SS_real-time.txt` - Transcription

## Troubleshooting

### "No input devices found"
- Plug in your microphone
- Check system sound settings
- On Linux: `arecord -l` to list devices

### "Failed to load Vosk model"
- Make sure you **extracted** the zip file
- Check the path: `models/vosk-model-small-en-us-0.15/` should exist

### Build errors
- Update Rust: `rustup update`
- Check system dependencies are installed

## Next Steps

- Try a larger Vosk model for better accuracy
- Enable Whisper for post-processing (requires `--features whisper`)
- Adjust `sample_rate` in config if needed
- Check README.md for advanced features

## Need Help?

Check the full README.md for detailed documentation and troubleshooting.
