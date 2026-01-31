mod config;
mod audio;
mod input;
mod buffers;
mod resampler;
mod writer;
mod recognition;
mod text_writer;
mod whisper;

use clap::{Parser, Subcommand};
use anyhow::{Result, Context};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::PathBuf;
use crossterm::terminal::{enable_raw_mode, disable_raw_mode};
use cpal::traits::{DeviceTrait, StreamTrait};
use std::sync::mpsc;
use chrono::Local;

use config::Config;
use buffers::{AudioPipeline};
use input::{InputCommand, check_input};

#[derive(Parser)]
#[command(name = "pstt")]
#[command(about = "Private Speech-to-Text - Terminal-based voice recorder with real-time transcription", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run accurate recognition on an existing WAV file
    Accurate {
        /// Path to the WAV file (can be just filename if in output directory)
        wav_file: String,
    },
}

struct RecordingSession {
    stream: cpal::Stream,
    threads: Vec<std::thread::JoinHandle<()>>,
    stop_signal: Arc<AtomicBool>,
    text_tx: mpsc::Sender<recognition::RecognizedText>,
}

impl RecordingSession {
    fn start(device: cpal::Device, config: Arc<Config>) -> Result<Self> {
        let (device_name, device_config) = audio::get_device_info(&device)?;
        log::info!("Using device: {} ({:?})", device_name, device_config);
        
        // Create audio pipeline with 10 seconds of buffer
        let pipeline = AudioPipeline::new(48000 * 10);
        let stop_signal = Arc::new(AtomicBool::new(false));
        
        // Create text channel
        let (text_tx, text_rx) = mpsc::channel::<recognition::RecognizedText>();
        
        let mut threads = Vec::new();
        
        // Thread 1: Microphone capture (handled by cpal stream)
        let raw_queue = Arc::clone(&pipeline.raw_queue);
        let stream = device.build_input_stream(
            &device_config.into(),
            move |data: &[f32], _: &_| {
                if !raw_queue.push(data.to_vec()) {
                    log::warn!("Mic: Failed to push to raw queue (overflow)");
                }
            },
            |err| log::error!("Stream error: {}", err),
            None,
        )?;
        
        stream.play()?;
        log::info!("Audio stream started");
        
        // Thread 2: Resampler
        let resampler_handle = {
            let raw_q = Arc::clone(&pipeline.raw_queue);
            let resampled_q = Arc::clone(&pipeline.resampled_queue);
            let cfg = Arc::clone(&config);
            let stop = Arc::clone(&stop_signal);
            
            std::thread::spawn(move || {
                resampler::resampler_thread(raw_q, resampled_q, cfg, stop);
            })
        };
        threads.push(resampler_handle);
        
        // Thread 3: WAV Writer
        let writer_handle = {
            let resampled_q = Arc::clone(&pipeline.resampled_queue);
            let cfg = Arc::clone(&config);
            let stop = Arc::clone(&stop_signal);
            
            std::thread::spawn(move || {
                match writer::writer_thread(resampled_q, cfg, stop) {
                    Ok(path) => log::info!("Recording saved: {}", path.display()),
                    Err(e) => log::error!("Writer thread error: {}", e),
                }
            })
        };
        threads.push(writer_handle);
        
        // Thread 4: Vosk Recognition
        let vosk_handle = {
            let resampled_q = Arc::clone(&pipeline.resampled_queue);
            let cfg = Arc::clone(&config);
            let stop = Arc::clone(&stop_signal);
            let tx = text_tx.clone();
            
            std::thread::spawn(move || {
                match recognition::vosk_thread(resampled_q, tx, cfg, stop) {
                    Ok(_) => log::info!("Vosk recognition completed"),
                    Err(e) => log::error!("Vosk thread error: {}", e),
                }
            })
        };
        threads.push(vosk_handle);
        
        // Thread 5: Text Writer
        let text_writer_handle = {
            let timestamp = Local::now().format("%d-%m-%Y_%H-%M-%S");
            let output_path = format!(
                "{}/{}_real-time.txt",
                config.output_directory,
                timestamp
            );
            
            std::thread::spawn(move || {
                match text_writer::text_writer_thread(text_rx, output_path) {
                    Ok(_) => {},
                    Err(e) => log::error!("Text writer thread error: {}", e),
                }
            })
        };
        threads.push(text_writer_handle);
        
        Ok(Self {
            stream,
            threads,
            stop_signal,
            text_tx,
        })
    }
    
