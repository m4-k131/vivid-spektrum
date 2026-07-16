use anyhow::Result;
use clap::Parser;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Parser, Debug, Clone)]
#[command(name = "hyprgram", about = "PipeWire live spectrogram (Wayland window)")]
pub struct Args {
    #[arg(long, help = "PipeWire target object id or name for capture stream")]
    pub target_object: Option<String>,
    #[arg(long, help = "Built-in profile: laptop, default, foobar-like")]
    pub profile: Option<String>,
    #[arg(long, help = "Path to a TOML profile file")]
    pub config: Option<std::path::PathBuf>,
    #[arg(long, help = "Override: number of log-spaced frequency bins")]
    pub log_bins: Option<usize>,
    #[arg(
        long = "fft",
        alias = "window",
        help = "Override: FFT window length (samples)"
    )]
    pub window: Option<usize>,
    #[arg(
        long = "hop",
        help = "Override: STFT hop (samples)"
    )]
    pub hop: Option<usize>,
    #[arg(long = "window-fn", help = "Override: window function (hann, hamming, blackman, blackman-harris)")]
    pub window_fn: Option<String>,
    #[arg(long = "band-agg", help = "Override: band aggregation (nearest, triangular)")]
    pub band_agg: Option<String>,
    #[arg(long = "f-min", help = "Override: minimum frequency in Hz (default 20)")]
    pub f_min: Option<f32>,
    #[arg(long = "f-max", help = "Override: maximum frequency in Hz (default 20000)")]
    pub f_max: Option<f32>,
    #[arg(long = "db-floor", help = "Override: dB floor — magnitudes below mapped to 0 (default -90)")]
    pub db_floor: Option<f32>,
    #[arg(long = "db-ceil", help = "Override: dB ceiling — magnitudes above mapped to 1 (default 0)")]
    pub db_ceil: Option<f32>,
    #[arg(long = "smoothing", help = "Override: Gaussian frequency smoothing sigma (0=off, try 0.5-2.0)")]
    pub smoothing: Option<f32>,
    #[arg(long = "gamma", help = "Override: amplitude gamma (<1 brightens, >1 darkens)")]
    pub gamma: Option<f32>,
    #[arg(long = "temporal-alpha", help = "Override: EMA temporal smoothing (0=off, 0.3-0.7 typical)")]
    pub temporal_alpha: Option<f32>,
    #[arg(long = "peak-decay", help = "Override: peak hold decay per frame (0=off, 0.5-0.9 typical)")]
    pub peak_decay: Option<f32>,
    #[arg(long = "colormap", help = "Override: colormap (viridis, inferno, magma, plasma, turbo, grayscale, heat, gruvbox-dark, gruvbox-dark-5, catppuccin-mocha, catppuccin-mocha-5, nord, nord-5, tokyo-night, tokyo-night-5)")]
    pub colormap: Option<String>,
    #[arg(long = "weighting", help = "Override: frequency weighting (none, a, c)")]
    pub weighting: Option<String>,
    #[arg(long = "transform", help = "Override: transform (stft, cqt)")]
    pub transform: Option<String>,
    #[arg(long = "cqt-bpo", help = "Override: CQT bins per octave (default 12)")]
    pub cqt_bpo: Option<u32>,
    #[arg(long, help = "Override: window width (px)")]
    pub width: Option<u32>,
    #[arg(long, help = "Override: window height (px)")]
    pub height: Option<u32>,
    #[arg(long, default_value_t = 512, help = "Time rows in waterfall")]
    pub history: u32,
    #[arg(long, help = "Override: sample rate (Hz)")]
    pub sample_rate: Option<u32>,
    #[arg(long, help = "Scroll time top-to-bottom instead of right-to-left")]
    pub legacy_vertical_scroll: bool,
    #[arg(long, help = "Remove window title bar and decorations")]
    pub no_decorations: bool,
    #[arg(long, help = "Keep window always on top of other windows")]
    pub always_on_top: bool,
    #[arg(long, help = "Keep window always below other windows (desktop widget)")]
    pub always_on_bottom: bool,
    #[arg(long, help = "Enable transparent window background")]
    pub transparent: bool,
    #[arg(long, help = "Window position as X,Y (e.g. 100,50)")]
    pub position: Option<String>,
    #[arg(long, help = "Override: frequency scale exponent (<1 compresses lows, >1 stretches lows)")]
    pub freq_scale_exp: Option<f32>,
    #[arg(long, help = "Centered analysis window (adds half-window latency for better frequency accuracy)")]
    pub centered: bool,
    #[arg(long, default_value_t = 1.0, help = "GPU contrast (1.0=neutral, >1 increases, <1 decreases)")]
    pub contrast: f32,
    #[arg(long, default_value_t = 1.0, help = "GPU saturation (1.0=neutral, 0=grayscale, >1 oversaturated)")]
    pub saturation: f32,
    #[arg(long, help = "List available builtin colormaps and exit")]
    pub list_colormaps: bool,
    #[arg(long, help = "List available preset configs and exit")]
    pub list_presets: bool,
}

// Phase 4 manual verification (Linux/Wayland):
// - Latency vs resolution: window/hop/sample-rate tradeoff (see hyprgram-core dsp defaults).
// - CPU: profile with perf; watch extra copies between PipeWire ring, DSP, and GPU upload.

fn main() -> Result<()> {
    let args = Args::parse();
    if args.list_colormaps {
        println!("Available colormaps:");
        for name in hyprgram_core::builtin_colormap_names() {
            println!("  {}", name);
        }
        println!("\nOr pass a path to a custom .toml colormap file.");
        return Ok(());
    }
    if args.list_presets {
        println!("Available preset configs (use with --config):");
        for name in hyprgram_core::profiles::builtin_profile_names() {
            println!("  --profile {}", name);
        }
        let presets_dir = std::path::Path::new("presets");
        if presets_dir.is_dir() {
            println!("\nPreset files in presets/:");
            if let Ok(entries) = std::fs::read_dir(presets_dir) {
                let mut names: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
                    .map(|e| e.path().display().to_string())
                    .collect();
                names.sort();
                for name in names {
                    println!("  --config {}", name);
                }
            }
        }
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        linux::run(args)
    }
    #[cfg(target_os = "windows")]
    {
        windows::run(args)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = args;
        anyhow::bail!("hyprgram requires Linux or Windows")
    }
}
