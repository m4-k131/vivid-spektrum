# vividspektrum

A **spectrogram visualizer** in Rust. Generate high-resolution spectrograms from audio — offline as PNG images, or live via PipeWire capture with GPU rendering on Linux/Wayland.

## Quick start

### Offline PNG (Windows, macOS, Linux)

```bash
cargo run --release -p vividspektrum --bin audio_to_png -- song.mp3 spectrogram.png
```

Supports WAV, MP3, FLAC, AAC, Ogg Vorbis.

### Live window (Linux/Wayland only)

```bash
cargo run --release --bin vividspektrum
```

Captures system audio via PipeWire and renders a scrolling spectrogram in a GPU-accelerated Wayland window.

### Desktop widget (layer-shell)

```bash
cargo run --release --bin kde_overlay -- --anchor bottom --layer background --height 120
```

## Presets

9 ready-to-use TOML configs in `presets/`:

```bash
cargo run --bin vividspektrum -- --config presets/high-resolution.toml
cargo run --bin vividspektrum -- --config presets/music-production.toml
cargo run --bin vividspektrum -- --config presets/bass-heavy.toml
cargo run --bin vividspektrum -- --list-presets   # see all available
```

| Preset | FFT | Bins | Use case |
|--------|-----|------|----------|
| `default` | 8192 | 1024 | Balanced quality/performance |
| `laptop` | 4096 | 256 | Low CPU, battery-friendly |
| `high-resolution` | 32768 | 2048 | Maximum resolution |
| `music-production` | 16384 | 1536 | Mixing/mastering (centered, A-weighted) |
| `voice` | 4096 | 512 | Speech/vocal range (80 Hz–8 kHz) |
| `bass-heavy` | 16384 | 1024 | Sub-bass emphasis |
| `cqt-musical` | 8192 | 512 | Constant-Q, 24 bins/octave |
| `minimal-dark` | 8192 | 512 | Desktop widget aesthetic |
| `ultra-responsive` | 2048 | 512 | Minimum latency |

Or write your own — see `default_config.toml` for a fully documented reference.

## Colormaps

15 builtin colormaps, or load a custom `.toml` file:

```bash
cargo run --bin vividspektrum -- --colormap inferno
cargo run --bin vividspektrum -- --colormap example_colormap.toml
cargo run --bin vividspektrum -- --list-colormaps  # see all available
```

**Builtin**: viridis, inferno, magma, plasma, turbo, grayscale, heat, fire, ember, gold, cyanfire, rose, aurora, nebula, spectrum, ocean, sunset, gruvbox-dark, catppuccin-mocha, nord, tokyo-night (+ 5-stop variants)

Colormaps are uploaded as GPU LUT textures — zero CPU cost at runtime.

## Frequency overlays

Draw horizontal reference lines (staff lines, tuning notes, etc.) from TOML files:

```bash
cargo run --bin vividspektrum -- --overlay treble-bass
cargo run --bin vividspektrum -- --overlay guitar-standard
cargo run --bin vividspektrum -- --overlay a440
cargo run --bin vividspektrum -- --list-overlays  # see all available
```

**Builtin**: treble, treble-bass, guitar-standard, a440

Custom overlay format (`overlays/my-scale.toml`):

```toml
name = "my-scale"
color = [255, 200, 100]   # RGB line color
opacity = 0.7             # blend strength (0.0–1.0)
thickness = 0.004         # fraction of screen height

[[lines]]
freq = 440.0
label = "A4"
```

Drop any `.toml` in `overlays/` or pass a full path with `--overlay path/to/file.toml`.

## GPU post-processing

Contrast and saturation are applied in the fragment shader:

```bash
cargo run --bin vividspektrum -- --contrast 1.3 --saturation 1.2
```

| Flag | Default | Description |
|------|---------|-------------|
| `--contrast` | 1.0 | Pivot around 0.5 magnitude (>1 = more punch) |
| `--saturation` | 1.0 | Luminance-preserving (0 = grayscale, >1 = vivid) |

## Key CLI flags

| Flag | Default | Description |
|------|---------|-------------|
| `--profile` | `default` | Built-in profile name |
| `--config` | — | Path to a TOML config file |
| `--colormap` | viridis | Colormap name or .toml path |
| `--fft` | 8192 | FFT window size (samples) |
| `--hop` | 1024 | Hop size (samples) |
| `--log-bins` | 1024 | Log-spaced frequency bins |
| `--window-fn` | blackman-harris | Window: hann, hamming, blackman, blackman-harris |
| `--band-agg` | triangular | Band aggregation: nearest, triangular |
| `--smoothing` | 1.0 | Frequency smoothing sigma (0=off) |
| `--gamma` | 0.5 | Amplitude gamma (<1 brightens) |
| `--temporal-alpha` | 0.3 | EMA temporal smoothing |
| `--peak-decay` | 0.92 | Peak hold decay |
| `--transform` | stft | Transform: stft, cqt |
| `--weighting` | none | Frequency weighting: none, a, c |
| `--freq-scale-exp` | 0.5 | Frequency axis warping (<1 compresses lows) |
| `--centered` | off | Centered analysis (better accuracy, +latency) |
| `--width` | 800 | Window width (px) |
| `--height` | 200 | Window height (px) |
| `--no-decorations` | off | Remove window title bar |
| `--always-on-top` | off | Keep above other windows |
| `--always-on-bottom` | off | Keep below (desktop widget) |
| `--transparent` | off | Transparent background |
| `--position` | — | Window position as X,Y |
| `--overlay` | — | Frequency overlay name or .toml path |
| `--debug-profile` | off | Print DSP/GPU performance stats every second |

## How it works

1. **Capture** — PipeWire audio → lock-free ring buffer (Linux) or CPAL (Windows)
2. **DSP** — STFT or CQT → log-frequency bins → temporal smoothing → gamma → 0..1 magnitudes
3. **GPU** — R8Unorm texture (bilinear interpolation) → colormap LUT sampling → contrast/saturation → Wayland surface

All DSP runs on a dedicated thread; the GPU shader does colormap lookup and post-processing at 60fps.

## Building

Requires Rust (stable). See `BUILD.md` for Linux system dependencies (PipeWire), or `WINDOWS.md` for Windows notes.

```bash
cargo check --workspace
cargo build --release
cargo test --workspace   # 74 tests
```

## Project structure

| Crate | Purpose |
|-------|---------|
| `spektrum-core` | DSP library: STFT/CQT, colormaps, profiles, ring buffer, PipeWire |
| `vividspektrum` | Application: CLI, GPU shaders, live window, offline PNG, layer-shell overlay |

## Roadmap

See `ROADMAP.md`. Phases 1–5 complete. Phase 6 (overlay/annotation) and Phase 7 (research/extras) remain for future exploration.
