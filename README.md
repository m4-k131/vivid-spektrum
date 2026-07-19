# vividspektrum

A Rust **spectrogram visualizer** that generates high-resolution, GPU-accelerated spectrograms from audio. It can render offline PNG images, a live scrolling window on Linux/Wayland, or a desktop widget via wlr-layer-shell.

The pipeline is split into a DSP core (`spektrum-core`) and the application (`spektrum`). `spektrum-core` handles audio decoding, STFT/CQT, log-frequency mapping, colormaps and profiles; the application adds Iced/WGPU rendering, the settings overlay, and CLI front-ends.

## What it does

- **Offline**: decode an audio file (WAV/MP3/FLAC/AAC/Ogg Vorbis) and render a PNG spectrogram.
- **Live**: capture system audio through PipeWire (Linux) or CPAL (Windows), compute STFT/CQT columns in a background thread, and stream them to a WGPU fragment shader that performs colormap lookup, contrast and saturation in real time.
- **Widget**: run as a layer-shell overlay on wlroots/KDE Plasma, anchored to any screen edge.

Data flow (live):

```
PipeWire/CPAL → lock-free ring buffer → SpectrumProcessor (STFT/CQT)
   → Vec<Vec<f32>> columns → GPU R8Unorm texture → WGSL shader
   → colormap LUT + contrast/saturation → Wayland surface
```

## Running

### Offline PNG

```bash
cargo run --release -p spektrum --bin audio_to_png -- song.mp3 spectrogram.png
```

Optional: `--auto-width` makes the PNG width exactly one pixel per STFT hop. Use `--from mm:ss` and `--to mm:ss` to render a clip.

### Live window

```bash
cargo run --release --bin vividspektrum
cargo run --release --bin vividspektrum -- --profile high-resolution --colormap inferno
```

Right-click (or press `M`/`Esc`) to toggle the settings overlay. Double-click the spectrogram to toggle fullscreen.

### Layer-shell desktop widget

```bash
cargo run --release --bin kde_overlay -- --anchor bottom --layer background --height 120
```

`--width 0` stretches the widget to the screen edge in the direction of the anchor.

## Presets

`presets/` contains 10 ready-to-use TOML profiles. Built-ins plus `.toml` files in `presets/` are resolved by `--profile <name>`:

