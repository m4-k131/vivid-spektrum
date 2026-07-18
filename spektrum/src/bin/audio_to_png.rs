use anyhow::{Context, Result};
use clap::Parser;
use spektrum_core::{
    freq_grid::ScaleConfig, profiles, render_spectrogram_png_with_grid, samples_to_spectrogram,
};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;
use symphonia::core::audio::{AudioBufferRef, SampleBuffer};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};

#[derive(Parser, Debug)]
#[command(about = "Render a WAV/MP3 audio file to a vividspektrum spectrogram PNG")]
struct Args {
    input: PathBuf,
    output: PathBuf,
    #[arg(long, help = "Built-in profile: laptop, default, high-resolution")]
    profile: Option<String>,
    #[arg(long, help = "Path to a TOML profile file")]
    config: Option<PathBuf>,
    #[arg(long, help = "Override: number of log-spaced frequency bins")]
    log_bins: Option<usize>,
    #[arg(long = "fft", alias = "window", help = "Override: FFT window size (samples)")]
    window: Option<usize>,
    #[arg(long, help = "Override: STFT hop (samples)")]
    hop: Option<usize>,
    #[arg(long = "window-fn", help = "Override: window function (hann, hamming, blackman, blackman-harris)")]
    window_fn: Option<String>,
    #[arg(long = "band-agg", help = "Override: band aggregation (nearest, triangular)")]
    band_agg: Option<String>,
    #[arg(long = "smoothing", help = "Override: Gaussian frequency smoothing sigma (0=off, try 0.5-2.0)")]
    smoothing: Option<f32>,
    #[arg(long = "gamma", help = "Override: amplitude gamma (<1 brightens, >1 darkens)")]
    gamma: Option<f32>,
    #[arg(long = "temporal-alpha", help = "Override: EMA temporal smoothing (0=off, 0.3-0.7 typical)")]
    temporal_alpha: Option<f32>,
    #[arg(long = "peak-decay", help = "Override: peak hold decay per frame (0=off, 0.5-0.9 typical)")]
    peak_decay: Option<f32>,
    #[arg(long = "colormap", help = "Override: colormap name (viridis, inferno, magma, plasma, turbo, grayscale, heat, fire, ember, gold, cyanfire, rose, aurora, nebula, spectrum, ocean, sunset, gruvbox-dark, gruvbox-dark-5, catppuccin-mocha, catppuccin-mocha-5, nord, nord-5, tokyo-night, tokyo-night-5) or path to a .toml file")]
    colormap: Option<String>,
    #[arg(long = "weighting", help = "Override: frequency weighting (none, a, c)")]
    weighting: Option<String>,
    #[arg(long = "transform", help = "Override: transform (stft, cqt)")]
    transform: Option<String>,
    #[arg(long = "cqt-bpo", help = "Override: CQT bins per octave (default 12)")]
    cqt_bpo: Option<u32>,
    #[arg(long, help = "Override: output image width (px)")]
    width: Option<u32>,
    #[arg(long, help = "Override: output image height (px)")]
    height: Option<u32>,
    #[arg(long, help = "Override: render time top-to-bottom instead of left-to-right")]
    legacy_vertical_scroll: bool,
    #[arg(long, help = "Path to JSON scale config for frequency grid overlay")]
    scale: Option<PathBuf>,
    #[arg(long, default_value_t = 1.0, help = "Output contrast (>1 = more punch, 0.5 = flatter)")]
    contrast: f32,
    #[arg(long, default_value_t = 1.0, help = "Output saturation (>1 = vivid, 0 = grayscale)")]
    saturation: f32,
    #[arg(long, help = "Set PNG width from audio duration (one column = one pixel)")]
    auto_width: bool,
    #[arg(long, help = "Render from timestamp (mm:ss). If omitted, starts at 0.")]
    from: Option<String>,
    #[arg(long, help = "Render to timestamp (mm:ss). If omitted, renders to end.")]
    to: Option<String>,
}

