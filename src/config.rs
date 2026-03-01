use serde::Deserialize;
use anyhow::{Result, Context};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub sample_rate: u32,
    pub audio_gain: f32,
    pub output_directory: String,
    #[serde(default)]
    pub vosk_model_path: Option<String>,
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
                // Path is required when using the vosk engine
                let path = self.vosk_model_path.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "vosk_model_path must be set when realtime_engine = \"vosk\""
                    )
                })?;
                if path.trim().is_empty() {
                    anyhow::bail!("vosk_model_path must not be empty when realtime_engine = \"vosk\"");
                }
                if !Path::new(path).exists() {
                    log::warn!("Vosk model path does not exist: {}", path);
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

// -----------------------------------------------------------------------------
// Unit tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use toml;

    fn parse_toml(s: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(s)
    }

    #[test]
    fn sherpa_engine_without_vosk_path_is_ok() {
        let toml = r#"
            sample_rate = 16000
            audio_gain = 1.0
            output_directory = "./recordings"
            realtime_engine = "sherpa-onnx"
            whisper_model_path_accurate = "./models/ggml-small.en.bin"
            enable_accurate_recognition = false
        "#;
        let cfg: Config = parse_toml(toml).expect("parsing failed");
        assert!(cfg.vosk_model_path.is_none());
        // validation may still fail because sherpa paths are missing, but it
        // should not complain about vosk_model_path.
        let err = cfg.validate().unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains("vosk_model_path"), "unexpected vosk error: {}", msg);
    }

    #[test]
    fn vosk_engine_requires_vosk_path() {
        let toml = r#"
            sample_rate = 16000
            audio_gain = 1.0
            output_directory = "./recordings"
            realtime_engine = "vosk"
            whisper_model_path_accurate = "./models/ggml-small.en.bin"
            enable_accurate_recognition = false
        "#;
        let cfg: Config = parse_toml(toml).expect("parsing failed");
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("vosk_model_path must be set"));
    }

    #[test]
    fn vosk_engine_empty_path_errors() {
        let toml = r#"
            sample_rate = 16000
            audio_gain = 1.0
            output_directory = "./recordings"
            realtime_engine = "vosk"
            vosk_model_path = ""
            whisper_model_path_accurate = "./models/ggml-small.en.bin"
            enable_accurate_recognition = false
        "#;
        let cfg: Config = parse_toml(toml).expect("parsing failed");
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }
}