```bash
cargo run --bin vividspektrum -- --profile high-resolution
cargo run --bin vividspektrum -- --config presets/personal.toml
cargo run --bin vividspektrum -- --list-presets
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
| `personal` | 8192 | 4096 | Tuned all-rounder (custom `presets/personal.toml`) |

See `default_config.toml` for the full annotated config reference.

## Configuration and settings

Settings are loaded from a TOML profile, then any CLI flags override the profile. In the live window, the settings overlay lets you change most values on the fly. Some changes are applied in real time, others require re-initializing the DSP (a short audio dropout while the FFT/planner is rebuilt).

### TOML profile sections

`[spectrum]` — core DSP parameters:

| Field | Default | Notes |
|-------|---------|-------|
| `window_size` | 8192 | FFT length in samples; sets frequency resolution. Must be ≥ 8, non-power-of-two is supported. |
| `hop_size` | 1024 | Samples between STFT columns; smaller = smoother scroll, higher CPU. |
| `sample_rate` | 48000 | Should match the capture device. |
| `log_bins` | 1024 | Vertical log-spaced frequency bins in the spectrogram texture. |
| `f_min_hz` | 20.0 | Lowest displayed frequency. |
| `f_max_hz` | 20000.0 | Highest displayed frequency (clamped to Nyquist). |
| `freq_scale_exp` | 0.5 | Vertical axis warping: <1 compresses lows, 1 = pure log. |
| `window_fn` | blackman-harris | hann, hamming, blackman, blackman-harris. |
| `band_aggregation` | triangular | nearest or triangular FFT-bin weighting. |
| `freq_smoothing_sigma` | 1.0 | Gaussian smoothing across frequency bins (0 = off). |
| `amplitude_gamma` | 0.5 | Power curve on magnitude (<1 brightens quiet parts). |
| `temporal_alpha` | 0.3 | EMA between consecutive columns (0 = off). |
| `peak_hold_decay` | 0.92 | Peak-hold decay per column (0 = off). |
| `db_floor` | -90.0 | Magnitudes below are black. |
| `db_ceil` | 0.0 | Magnitudes above are full color. |
| `weighting` | none | IEC 61672 frequency weighting: none, a, c. |
| `transform` | stft | stft or cqt. |
| `cqt_bins_per_octave` | 12 | Only used when `transform = "cqt"`. |
| `centered` | false | Center the analysis window: better accuracy, adds ~window/2 samples latency. |

`[image]` — display/window defaults used by the live binaries:

| Field | Default | Notes |
|-------|---------|-------|
| `width` | 800 | Initial window width in pixels (PNG width for `audio_to_png`). |
| `height` | 200 | Initial window height in pixels (PNG height for `audio_to_png`). |
| `scroll_right_to_left` | true | `true` = horizontal scroll, `false` = vertical scroll. |
| `colormap` | viridis | Builtin colormap name or `.toml` path. |
| `contrast` | 1.0 | GPU contrast (>1 = more punch). |
| `saturation` | 1.0 | GPU saturation (0 = grayscale). |

### Live settings overlay

Open the overlay with a right-click or `M`. Every label has an `(i)` icon explaining the setting.

**GPU / display (runtime)**:

| Control | Effect |
|---------|--------|
| Colormap | Selects the GPU LUT used to map magnitude to color. |
| Contrast | Pivot around mid-magnitude; >1 stretches, <1 compresses. |
| Saturation | Luminance-preserving color saturation; 0 = grayscale. |
| Overlay | Frequency reference lines (treble-bass, guitar-standard, a440). |

**Advanced DSP (restart required unless marked runtime)**:

| Control | Restart? | Effect |
|---------|----------|--------|
| Profile preset | yes | Loads a profile and applies its values to the sliders. |
| Window function | yes | Time-domain window applied before FFT. |
| Transform | yes | `stft` or `cqt`. |
| Band aggregation | yes | `nearest` or `triangular` FFT-to-log-bin weighting. |
| Weighting | yes | A/C frequency weighting curves. |
| Centered window | yes | Centered analysis window; adds latency. |
| FFT window size | yes | FFT length in samples. |
| Time step / scroll speed | no | STFT hop; controls scroll rate. |
| Log frequency bins | yes | Vertical texture resolution. |
| Freq min / max | yes | Displayed frequency range. |
| dB floor / ceil | no | Magnitude range mapped to black/bright. |
| Freq smoothing | no | Gaussian smoothing width. |
| Amplitude gamma | no | Power curve on magnitude. |
| Temporal alpha | no | EMA between columns. |
| Peak hold decay | no | Peak-hold decay per column. |
| CQT bins / octave | yes | Only used in CQT mode. |
| Freq scale exp | yes | Vertical axis warping. |
| History / buffer | no | Number of time columns in the live texture. |

## Colormaps

Colormaps are GPU LUTs; switching has zero CPU cost at runtime.

```bash
cargo run --bin vividspektrum -- --colormap inferno
cargo run --bin vividspektrum -- --colormap colormaps/nebula.toml
cargo run --bin vividspektrum -- --list-colormaps
```

**Builtin**: viridis, inferno, magma, plasma, turbo, grayscale, heat, fire, ember, gold, cyanfire, rose, aurora, nebula, spectrum, ocean, sunset, gruvbox-dark, gruvbox-dark-5, catppuccin-mocha, catppuccin-mocha-5, nord, nord-5, tokyo-night, tokyo-night-5

Custom colormaps follow `example_colormap.toml`.

## Frequency overlays

Overlays are procedural lines drawn in the fragment shader; they are not baked into the spectrogram texture.

```bash
cargo run --bin vividspektrum -- --overlay treble-bass
cargo run --bin vividspektrum -- --overlay guitar-standard
cargo run --bin vividspektrum -- --overlay a440
cargo run --bin vividspektrum -- --list-overlays
```

**Builtin**: `treble`, `treble-bass`, `guitar-standard`, `a440`

Custom overlay (`overlays/my-scale.toml`):

```toml
name = "my-scale"
color = [255, 200, 100]
opacity = 0.7
thickness = 0.004

