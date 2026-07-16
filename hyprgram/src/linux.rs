use crate::Args;
use hyprgram::dev::{effective_spectrogram_history, SpectrogramDevConfig};
use hyprgram::spectrogram::SpectrogramProgram;
use hyprgram_core::{overlay, profiles, resolve_colormap, sample_ring_pair, SpectrumProcessor};
use iced::widget::container;
use iced::widget::shader::Shader;
use iced::{Element, Length, Size, Subscription, Task};
use iced::mouse;
use iced::event::Event;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
enum Message {
    Tick,
    WindowEvent(Event),
}

pub struct App {
    pub prog: SpectrogramProgram,
    fullscreen: bool,
    last_click: Option<Instant>,
}

impl App {
    fn bootstrap(args: Args) -> Self {
        let profile = if let Some(path) = &args.config {
            profiles::load_profile(path).expect("failed to load config")
        } else if let Some(name) = &args.profile {
            profiles::builtin_profile(name)
                .unwrap_or_else(|| panic!("unknown profile '{}'. Available: {:?}", name, profiles::builtin_profile_names()))
        } else {
            profiles::builtin_profile("default").unwrap()
        };
        let mut spectrum = profile.spectrum;
        if let Some(v) = args.log_bins { spectrum.log_bins = v; }
        if let Some(v) = args.window { spectrum.window_size = v; }
        if let Some(v) = args.hop { spectrum.hop_size = v; }
        if let Some(v) = args.sample_rate { spectrum.sample_rate = v; }
        if let Some(ref v) = args.window_fn {
            spectrum.window_fn = match v.to_lowercase().as_str() {
                "hann" => hyprgram_core::WindowFunction::Hann,
                "hamming" => hyprgram_core::WindowFunction::Hamming,
                "blackman" => hyprgram_core::WindowFunction::Blackman,
                "blackman-harris" => hyprgram_core::WindowFunction::BlackmanHarris,
                other => panic!("unknown window function '{}'", other),
            };
        }
        if let Some(ref v) = args.band_agg {
            spectrum.band_aggregation = match v.to_lowercase().as_str() {
                "nearest" => hyprgram_core::BandAggregation::Nearest,
                "triangular" => hyprgram_core::BandAggregation::Triangular,
                other => panic!("unknown band aggregation '{}'", other),
            };
        }
        if let Some(v) = args.f_min { spectrum.f_min_hz = v; }
        if let Some(v) = args.f_max { spectrum.f_max_hz = v; }
        if let Some(v) = args.db_floor { spectrum.db_floor = v; }
        if let Some(v) = args.db_ceil { spectrum.db_ceil = v; }
        if let Some(v) = args.smoothing { spectrum.freq_smoothing_sigma = v; }
        if let Some(v) = args.gamma { spectrum.amplitude_gamma = v; }
        if let Some(v) = args.temporal_alpha { spectrum.temporal_alpha = v; }
        if let Some(v) = args.peak_decay { spectrum.peak_hold_decay = v; }
        if let Some(ref v) = args.weighting {
            spectrum.weighting = match v.to_lowercase().as_str() {
                "none" => hyprgram_core::Weighting::None,
                "a" => hyprgram_core::Weighting::A,
                "c" => hyprgram_core::Weighting::C,
                other => panic!("unknown weighting '{}'", other),
            };
        }
        if let Some(ref v) = args.transform {
            spectrum.transform = match v.to_lowercase().as_str() {
                "stft" => hyprgram_core::Transform::Stft,
                "cqt" => hyprgram_core::Transform::Cqt,
                other => panic!("unknown transform '{}'", other),
            };
        }
        if let Some(v) = args.cqt_bpo { spectrum.cqt_bins_per_octave = v; }
        if let Some(v) = args.freq_scale_exp { spectrum.freq_scale_exp = v; }
        if args.centered { spectrum.centered = true; }
        let colormap = resolve_colormap(args.colormap.as_deref().unwrap_or("viridis"))
            .expect("invalid colormap");
        let colormap_lut = Arc::new(colormap.build_lut_rgba(256));
        let img = profile.image.as_ref();
        let width = args.width.unwrap_or(img.map_or(800, |i| i.width));
        let height = args.height.unwrap_or(img.map_or(200, |i| i.height));
        let rtl = if args.legacy_vertical_scroll { false } else { img.is_none_or(|i| i.scroll_right_to_left) };

        let history = effective_spectrogram_history(args.history, width, height, rtl);
        let backlog_cap = (history as usize).saturating_mul(8).saturating_add(256).max(1024);
        let pending_spectra = Arc::new(Mutex::new(VecDeque::new()));
        let pending_w = pending_spectra.clone();
        let (producer, mut consumer) = sample_ring_pair((spectrum.sample_rate as usize) * 2);
        let _pw = hyprgram_core::pipewire::spawn_capture_lockfree(args.target_object.clone(), producer);
        let mut proc = SpectrumProcessor::new(spectrum.clone()).expect("spectrum processor");
        let sr = spectrum.sample_rate as f32;
        let window_latency_ms = spectrum.window_size as f32 / sr * 1000.0;
        let centered_latency_ms = if spectrum.centered { spectrum.window_size as f32 / 2.0 / sr * 1000.0 } else { 0.0 };
        let total_latency_ms = window_latency_ms + centered_latency_ms;
        eprintln!(
            "[hyprgram] DSP: fft={} hop={} bins={} rate={} | latency: window {:.1}ms + centered {:.1}ms = {:.1}ms | cols/sec: {:.0}",
            spectrum.window_size, spectrum.hop_size, spectrum.log_bins, spectrum.sample_rate,
            window_latency_ms, centered_latency_ms, total_latency_ms,
            sr / spectrum.hop_size as f32,
        );
        let debug_profile = args.debug_profile;
        std::thread::spawn(move || {
            let mut scratch = vec![0.0f32; 65536];
            let mut prof_last = Instant::now();
            let mut prof_dsp_us: u64 = 0;
            let mut prof_cols: u64 = 0;
            let mut prof_samples: u64 = 0;
            loop {
                let n = consumer.pop_into(&mut scratch);
                if n == 0 {
                    std::thread::sleep(Duration::from_micros(500));
                    continue;
                }
                let t0 = Instant::now();
                let mut cols = Vec::new();
                proc.push_samples(&scratch[..n], &mut cols);
                let dsp_elapsed = t0.elapsed();
                let mut q = pending_w.lock().unwrap();
                for c in &cols {
                    while q.len() >= backlog_cap {
                        q.pop_front();
                    }
                    q.push_back(c.clone());
                }
                if debug_profile {
                    prof_dsp_us += dsp_elapsed.as_micros() as u64;
                    prof_cols += cols.len() as u64;
                    prof_samples += n as u64;
                    let elapsed = prof_last.elapsed();
                    if elapsed >= Duration::from_secs(1) {
                        let secs = elapsed.as_secs_f64();
                        eprintln!(
                            "[profile] DSP: {:.1}ms/sec total | {:.2}ms/col avg | cols/sec: {:.0} | samples/sec: {:.0}",
                            prof_dsp_us as f64 / 1000.0 / secs,
                            if prof_cols > 0 { prof_dsp_us as f64 / prof_cols as f64 / 1000.0 } else { 0.0 },
                            prof_cols as f64 / secs,
                            prof_samples as f64 / secs,
                        );
                        prof_last = Instant::now();
                        prof_dsp_us = 0;
                        prof_cols = 0;
                        prof_samples = 0;
                    }
                }
            }
        });
        let (overlay_lines, overlay_color, overlay_opacity, overlay_thickness) = if let Some(ref name) = args.overlay {
            let cfg = overlay::load_overlay(name)
                .unwrap_or_else(|| panic!("unknown overlay '{}'. Available: {:?}", name, overlay::builtin_overlay_names()));
            let f_min = spectrum.f_min_hz;
            let f_max = spectrum.f_max_hz;
            let exp = spectrum.freq_scale_exp.max(0.1);
            let log_range = (f_max / f_min).ln();
            let lines: Vec<f32> = cfg.lines.iter()
                .map(|l| l.freq)
                .filter(|&f| f >= f_min && f <= f_max)
                .map(|f| {
                    let t = (f / f_min).ln() / log_range;
                    let bin_pos = t.powf(1.0 / exp);
                    1.0 - bin_pos
                })
                .collect();
            let color = [cfg.color[0] as f32 / 255.0, cfg.color[1] as f32 / 255.0, cfg.color[2] as f32 / 255.0];
            (lines, color, cfg.opacity, cfg.thickness)
        } else {
            (Vec::new(), [0.9, 0.9, 0.9], 0.6, 0.003)
        };
        Self {
            prog: SpectrogramProgram {
                pending_spectra,
                bins: spectrum.log_bins as u32,
                min_history: history,
                dev: SpectrogramDevConfig {
                    scroll_right_to_left: rtl,
                },
                colormap_lut,
                contrast: args.contrast,
                saturation: args.saturation,
                debug_profile,
                overlay_lines,
                overlay_color,
                overlay_opacity,
                overlay_thickness,
            },
            fullscreen: false,
            last_click: None,
        }
    }
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Tick => Task::none(),
        Message::WindowEvent(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))) => {
            let now = Instant::now();
            if let Some(prev) = app.last_click {
                if now.duration_since(prev) < Duration::from_millis(400) {
                    app.fullscreen = !app.fullscreen;
                    let mode = if app.fullscreen {
                        iced::window::Mode::Fullscreen
                    } else {
                        iced::window::Mode::Windowed
                    };
                    app.last_click = None;
                    return iced::window::latest()
                        .and_then(move |id| iced::window::set_mode(id, mode));
                }
            }
            app.last_click = Some(now);
            Task::none()
        }
        Message::WindowEvent(_) => Task::none(),
    }
}

