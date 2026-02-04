use anyhow::Result;
use std::path::PathBuf;

use whisper_rs::{WhisperContext, FullParams, SamplingStrategy};

// Import Config from your config module (adjust the path if needed)
use crate::config::Config;

pub fn transcribe_with_whisper(
    wav_path: &PathBuf,
    model_path: &str,
    output_dir: &str,
    config: &Config,
) -> Result<String> {
    use std::fs::File;
    use std::io::Write;
    use whisper_rs::WhisperContextParameters;
    
    log::info!("Loading Whisper accurate model from: {}", model_path);
    let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())?;
        
        log::info!("Loading audio from: {}", wav_path.display());
        let samples = load_audio_samples(wav_path, config)?;
        
        log::info!("Loaded {} samples", samples.len());
        
        // Set up parameters
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_debug_mode(false);
        
        params.set_language(Some("en"));
        
        log::info!("Transcribing with Whisper...");
        let mut state = ctx.create_state()?;
        state.full(params, &samples)?;
        
        let num_segments = state.full_n_segments();
        let mut full_text = String::new();
        
        log::info!("Processing {} segments", num_segments);
        
        for i in 0..num_segments {
            let segment = state.get_segment(i)
                .ok_or_else(|| anyhow::anyhow!("No segment found"))?;
            full_text.push_str(segment.to_str()?);
            full_text.push(' ');
        }
        
        let filename = wav_path.file_stem().unwrap().to_str().unwrap();
        let output_path = format!("{}/{}_accurate.txt", output_dir, filename);
        let mut file = File::create(&output_path)?;
        writeln!(file, "{}", full_text.trim())?;
        
        log::info!("Accurate transcription saved to: {}", output_path);
        println!("📝 Accurate transcription saved to: {}", output_path);
        
        Ok(full_text)
}

fn load_audio_samples(path: &PathBuf, config: &Config) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let samples: Vec<f32> = reader.samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();
    
    // Calculate statistics
    let rms = (samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let max = samples.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min = samples.iter().cloned().fold(f32::INFINITY, f32::min);
    
    let clipped_count = samples.iter()
        .filter(|&&s| s.abs() > 0.99)
        .count();
    let clipped_percent = (clipped_count as f32 / samples.len() as f32) * 100.0;
    
    log::info!("=== AUDIO ANALYSIS ===");
    log::info!("Samples: {}", samples.len());
    log::info!("Duration: {:.1} seconds ({:.1} minutes)", 
               samples.len() as f32 / 16000.0,
               samples.len() as f32 / 16000.0 / 60.0);
    log::info!("");

    // Calculate distribution
    let loud_samples = samples.iter().filter(|&&s| s.abs() > 0.1).count();
    let very_loud_samples = samples.iter().filter(|&&s| s.abs() > 0.5).count();
    let quiet_samples = samples.iter().filter(|&&s| s.abs() < 0.01).count();

    let very_quiet_pct = quiet_samples as f32 / samples.len() as f32 * 100.0;
    let normal_pct = (samples.len() - loud_samples - quiet_samples) as f32 / samples.len() as f32 * 100.0;
    let loud_pct = loud_samples as f32 / samples.len() as f32 * 100.0;
    let very_loud_pct = very_loud_samples as f32 / samples.len() as f32 * 100.0;

    log::info!("Audio Levels:");
    log::info!("  Average (RMS): {:.4}", rms);
    log::info!("  Peak (max):    {:.4}", max);
    log::info!("  Minimum (min): {:.4}", min);
    log::info!("");

    log::info!("Distribution:");
    log::info!("  Very quiet (< 0.01): {:.1}%", very_quiet_pct);
    log::info!("  Normal (0.01-0.1):   {:.1}%", normal_pct);
    log::info!("  Loud (> 0.1):        {:.1}%", loud_pct);
    log::info!("  Very loud (> 0.5):   {:.1}%", very_loud_pct);
    log::info!("  Clipped (≈ 1.0):     {:.2}%", clipped_percent);
    log::info!("=====================");
    log::info!("");

    // Get current gain
    let current_gain = config.audio_gain;

    // Smart diagnostics with calculated recommendations
    if very_quiet_pct > 40.0 {
        // Calculate recommended gain to achieve RMS ~0.08
        let target_rms = 0.08;
        let recommended_gain = (target_rms / rms * current_gain).min(20.0);
        
        log::error!("❌ PROBLEM: {:.0}% of audio is very quiet!", very_quiet_pct);
        log::error!("   This will cause poor transcription quality.");
        log::error!("");
        log::error!("   SOLUTION: Increase audio_gain in config.toml");
        log::error!("   Current: audio_gain = {:.1}", current_gain);
        log::error!("   Recommended: audio_gain = {:.1}", recommended_gain);
        log::error!("");
        if clipped_percent < 1.0 {
            log::error!("   Note: You have headroom - only {:.2}% clipping", clipped_percent);
            log::error!("   It's OK to increase gain even if a few samples clip!");
        }
    } else if rms < 0.05 {
        let target_rms = 0.08;
        let recommended_gain = (target_rms / rms * current_gain).min(20.0);
        
        log::warn!("⚠️  Audio is quieter than ideal (RMS = {:.4})", rms);
        log::warn!("   Recommended RMS: 0.05 - 0.30 for best results");
        log::warn!("   Current gain: {:.1}", current_gain);
        log::warn!("   Suggested gain: {:.1}", recommended_gain);
    } else if rms > 0.5 {
        let target_rms = 0.15;
        let recommended_gain = (target_rms / rms * current_gain).max(0.5);
        
        log::warn!("⚠️  Audio is very loud (RMS = {:.4})", rms);
        log::warn!("   Risk of distortion.");
        log::warn!("   Current gain: {:.1}", current_gain);
        log::warn!("   Suggested gain: {:.1}", recommended_gain);
    } else if clipped_percent > 5.0 {
        let target_rms = 0.15;
        let recommended_gain = (target_rms / rms * current_gain).max(0.5);
        
        log::error!("❌ SEVERE CLIPPING: {:.1}% of samples are clipped!", clipped_percent);
        log::error!("   Audio is distorted. REDUCE audio_gain in config.toml");
        log::error!("   Current: audio_gain = {:.1}", current_gain);
        log::error!("   Recommended: audio_gain = {:.1}", recommended_gain);
    } else if clipped_percent > 1.0 {
        log::warn!("⚠️  Moderate clipping: {:.2}% clipped", clipped_percent);
        log::warn!("   Current gain: {:.1}", current_gain);
        log::warn!("   Consider reducing to {:.1} for cleaner audio", current_gain * 0.8);
    } else {
        log::info!("✅ AUDIO QUALITY: Good");
        log::info!("   RMS level is in optimal range for transcription");
        log::info!("   Current gain: {:.1}", current_gain);
    }

    log::info!("");
    
    Ok(samples)
}