fn main() -> Result<()> {
    let total_start = Instant::now();
    let args = Args::parse();

    let profile = if let Some(path) = &args.config {
        eprintln!("Loading config: {}", path.display());
        profiles::load_profile(path)?
    } else if let Some(name) = &args.profile {
        eprintln!("Loading profile: {}", name);
        profiles::resolve_profile(name)
            .with_context(|| format!("unknown profile '{}'. Available: {:?}", name, profiles::list_profile_names()))?
    } else {
        profiles::builtin_profile("default").unwrap()
    };

    let mut image_config = profile.to_image_config();
    if let Some(v) = args.log_bins { image_config.spectrum.log_bins = v; }
    if let Some(v) = args.window { image_config.spectrum.window_size = v; }
    if let Some(v) = args.hop { image_config.spectrum.hop_size = v; }
    if let Some(ref v) = args.window_fn {
        image_config.spectrum.window_fn = match v.to_lowercase().as_str() {
            "hann" => spektrum_core::WindowFunction::Hann,
            "hamming" => spektrum_core::WindowFunction::Hamming,
            "blackman" => spektrum_core::WindowFunction::Blackman,
            "blackman-harris" => spektrum_core::WindowFunction::BlackmanHarris,
            other => anyhow::bail!("unknown window function '{}'. Options: hann, hamming, blackman, blackman-harris", other),
        };
    }
    if let Some(ref v) = args.band_agg {
        image_config.spectrum.band_aggregation = match v.to_lowercase().as_str() {
            "nearest" => spektrum_core::BandAggregation::Nearest,
            "triangular" => spektrum_core::BandAggregation::Triangular,
            other => anyhow::bail!("unknown band aggregation '{}'. Options: nearest, triangular", other),
        };
    }
    if let Some(v) = args.smoothing { image_config.spectrum.freq_smoothing_sigma = v; }
    if let Some(v) = args.gamma { image_config.spectrum.amplitude_gamma = v; }
    if let Some(v) = args.temporal_alpha { image_config.spectrum.temporal_alpha = v; }
    if let Some(v) = args.peak_decay { image_config.spectrum.peak_hold_decay = v; }
    if let Some(v) = args.colormap { image_config.colormap = v; }
    if let Some(ref v) = args.weighting {
        image_config.spectrum.weighting = match v.to_lowercase().as_str() {
            "none" => spektrum_core::Weighting::None,
            "a" => spektrum_core::Weighting::A,
            "c" => spektrum_core::Weighting::C,
            other => anyhow::bail!("unknown weighting '{}'. Options: none, a, c", other),
        };
    }
    if let Some(ref v) = args.transform {
        image_config.spectrum.transform = match v.to_lowercase().as_str() {
            "stft" => spektrum_core::Transform::Stft,
            "cqt" => spektrum_core::Transform::Cqt,
            other => anyhow::bail!("unknown transform '{}'. Options: stft, cqt", other),
        };
    }
    if let Some(v) = args.cqt_bpo { image_config.spectrum.cqt_bins_per_octave = v; }
    if let Some(v) = args.width { image_config.width = v; }
    if let Some(v) = args.height { image_config.height = v; }
    if args.legacy_vertical_scroll { image_config.scroll_right_to_left = false; }
    image_config.contrast = args.contrast;
    image_config.saturation = args.saturation;

    eprintln!("=== vividspektrum audio_to_png ===");
    eprintln!("input   : {}", args.input.display());
    eprintln!("output  : {}", args.output.display());
    eprintln!("fft     : {} samples  |  hop : {} samples  |  window : {:?}  |  bands : {:?}", image_config.spectrum.window_size, image_config.spectrum.hop_size, image_config.spectrum.window_fn, image_config.spectrum.band_aggregation);
    eprintln!("smooth  : {:.2} sigma  |  gamma : {:.2}  |  ema : {:.2}  |  peak : {:.2}", image_config.spectrum.freq_smoothing_sigma, image_config.spectrum.amplitude_gamma, image_config.spectrum.temporal_alpha, image_config.spectrum.peak_hold_decay);
    eprintln!("cmap    : {}", image_config.colormap);
    eprintln!("weight  : {:?}  |  transform : {:?}", image_config.spectrum.weighting, image_config.spectrum.transform);
    if image_config.spectrum.transform == spektrum_core::Transform::Cqt {
        eprintln!("cqt     : {} bins/octave", image_config.spectrum.cqt_bins_per_octave);
    }
    eprintln!("bins    : {} (log)", image_config.spectrum.log_bins);
    eprintln!("image   : {} x {} px", image_config.width, image_config.height);
    eprintln!("scroll  : {}", if image_config.scroll_right_to_left { "right-to-left" } else { "top-to-bottom" });
    eprintln!();

    let decode_start = Instant::now();
    eprintln!("[1/3] Decoding audio...");
    let (samples, sample_rate) = decode_mono_f32(&args.input)?;
    let decode_elapsed = decode_start.elapsed();
    let total_duration_secs = samples.len() as f64 / sample_rate as f64;
    eprintln!("       {} samples @ {} Hz = {:.1}s audio", samples.len(), sample_rate, total_duration_secs);
    eprintln!("       decode took {:.2}s", decode_elapsed.as_secs_f64());
    eprintln!();

    image_config.spectrum.sample_rate = sample_rate;

    let from_sample = args.from.as_ref().map(|s| parse_timestamp(s, sample_rate)).transpose()?.unwrap_or(0).min(samples.len());
    let to_sample = args.to.as_ref().map(|s| parse_timestamp(s, sample_rate)).transpose()?.unwrap_or(samples.len()).min(samples.len());
    if from_sample > to_sample {
        anyhow::bail!("--from timestamp is after --to timestamp");
    }
    let samples = &samples[from_sample..to_sample];
    let range_duration_secs = samples.len() as f64 / sample_rate as f64;
    if from_sample != 0 || to_sample != samples.len() {
        eprintln!("       selected range: {} samples = {:.1}s (from {} to {})", samples.len(), range_duration_secs, from_sample, to_sample);
    }

    let w = image_config.spectrum.window_size;
    let h = image_config.spectrum.hop_size;
    let num_windows = if samples.len() >= w { (samples.len() - w) / h + 1 } else { 0 };
    eprintln!("[2/3] Computing spectrogram ({} windows)...", num_windows);
    let fft_start = Instant::now();
    let columns = samples_to_spectrogram(samples, image_config.spectrum.clone())?;
    let fft_elapsed = fft_start.elapsed();
    eprintln!("       {} columns in {:.2}s ({:.0} windows/s)", columns.len(), fft_elapsed.as_secs_f64(), columns.len() as f64 / fft_elapsed.as_secs_f64().max(0.001));
    eprintln!();

    if args.auto_width {
        image_config.width = columns.len().max(1) as u32;
        eprintln!("       auto-width: {} px (one column per pixel)", image_config.width);
    }

    let scale = if let Some(path) = &args.scale {
        eprintln!("       loading scale config: {}", path.display());
        Some(ScaleConfig::load(path)?)
    } else {
        None
    };

    eprintln!("[3/3] Rendering PNG...");
    let render_start = Instant::now();
    render_spectrogram_png_with_grid(&columns, &image_config, &args.output, scale.as_ref())?;
    let render_elapsed = render_start.elapsed();
    eprintln!("       render took {:.2}s", render_elapsed.as_secs_f64());
    eprintln!();

    let total_elapsed = total_start.elapsed();
    eprintln!("Done. Total: {:.2}s  ->  {}", total_elapsed.as_secs_f64(), args.output.display());
    Ok(())
}