fn view(app: &App) -> Element<'_, Message> {
    let sh = Shader::new(app.prog.clone()).width(Length::Fill).height(Length::Fill);
    container(sh).width(Length::Fill).height(Length::Fill).into()
}

fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick),
        iced::event::listen().map(Message::WindowEvent),
    ])
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let size = Size::new(args.width.unwrap_or(800) as f32, args.height.unwrap_or(200) as f32);
    let level = if args.always_on_top {
        iced::window::Level::AlwaysOnTop
    } else if args.always_on_bottom {
        iced::window::Level::AlwaysOnBottom
    } else {
        iced::window::Level::Normal
    };
    let position = args.position.as_deref().and_then(|s| {
        let mut parts = s.splitn(2, ',');
        let x: f32 = parts.next()?.trim().parse().ok()?;
        let y: f32 = parts.next()?.trim().parse().ok()?;
        Some(iced::window::Position::Specific(iced::Point::new(x, y)))
    });
    let no_decorations = args.no_decorations;
    let transparent = args.transparent;
    let mut app = iced::application(move || App::bootstrap(args.clone()), update, view)
        .title("hyprgram")
        .window_size(size)
        .subscription(subscription)
        .theme(iced::Theme::Dark)
        .decorations(!no_decorations)
        .transparent(transparent)
        .level(level);
    if let Some(pos) = position {
        app = app.position(pos);
    } else {
        app = app.centered();
    }
    app.run().map_err(|e| anyhow::anyhow!("{e:?}"))
}
