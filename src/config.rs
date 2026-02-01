use serde::Deserialize;
use anyhow::{Result, Context};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub sample_rate: u32,
    pub audio_gain: f32,
    pub output_directory: String,
    pub whisper_model_path_realtime: String,
    pub whisper_model_path_accurate: String,
    pub enable_accurate_recognition: bool,
    pub chunk_duration_secs: u32,
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
        
        // Validate chunk duration
        if self.chunk_duration_secs < 1 || self.chunk_duration_secs > 30 {
            anyhow::bail!("chunk_duration_secs must be between 1 and 30 seconds (recommended: 3-5 seconds)");
        }
        
        // Check if real-time Whisper model path exists
        if !Path::new(&self.whisper_model_path_realtime).exists() {
            log::warn!("Real-time Whisper model path does not exist: {}", self.whisper_model_path_realtime);
            log::warn!("Please download a model from https://huggingface.co/ggerganov/whisper.cpp");
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
        
        Ok(())
    }
}
