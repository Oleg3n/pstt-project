use cpal::traits::{DeviceTrait, HostTrait};
use anyhow::{Result, Context};

pub fn list_input_devices() -> Result<Vec<(usize, String)>> {
    let host = cpal::default_host();
    let devices: Result<Vec<_>> = host.input_devices()?
        .enumerate()
        .map(|(i, device)| {
            let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            Ok((i, name))
        })
        .collect();
    
    devices
}

pub fn select_device(index: usize) -> Result<cpal::Device> {
    let host = cpal::default_host();
    let device = host.input_devices()?
        .nth(index)
        .context("Invalid device index")?;
    Ok(device)
}

pub fn get_device_info(device: &cpal::Device) -> Result<(String, cpal::SupportedStreamConfig)> {
    let name = device.name()?;
    let config = device.default_input_config()?;
    Ok((name, config))
}
