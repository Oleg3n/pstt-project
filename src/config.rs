use serde::Deserialize;
use anyhow::{Result, Context};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub sample_rate: u32,
    pub output_directory: String,
    pub vosk_model_path: String,
    pub whisper_model_path: String,
    pub enable_accurate_recognition: bool,
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
        
        // Check if Vosk model path exists
        if !Path::new(&self.vosk_model_path).exists() {
            log::warn!("Vosk model path does not exist: {}", self.vosk_model_path);
            log::warn!("Please download a model from https://alphacephei.com/vosk/models");
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