    fn stop(self) {
        log::info!("Stopping recording...");
        
        // Signal all threads to stop
        self.stop_signal.store(true, Ordering::Relaxed);
        
        // Stop the audio stream
        drop(self.stream);
        
        // Drop the text sender to close the channel
        drop(self.text_tx);
        
        // Wait for all threads to finish
        for thread in self.threads {
            let _ = thread.join();
        }
        
        log::info!("Recording stopped");
    }
}

fn run_recording_mode(config: Arc<Config>) -> Result<()> {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         Private Speech-to-Text (PSTT) v0.1.0                ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    
    // List available microphones
    println!("📡 Available microphones:");
    let devices = audio::list_input_devices()?;
    
    if devices.is_empty() {
        anyhow::bail!("No input devices found!");
    }
    
    for (i, name) in &devices {
        println!("  {}. {}", i + 1, name);
    }
    println!();
    
    // Get user selection
    print!("Select microphone (1-{}): ", devices.len());
    std::io::Write::flush(&mut std::io::stdout())?;
    
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let index: usize = input.trim().parse::<usize>()
        .context("Invalid number")?
        .checked_sub(1)
        .context("Invalid selection")?;
    
    if index >= devices.len() {
        anyhow::bail!("Selection out of range");
    }
    
    let device = audio::select_device(index)?;
    println!("✓ Selected: {}", devices[index].1);
    println!();
    
    println!("Controls:");
    println!("  [Enter]  - Start recording");
    println!("  [Esc]    - Stop recording");
    println!("  [Ctrl+C] - Exit");
    println!();
    
    // Set up Ctrl+C handler before enabling raw mode
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    
    ctrlc::set_handler(move || {
        r.store(false, Ordering::Relaxed);
    }).expect("Error setting Ctrl+C handler");
    
    enable_raw_mode()?;
    
    // Clear any pending keyboard events (like the Enter from mic selection)
    while crossterm::event::poll(std::time::Duration::from_millis(0))? {
        crossterm::event::read()?;
    }
    
    let mut session: Option<RecordingSession> = None;
    let mut is_recording = false;
    
    loop {
        // Check if Ctrl+C was pressed
        if !running.load(Ordering::Relaxed) {
            if is_recording {
                if let Some(s) = session.take() {
                    s.stop();
                }
            }
            disable_raw_mode()?;
            println!("\n\n👋 Goodbye!");
            break;
        }
        
        match check_input()? {
            InputCommand::StartRecording => {
                if !is_recording {
                    println!("\n🔴 Recording started...");
                    session = Some(RecordingSession::start(device.clone(), Arc::clone(&config))?);
                    is_recording = true;
                }
            }
            InputCommand::StopRecording => {
                if is_recording {
                    println!("\n⏹️  Stopping recording...");
                    if let Some(s) = session.take() {
                        s.stop();
                    }
                    is_recording = false;
                    
                    // Optionally run Whisper for accurate transcription
                    if config.enable_accurate_recognition {
                        println!("Running accurate transcription with Whisper...");
                        // Note: This would need the wav path from the writer thread
                        println!("(Whisper integration pending)");
                    }
                    
                    println!("\n✓ Recording saved. Press Enter to record again, or Ctrl+C to exit.");
                }
            }
            InputCommand::Exit | InputCommand::None => {}
        }
    }
    
    Ok(())
}

fn run_accurate_mode(config: Arc<Config>, wav_file: String) -> Result<()> {
    println!("Running accurate transcription on: {}", wav_file);
    
    // Check if it's just a filename or full path
    let wav_path = if PathBuf::from(&wav_file).exists() {
        PathBuf::from(&wav_file)
    } else {
        PathBuf::from(&config.output_directory).join(&wav_file)
    };
    
    if !wav_path.exists() {
        anyhow::bail!("WAV file not found: {}", wav_path.display());
    }
    
    whisper::transcribe_with_whisper(
        &wav_path,
        &config.whisper_model_path,
        &config.output_directory,
    )?;
    
    Ok(())
}

fn main() -> Result<()> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
    
    let cli = Cli::parse();
    let config = Arc::new(Config::load()?);
    
    match cli.command {
        Some(Commands::Accurate { wav_file }) => {
            run_accurate_mode(config, wav_file)?;
        }
        None => {
            run_recording_mode(config)?;
        }
    }
    
    Ok(())
}
