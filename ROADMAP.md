# Hyprgram: foobar-like spectrogram roadmap

This document lists **features and techniques** (not magic numbers) toward a **foobar2000-class** spectrogram: high resolution, CPU-heavy but acceptable. It is meant for **planning and handoff**; implement in order of phases unless a later item is explicitly scoped.

**Reference:** third-party component `foo_vis_spectrum_analyzer` (and wiki), not a single built-in default. See `.cursorrules` (Spectrogram quality target) for links.

---

## A. Acquisition and timing

| Feature / technique | What it buys you | Status |
|---------------------|------------------|--------|
| Clock-locked analysis | Analysis aligned to real playback time, less drift vs what you hear. | Done: sample counter (`total_samples_pushed`) tracks position; pipeline latency logged at startup. |
| Reaction / lookahead alignment | Foobar exposes "reaction alignment" (centered vs causal window). | Done: `--centered` flag adds half-window prefill delay for centered (non-causal) analysis. Default: causal. |
| Refresh decoupled from FFT rate | UI can draw at display Hz while STFT runs faster (foobar: >60 Hz refresh). | Done: render at 60fps (iced tick), DSP at ~47 cols/sec (hop=1024); pending_spectra queue with backlog cap. |
| Lock-free audio pipeline | PipeWire RT thread never blocks DSP thread. | Done: `SampleRingProducer`/`SampleRingConsumer` split; `spawn_capture_lockfree()`. |

---

## B. Time-frequency transforms (beyond plain STFT)

| Feature / technique | What it buys you | Status |
|-----------------------|------------------|--------|
| Constant-Q (CQT) or filter-bank STFT | Log-frequency resolution that matches musical pitch; fewer misleading bins at low end. | Done: CQT path with configurable bins/octave (`--transform cqt`, `--cqt-bpo`). |
| SWIFT / IIR-style bands (foobar option) | Alternative time-frequency tiling; different CPU profile. | Pending (Phase 6). |
| Configurable window family | Hann vs Hamming vs Gaussian/Kaiser (foobar has window + skew). | Done: 4 window functions (`--window-fn hann|hamming|blackman|blackman-harris`). |

---

## C. FFT pipeline polish

| Feature / technique | What it buys you | Status |
|-----------------------|------------------|--------|
| Non-power-of-two FFT (optional) | Foobar allows custom sizes at CPU cost; matches arbitrary ms windows. | Done: rustfft mixed-radix planner supports arbitrary N. |
| Per-bin aggregation | min/max/mean/RMS across FFT bins mapped to one display band (foobar has many modes). | Done: Triangular filter bank (default) + Nearest-bin; selectable via `--band-agg`. |
| Lanczos (or similar) smoothing across frequency | Softer, less "sparkly" spectrum; foobar documents Lanczos kernel size. | Done: Gaussian frequency smoothing (`--smoothing`, default sigma=1.0). |

---

## D. Frequency axis and display mapping

| Feature / technique | What it buys you | Status |
|-----------------------|------------------|--------|
| Triangular / mel / bark filter banks | Perceptual weighting; closer to "analyzer" sound than raw FFT magnitude. | Partial: Triangular filter bank is default; mel/bark not yet implemented. |
| Frequency scale warping | Control how much screen space low vs high frequencies get. | Done: `freq_scale_exp` (`--freq-scale-exp`, default 0.5): <1 compresses lows, >1 stretches lows. |
| Brown-Puckette-style CQT mapping | Foobar-specific option for CQT path. | Partial: CQT transform exists (`--transform cqt`), but Brown-Puckette mapping not implemented. |
| Suppress mirror / Nyquist guard | Cleaner high-frequency end. | Pending. |

---

## E. Amplitude and dynamics

| Feature / technique | What it buys you | Status |
|-----------------------|------------------|--------|
| dB scale + stable floor/ceiling | Foobar uses dB ranges on axes; avoids "everything is neon". | Done: dB floor/ceil (default -90/0), amplitude gamma (default 0.5). |
| Temporal smoothing (per bin or per frame) | Less flicker; foobar has smoothing factor + peak hold modes. | Done: EMA temporal smoothing (`--temporal-alpha`, default 0.3) + peak hold decay (`--peak-decay`, default 0.92). |
| A/C-weighting (optional) | Loudness-relevant spectrum. | Done: IEC 61672 A and C weighting (`--weighting a|c`). |

