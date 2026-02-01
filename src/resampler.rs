use rubato::{Resampler, SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};
use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::buffers::BlockingQueue;
use crate::config::Config;
use std::time::Duration;

pub struct AudioResampler {
    resampler: SincFixedIn<f32>,
    chunk_size: usize,
    buffer: Vec<f32>,
}

impl AudioResampler {
    pub fn new(input_rate: u32, output_rate: u32, chunk_size: usize) -> Result<Self> {
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
            chunk_size,
            1, // mono
        )?;
        
        Ok(Self { 
            resampler,
            chunk_size,
            buffer: Vec::with_capacity(chunk_size * 2),
        })
    }
    
    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }
        
        // Add incoming samples to buffer
        self.buffer.extend_from_slice(input);
        
        let mut output = Vec::new();
        
        // Process complete chunks
        while self.buffer.len() >= self.chunk_size {
            // Take exactly chunk_size samples
            let chunk: Vec<f32> = self.buffer.drain(..self.chunk_size).collect();
            
            // Process with Rubato
            let input_frames = vec![chunk];
            let output_frames = self.resampler.process(&input_frames, None)?;
            
            // Collect output
            output.extend_from_slice(&output_frames[0]);
        }
        
        Ok(output)
    }
    
    pub fn flush(&mut self) -> Result<Vec<f32>> {
        // Process any remaining samples by padding to chunk_size
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        
        let remaining = self.buffer.len();
        if remaining > 0 {
            // Pad with zeros to reach chunk_size
            self.buffer.resize(self.chunk_size, 0.0);
            
            let chunk = self.buffer.clone();
            self.buffer.clear();
            
            let input_frames = vec![chunk];
            let output_frames = self.resampler.process(&input_frames, None)?;
            
            // Only return the portion corresponding to actual samples
            let output_len = (remaining as f64 * self.resampler.output_frames_next() as f64 / self.chunk_size as f64) as usize;
            Ok(output_frames[0][..output_len.min(output_frames[0].len())].to_vec())
        } else {
            Ok(Vec::new())
        }
    }
}

pub fn resampler_thread(
    raw_queue: Arc<BlockingQueue<f32>>,
    resampled_queue_writer: Arc<BlockingQueue<f32>>,
    resampled_queue_vosk: Arc<BlockingQueue<f32>>,
    config: Arc<Config>,
    stop_signal: Arc<AtomicBool>,
) {
    log::info!("Resampler thread started");
    
    // Get device sample rate from the first samples (assuming 48000 Hz for now)
    let input_rate = 48000;
    let output_rate = config.sample_rate;
    let gain = config.audio_gain;
    
    // Use a reasonable chunk size for Rubato (1024 samples at 48kHz = ~21ms)
    let chunk_size = 1024;
    
    let mut resampler = match AudioResampler::new(input_rate, output_rate, chunk_size) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create resampler: {}", e);
            return;
        }
    };
    
    log::info!("Resampling from {} Hz to {} Hz (chunk size: {} samples, gain: {}x)", 
               input_rate, output_rate, chunk_size, gain);
    
    while !stop_signal.load(Ordering::Relaxed) {
        if let Some(samples) = raw_queue.try_pop_batch(4096) {
            // Convert stereo to mono if needed (average channels)
            let mono_samples: Vec<f32> = if samples.len() % 2 == 0 {
                samples.chunks(2)
                    .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) / 2.0)
                    .collect()
            } else {
                samples
            };
            
            // Apply gain (amplification)
            let amplified: Vec<f32> = mono_samples.iter()
                .map(|&s| (s * gain).clamp(-1.0, 1.0))  // Apply gain and clamp to prevent clipping
                .collect();
            
            // Process samples (will buffer internally until chunk_size is reached)
            match resampler.process(&amplified) {
                Ok(resampled) => {
                    if !resampled.is_empty() {
                        let resampled_clone = resampled.clone();
                        if !resampled_queue_writer.push(resampled) {
                            log::warn!("Resampler: Failed to push to resampled writer queue");
                        }
                        if !resampled_queue_vosk.push(resampled_clone) {
                            log::warn!("Resampler: Failed to push to resampled Vosk queue");
                        }
                    }
                }
                Err(e) => {
                    log::error!("Resampling error: {}", e);
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    
    // Drain remaining samples in raw_queue
    while let Some(samples) = raw_queue.try_pop_batch(4096) {
        // Convert stereo to mono if needed (average channels)
        let mono_samples: Vec<f32> = if samples.len() % 2 == 0 {
            samples.chunks(2)
                .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) / 2.0)
                .collect()
        } else {
            samples
        };
        
        // Apply gain (amplification)
        let amplified: Vec<f32> = mono_samples.iter()
            .map(|&s| (s * gain).clamp(-1.0, 1.0))  // Apply gain and clamp to prevent clipping
            .collect();
        
        // Process samples (will buffer internally until chunk_size is reached)
        match resampler.process(&amplified) {
            Ok(resampled) => {
                if !resampled.is_empty() {
                    let resampled_clone = resampled.clone();
                    if !resampled_queue_writer.push(resampled) {
                        log::warn!("Resampler: Failed to push to resampled writer queue");
                    }
                    if !resampled_queue_vosk.push(resampled_clone) {
                        log::warn!("Resampler: Failed to push to resampled Vosk queue");
                    }
                }
            }
            Err(e) => {
                log::error!("Resampling error: {}", e);
            }
        }
    }
    
    // Flush any remaining buffered samples
    log::info!("Flushing resampler buffer...");
    match resampler.flush() {
        Ok(resampled) => {
            if !resampled.is_empty() {
                let resampled_clone = resampled.clone();
                if !resampled_queue_writer.push(resampled) {
                    log::warn!("Resampler: Failed to push final samples to writer queue");
                }
                if !resampled_queue_vosk.push(resampled_clone) {
                    log::warn!("Resampler: Failed to push final samples to Vosk queue");
                }
            }
        }
        Err(e) => {
            log::error!("Error flushing resampler: {}", e);
        }
    }
    
    log::info!("Resampler thread finished");
}
