use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{self, size};
use hound::{WavSpec, WavWriter};
use serde::{Deserialize, Serialize};

use vosk::{Model, Recognizer};

use std::ffi::c_void;
use whisper_rs::{WhisperContext, FullParams};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use rubato::{Resampler, SincFixedOut, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use clap::{Parser, Subcommand};
use chrono::{Datelike, Timelike, Local};

fn print_wrapped_incremental(text: &str, max_len: usize, current_line_len: &mut usize) {
    let words: Vec<&str> = text.split_whitespace().collect();
    for word in words {
        let word_len = word.chars().count();
        let space_needed = if *current_line_len > 0 { 1 } else { 0 };
        if *current_line_len + space_needed + word_len > max_len {
            println!();
            print!("{}", word);
            *current_line_len = word_len;
        } else {
            if *current_line_len > 0 {
                print!(" ");
            }
            print!("{}", word);
            *current_line_len += space_needed + word_len;
        }
    }
    let _ = io::stdout().flush();
}

fn run_accurate_recognition(wav_path: &PathBuf, txt_path: &PathBuf, model_path: &PathBuf) -> Result<String> {
    println!("Starting accurate recognition for: {}", wav_path.display());

    let model_path = model_path.clone();
    if !model_path.exists() {
        return Err(anyhow::anyhow!("Whisper model not found: {}", model_path.display()));
    }

    // Load WAV file
    let mut reader = hound::WavReader::open(wav_path)?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate as f32;
    let channels = spec.channels as usize;
    let mut samples: Vec<f32> = reader.samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();

    // Convert to mono if necessary
    if channels > 1 {
        let mut mono_samples = Vec::with_capacity(samples.len() / channels);
        for chunk in samples.chunks(channels) {
            let avg = chunk.iter().sum::<f32>() / channels as f32;
            mono_samples.push(avg);
        }
        samples = mono_samples;
    }

    if sample_rate != 16000.0 {
        samples = resample_audio(&samples, sample_rate, 16000.0);
    }

    // Suppress Whisper logging
    unsafe extern "C" fn dummy_log(_level: i32, _message: *const i8, _user_data: *mut c_void) {}
    unsafe { whisper_rs::set_log_callback(Some(dummy_log), std::ptr::null_mut()) };

    let params = whisper_rs::WhisperContextParameters::default();
    let ctx = WhisperContext::new_with_params(&model_path.to_string_lossy(), params)?;
    let mut state = ctx.create_state()?;
    let mut full_params = FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
    full_params.set_print_progress(false);
    full_params.set_print_realtime(false);
    full_params.set_print_timestamps(false);
    state.full(full_params, &samples)?;

    let num_segments = state.full_n_segments();
    let mut transcription = String::new();
    for i in 0..num_segments {
        let segment = state.get_segment(i as i32).ok_or_else(|| anyhow::anyhow!("Segment {} not found", i))?;
        transcription.push_str(&format!("{}", segment));
        transcription.push('\n');
    }

    let trimmed = transcription.trim().to_string();
    fs::write(txt_path, &trimmed)?;
    Ok(trimmed)
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    output_dir: PathBuf,
    sample_rate: f32,
    instant_model_dir: PathBuf,
    auto_accurate_recognition: bool,
    whisper_model_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("./recordings"),
            sample_rate: 16000.0,
            instant_model_dir: PathBuf::from("./model"),
            auto_accurate_recognition: false,
            whisper_model_path: PathBuf::from("./models/whisper/ggml-base.en.bin"),
        }
    }
}

fn resample_audio(input: &[f32], from_rate: f32, to_rate: f32) -> Vec<f32> {
    if (from_rate - to_rate).abs() < 0.1 {
        return input.to_vec();
    }
    let ratio = to_rate as f64 / from_rate as f64;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let output_len = (input.len() as f64 * ratio).ceil() as usize;
    let mut resampler = SincFixedOut::<f32>::new(ratio, 2.0, params, output_len, 1).unwrap();
    let resampled = resampler.process(&[input], None).unwrap();
    resampled[0].clone()
}

fn load_config() -> Result<Config> {
    let config_path = PathBuf::from("config.toml");
    if config_path.exists() {
        let contents = fs::read_to_string(config_path)?;
        toml::from_str(&contents).context("Failed to parse config file")
    } else {
        let config = Config::default();
        let toml_string = toml::to_string(&config)?;
        fs::write("config.toml", toml_string)?;
        println!("Created default config.toml file");
        Ok(config)
    }
}

