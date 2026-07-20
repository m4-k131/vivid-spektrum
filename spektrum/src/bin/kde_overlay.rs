#[cfg(target_os = "linux")]
mod inner {
    use clap::Parser;
    use spektrum::dev::{SpectrogramDevConfig, effective_spectrogram_history};
    use spektrum::spectrogram::SpectrogramProgram;
    use spektrum_core::{default_colormap, profiles, SampleRing, SpectrumProcessor};
    use iced::widget::container;
    use iced::widget::shader::Shader;
    use iced::{Color, Element, Length, Subscription, Task};
    use iced_layershell::application;
    use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
    use iced_layershell::settings::{LayerShellSettings, StartMode};
    use iced_layershell::to_layer_message;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Parser, Debug, Clone)]
    #[command(
        name = "kde_overlay",
        about = "Layer-shell spectrogram overlay for KDE Plasma / any wlr-layer-shell compositor"
    )]
    pub struct Args {
        #[arg(long, help = "PipeWire target object id or name")]
        pub target_object: Option<String>,
        #[arg(long, help = "Built-in profile: laptop, default, high-resolution")]
        pub profile: Option<String>,
        #[arg(long, help = "Path to a TOML profile file")]
        pub config: Option<std::path::PathBuf>,
        #[arg(long, help = "Override: number of log-spaced frequency bins")]
        pub log_bins: Option<usize>,
        #[arg(long = "fft", alias = "window", help = "Override: FFT window length (samples)")]
        pub window: Option<usize>,
        #[arg(long, help = "Override: STFT hop (samples)")]
        pub hop: Option<usize>,
        #[arg(long, help = "Override: window width (px); 0 = stretch to screen edge")]
        pub width: Option<u32>,
        #[arg(long, help = "Override: window height (px)")]
        pub height: Option<u32>,
        #[arg(long, default_value_t = 512, help = "Time rows in waterfall")]
        pub history: u32,
        #[arg(
            long,
            default_value = "bottom",
            help = "Screen edge to anchor to: top, bottom, left, right"
        )]
        pub anchor: String,
        #[arg(
            long,
            default_value = "bottom",
            help = "Layer: background, bottom, top, overlay"
        )]
        pub layer: String,
        #[arg(long, default_value_t = 0, help = "Exclusive zone (px); -1 = ignore, 0 = no reservation")]
        pub exclusive_zone: i32,
        #[arg(long, help = "Output/screen name to bind to (e.g. HDMI-A-1)")]
        pub output: Option<String>,
        #[arg(long, help = "Scroll time top-to-bottom instead of right-to-left")]
        pub legacy_vertical_scroll: bool,
    }

    fn parse_anchor(s: &str) -> Anchor {
        let mut a = Anchor::empty();
        for part in s.split(',') {
            a |= match part.trim().to_lowercase().as_str() {
                "top" => Anchor::Top,
                "bottom" => Anchor::Bottom,
                "left" => Anchor::Left,
                "right" => Anchor::Right,
                _ => Anchor::empty(),
            };
        }
        if a.is_empty() {
            Anchor::Bottom
        } else {
            a
        }
    }

    fn parse_layer(s: &str) -> Layer {
        match s.trim().to_lowercase().as_str() {
            "background" => Layer::Background,
            "bottom" => Layer::Bottom,
            "overlay" => Layer::Overlay,
            _ => Layer::Top,
        }
    }

    pub struct App {
        prog: SpectrogramProgram,
    }

    #[to_layer_message]
    #[derive(Debug, Clone)]
    enum Message {
        Tick,
    }

    fn update(_app: &mut App, message: Message) -> Task<Message> {
        match message {
            Message::Tick => Task::none(),
            _ => unreachable!(),
        }
    }

    fn view(app: &App) -> Element<'_, Message> {
        let sh = Shader::new(app.prog.clone()).width(Length::Fill).height(Length::Fill);
        container(sh).width(Length::Fill).height(Length::Fill).into()
    }

    fn subscription(_app: &App) -> Subscription<Message> {
        iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick)
    }

    fn style(_app: &App, _theme: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: iced::Color::WHITE,
        }
    }

    pub fn run() -> anyhow::Result<()> {
        let args = Args::parse();

        let profile = if let Some(path) = &args.config {
            profiles::load_profile(path).expect("failed to load config")
        } else if let Some(name) = &args.profile {
            profiles::resolve_profile(name)
                .unwrap_or_else(|e| panic!("{}. Available: {:?}", e, profiles::list_profile_names()))
        } else {
            profiles::builtin_profile("default").unwrap()
        };

        let spectrum = profile.dsp;
        let img = profile.image.as_ref();
        let width = args.width.unwrap_or(img.map_or(0, |i| i.width));
        let height = args.height.unwrap_or(img.map_or(200, |i| i.height));
        let rtl = if args.legacy_vertical_scroll { false } else { img.is_none_or(|i| i.scroll_right_to_left) };

        let history = effective_spectrogram_history(args.history);
        let backlog_cap = (history as usize).saturating_mul(8).saturating_add(256).max(1024);
        let pending_spectra: Arc<Mutex<VecDeque<Vec<f32>>>> = Arc::new(Mutex::new(VecDeque::new()));
        let pending_w = pending_spectra.clone();
        let ring = SampleRing::new((spectrum.sample_rate as usize) * 2);
        let _pw = spektrum_core::pipewire::spawn_capture(args.target_object.clone(), ring.clone());
        let bins = spectrum.log_bins;
        let mut proc = SpectrumProcessor::new(spectrum).expect("spectrum processor");
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

        let prog = SpectrogramProgram {
            pending_spectra,
            bins: bins as u32,
            min_history: history,
            paused: false,
            dev: SpectrogramDevConfig { scroll_right_to_left: rtl },
            colormap_lut: Arc::new(default_colormap().build_lut_rgba(256)),
            contrast: 1.0,
            saturation: 1.0,
            debug_profile: false,
            overlay_lines: Vec::new(),
            overlay_color: [0.9, 0.9, 0.9],
            overlay_opacity: 0.6,
            overlay_thickness: 0.003,
        };

        let anchor = parse_anchor(&args.anchor);
        let layer = parse_layer(&args.layer);
        let size = if width == 0 { None } else { Some((width, height)) };

        let start_mode = match args.output {
            Some(name) => StartMode::TargetScreen(name),
            None => StartMode::Active,
        };

        application(move || App { prog: prog.clone() }, "vividspektrum-overlay", update, view)
            .subscription(subscription)
            .theme(iced::Theme::Dark)
            .style(style)
            .layer_settings(LayerShellSettings {
                anchor,
                layer,
                exclusive_zone: args.exclusive_zone,
                size,
                keyboard_interactivity: KeyboardInteractivity::None,
                start_mode,
                ..Default::default()
            })
            .run()
            .map_err(|e| anyhow::anyhow!("{e:?}"))
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        inner::run()
    }
    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!("kde_overlay requires Linux with wlr-layer-shell support")
    }
}
