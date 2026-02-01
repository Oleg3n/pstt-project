use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
use anyhow::Result;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::buffers::BlockingQueue;
use crate::config::Config;
use chrono::Local;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RecognizedText {
    pub text: String,
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub is_final: bool,
}

pub struct WhisperRecognizer {
    context: WhisperContext,
    text_sender: mpsc::Sender<RecognizedText>,
    buffer: Vec<f32>,
    chunk_size: usize,
    sample_rate: u32,
}

impl WhisperRecognizer {
    pub fn new(
        model_path: &str, 
        sample_rate: u32,
        chunk_duration_secs: u32,
        text_sender: mpsc::Sender<RecognizedText>,
    ) -> Result<Self> {
        log::info!("Loading Whisper model from: {}", model_path);
        let context = WhisperContext::new_with_params(
            model_path,
            WhisperContextParameters::default()
        )?;
        
        log::info!("Whisper model loaded successfully");
        
        // Process audio in configurable chunks for real-time transcription
        // Example: At 16kHz, 3 seconds = 48,000 samples
        let chunk_size = (sample_rate * chunk_duration_secs) as usize;
        
        log::info!(
            "Real-time transcription configured: {} second chunks ({} samples at {} Hz)",
            chunk_duration_secs,
            chunk_size,
            sample_rate
        );
        
        Ok(Self {
            context,
            text_sender,
            buffer: Vec::with_capacity(chunk_size * 2),
            chunk_size,
            sample_rate,
        })
    }
    
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        
        // Add samples to buffer
        self.buffer.extend_from_slice(samples);
        
        // Process complete chunks
        while self.buffer.len() >= self.chunk_size {
            let chunk: Vec<f32> = self.buffer.drain(..self.chunk_size).collect();
            self.transcribe_chunk(&chunk, false)?;
        }
        
        Ok(())
    }
    
    fn transcribe_chunk(&mut self, samples: &[f32], is_final: bool) -> Result<()> {
        // Set up parameters for real-time transcription
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_language(Some("en"));
        params.set_n_threads(2); // Use 2 threads for faster processing
        params.set_translate(false);
        
        // Create a new state for this transcription
        let mut state = self.context.create_state()?;
        
        // Run transcription
        state.full(params, samples)?;
        
        // Extract transcribed text
        let num_segments = state.full_n_segments();
        let mut full_text = String::new();
        
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                if let Ok(text) = segment.to_str() {
                    full_text.push_str(text);
                    full_text.push(' ');
                }
            }
        }
        
        let full_text = full_text.trim().to_string();
        
        if !full_text.is_empty() {
            if is_final {
                println!("🎤 Final: {}", full_text);
            } else {
                println!("🎤 Recognized: {}", full_text);
            }
            
            // Send to writer thread (non-blocking)
            let _ = self.text_sender.send(RecognizedText {
                text: full_text,
                timestamp: Local::now(),
                is_final,
            });
        }
        
        Ok(())
    }
    
    pub fn finalize(&mut self) -> Result<()> {
        // Process any remaining buffered samples
        if !self.buffer.is_empty() {
            let remaining = self.buffer.clone();
            self.buffer.clear();
            
            // Pad to minimum size if needed (Whisper needs at least some samples)
            let padded = if remaining.len() < self.sample_rate as usize {
                let mut padded = remaining.clone();
                padded.resize(self.sample_rate as usize, 0.0);
                padded
            } else {
                remaining
            };
            
            self.transcribe_chunk(&padded, true)?;
        }
        Ok(())
    }
}

pub fn whisper_thread(
    resampled_queue: Arc<BlockingQueue<f32>>,
    text_sender: mpsc::Sender<RecognizedText>,
    config: Arc<Config>,
    stop_signal: Arc<AtomicBool>,
) -> Result<()> {
    log::info!("Whisper real-time recognition thread started");
    
    let mut recognizer = WhisperRecognizer::new(
        &config.whisper_model_path_realtime,
        config.sample_rate,
        config.chunk_duration_secs,
        text_sender,
    )?;
    
    while !stop_signal.load(Ordering::Relaxed) {
        if let Some(samples) = resampled_queue.try_pop_batch(4096) {
            recognizer.process_audio(&samples)?;
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    
    // Process any remaining samples
    while let Some(samples) = resampled_queue.try_pop_batch(4096) {
        recognizer.process_audio(&samples)?;
    }
    
    recognizer.finalize()?;
    log::info!("Whisper recognition thread finished");
    
    Ok(())
}
