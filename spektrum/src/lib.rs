//! Shared between `spektrum` and dev binaries (e.g. sine preview).
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod dev;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod settings;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod spectrogram;
