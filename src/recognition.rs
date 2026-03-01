use vosk::{Model, Recognizer};
use anyhow::Result;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::buffers::BlockingQueue;
use crate::config::Config;
use chrono::Local;
use std::time::Duration;

// ── Shared text type ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RecognizedText {
    pub text: String,
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub is_final: bool,
}

// ── Engine abstraction ────────────────────────────────────────────────────────

/// Common interface for every streaming real-time recognition engine.
///
/// To add a new engine:
///   1. Create a `struct MyEngineRecognizer { ... }` that stores a cloned
///      `mpsc::Sender<RecognizedText>` and whatever native state is needed.
///   2. Implement this trait.
///   3. Add a match arm in `create_realtime_recognizer`.
pub trait RealtimeRecognizer {
    /// Feed a batch of 16-kHz mono f32 PCM samples and optionally emit
    /// `RecognizedText` messages via the internal sender.
    fn process_audio(&mut self, samples: &[f32]) -> Result<()>;

    /// Flush any buffered state and emit the last `RecognizedText` with
    /// `is_final: true`.  Called once when recording stops.
    fn finalize(&mut self) -> Result<()>;
}

// ── Factory ───────────────────────────────────────────────────────────────────

/// Create the engine selected by `config.realtime_engine`.
///
/// Returns an error if:
/// - the engine name is unknown, or
/// - the required Cargo feature was not compiled in (sherpa-onnx), or
/// - the model files cannot be opened.
pub fn create_realtime_recognizer(
    config: &Config,
    text_sender: mpsc::Sender<RecognizedText>,
) -> Result<Box<dyn RealtimeRecognizer>> {
    match config.realtime_engine.as_str() {
        "vosk" => {
            log::info!("Real-time engine: Vosk");
            let path = config.vosk_model_path.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "vosk_model_path must be set when realtime_engine is \"vosk\""
                )
            })?;
            Ok(Box::new(VoskRecognizer::new(
                path,
                config.sample_rate as f32,
                text_sender,
            )?))
        }
        "sherpa-onnx" => {
            #[cfg(feature = "sherpa-engine")]
            {
                log::info!("Real-time engine: sherpa-onnx");
                Ok(Box::new(crate::sherpa::SherpaOnnxRecognizer::new(
                    &config.sherpa_encoder,
                    &config.sherpa_decoder,
                    &config.sherpa_joiner,
                    &config.sherpa_tokens,
                    config.sample_rate,
                    text_sender,
                )?))
            }
            #[cfg(not(feature = "sherpa-engine"))]
            {
                anyhow::bail!(
                    "realtime_engine is set to \"sherpa-onnx\" but the binary was compiled \
                     without the `sherpa-engine` feature.\n\
                     Rebuild with:  cargo build --features sherpa-engine"
                );
            }
        }
        other => anyhow::bail!(
            "Unknown realtime_engine: \"{}\". Valid values: \"vosk\", \"sherpa-onnx\"",
            other
        ),
    }
}

// ── Vosk engine ───────────────────────────────────────────────────────────────

pub struct VoskRecognizer {
    recognizer: Recognizer,
    text_sender: mpsc::Sender<RecognizedText>,
}

impl VoskRecognizer {
    pub fn new(
        model_path: &str,
        sample_rate: f32,
        text_sender: mpsc::Sender<RecognizedText>,
    ) -> Result<Self> {
        log::info!("Loading Vosk model from: {}", model_path);
        let model = Model::new(model_path)
            .ok_or_else(|| anyhow::anyhow!("Failed to load Vosk model from: {}", model_path))?;

        let mut recognizer = Recognizer::new(&model, sample_rate)
            .ok_or_else(|| anyhow::anyhow!("Failed to create Vosk recognizer"))?;

        recognizer.set_words(true);
        recognizer.set_partial_words(true);

        log::info!("Vosk model loaded successfully (sample_rate: {} Hz)", sample_rate);

        Ok(Self { recognizer, text_sender })
    }
}

impl RealtimeRecognizer for VoskRecognizer {
    fn process_audio(&mut self, samples: &[f32]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }

        // Vosk expects i16 samples
        let samples_i16: Vec<i16> = samples.iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();

        match self.recognizer.accept_waveform(&samples_i16) {
            Ok(state) => {
                if state == vosk::DecodingState::Finalized {
                    if let Some(single) = self.recognizer.result().single() {
                        let text = single.text;
                        if !text.is_empty() {
                            println!("🎤 Recognized: {}", text);
                            let _ = self.text_sender.send(RecognizedText {
                                text: text.to_string(),
                                timestamp: Local::now(),
                                is_final: false,
                            });
                        }
                    }
                } else {
                    let partial = self.recognizer.partial_result();
                    let text = partial.partial;
                    if !text.is_empty() && text.split_whitespace().count() >= 3 {
                        log::debug!("Partial: {}", text);
                    }
                }
            }
            Err(e) => log::warn!("Accept waveform error: {:?}", e),
        }

        Ok(())
    }

    fn finalize(&mut self) -> Result<()> {
        if let Some(single) = self.recognizer.final_result().single() {
            let text = single.text;
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

// ── Thread entry point ────────────────────────────────────────────────────────

pub fn realtime_recognition_thread(
    resampled_queue: Arc<BlockingQueue<f32>>,
    text_sender: mpsc::Sender<RecognizedText>,
    config: Arc<Config>,
    stop_signal: Arc<AtomicBool>,
) -> Result<()> {
    log::info!("Real-time recognition thread started (engine: {})", config.realtime_engine);

    let mut recognizer = create_realtime_recognizer(&config, text_sender)?;

    while !stop_signal.load(Ordering::Relaxed) {
        if let Some(samples) = resampled_queue.try_pop_batch(4096) {
            recognizer.process_audio(&samples)?;
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    // Drain any remaining buffered samples
    while let Some(samples) = resampled_queue.try_pop_batch(4096) {
        recognizer.process_audio(&samples)?;
    }

    recognizer.finalize()?;
    log::info!("Real-time recognition thread finished");

    Ok(())
}
