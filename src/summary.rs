use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;

#[derive(Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

pub fn build_summary_path(output_dir: &str, base_name: &str, suffix: &str) -> PathBuf {
    let mut suffix = suffix.trim().to_string();
    if suffix.is_empty() {
        suffix = "_summary".to_string();
    }

    let has_txt = suffix.to_ascii_lowercase().ends_with(".txt");
    let filename = if has_txt {
        format!("{}{}", base_name, suffix)
    } else {
        format!("{}{}.txt", base_name, suffix)
    };

    PathBuf::from(output_dir).join(filename)
}

pub fn generate_summary_from_file(
    config: &Config,
    input_path: &Path,
    output_path: &Path,
) -> Result<()> {
    let input_text = fs::read_to_string(input_path)
        .with_context(|| format!("Failed to read transcript: {}", input_path.display()))?;

    if input_text.trim().is_empty() {
        log::warn!("Transcript is empty, skipping summary generation: {}", input_path.display());
        return Ok(());
    }

    let summary = generate_summary(config, &input_text)?;

    fs::write(output_path, summary)
        .with_context(|| format!("Failed to write summary: {}", output_path.display()))?;

    log::info!("Summary saved to: {}", output_path.display());
    println!("📝 Summary saved to: {}", output_path.display());

    Ok(())
}

fn generate_summary(config: &Config, transcript: &str) -> Result<String> {
    let prompt = format!("{}\n\n{}", config.ollama_prompt, transcript);
    let url = build_ollama_url(&config.ollama_host);

    let request = OllamaGenerateRequest {
        model: config.ollama_model.clone(),
        prompt,
        stream: false,
    };

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(url)
        .json(&request)
        .send()
        .context("Failed to send request to Ollama")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        anyhow::bail!("Ollama request failed ({}): {}", status, body);
    }

    let payload: OllamaGenerateResponse = response
        .json()
        .context("Failed to parse Ollama response")?;

    Ok(payload.response.trim().to_string())
}

fn build_ollama_url(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('/');
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    };

    format!("{}/api/generate", with_scheme)
}
