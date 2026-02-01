use vosk::{Model, Recognizer};
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

pub struct VoskRecognizer {
    recognizer: Recognizer,
    text_sender: mpsc::Sender<RecognizedText>,
}

impl VoskRecognizer {
    pub fn new(
        model_path: &str, 
        sample_rate: u32,
        text_sender: mpsc::Sender<RecognizedText>,
    ) -> Result<Self> {
        log::info!("Loading Vosk model from: {}", model_path);
        let model = Model::new(model_path).ok_or_else(|| anyhow::anyhow!("Failed to load Vosk model"))?;
        let recognizer = Recognizer::new(&model, sample_rate as f32).ok_or_else(|| anyhow::anyhow!("Failed to create Vosk recognizer"))?;
        
        log::info!("Vosk model loaded successfully");
        
        Ok(Self {
            recognizer,
            text_sender,
        })
    }
    
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        
        // Convert f32 to i16
        let samples_i16: Vec<i16> = samples.iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();
        
        // Accept waveform expects &[i16], returns Result<DecodingState, AcceptWaveformError>
        match self.recognizer.accept_waveform(&samples_i16) {
            Ok(_) => {
                if let Some(result) = self.recognizer.result().single() {
                    let text = result.text;
                    if !text.is_empty() {
                        println!("🎤 Recognized: {}", text);
                        // Send to writer thread (non-blocking)
                        let _ = self.text_sender.send(RecognizedText {
                            text: text.to_string(),
                            timestamp: Local::now(),
                            is_final: false,
                        });
                    }
                }
            },
            Err(e) => {
                log::error!("accept_waveform error: {:?}", e);
            }
        }
        
        Ok(())
    }
    
    pub fn finalize(&mut self) -> Result<()> {
        if let Some(result) = self.recognizer.final_result().single() {
            let text = result.text;
            if !text.is_empty() {
                println!("🎤 Final: {}", text);
                let _ = self.text_sender.send(RecognizedText {
                    text: text.to_string(),
                    timestamp: Local::now(),
                    is_final: true,
                });
            }
        }
        Ok(())
    }
}

pub fn vosk_thread(
    resampled_queue: Arc<BlockingQueue<f32>>,
    text_sender: mpsc::Sender<RecognizedText>,
    config: Arc<Config>,
    stop_signal: Arc<AtomicBool>,
) -> Result<()> {
    log::info!("Vosk recognition thread started");
    
    let mut recognizer = VoskRecognizer::new(
        &config.vosk_model_path,
        config.sample_rate,
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
    log::info!("Vosk recognition thread finished");
    
    Ok(())
}