#[derive(Parser)]
#[command(name = "stt-rust")]
#[command(about = "Speech-to-Text recording and transcription tool")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run accurate transcription on a WAV file
    Accurate {
        /// Path to the WAV file to transcribe
        file: PathBuf,
    },
}

fn enumerate_microphones() -> Result<Vec<cpal::Device>> {
    let host = cpal::default_host();
    let devices = host.input_devices()
        .context("Failed to get input devices")?
        .collect::<Vec<_>>();

    if devices.is_empty() {
        println!("No microphone devices found!");
        return Ok(vec![]);
    }

    println!("Available microphones:");
    for (i, device) in devices.iter().enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown Device".to_string());
        println!("  {}. {}", i + 1, name);
    }

    Ok(devices)
}

fn select_microphone(microphones: &[cpal::Device]) -> Result<usize> {
    println!("\nSelect a microphone by entering its number (1-{}):", microphones.len());

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().parse::<usize>() {
            Ok(num) if num >= 1 && num <= microphones.len() => {
                return Ok(num - 1); // Convert to 0-based index
            }
            _ => {
                println!("Invalid selection. Please enter a number between 1 and {}.", microphones.len());
            }
        }
    }
}

fn generate_filename() -> String {
    let now = Local::now();
    format!("record-{}-{:02}-{:02}_{:02}-{:02}-{:02}.wav",
            now.year(), now.month(), now.day(),
            now.hour(), now.minute(), now.second())
}