fn parse_timestamp(s: &str, sample_rate: u32) -> Result<usize> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        anyhow::bail!("timestamp must be mm:ss, got '{}'", s);
    }
    let minutes: usize = parts[0]
        .parse()
        .with_context(|| format!("invalid minutes in timestamp '{}'", s))?;
    let seconds: usize = parts[1]
        .parse()
        .with_context(|| format!("invalid seconds in timestamp '{}'", s))?;
    let total = minutes * 60 + seconds;
    Ok(total * sample_rate as usize)
}

fn decode_mono_f32(path: &Path) -> Result<(Vec<f32>, u32)> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("failed to probe audio format")?;
    let mut format = probed.format;
    let track = format.default_track().context("missing default audio track")?;
    let codec_params = &track.codec_params;
    let sample_rate = codec_params.sample_rate.context("missing sample rate")?;
    let track_id = track.id;
    let mut decoder = get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .context("failed to create audio decoder")?;
    let mut samples = Vec::new();
    let mut packet_count: u64 = 0;
    let decode_start = Instant::now();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(_)) | Err(Error::ResetRequired) => break,
            Err(err) => return Err(err).context("failed to read audio packet"),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio) => {
                push_mono_samples(audio, &mut samples);
                packet_count += 1;
                if packet_count.is_multiple_of(500) {
                    let elapsed = decode_start.elapsed();
                    let secs = samples.len() as f64 / sample_rate as f64;
                    eprintln!("       {} packets, {:.1}s audio decoded ({:.1}s elapsed)", packet_count, secs, elapsed.as_secs_f64());
                }
            }
            Err(Error::DecodeError(_)) => continue,
            Err(err) => return Err(err).context("failed to decode audio packet"),
        }
    }
    Ok((samples, sample_rate))
}

fn push_mono_samples(audio: AudioBufferRef<'_>, out: &mut Vec<f32>) {
    let spec = *audio.spec();
    let channels = spec.channels.count();
    if channels == 0 {
        return;
    }
    let mut buffer = SampleBuffer::<f32>::new(audio.capacity() as u64, spec);
    buffer.copy_interleaved_ref(audio);
    for frame in buffer.samples().chunks(channels) {
        let mut sum = 0.0;
        for sample in frame {
            sum += *sample;
        }
        out.push(sum / frame.len() as f32);
    }
}
