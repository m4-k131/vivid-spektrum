use crate::Args;
use spektrum::dev::{effective_spectrogram_history, SpectrogramDevConfig};
use spektrum::spectrogram::SpectrogramProgram;
use spektrum_core::{profiles, resolve_colormap, SampleRing, SpectrumProcessor};
use iced::widget::container;
use iced::widget::shader::Shader;
use iced::{Element, Length, Size, Subscription, Task};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
enum Message {
    Tick,
}

pub struct App {
    pub prog: SpectrogramProgram,
}

impl App {
    fn bootstrap(args: Args) -> Self {
        let profile = if let Some(path) = &args.config {
            profiles::load_profile(path).expect("failed to load config")
        } else if let Some(name) = &args.profile {
            profiles::resolve_profile(name)
                .unwrap_or_else(|e| panic!("{}. Available: {:?}", e, profiles::list_profile_names()))
        } else {
            profiles::builtin_profile("default").unwrap()
        };
        let mut spectrum = profile.dsp;
        if let Some(v) = args.log_bins { spectrum.log_bins = v; }
        if let Some(v) = args.window { spectrum.window_size = v; }
        if let Some(v) = args.hop { spectrum.hop_size = v; }
        if let Some(v) = args.sample_rate { spectrum.sample_rate = v; }
        if let Some(ref v) = args.window_fn {
            spectrum.window_fn = match v.to_lowercase().as_str() {
                "hann" => spektrum_core::WindowFunction::Hann,
                "hamming" => spektrum_core::WindowFunction::Hamming,
                "blackman" => spektrum_core::WindowFunction::Blackman,
                "blackman-harris" => spektrum_core::WindowFunction::BlackmanHarris,
                other => panic!("unknown window function '{}'", other),
            };
        }
        if let Some(ref v) = args.band_agg {
            spectrum.band_aggregation = match v.to_lowercase().as_str() {
                "nearest" => spektrum_core::BandAggregation::Nearest,
                "triangular" => spektrum_core::BandAggregation::Triangular,
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
                "none" => spektrum_core::Weighting::None,
                "a" => spektrum_core::Weighting::A,
                "c" => spektrum_core::Weighting::C,
                other => panic!("unknown weighting '{}'", other),
            };
        }
        if let Some(ref v) = args.transform {
            spectrum.transform = match v.to_lowercase().as_str() {
                "stft" => spektrum_core::Transform::Stft,
                "cqt" => spektrum_core::Transform::Cqt,
                other => panic!("unknown transform '{}'", other),
            };
        }
        if let Some(v) = args.cqt_bpo { spectrum.cqt_bins_per_octave = v; }
        if let Some(v) = args.freq_scale_exp { spectrum.freq_scale_exp = v; }
        if args.centered { spectrum.centered = true; }
        let colormap = resolve_colormap(args.colormap.as_deref().unwrap_or(&profile.colors.colormap))
            .expect("invalid colormap");
        let colormap_lut = Arc::new(colormap.build_lut_rgba(256));
        let img = profile.image.as_ref();
        let width = args.width.unwrap_or(img.map_or(800, |i| i.width));
        let height = args.height.unwrap_or(img.map_or(800, |i| i.height));
        let rtl = if args.legacy_vertical_scroll { false } else { img.map_or(true, |i| i.scroll_right_to_left) };

        let history = effective_spectrogram_history(args.history);
        let backlog_cap = (history as usize).saturating_mul(8).saturating_add(256).max(1024);
        let pending_spectra = Arc::new(Mutex::new(VecDeque::new()));
        let pending_w = pending_spectra.clone();
        let ring = SampleRing::new((spectrum.sample_rate as usize) * 2);
        let _audio = spektrum_core::cpal::spawn_capture(args.target_object.clone(), ring.clone());
        let mut proc = SpectrumProcessor::new(spectrum.clone()).expect("spectrum processor");
        std::thread::spawn(move || {
            let mut scratch = vec![0.0f32; 65536];
            loop {
                let n = ring.pop_into(&mut scratch);
                if n == 0 {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                let mut cols = Vec::new();
                proc.push_samples(&scratch[..n], &mut cols);
                let mut q = pending_w.lock().unwrap();
                for c in cols {
                    while q.len() >= backlog_cap {
                        q.pop_front();
                    }
                    q.push_back(c);
                }
            }
        });
        Self {
            prog: SpectrogramProgram {
                pending_spectra,
                bins: spectrum.log_bins as u32,
                min_history: history,
                paused: false,
                dev: SpectrogramDevConfig {
                    scroll_right_to_left: rtl,
                },
                colormap_lut,
                contrast: args.contrast.unwrap_or(profile.colors.contrast),
                saturation: args.saturation.unwrap_or(profile.colors.saturation),
            },
        }
    }
}

fn update(_app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Tick => Task::none(),
    }
}

fn view(app: &App) -> Element<'_, Message> {
    let sh = Shader::new(app.prog.clone()).width(Length::Fill).height(Length::Fill);
    container(sh).width(Length::Fill).height(Length::Fill).into()
}

fn subscription(_app: &App) -> Subscription<Message> {
    iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick)
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let size = Size::new(args.width.unwrap_or(800) as f32, args.height.unwrap_or(200) as f32);
    iced::application(move || App::bootstrap(args.clone()), update, view)
        .title("vividspektrum")
        .window_size(size)
        .centered()
        .subscription(subscription)
        .theme(iced::Theme::Dark)
        .run()
        .map_err(|e| anyhow::anyhow!("{e:?}"))
}
