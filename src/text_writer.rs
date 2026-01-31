use std::sync::mpsc;
use std::fs::File;
use std::io::{Write, BufWriter};
use anyhow::Result;
use crate::recognition::RecognizedText;

pub fn text_writer_thread(
    text_receiver: mpsc::Receiver<RecognizedText>,
    output_path: String,
) -> Result<()> {
    log::info!("Text writer thread started");
    
    let file = File::create(&output_path)?;
    let mut writer = BufWriter::new(file);
    
    log::info!("Saving recognized text to: {}", output_path);
    
    let mut line_count = 0;
    
    while let Ok(recognized) = text_receiver.recv() {
        // Write with timestamp
        writeln!(
            writer,
            "[{}] {}",
            recognized.timestamp.format("%H:%M:%S"),
            recognized.text
        )?;
        
        line_count += 1;
        
        // Flush on final result to ensure it's saved
        if recognized.is_final {
            writer.flush()?;
        }
        
        // Periodic flush every 5 lines for safety
        if line_count % 5 == 0 {
            writer.flush()?;
        }
    }
    
    // Final flush when channel closes
    writer.flush()?;
    log::info!("Text writer thread finished: {} lines written", line_count);
    
    Ok(())
}