---

## F. GPU / visualization

| Feature / technique | What it buys you | Status |
|-----------------------|------------------|--------|
| Interpolation in shader | Sub-texel scrolling; less blocky than nearest-neighbor history. | Pending (Phase 4). Bilinear where format allows. |
| Colormap control | Foobar-grade presets (gradient stops, SoX-style, etc.). | Partial: 7 CPU colormaps in `audio_to_png`; live shader only has viridis polynomial. GPU LUT pending (Phase 4). |
| Multi-pass or mip / blur | Cheap glow / temporal smear without more FFTs. | Pending (Phase 4). |

---

## G. Product / engineering

| Feature / technique | What it buys you | Status |
|-----------------------|------------------|--------|
| Preset export/import | Match foobar workflow (named tunings). | Done: TOML profiles (`--config file.toml`); builtin presets (`--profile laptop|default|foobar-like`). |
| CPU profiles | "Laptop" vs "foobar-like". | Done: Named profiles with distinct FFT/hop/smoothing/gamma settings. |
| Regression captures | Know when a change breaks "look". | Pending: golden PNG generation exists via `audio_to_png`, CI not yet set up. |
| Window management | KDE/Wayland window customization. | Done: `--no-decorations`, `--always-on-top`, `--always-on-bottom`, `--transparent`, `--position`. |
| Layer-shell overlay | Desktop widget via Wayland layer-shell. | Done: `kde_overlay` binary using `iced_layershell`. |
| Dynamic resize | Spectrogram texture tracks window size. | Done: `min_history` + dynamic `history` from widget bounds. |

---

## Phased implementation order

*Algorithm-first: every phase is testable via `audio_to_png` before touching realtime code.*

1. **Phase 1 -- Core spectrogram quality** (complete)
   Pluggable **window functions** (Hann, Hamming, Blackman, Blackman-Harris); **band aggregation** (nearest / triangular); **frequency-domain smoothing** (Gaussian kernel); **temporal smoothing** (EMA, peak hold); **amplitude pipeline** (dB floor/ceil, gamma); **colormap presets** (viridis, inferno, magma, plasma, turbo, grayscale, heat).

2. **Phase 2 -- Transform upgrades** (complete)
   **CQT** or **constant-Q filter bank** path; compare to STFT+log; optional **non-power-of-two** FFT for ms-based windows; **A/C-weighting** filters.

3. **Phase 3 -- Realtime integration** (complete)
   Lock-free **ring buffer** (PipeWire -> DSP decoupled); **sample counter** + pipeline **latency logging**; **centered vs causal** reaction alignment (`--centered`); formalized **analysis vs render** rates (hop=1024 -> ~47 cols/sec, render at 60fps); **dynamic window resize** (texture tracks widget bounds); **frequency axis flip** (low freq at bottom); **freq_scale_exp** for log-scale warping; KDE/Wayland **window flags** and **layer-shell overlay**.

4. **Phase 4 -- GPU / visualization polish** (complete)
   Shader **bilinear interpolation** (R8Unorm filterable texture); GPU **colormap** as 256x1 RGBA8 LUT texture (all 15 builtins + custom TOML colormaps); **contrast** and **saturation** post-processing in fragment shader; `--contrast` / `--saturation` CLI flags.

5. **Phase 5 -- Product / engineering** (complete)
   **Profiles** and **preset files** (TOML); builtin presets (laptop, default, foobar-like); CLI overrides for all DSP parameters; `kde_overlay` binary; window management flags; `default_config.toml` reference config with full documentation; `presets/` directory with 8 ready-to-use configs (default, laptop, foobar-like, music-production, voice, bass-heavy, cqt-musical, minimal-dark, ultra-responsive).

6. **Phase 6 -- Research / extras**
   **AGC** (automatic gain control — adaptive peak tracking + normalization); SWIFT/IIR analog modes; Brown-Puckette CQT mapping; mel/bark filter banks; heavy visual post-processing; golden PNG regression captures + CI.

---

## Notes for implementers

- Prefer **correctness and resolution** over shaving CPU until a knob or profile says otherwise (see `.cursorrules`).
- Exact foobar **defaults** are preset-dependent; bit-identical parity requires capturing **preset files** or metrics from a reference install -- do not assume one global numeric default.
