//! spektrum-core: DSP, ring buffer, and (Linux) PipeWire capture.

pub mod colormap;
pub mod dsp;
pub mod error;
pub mod freq_grid;
pub mod overlay;
#[cfg(target_os = "linux")]
pub mod pipewire;
#[cfg(target_os = "windows")]
pub mod cpal;
pub mod render;
pub mod ring;
pub mod profiles;

pub use dsp::{
    normalize_hop_size, BandAggregation, SpectrumConfig, SpectrumProcessor, Transform, Weighting,
    WindowFunction, DEFAULT_FFT_HOP_SAMPLES, DEFAULT_FFT_WINDOW_SAMPLES,
};
pub use error::CoreError;
pub use render::{render_spectrogram_png, render_spectrogram_png_with_grid, samples_to_spectrogram, SpectrogramImageConfig};
pub use ring::{SampleRing, SampleRingProducer, SampleRingConsumer, sample_ring_pair};
pub use colormap::{Colormap, builtin_colormap, builtin_colormap_names, default_colormap, resolve_colormap};
