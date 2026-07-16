# Hyprgram: foobar-like spectrogram roadmap

This document lists **features and techniques** (not magic numbers) toward a **foobar2000-class** spectrogram: high resolution, CPU-heavy but acceptable. It is meant for **planning and handoff**; implement in order of phases unless a later item is explicitly scoped.

**Reference:** third-party component `foo_vis_spectrum_analyzer` (and wiki), not a single built-in default. See `.cursorrules` (Spectrogram quality target) for links.

---

## A. Acquisition and timing

| Feature / technique | What it buys you | Status |
|---------------------|------------------|--------|
| Clock-locked analysis | Analysis aligned to real playback time, less drift vs what you hear. | ✅ Sample counter (`total_samples_pushed`) tracks position; pipeline latency logged at startup. |
| Reaction / lookahead alignment | Foobar exposes “reaction alignment” (centered vs causal window). | Add an explicit **analysis delay policy** (centered Hann vs causal) and document perceptual tradeoff. |
| Refresh decoupled from FFT rate | UI can draw at display Hz while STFT runs faster (foobar: >60 Hz refresh). | Separate **analysis cadence** from **present cadence**; queue already helps—formalize max backlog and “catch-up” policy. |

---

## B. Time–frequency transforms (beyond plain STFT)

| Feature / technique | What it buys you | Plan |
|-----------------------|------------------|------|
| Constant-Q (CQT) or filter-bank STFT | Log-frequency resolution that matches musical pitch; fewer misleading bins at low end. | Spike: small CQT path or use a **library**; compare CPU vs FFT+log mapping; mono channel first. |
| SWIFT / IIR-style bands (foobar option) | Alternative time–frequency tiling; different CPU profile. | Research milestone: when CQT/STFT is not enough; low priority unless foobar parity by mode is required. |
| Configurable window family | Hann vs Hamming vs Gaussian/Kaiser (foobar has window + skew). | Pluggable **window function** + optional **skew** parameter; keep Hann as default. |

---

## C. FFT pipeline polish

| Feature / technique | What it buys you | Plan |
|-----------------------|------------------|------|
| Non-power-of-two FFT (optional) | Foobar allows custom sizes at CPU cost; matches arbitrary ms windows. | Ensure planner path supports arbitrary **N**; benchmark; guard with CLI or “expert” flag. |
| Per-bin aggregation | min/max/mean/RMS across FFT bins mapped to one display band (foobar has many modes). | After FFT, define **band mapping** layer: triangular or nearest-bin → **one value per display column** with selectable norm. |
| Lanczos (or similar) smoothing across frequency | Softer, less “sparkly” spectrum; foobar documents Lanczos kernel size. | Post-FFT **1D smoothing along frequency** (or along log index) with width as a parameter. |

---

## D. Frequency axis and display mapping

| Feature / technique | What it buys you | Plan |
|-----------------------|------------------|------|
| Triangular / mel / bark filter banks | Perceptual weighting; closer to “analyzer” sound than raw FFT magnitude. | Roadmap: **mel** or **triangular** as second mapping mode beside current log-sparse columns. |
| Brown–Puckette-style CQT mapping | Foobar-specific option for CQT path. | Only after a real CQT or hybrid exists. |
| Suppress mirror / Nyquist guard | Cleaner high-frequency end. | Cheap: zero or fade bins near Nyquist in display mapping. |

---

## E. Amplitude and dynamics

| Feature / technique | What it buys you | Plan |
|-----------------------|------------------|------|
| dB scale + stable floor/ceiling | Foobar uses dB ranges on axes; avoids “everything is neon”. | Unify **normalization**: reference level, **noise floor** in dB, optional **gamma** on magnitude (foobar mentions gamma on some scales). |
| Temporal smoothing (per bin or per frame) | Less flicker; foobar has smoothing factor + peak hold modes. | Optional **EMA** or **peak decay** on scalars before GPU upload. |
| A/C-weighting (optional) | Loudness-relevant spectrum. | Optional **IIR weighting** stage before FFT or on magnitude (document phase implications). |

---

## F. GPU / visualization

| Feature / technique | What it buys you | Plan |
|-----------------------|------------------|------|
| Interpolation in shader | Sub-texel scrolling; less blocky than nearest-neighbor history. | Bilinear where format allows, or **separate** RGBA8 filterable path for display only. |
| Colormap control | Foobar-grade presets (gradient stops, SoX-style, etc.). | Uniform or 1D LUT texture; **preset** file format later. |
| Multi-pass or mip / blur | Cheap glow / temporal smear without more FFTs. | Optional post-process pass on spectrogram texture. |

---

## G. Product / engineering

| Feature / technique | What it buys you | Plan |
|-----------------------|------------------|------|
| Preset export/import | Match foobar workflow (named tunings). | After core parameters stabilize: JSON/TOML **preset** for fft/hop/window/mapping/smoothing. |
| CPU profiles | “Laptop” vs “foobar-like”. | Named **profiles** that set hop, FFT size, smoothing, refresh—not single defaults. |
| Regression captures | Know when a change breaks “look”. | Golden **PNG** or vector dumps from sine/chirp—optional CI later. |

---

## Phased implementation order

*Algorithm-first: every phase is testable via `audio_to_png` before touching realtime code.*

1. **Phase 1 — Core spectrogram quality** ✅ *complete*
   Pluggable **window functions** (Hann, Hamming, Blackman, Blackman-Harris); **band aggregation** (nearest / triangular); **frequency-domain smoothing** (Gaussian kernel); **temporal smoothing** (EMA, peak hold); **amplitude pipeline** (dB floor/ceil, gamma); **colormap presets** (viridis, inferno, magma, plasma, turbo, grayscale, heat).

2. **Phase 2 — Transform upgrades** ✅ *complete*
   **CQT** or **constant-Q filter bank** path; compare to STFT+log; optional **non-power-of-two** FFT for ms-based windows; **A/C-weighting** filters.

3. **Phase 3 — Realtime integration** ✅ *complete*
   Lock-free **ring buffer** (PipeWire → DSP decoupled); **sample counter** + pipeline **latency logging**; **centered vs causal** reaction alignment (`--centered`); formalized **analysis vs render** rates (hop=1024 → ~47 cols/sec, render at 60fps); **dynamic window resize** (texture tracks widget bounds); **frequency axis flip** (low freq at bottom); **freq_scale_exp** for log-scale warping; KDE/Wayland **window flags** and **layer-shell overlay**.

4. **Phase 4 — GPU / visualization polish**
   Shader **interpolation** (bilinear); GPU **colormap** as 1D LUT texture (currently viridis polynomial in WGSL — 7 CPU colormaps not available in live shader); optional **post-process** pass (blur, glow).

5. **Phase 5 — Product / engineering** ✅ *complete*
   **Profiles** and **preset files** (TOML); builtin presets (laptop, default, foobar-like); CLI overrides for all DSP parameters; `kde_overlay` binary; window management flags.

6. **Phase 6 — Research / extras**
   SWIFT/IIR analog modes; Brown–Puckette CQT mapping; mel/bark filter banks; heavy visual post-processing; golden PNG regression captures + CI.

---

## Notes for implementers

- Prefer **correctness and resolution** over shaving CPU until a knob or profile says otherwise (see `.cursorrules`).
- Exact foobar **defaults** are preset-dependent; bit-identical parity requires capturing **preset files** or metrics from a reference install—do not assume one global numeric default.
