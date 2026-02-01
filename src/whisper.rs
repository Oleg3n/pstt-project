use anyhow::Result;
use std::path::PathBuf;

use whisper_rs::{WhisperContext, FullParams, SamplingStrategy};

pub fn transcribe_with_whisper(
    wav_path: &PathBuf,
    model_path: &str,
    output_dir: &str,
) -> Result<String> {
    use std::fs::File;
    use std::io::Write;
    use whisper_rs::WhisperContextParameters;
    
    log::info!("Loading Whisper accurate model from: {}", model_path);
    let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())?;
        
        log::info!("Loading audio from: {}", wav_path.display());
        let samples = load_audio_samples(wav_path)?;
        
        log::info!("Loaded {} samples", samples.len());
        
        // Set up parameters
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
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
        println!("ðŸ“ Accurate transcription saved to: {}", output_path);
        
        Ok(full_text)
}

fn load_audio_samples(path: &PathBuf) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let samples: Vec<f32> = reader.samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();
    Ok(samples)
}
