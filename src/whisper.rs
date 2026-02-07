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
        let file_size = std::fs::metadata(wav_path)?.len();
        let file_size_mb = file_size as f64 / (1024.0 * 1024.0);
        log::info!("Audio file size: {} bytes ({:.2} MB)", file_size, file_size_mb);

        let samples = load_audio_samples(wav_path)?;
        log::info!("Loaded {} samples", samples.len());

        // Get current gain
        let current_gain = config.audio_gain;

        analyze_audio_and_recommend_gain(&samples, current_gain)?;
        
        // Set up parameters
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(true);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_debug_mode(false);
        
        // params.set_language(Some("en"));

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

pub fn analyze_audio_and_recommend_gain(
    samples: &[f32],
    current_gain: f32,
) -> Result<()> {
    if samples.is_empty() {
        return Ok(());
    }

    // Calculate audio statistics
    let mut sum_squares = 0.0f64;
    let mut max_val = 0.0f32;
    let mut min_val = 0.0f32;
    
    let mut very_quiet_count = 0;
    let mut normal_count = 0;
    let mut loud_count = 0;
    let mut very_loud_count = 0;
    let mut clipped_count = 0;
    
    for &sample in samples {
        let abs_sample = sample.abs();
        sum_squares += (abs_sample as f64).powi(2);
        
        if sample > max_val {
            max_val = sample;
        }
        if sample < min_val {
            min_val = sample;
        }
        
        // Categorize samples
        if abs_sample < 0.01 {
            very_quiet_count += 1;
        } else if abs_sample < 0.1 {
            normal_count += 1;
        } else if abs_sample < 0.5 {
            loud_count += 1;
        } else if abs_sample >= 0.99 {
            clipped_count += 1;
        } else {
            very_loud_count += 1;
        }
    }
    
    let rms = (sum_squares / samples.len() as f64).sqrt() as f32;
    
    // Calculate percentages
    let total = samples.len() as f32;
    let very_quiet_pct = (very_quiet_count as f32 / total) * 100.0;
    let normal_pct = (normal_count as f32 / total) * 100.0;
    let loud_pct = (loud_count as f32 / total) * 100.0;
    let very_loud_pct = (very_loud_count as f32 / total) * 100.0;
    let clipped_pct = (clipped_count as f32 / total) * 100.0;


    log::info!("=== AUDIO ANALYSIS ===");
    log::info!("Samples: {}", samples.len());
    log::info!("Duration: {:.1} seconds ({:.1} minutes)", 
            samples.len() as f32 / 16000.0,
            samples.len() as f32 / 16000.0 / 60.0);


    // Log audio statistics
    log::info!("=====================");
    log::info!("Audio Levels:");
    log::info!("  Average (RMS): {:.4}", rms);
    log::info!("  Peak (max):    {:.4}", max_val);
    log::info!("  Minimum (min): {:.4}", min_val);
    log::info!("");
    log::info!("Distribution:");
    log::info!("  Very quiet (< 0.01): {:.1}%", very_quiet_pct);
    log::info!("  Normal (0.01-0.1):   {:.1}%", normal_pct);
    log::info!("  Loud (> 0.1):        {:.1}%", loud_pct);
    log::info!("  Very loud (> 0.5):   {:.1}%", very_loud_pct);
    log::info!("  Clipped (≈ 1.0):     {:.2}%", clipped_pct);
    log::info!("=====================");
    log::info!("");
    
    // Improved gain recommendation logic
    let target_rms = 0.08;
    let target_quiet_pct = 30.0; // Ideal: less than 30% should be very quiet
    
    let recommended_gain = if very_quiet_pct > 50.0 {
        // Case 1: Audio is mostly silence/noise - need MORE gain
        // Calculate how much more gain we need to reduce quiet percentage
        let quiet_ratio = very_quiet_pct / target_quiet_pct;
        let gain_multiplier = quiet_ratio.sqrt().min(3.0); // sqrt to be less aggressive
        (current_gain * gain_multiplier).min(20.0).max(current_gain * 1.5)
    } else if rms < target_rms {
        // Case 2: Audio is present but RMS too low - increase proportionally
        let rms_ratio = target_rms / rms.max(0.001); // Avoid division by zero
        (current_gain * rms_ratio).min(20.0)
    } else if clipped_pct > 1.0 {
        // Case 3: Too much clipping - decrease gain
        let clip_reduction = 1.0 - (clipped_pct / 100.0).min(0.5);
        (current_gain * clip_reduction).max(1.0)
    } else {
        // Case 4: Audio levels are good
        current_gain
    };
    
    // Determine if there's a problem and show appropriate message
    let has_problem = very_quiet_pct > 50.0 || rms < 0.05 || clipped_pct > 5.0;
    
    if has_problem {
        if very_quiet_pct > 50.0 {
            log::error!("❌ PROBLEM: {:.0}% of audio is very quiet!", very_quiet_pct);
            log::error!("   This will cause poor transcription quality.");
        } else if clipped_pct > 5.0 {
            log::error!("❌ PROBLEM: {:.1}% of audio is clipped!", clipped_pct);
            log::error!("   This causes distortion and poor quality.");
        } else if rms < 0.05 {
            log::error!("❌ PROBLEM: Overall audio level too low (RMS: {:.4})", rms);
            log::error!("   This will cause poor transcription quality.");
        }
        
        log::error!("");
        
        // Show the appropriate recommendation
        if recommended_gain > current_gain {
            log::error!("   SOLUTION: Increase audio_gain in config.toml");
            log::error!("   Current: audio_gain = {:.1}", current_gain);
            log::error!("   Recommended: audio_gain = {:.1}", recommended_gain);
        } else if recommended_gain < current_gain {
            log::error!("   SOLUTION: Decrease audio_gain in config.toml");
            log::error!("   Current: audio_gain = {:.1}", current_gain);
            log::error!("   Recommended: audio_gain = {:.1}", recommended_gain);
        } else {
            log::error!("   Note: Current gain seems appropriate.");
            log::error!("   Issue may be with microphone input level.");
        }
        
        log::error!("");
    } else {
        log::info!("✅ Audio levels look good!");
        log::info!("   Current gain ({:.1}) is appropriate.", current_gain);
        log::info!("");
    }
    
    Ok(())
}

fn load_audio_samples(path: &PathBuf) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let samples: Vec<f32> = reader.samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();
        
    Ok(samples)
}

