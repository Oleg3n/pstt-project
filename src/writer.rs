use hound::{WavWriter, WavSpec};
use std::path::PathBuf;
use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::buffers::BlockingQueue;
use std::time::Duration;

pub fn build_wav_path(output_dir: &str, base_name: &str) -> PathBuf {
    let filename = format!("{}.wav", base_name);
    PathBuf::from(output_dir).join(filename)
}

pub fn create_wav_writer(
    path: &PathBuf, 
    sample_rate: u32
) -> Result<WavWriter<std::io::BufWriter<std::fs::File>>> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    
    std::fs::create_dir_all(path.parent().unwrap())?;
    let writer = WavWriter::create(path, spec)?;
    Ok(writer)
}

pub fn writer_thread(
    resampled_queue: Arc<BlockingQueue<f32>>,
    output_path: PathBuf,
    sample_rate: u32,
    stop_signal: Arc<AtomicBool>,
) -> Result<PathBuf> {
    log::info!("WAV writer thread started");
    
    log::info!("Recording to: {}", output_path.display());
    
    let mut writer = create_wav_writer(&output_path, sample_rate)?;
    
    while !stop_signal.load(Ordering::Relaxed) {
        // Use try_pop_batch with a timeout to check stop signal periodically
        if let Some(samples) = resampled_queue.try_pop_batch(1024) {
            for sample in samples {
                let sample_i16 = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                writer.write_sample(sample_i16)?;
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    
    // Drain remaining samples
    while let Some(samples) = resampled_queue.try_pop_batch(1024) {
        for sample in samples {
            let sample_i16 = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer.write_sample(sample_i16)?;
        }
    }
    
    writer.finalize()?;
    log::info!("WAV writer thread finished: {}", output_path.display());
    
    Ok(output_path)
}