[[lines]]
freq = 440.0
label = "A4"
```

## Resolution, performance, and how the GPU render works

The live spectrogram is a single WGPU texture with dimensions:

- `width = log_bins` (vertical frequency resolution)
- `height = history` (horizontal time resolution, independent of window width)

The fragment shader (`spektrum/src/spectrogram.rs`) draws a full-screen quad and samples that texture with bilinear filtering. The window size does **not** change the texture size — resizing the window only stretches or shrinks the same texture.

Practical limits:

- `log_bins` should be ≤ `window_size / 2` (the number of positive FFT bins). Above that, the log-frequency mapping is interpolating FFT data, not resolving new frequencies.
- `log_bins` larger than the window height still improves log-frequency mapping accuracy and gives smoother GPU interpolation, but it does not add display pixels.
- `history` should be roughly the window width if you want one STFT column per screen pixel. Larger values keep more past columns but increase GPU memory.
- Texture dimensions are capped by the GPU's `max_texture_dimension_2d` (commonly 8192 or 16384). `prepare()` skips uploading if the requested size is too large.

## CLI flags

### `vividspektrum` (live window)

| Flag | Default | Description |
|------|---------|-------------|
| `--profile` | `default` | Preset name (also resolves `presets/<name>.toml`) |
| `--config` | — | Path to a TOML profile file |
| `--colormap` | viridis | Colormap name or `.toml` path |
| `--fft` / `--window` | 8192 | FFT window size (samples) |
| `--hop` | 1024 | STFT hop (samples) |
| `--log-bins` | 1024 | Number of log-frequency bins |
| `--window-fn` | blackman-harris | hann, hamming, blackman, blackman-harris |
| `--band-agg` | triangular | nearest, triangular |
| `--f-min` | 20.0 | Lowest displayed frequency |
| `--f-max` | 20000.0 | Highest displayed frequency |
| `--db-floor` | -90.0 | dB mapped to black |
| `--db-ceil` | 0.0 | dB mapped to full color |
| `--smoothing` | 1.0 | Frequency Gaussian smoothing sigma |
| `--gamma` | 0.5 | Amplitude gamma |
| `--temporal-alpha` | 0.3 | EMA between columns |
| `--peak-decay` | 0.92 | Peak-hold decay |
| `--transform` | stft | stft or cqt |
| `--cqt-bpo` | 12 | CQT bins per octave |
| `--weighting` | none | none, a, c |
| `--freq-scale-exp` | 0.5 | Vertical axis warping |
| `--centered` | off | Centered analysis window |
| `--width` | 800 | Initial window width |
| `--height` | 800 | Initial window height |
| `--history` | 512 | Time columns in the live texture |
| `--sample-rate` | 48000 | Audio sample rate |
| `--legacy-vertical-scroll` | off | Scroll top-to-bottom |
| `--no-decorations` | off | Borderless window |
| `--always-on-top` | off | Keep above other windows |
| `--always-on-bottom` | off | Keep below (desktop widget) |
| `--transparent` | off | Transparent background |
| `--position` | — | Window position as `X,Y` |
| `--overlay` | — | Frequency overlay name or `.toml` path |
| `--contrast` | 1.0 | GPU contrast |
| `--saturation` | 1.0 | GPU saturation |
| `--debug-profile` | off | Print DSP/GPU timing every second |
| `--list-colormaps` | — | Print available colormaps and exit |
| `--list-presets` | — | Print available profiles and exit |
| `--list-overlays` | — | Print available overlays and exit |

### `audio_to_png`

```bash
cargo run --release -p spektrum --bin audio_to_png -- input.mp3 out.png \
  --profile high-resolution --from 00:30 --to 01:30 --auto-width
