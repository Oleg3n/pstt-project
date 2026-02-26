use serde::Deserialize;
use anyhow::{Result, Context};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub sample_rate: u32,
    pub audio_gain: f32,
    pub output_directory: String,
    pub vosk_model_path: String,
    pub whisper_model_path_accurate: String,
    pub enable_accurate_recognition: bool,
    /// Which real-time recognition engine to use: "vosk" or "sherpa-onnx"
    #[serde(default = "default_realtime_engine")]
    pub realtime_engine: String,
    /// Paths to the four sherpa-onnx streaming Zipformer model files.
    /// Model archives from GitHub contain versioned filenames like
    /// `encoder-epoch-99-avg-1-chunk-16-left-128.onnx` — set each path explicitly.
    #[serde(default = "default_sherpa_encoder")]
    pub sherpa_encoder: String,
    #[serde(default = "default_sherpa_decoder")]
    pub sherpa_decoder: String,
    #[serde(default = "default_sherpa_joiner")]
    pub sherpa_joiner: String,
    #[serde(default = "default_sherpa_tokens")]
    pub sherpa_tokens: String,
    #[serde(default = "default_ollama_enabled")]
    pub ollama_enabled: bool,
    #[serde(default = "default_ollama_host")]
    pub ollama_host: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    #[serde(default = "default_ollama_prompt")]
    pub ollama_prompt: String,
    #[serde(default = "default_summary_suffix")]
    pub summary_suffix: String,
    #[serde(default = "default_ollama_timeout_secs")]
    pub ollama_timeout_secs: u64,
}

fn default_realtime_engine() -> String {
    "vosk".to_string()
}

fn default_sherpa_encoder() -> String { String::new() }
fn default_sherpa_decoder() -> String { String::new() }
fn default_sherpa_joiner() -> String { String::new() }
fn default_sherpa_tokens() -> String { String::new() }

fn default_ollama_enabled() -> bool {
    false
}

fn default_ollama_host() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "llama3.2".to_string()
}

fn default_ollama_prompt() -> String {
    "Summarize the following transcript in concise bullet points.".to_string()
}

fn default_summary_suffix() -> String {
    "_summary".to_string()
}

fn default_ollama_timeout_secs() -> u64 {
    30
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = "config.toml";
        
        if !Path::new(config_path).exists() {
            anyhow::bail!(
                "Config file not found: {}\nPlease create config.toml in the current directory.",
                config_path
            );
        }
        
        let content = fs::read_to_string(config_path)
            .context("Failed to read config.toml")?;
        
        let config: Config = toml::from_str(&content)
            .context("Failed to parse config.toml")?;
        
        // Validate configuration
        config.validate()?;
        
        Ok(config)
    }
    
    fn validate(&self) -> Result<()> {
        // Validate sample rate
        if self.sample_rate < 8000 || self.sample_rate > 48000 {
            anyhow::bail!("sample_rate must be between 8000 and 48000 Hz");
        }
        
        // Validate audio gain
        if self.audio_gain <= 0.0 || self.audio_gain > 10.0 {
            anyhow::bail!("audio_gain must be between 0.0 and 10.0 (recommended: 1.0-5.0)");
        }
        
        // Validate realtime_engine selection
        match self.realtime_engine.as_str() {
            "vosk" => {
                if !Path::new(&self.vosk_model_path).exists() {
                    log::warn!("Vosk model path does not exist: {}", self.vosk_model_path);
                    log::warn!("Please download a model from https://alphacephei.com/vosk/models");
                }
            }
            "sherpa-onnx" => {
                for (name, path) in &[
                    ("sherpa_encoder", &self.sherpa_encoder),
                    ("sherpa_decoder", &self.sherpa_decoder),
                    ("sherpa_joiner",  &self.sherpa_joiner),
                    ("sherpa_tokens",  &self.sherpa_tokens),
                ] {
                    if path.is_empty() {
                        anyhow::bail!(
                            "`{}` must be set when realtime_engine = \"sherpa-onnx\"",
                            name
                        );
                    }
                    if !Path::new(path.as_str()).exists() {
                        anyhow::bail!(
                            "`{}` file not found: {}",
                            name, path
                        );
                    }
                }
            }
            other => {
                anyhow::bail!(
                    "Unknown realtime_engine: \"{}\". Valid values: \"vosk\", \"sherpa-onnx\"",
                    other
                );
            }
        }
        
        // Check if accurate Whisper model path exists (only if accurate recognition is enabled)
        if self.enable_accurate_recognition && !Path::new(&self.whisper_model_path_accurate).exists() {
            log::warn!("Accurate Whisper model path does not exist: {}", self.whisper_model_path_accurate);
            log::warn!("Please download a model from https://huggingface.co/ggerganov/whisper.cpp");
        }
        
        // Create output directory if it doesn't exist
        if !Path::new(&self.output_directory).exists() {
            fs::create_dir_all(&self.output_directory)
                .context("Failed to create output directory")?;
            log::info!("Created output directory: {}", self.output_directory);
        }

        if self.ollama_enabled {
            if self.ollama_model.trim().is_empty() {
                anyhow::bail!("ollama_model must not be empty when ollama_enabled is true");
            }
            if self.ollama_host.trim().is_empty() {
                anyhow::bail!("ollama_host must not be empty when ollama_enabled is true");
            }
            if self.ollama_timeout_secs == 0 {
                anyhow::bail!("ollama_timeout_secs must be greater than 0");
            }
        }
        
        Ok(())
    }
}
