# Windows Development Guide

## Prerequisites

Rust is installed via `rustup` at `%USERPROFILE%\.cargo\bin\`. If `cargo` is not found in your terminal, use the full path:

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" <command>
```

> **conda users:** conda environments can shadow the system `PATH`. Either use the full path above, or open a fresh PowerShell window outside conda.

---

## a) Compile

From the workspace root (`vividspektrum/`):

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" check --workspace
```

To build the release binary:

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release
```

---

## b) Generate a spectrogram PNG from an audio file

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run -p spektrum --bin audio_to_png -- input.mp3 output.png
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--profile` | `default` | Built-in profile: `laptop`, `default`, `high-resolution` |
| `--config` | — | Path to a TOML profile file (see `example_profile.toml`) |
| `--width` | 800 | Output image width (px) |
| `--height` | 200 | Output image height (px) |
| `--log-bins` | 256 | Frequency bins |
| `--fft` / `--window` | 20480 | FFT window size (samples) |
| `--hop` | 256 | STFT hop (samples) |
| `--legacy-vertical-scroll` | off | Render time top-to-bottom instead of left-to-right |

CLI flags override profile/config values.

### Examples

```powershell
# Use a built-in profile
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run -p spektrum --bin audio_to_png -- music.wav out.png --profile laptop

# Use a TOML config file
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run -p spektrum --bin audio_to_png -- music.wav out.png --config my_profile.toml

# Profile + overrides
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run -p spektrum --bin audio_to_png -- music.wav out.png --profile high-resolution --width 1200 --height 300
```

Supported input formats: WAV, MP3, FLAC, AAC, Ogg Vorbis.
