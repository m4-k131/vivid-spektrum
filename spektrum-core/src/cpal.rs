use crate::error::CoreError;
use crate::ring::SampleRing;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::thread;

pub fn output_device_names() -> Vec<String> {
    cpal::default_host()
        .output_devices()
        .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
        .unwrap_or_default()
}

pub fn spawn_capture(target_device: Option<String>, ring: SampleRing) -> (thread::JoinHandle<()>, mpsc::Sender<String>) {
    let (target_tx, target_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut target = target_device;
        loop {
            match run_capture(target.clone(), ring.clone(), &target_rx) {
                Ok(Some(next_target)) => target = Some(next_target),
                Ok(None) => return,
                Err(e) => {
                    eprintln!("cpal capture ended: {e}");
                    return;
                }
            }
        }
    });
    (worker, target_tx)
}

fn run_capture(target_device: Option<String>, ring: SampleRing, target_rx: &mpsc::Receiver<String>) -> Result<Option<String>, CoreError> {
    let host = cpal::default_host();
    
    let device = if let Some(name) = target_device.filter(|name| name != "default output") {
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

    match target_rx.recv() {
        Ok(next_target) => Ok(Some(next_target)),
        Err(_) => Ok(None),
    }
}
