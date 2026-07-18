# AGENTS.md — vividspektrum codebase overview

## What is this?

A **spectrogram visualizer** in Rust. Generates high-resolution spectrograms from audio — both offline (PNG from files) and live (PipeWire capture → GPU-rendered window on Linux/Wayland).

**Workspace root:** `c:\Users\malte\OneDrive\Documents\Code\vivid-spektrum`

---

## Crate layout

```
vivid-spektrum/                 # workspace root (Cargo.toml with [workspace])
├── spektrum-core/           # DSP + rendering library (no GPU, no audio I/O except PipeWire)
│   └── src/
│       ├── dsp.rs           # SpectrumProcessor (STFT/FFT), SpectrumConfig, WindowFunction
│       ├── render.rs        # samples_to_spectrogram (parallel), render_spectrogram_png (CPU viridis)
│       ├── profiles.rs      # builtin_profile(), load_profile() — TOML config + named presets
│       ├── error.rs         # CoreError enum
│       ├── ring.rs          # SampleRing (lock-free SPSC ring buffer)
│       ├── pipewire.rs      # #[cfg(linux)] PipeWire capture → SampleRing
│       └── lib.rs           # re-exports
├── spektrum/                     # application crate
│   └── src/
│       ├── main.rs          # CLI entry point (Args with --profile/--config/overrides)
│       ├── linux.rs         # #[cfg(linux)] Iced + WGPU live spectrogram window
│       ├── spectrogram.rs   # GPU shader (WGSL) for realtime spectrogram display
│       ├── lib.rs           # #[cfg(linux)] modules: dev, spectrogram
│       └── bin/
│           ├── audio_to_png.rs  # Offline: audio file → spectrogram PNG (cross-platform)
│           └── sine_preview.rs  # #[cfg(linux)] sine generator → live spectrogram window
├── example_profile.toml     # Reference TOML config
├── BUILD.md                 # Linux build instructions (system deps)
├── WINDOWS.md               # Windows dev guide (conda PATH workaround, commands)
└── .gitignore               # target/, *.mp3, *.png
```

---

## Core data flow

### Offline path (`audio_to_png`)

```
audio file (mp3/wav/flac/…)
  → symphonia decode → Vec<f32> mono samples
  → samples_to_spectrogram() [rayon-parallel STFT]
      → N × SpectrumProcessor (one per thread chunk)
      → Vec<Vec<f32>> columns (each column = log_bins normalized magnitudes 0..1)
  → render_spectrogram_png()
      → CPU double-loop: for each pixel, sample column/bin → viridis_u8 → Rgb
      → image crate → PNG file
```

### Live path (Linux only)

```
PipeWire capture thread
  → SampleRing (lock-free ring buffer)
  → DSP thread: SpectrumProcessor::push_samples() → Vec<Vec<f32>>
  → Arc<Mutex<VecDeque<Vec<f32>>>> pending_spectra
  → Iced/WGPU render loop (16ms tick)
      → SpectrogramProgram (WGSL shader) reads pending_spectra
      → uploads as texture → GPU renders to Wayland surface
```

---

## Key types

| Type | Crate | Purpose |
|------|-------|---------|
| `SpectrumConfig` | core | All DSP parameters (window_size, hop, bins, dB range, window_fn, band_aggregation, freq_smoothing_lobes, amplitude_gamma) |
| `SpectrumProcessor` | core | Stateful STFT engine: windowing → FFT → log-magnitude mapping |
| `WindowFunction` | core | Enum: Hann, Hamming, Blackman, BlackmanHarris |
| `BandAggregation` | core | Enum: Nearest, Triangular |
| `SpectrogramImageConfig` | core | SpectrumConfig + image dimensions + scroll direction |
| `Profile` | core | TOML-deserializable: spectrum + optional image section |
| `SpectrogramProgram` | app | GPU shader state (pending_spectra queue, bins, history, dev config) |
| `SampleRing` | core | Lock-free ring buffer for PipeWire → DSP thread |

---

## What's implemented

- **STFT spectrogram** with configurable FFT window size, hop, log-frequency bins
- **4 window functions**: Hann (default), Hamming, Blackman, Blackman-Harris
- **Band aggregation**: Nearest (single-bin) or Triangular (weighted filter bank)
- **Frequency-domain smoothing**: Gaussian kernel (configurable sigma, 0=off)
- **Amplitude gamma**: power-curve control over brightness
- **Temporal smoothing**: EMA (exponential moving average) and peak hold decay across columns
- **Colormap presets**: 7 built-in colormaps (viridis, inferno, magma, plasma, turbo, grayscale, heat) via gradient-stop LUT
- **Parallel FFT**: `samples_to_spectrogram` splits windows across rayon threads
- **TOML profiles**: `--profile laptop|default|high-resolution` or `--config file.toml`
- **CLI overrides**: any `--fft`, `--hop`, `--window-fn`, `--width`, etc. overrides profile
- **Offline PNG**: `audio_to_png` binary works on Windows, macOS, Linux
- **Live GPU rendering**: Linux/Wayland only (iced + wgpu + PipeWire)
- **Verbose progress**: decode packets, FFT windows/s, render timing
- **A/C-weighting**: IEC 61672 A and C frequency weighting curves
- **CQT**: constant-Q transform with configurable bins/octave (alternative to STFT+log)
- **Non-power-of-two FFT**: supported by rustfft mixed-radix planner

## Scope

The current codebase is feature complete for the planned offline/live spectrogram pipeline; remaining work is cleanup and polish.

---

## Build commands (Windows, conda workaround)

```powershell
# Check compilation
& "$env:USERPROFILE\.cargo\bin\cargo.exe" check --workspace

# Run offline PNG generator
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run -p spektrum --bin audio_to_png -- input.mp3 output.png

# With profile + overrides
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run -p spektrum --bin audio_to_png -- input.mp3 out.png --profile high-resolution --window-fn blackman-harris
```

On Linux without conda, just use `cargo` directly.

---

## Code conventions

- **No comments or docstrings** are added unless explicitly requested
- Platform-gating: `#[cfg(target_os = "linux")]` on modules and binaries that need PipeWire/iced
- Errors: `CoreError::Dsp(String)` for DSP failures, `anyhow::Result` in binaries
- Serde: `#[serde(rename_all = "kebab-case")]` on enums, `#[serde(default)]` on new fields for backward compat
- Parallelism: `rayon::par_iter()` in `samples_to_spectrogram`, one `SpectrumProcessor` per thread chunk
- Window generation: `WindowFunction::generate(size) -> Vec<f32>` — called once in `SpectrumProcessor::new()`
