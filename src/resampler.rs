use rubato::{Resampler, SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};
use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::buffers::BlockingQueue;
use crate::config::Config;

pub struct AudioResampler {
    resampler: SincFixedIn<f32>,
}

impl AudioResampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Result<Self> {
        let resample_ratio = output_rate as f64 / input_rate as f64;
        
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        
        let resampler = SincFixedIn::<f32>::new(
            resample_ratio,
            2.0,
            params,
            1024,
            1, // mono
        )?;
        
        Ok(Self { resampler })
    }
    
    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }
        
        // Process as mono
        let input_frames = vec![input.to_vec()];
        let output_frames = self.resampler.process(&input_frames, None)?;
        
        Ok(output_frames[0].clone())
    }
}

pub fn resampler_thread(
    raw_queue: Arc<BlockingQueue<f32>>,
    resampled_queue: Arc<BlockingQueue<f32>>,
    config: Arc<Config>,
    stop_signal: Arc<AtomicBool>,
) {
    log::info!("Resampler thread started");
    
    // Get device sample rate from the first samples (assuming 48000 Hz for now)
    let input_rate = 48000;
    let output_rate = config.sample_rate;
    
    let mut resampler = match AudioResampler::new(input_rate, output_rate) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create resampler: {}", e);
            return;
        }
    };
    
    log::info!("Resampling from {} Hz to {} Hz", input_rate, output_rate);
    
    while !stop_signal.load(Ordering::Relaxed) {
        let samples = raw_queue.pop_batch(4096);
        
        // Convert stereo to mono if needed (average channels)
        let mono_samples: Vec<f32> = if samples.len() % 2 == 0 {
            samples.chunks(2)
                .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) / 2.0)
                .collect()
        } else {
            samples
        };
        
        match resampler.process(&mono_samples) {
            Ok(resampled) => {
                if !resampled_queue.push(resampled) {
                    log::warn!("Resampler: Failed to push to resampled queue");
                }
            }
            Err(e) => {
                log::error!("Resampling error: {}", e);
            }
        }
    }
    
    log::info!("Resampler thread finished");
}
