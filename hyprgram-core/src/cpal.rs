use crate::error::CoreError;
use crate::ring::SampleRing;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::thread;
use std::time::Duration;

pub fn spawn_capture(target_device: Option<String>, ring: SampleRing) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        if let Err(e) = run_capture(target_device, ring) {
            eprintln!("cpal capture ended: {}", e);
        }
    })
}

fn run_capture(target_device: Option<String>, ring: SampleRing) -> Result<(), CoreError> {
    let host = cpal::default_host();
    
    let device = if let Some(name) = target_device {
        host.devices()
            .map_err(|e| CoreError::Audio(format!("failed to enumerate devices: {}", e)))?
            .find(|d| {
                d.name()
                    .map(|n| n.to_lowercase().contains(&name.to_lowercase()))
                    .unwrap_or(false)
            })
            .ok_or_else(|| CoreError::Audio(format!("device '{}' not found", name)))?
    } else {
        host.default_output_device()
            .ok_or_else(|| CoreError::Audio("no default output device found".to_string()))?
    };

    let name = device.name().unwrap_or_default();
    eprintln!("cpal: using output device '{}' for loopback", name);

    let mut supported_configs = device
        .supported_output_configs()
        .map_err(|e| CoreError::Audio(format!("failed to get configs: {}", e)))?;

    let config = supported_configs
        .find(|c| c.sample_format() == cpal::SampleFormat::F32)
        .map(|c| c.with_max_sample_rate())
        .ok_or_else(|| CoreError::Audio("no f32 config found".to_string()))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    eprintln!(
        "cpal: config sample_rate={} channels={}",
        sample_rate, channels
    );

    let err_fn = |err| eprintln!("cpal stream error: {}", err);

    let stream = device
        .build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = ring.push_interleaved(data, channels);
            },
            err_fn,
            None,
        )
        .map_err(|e| CoreError::Audio(format!("failed to build stream: {}", e)))?;

    stream
        .play()
        .map_err(|e| CoreError::Audio(format!("failed to play stream: {}", e)))?;

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