```

| Flag | Default | Description |
|------|---------|-------------|
| `input` | — | Audio file |
| `output` | — | PNG file |
| `--profile` / `--config` | default | Profile to use |
| `--fft`, `--hop`, `--log-bins`, `--window-fn`, `--band-agg`, `--smoothing`, `--gamma`, `--temporal-alpha`, `--peak-decay`, `--transform`, `--cqt-bpo`, `--weighting`, `--freq-scale-exp` | | Spectrum overrides (same semantics as live) |
| `--width` | profile image | Output PNG width |
| `--height` | profile image | Output PNG height |
| `--auto-width` | off | Width = one pixel per STFT hop |
| `--from` | 0:00 | Start timestamp `mm:ss` |
| `--to` | end | End timestamp `mm:ss` |
| `--contrast`, `--saturation` | 1.0 | GPU post-processing in PNG output |
| `--scale` | — | JSON scale config for overlay grid |
| `--legacy-vertical-scroll` | off | Render vertical instead of horizontal |

### `kde_overlay`

| Flag | Default | Description |
|------|---------|-------------|
| `--profile` / `--config` | default | Profile |
| `--fft`, `--hop`, `--log-bins` | | Spectrum overrides |
| `--width` | profile image | `0` = stretch to edge |
| `--height` | 200 | Widget height |
| `--history` | 512 | Time columns |
| `--anchor` | bottom | `top,bottom,left,right` or combinations |
| `--layer` | bottom | `background`, `bottom`, `top`, `overlay` |
| `--exclusive-zone` | 0 | Layer-shell exclusive zone |
| `--output` | — | Output/screen name to bind to |
| `--legacy-vertical-scroll` | off | Vertical scroll |

## How it works

1. **Audio capture / decode**
   - Live: PipeWire (Linux) or CPAL (Windows) fills a lock-free `SampleRing`.
   - Offline: `symphonia` decodes the audio file to `f32` mono samples.

2. **DSP (`spektrum-core/dsp.rs`)**
   - `SpectrumProcessor` windows the incoming samples (`hann`/`hamming`/`blackman`/`blackman-harris`), runs a real FFT (`rustfft`), and maps the FFT magnitude to `log_bins` log-spaced frequency bins using `nearest` or `triangular` aggregation.
   - Optional: IEC A/C weighting, frequency-domain Gaussian smoothing, CQT, amplitude gamma, EMA temporal smoothing, peak-hold decay.
   - Each output column is a `Vec<f32>` of length `log_bins` with magnitudes in `[0, 1]`.

3. **GPU rendering (`spektrum/src/spectrogram.rs`)**
   - Columns are pushed into an `R8Unorm` texture (`bins × history`) on the WGPU queue.
   - The WGSL shader samples the spectrogram texture, applies contrast (`(mag - 0.5) * contrast + 0.5`), looks up the colormap from a 256×1 RGBA LUT, applies luminance-preserving saturation, and blends overlay lines by distance.

4. **Live loop**
   - DSP thread pushes columns into `pending_spectra`.
   - Iced's 60fps tick triggers `SpectrogramPrimitive::prepare`, which drains pending columns and updates the texture.
   - Settings overlay messages are handled in the main thread; structural changes restart `SpectrumProcessor` and preserve the existing history buffer.

## Building

Requires Rust (stable).

```bash
cargo check --workspace
cargo build --release
cargo test --workspace   # 74 tests
```

See `BUILD.md` for Linux system dependencies (PipeWire, Wayland dev libs) and `WINDOWS.md` for Windows notes.

## Project structure

| Crate | Purpose |
|-------|---------|
| `spektrum-core` | DSP: STFT/CQT, colormaps, profiles, ring buffer, PipeWire capture. |
| `spektrum` | Application binaries: `vividspektrum`, `audio_to_png`, `kde_overlay`, plus the Iced/WGPU shader and settings overlay. |