fn start_recording(
    device: &cpal::Device,
    config: &Config,
    stt_recognizer: &Arc<Mutex<Recognizer>>,
) -> Result<(Stream, Arc<Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>>, Arc<Mutex<Option<std::fs::File>>>, String)> {
    let supported_configs: Vec<_> = device.supported_input_configs()
        .context("Failed to get supported input configs")?
        .collect();

    if supported_configs.is_empty() {
        return Err(anyhow::anyhow!("No supported input configurations found"));
    }

    // Find the best matching configuration
    // Prefer mono, then stereo; prefer F32, then I16
    let selected_config_range = supported_configs
        .iter()
        .find(|config_range| {
            config_range.channels() == 1 &&
            config_range.sample_format() == SampleFormat::F32
        })
        .or_else(|| {
            supported_configs.iter().find(|config_range| {
                config_range.channels() == 1 &&
                config_range.sample_format() == SampleFormat::I16
            })
        })
        .or_else(|| {
            supported_configs.iter().find(|config_range| {
                config_range.sample_format() == SampleFormat::F32
            })
        })
        .or_else(|| supported_configs.first())
        .context("No suitable input config found")?
        .clone();

    let sample_format = selected_config_range.sample_format();

    // Find the best sample rate within the supported range
    let min_sample_rate = selected_config_range.min_sample_rate().0;
    let max_sample_rate = selected_config_range.max_sample_rate().0;
    let requested_sample_rate = config.sample_rate as u32;

    let actual_sample_rate = if requested_sample_rate >= min_sample_rate && requested_sample_rate <= max_sample_rate {
        requested_sample_rate
    } else {
        // Find closest supported rate, preferring higher rates for quality
        if requested_sample_rate < min_sample_rate {
            min_sample_rate
        } else {
            max_sample_rate
        }
    };

    let selected_config = selected_config_range
        .with_sample_rate(cpal::SampleRate(actual_sample_rate));

    let stream_config = StreamConfig {
        channels: selected_config.channels(),
        sample_rate: selected_config.sample_rate(),
        buffer_size: cpal::BufferSize::Fixed(16000),  // 1 second buffer for better accuracy
    };

    println!("Using audio config: {} channels, {} Hz, {:?} (requested: {} Hz)",
             stream_config.channels,
             stream_config.sample_rate.0,
             sample_format,
             config.sample_rate);

    if (actual_sample_rate as f32 - config.sample_rate).abs() > 1.0 {
        println!("Note: Using {} Hz instead of requested {} Hz (device limitations)",
                 actual_sample_rate, config.sample_rate);
    }

    let stt_sample_rate = config.sample_rate;

    let spec = WavSpec {
        channels: 1,  // Mono for correct timing
        sample_rate: stt_sample_rate as u32,  // STT sample rate
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let filename = generate_filename();
    let filepath = config.output_dir.join(&filename);
    let writer = WavWriter::create(filepath, spec)?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    // Create text file for transcription
    let txt_filename = filename.replace(".wav", "-realtime.txt");
    let txt_filepath = config.output_dir.join(&txt_filename);
    let txt_file = fs::File::create(&txt_filepath)?;
    let txt_writer = Arc::new(Mutex::new(Some(txt_file)));

    let writer_clone = Arc::clone(&writer);
    let stt_recognizer_clone = Arc::clone(stt_recognizer);

    let ratio = stt_sample_rate as f64 / actual_sample_rate as f64;
    let input_buffer = Arc::new(Mutex::new(Vec::new()));

    // Handle different sample formats
    let channels = stream_config.channels as usize;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            let mut resampler = SincFixedOut::<f32>::new(ratio, 2.0, params, 160, 1).unwrap();
            let input_buffer_clone = Arc::clone(&input_buffer);
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &_| {
                    // Extract mono
                    let mut mono_data = Vec::new();
                    let num_frames = data.len() / channels;
                    for i in 0..num_frames {
                        let frame_start = i * channels;
                        let sum: f32 = data[frame_start..frame_start + channels].iter().sum();
                        let avg = sum / channels as f32;
                        mono_data.push(avg);
                    }
                    // Accumulate input
                    {
                        let mut buffer = input_buffer_clone.lock().unwrap();
                        buffer.extend_from_slice(&mono_data);
                        while buffer.len() >= 610 {
                            let chunk: Vec<f32> = buffer.drain(0..610).collect();
                            let resampled = resampler.process(&[&chunk], None).unwrap();
                            if !resampled.is_empty() && !resampled[0].is_empty() {
                                let final_samples = resampled[0].clone();
                                let i16_resampled: Vec<i16> = final_samples.iter().map(|&s| (s * i16::MAX as f32) as i16).collect();
                                // Write resampled mono to WAV
                                if let Ok(mut guard) = writer_clone.lock() {
                                    if let Some(ref mut writer) = *guard {
                                        for &sample in &i16_resampled {
                                            let _ = writer.write_sample(sample);
                                        }
                                    }
                                }
                                // Use resampled for STT
                                if let Ok(mut guard) = stt_recognizer_clone.lock() {
                                    let _ = guard.accept_waveform(&i16_resampled);
                                }
                            }
                        }
                    }
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            let mut resampler = SincFixedOut::<f32>::new(ratio, 2.0, params, 160, 1).unwrap();
            let input_buffer_clone = Arc::clone(&input_buffer);
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &_| {
                    // Extract mono (average all channels)
                    let mut mono_data = Vec::new();
                    let num_frames = data.len() / channels;
                    for i in 0..num_frames {
                        let frame_start = i * channels;
                        let sum: i32 = data[frame_start..frame_start + channels].iter().map(|&x| x as i32).sum();
                        let avg = (sum / channels as i32) as i16;
                        mono_data.push(avg);
                    }
                    // Convert to f32 for resampling
                    let mono_f32: Vec<f32> = mono_data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    // Accumulate input
                    {
                        let mut buffer = input_buffer_clone.lock().unwrap();
                        buffer.extend_from_slice(&mono_f32);
                        while buffer.len() >= 610 {
                            let chunk: Vec<f32> = buffer.drain(0..610).collect();
                            let resampled = resampler.process(&[&chunk], None).unwrap();
                            if !resampled.is_empty() && !resampled[0].is_empty() {
                                let final_samples = resampled[0].clone();
                                let i16_resampled: Vec<i16> = final_samples.iter().map(|&s| (s * i16::MAX as f32) as i16).collect();
                                // Write resampled mono to WAV
                                if let Ok(mut guard) = writer_clone.lock() {
                                    if let Some(ref mut writer) = *guard {
                                        for &sample in &i16_resampled {
                                            let _ = writer.write_sample(sample);
                                        }
                                    }
                                }
                                // Use resampled for STT
                                if let Ok(mut guard) = stt_recognizer_clone.lock() {
                                    let _ = guard.accept_waveform(&i16_resampled);
                                }
                            }
                        }
                    }
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            let mut resampler = SincFixedOut::<f32>::new(ratio, 2.0, params, 160, 1).unwrap();
            let input_buffer_clone = Arc::clone(&input_buffer);
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _: &_| {
                    // Extract mono (average all channels)
                    let mut mono_data = Vec::new();
                    let num_frames = data.len() / channels;
                    for i in 0..num_frames {
                        let frame_start = i * channels;
                        let sum: u32 = data[frame_start..frame_start + channels].iter().map(|&x| x as u32).sum();
                        let avg = (sum / channels as u32) as u16;
                        mono_data.push(avg);
                    }
                    // Convert to f32 for resampling
                    let mono_f32: Vec<f32> = mono_data.iter().map(|&s| (s as f32 - i16::MIN as f32) / i16::MAX as f32).collect();
                    // Accumulate input
                    {
                        let mut buffer = input_buffer_clone.lock().unwrap();
                        buffer.extend_from_slice(&mono_f32);
                        while buffer.len() >= 610 {
                            let chunk: Vec<f32> = buffer.drain(0..610).collect();
                            let resampled = resampler.process(&[&chunk], None).unwrap();
                            if !resampled.is_empty() && !resampled[0].is_empty() {
                                let final_samples = resampled[0].clone();
                                let i16_resampled: Vec<i16> = final_samples.iter().map(|&s| (s * i16::MAX as f32) as i16).collect();
                                // Write resampled mono to WAV
                                if let Ok(mut guard) = writer_clone.lock() {
                                    if let Some(ref mut writer) = *guard {
                                        for &sample in &i16_resampled {
                                            let _ = writer.write_sample(sample);
                                        }
                                    }
                                }
                                // Use resampled for STT
                                if let Ok(mut guard) = stt_recognizer_clone.lock() {
                                    let _ = guard.accept_waveform(&i16_resampled);
                                }
                            }
                        }
                    }
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )?
        }
        _ => {
            return Err(anyhow::anyhow!("Unsupported sample format: {:?}", sample_format));
        }
    };

    println!("RECORDING STARTED");
    stream.play()?;

    Ok((stream, writer, txt_writer, filename))
}

fn main() -> Result<()> {
    println!("STT-Rust - Microphone Audio Recorder");
    println!("====================================");

    let cli = Cli::parse();

    // Load configuration
    let config = load_config()?;

    // Handle CLI commands
    if let Some(command) = cli.command {
        match command {
            Commands::Accurate { file } => {
                // Check if file exists
                if !file.exists() {
                    return Err(anyhow!("Input file does not exist: {}", file.display()));
                }

                // Check if it's a WAV file
                if file.extension().unwrap_or_default() != "wav" {
                    return Err(anyhow!("Input file must be a WAV file: {}", file.display()));
                }

                // Check Whisper model
                if !config.whisper_model_path.exists() {
                    return Err(anyhow!("Whisper model not found: {}. Please download a Whisper model and place it at this path.", config.whisper_model_path.display()));
                }

                // Derive output path
                let stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let txt_path = file.with_file_name(format!("{}-accurate.txt", stem));

                println!("Running accurate transcription on: {}", file.display());
                println!("Output will be saved to: {}", txt_path.display());

                run_accurate_recognition(&file, &txt_path, &config.whisper_model_path)?;

                println!("Accurate transcription completed successfully.");
                return Ok(());
            }
        }
    }

    // Interactive mode
    println!("Output directory: {}", config.output_dir.display());
    println!("Sample rate: {} Hz", config.sample_rate);
    println!("Model directory: {}", config.instant_model_dir.display());
    println!("Auto accurate recognition: {}", config.auto_accurate_recognition);
    println!("Whisper model path: {}", config.whisper_model_path.display());

    // Create output directory if it doesn't exist
    fs::create_dir_all(&config.output_dir)?;

    // Check Vosk model
    if !config.instant_model_dir.exists() {
        return Err(anyhow::anyhow!("Vosk model directory not found: {}. Please download a Vosk model and place it in this directory.", config.instant_model_dir.display()));
    }
    println!("Found model directory: {}", config.instant_model_dir.display());

    // Check Whisper model if auto accurate recognition is enabled
    if config.auto_accurate_recognition {
        if !config.whisper_model_path.exists() {
            println!("Warning: Whisper model not found: {}. Accurate recognition will be skipped.", config.whisper_model_path.display());
        } else {
            println!("Found Whisper model: {}", config.whisper_model_path.display());
        }
    }

    // Enumerate microphones
    let microphones = enumerate_microphones()?;

    if microphones.is_empty() {
        println!("No microphones available. Exiting.");
        return Ok(());
    }

    // Select microphone
    let selected_index = select_microphone(&microphones)?;
    let selected_device = &microphones[selected_index];
    let device_name = selected_device.name().unwrap_or_else(|_| "Unknown Device".to_string());
    println!("Selected microphone: {}", device_name);

    // Initialize Vosk
    // Suppress Vosk logs
    vosk::set_log_level(vosk::LogLevel::Warn);
    println!("Loading Vosk model from: {}", config.instant_model_dir.display());
    let model = Model::new(config.instant_model_dir.to_str().unwrap()).ok_or(anyhow!("Failed to load Vosk model"))?;
    println!("Vosk model loaded successfully");
    let mut recognizer = Recognizer::new(&model, config.sample_rate).ok_or(anyhow!("Failed to create Vosk recognizer"))?;
    println!("Vosk recognizer created successfully");
    // Enable settings for better partial result accuracy
    recognizer.set_partial_words(true);
    // Understand
    recognizer.set_max_alternatives(3);
    recognizer.set_words(true);
    let stt_recognizer = Arc::new(Mutex::new(recognizer));

    println!("\nRecording controls:");
    println!("- Press ENTER to start recording");
    println!("- Press ESCAPE to stop recording");
    println!("- Press Ctrl+C to exit");
    println!("\nReady to record. Press ENTER to begin...");

    // Set up terminal for raw input
    terminal::enable_raw_mode()?;

    // Get terminal width for text wrapping
    let (terminal_width, _) = size().unwrap_or((80, 24)); // Fallback to 80 columns if size can't be determined

    let mut recording_stream: Option<Stream> = None;
    let mut recording_writer: Option<Arc<Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>>> = None;
    let mut recording_txt_writer: Option<Arc<Mutex<Option<std::fs::File>>>> = None;
    let mut current_filename: Option<String> = None;
    let mut ready_for_recording = true; // Start in ready state
    let mut last_partial = String::new();
    let mut current_line_len = 0;
    let mut transcription_buffer = String::new(); // Accumulate partial results

    loop {
        // Check for partial STT results
        if recording_stream.is_some() {
            let partial_text = {
                if let Ok(mut guard) = stt_recognizer.lock() {
                    let partial_result = guard.partial_result();
                    partial_result.partial.to_string()
                } else {
                    "".to_string()
                }
            };
            if partial_text != last_partial {
                if partial_text.len() > last_partial.len() {
                    let new_part = &partial_text[last_partial.len()..];
                    print_wrapped_incremental(new_part, terminal_width as usize, &mut current_line_len);
                    // Accumulate the new partial text
                    transcription_buffer.push_str(new_part);
                    // Append to file immediately
                    if let Some(ref txt_writer_arc) = recording_txt_writer {
                        if let Ok(mut guard) = txt_writer_arc.lock() {
                            if let Some(ref mut file) = *guard {
                                let _ = write!(file, "{}", new_part);
                                let _ = file.flush();
                            }
                        }
                    }
                }
                last_partial = partial_text;
            }
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key_event) = event::read()? {
                match key_event.code {
                    KeyCode::Enter => {
                        if recording_stream.is_none() {
                            if ready_for_recording {
                                // First Enter after mic selection - transition to recording ready but don't start yet
                                ready_for_recording = false;
                                println!("Press ENTER again to start recording, or ESCAPE to stop recording, or Ctrl+C to exit.");
                            } else {
                                // Second/subsequent Enter - start recording
                                // Reset transcription buffer for new recording
                                transcription_buffer.clear();
                                last_partial.clear();
                                current_line_len = 0;

                                // Reset the Vosk recognizer to clear internal state
                                if let Ok(mut guard) = stt_recognizer.lock() {
                                    guard.reset();
                                }

                                match start_recording(selected_device, &config, &stt_recognizer) {
                                    Ok((stream, writer, txt_writer, filename)) => {
                                        recording_stream = Some(stream);
                                        recording_writer = Some(writer);
                                        recording_txt_writer = Some(txt_writer);
                                        current_filename = Some(filename);
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to start recording: {}", e);
                                        ready_for_recording = true; // Reset to ready state on error
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if let Some(stream) = recording_stream.take() {
                            // Update the last partial before stopping
                            if let Ok(mut guard) = stt_recognizer.lock() {
                                let current_partial = guard.partial_result().partial.to_string();
                                if current_partial != last_partial {
                                    let new_part = &current_partial[last_partial.len()..];
                                    print_wrapped_incremental(new_part, terminal_width as usize, &mut current_line_len);
                                    transcription_buffer.push_str(new_part);
                                    // Append to realtime file
                                    if let Some(ref txt_writer_arc) = recording_txt_writer {
                                        if let Ok(mut guard) = txt_writer_arc.lock() {
                                            if let Some(ref mut file) = *guard {
                                                let _ = write!(file, "{}", new_part);
                                                let _ = file.flush();
                                            }
                                        }
                                    }
                                    last_partial = current_partial;
                                }
                            }

                            println!("\nStopping recording...");
                            drop(stream); // This will stop the stream

                            // Get final result to capture any remaining transcription
                            if let Ok(mut guard) = stt_recognizer.lock() {
                                let final_result = guard.result();
                                let final_text = match final_result {
                                    vosk::CompleteResult::Single(single) => single.text.to_string(),
                                    vosk::CompleteResult::Multiple(multiple) => {
                                        multiple.alternatives.get(0).map(|alt| alt.text.to_string()).unwrap_or_default()
                                    }
                                }.trim().to_string();
                                if final_text.len() > transcription_buffer.trim().len() {
                                    let additional = &final_text[transcription_buffer.trim().len()..];
                                    if !additional.is_empty() {
                                        print_wrapped_incremental(additional, terminal_width as usize, &mut current_line_len);
                                        transcription_buffer.push_str(additional);
                                        // Append to realtime file
                                        if let Some(ref txt_writer_arc) = recording_txt_writer {
                                            if let Ok(mut guard) = txt_writer_arc.lock() {
                                                if let Some(ref mut file) = *guard {
                                                    let _ = write!(file, "{}", additional);
                                                    let _ = file.flush();
                                                }
                                            }
                                        }
                                    }
                                }
                                // Reset recognizer for next session
                                guard.reset();
                            }

                            // Finalize WAV file
                            if let Some(writer_arc) = recording_writer.take() {
                                if let Ok(mut guard) = writer_arc.lock() {
                                    if let Some(writer) = guard.take() {
                                        match writer.finalize() {
                                            Ok(_) => {
                                                println!("WAV file saved successfully.");
                                            }
                                            Err(e) => {
                                                eprintln!("Error finalizing WAV file: {}", e);
                                            }
                                        }
                                    }
                                }
                            }

                            // Close text file
                            if let Some(txt_writer_arc) = recording_txt_writer.take() {
                                if let Ok(mut guard) = txt_writer_arc.lock() {
                                    if let Some(file) = guard.take() {
                                        drop(file); // Close the file
                                    }
                                }
                            }

                            // Print realtime transcription file path
                            if let Some(filename) = &current_filename {
                                let realtime_txt_filename = filename.replace(".wav", "-realtime.txt");
                                let realtime_txt_filepath = config.output_dir.join(realtime_txt_filename);
                                println!("Realtime transcription saved to: {}", realtime_txt_filepath.display());
                            }

                            // Accurate recognition already run synchronously for fine transcription
                            if config.auto_accurate_recognition && config.whisper_model_path.exists() {
                                let wav_filepath = config.output_dir.join(&current_filename.as_ref().unwrap());
                                let accurate_txt_filepath = config.output_dir.join(current_filename.as_ref().unwrap().replace(".wav", "-accurate.txt"));
                                let model_path = config.whisper_model_path.clone();
                                std::thread::spawn(move || {
                                    if let Err(e) = run_accurate_recognition(&wav_filepath, &accurate_txt_filepath, &model_path) {
                                        eprintln!("Accurate recognition failed: {}", e);
                                    } else {
                                        println!("Accurate recognition completed.");
                                    }
                                });
                            }
                            println!("Recording session complete. Press ENTER to record again or Ctrl+C to exit.");
                            ready_for_recording = true; // Reset to ready state
                        } else {
                            // Not recording - just show we're ready
                            println!("Ready to record. Press ENTER to begin...");
                        }
                    }
                    KeyCode::Char('c') if key_event.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        // Ctrl+C - force exit
                        if let Some(stream) = recording_stream.take() {
                            drop(stream);
                        }
                        // Close text file if recording was in progress
                        if let Some(txt_writer_arc) = recording_txt_writer.take() {
                            if let Ok(mut guard) = txt_writer_arc.lock() {
                                if let Some(file) = guard.take() {
                                    drop(file);
                                }
                            }
                        }
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Clean up terminal
    terminal::disable_raw_mode()?;

    Ok(())
}
