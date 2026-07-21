use crate::Args;
use spektrum::dev::{effective_spectrogram_history, SpectrogramDevConfig};
use spektrum::settings::{DspSlider, SettingsMessage, SettingsState};
use spektrum::spectrogram::SpectrogramProgram;
use spektrum_core::{overlay, profiles, resolve_colormap, spectrum_output_bins, SampleRing, SpectrumConfig, SpectrumProcessor};
use iced::keyboard;
use iced::mouse;
use iced::widget::{container, stack};
use iced::widget::shader::Shader;
use iced::{Element, Event, Length, Size, Subscription, Task};
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
enum DspCommand {
    Restart(SpectrumConfig, u32),
    UpdateRuntime(SpectrumConfig),
    SetHistory(u32),
    SetPaused(bool),
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    WindowEvent(Event),
    Settings(SettingsMessage),
}

pub struct App {
    pub prog: SpectrogramProgram,
    settings: SettingsState,
    args: Args,
    spectrum: SpectrumConfig,
    fullscreen: bool,
    last_click: Option<Instant>,
    restart_tx: mpsc::Sender<DspCommand>,
    capture_tx: mpsc::Sender<String>,
    colormaps: Vec<String>,
    profiles: Vec<String>,
    dsp_settings: Vec<String>,
    overlays: Vec<String>,
    sources: Vec<String>,
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
        let profile_name = args.config.as_ref()
            .and_then(|path| path.file_stem().map(|name| name.to_string_lossy().into_owned()))
            .or(args.profile.clone())
            .unwrap_or_else(|| "default".to_string());
        let mut spectrum = profile.dsp.clone();
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
        let colormap_name = args.colormap.as_deref().unwrap_or(&profile.colors.colormap);
        let contrast = args.contrast.unwrap_or(profile.colors.contrast);
        let saturation = args.saturation.unwrap_or(profile.colors.saturation);
        let colormap = resolve_colormap(colormap_name).unwrap_or_else(|error| {
            eprintln!("{error}; using default colormap");
            spektrum_core::default_colormap()
        });
        let colormap_lut = Arc::new(colormap.build_lut_rgba(256));
        let img = profile.image.as_ref();
        let rtl = if args.legacy_vertical_scroll { false } else { img.map_or(true, |i| i.scroll_right_to_left) };

        let history = effective_spectrogram_history(args.history);
        let pending_spectra = Arc::new(Mutex::new(VecDeque::new()));
        let pending_w = pending_spectra.clone();
        let ring = SampleRing::new((spectrum.sample_rate as usize) * 2);
        let capture_target = args.target_object.clone().or_else(|| profile.audio.source.clone());
        let (_audio, capture_tx) = spektrum_core::cpal::spawn_capture(capture_target.clone(), ring.clone());
        let sources = spektrum_core::cpal::output_device_names();
        let source = capture_target.unwrap_or_else(|| "default output".to_string());
        let (restart_tx, restart_rx) = mpsc::channel::<DspCommand>();
        let initial_cfg = spectrum.clone();
        std::thread::spawn(move || {
            let mut scratch = vec![0.0f32; 65536];
            let mut cfg = initial_cfg;
            let mut backlog_cap = (history as usize).saturating_mul(8).saturating_add(256).max(1024);
            let mut proc = SpectrumProcessor::new(cfg.clone()).expect("spectrum processor");
            let mut paused = false;
            loop {
                while let Ok(cmd) = restart_rx.try_recv() {
                    match cmd {
                        DspCommand::Restart(new_cfg, new_history) => {
                            cfg = new_cfg;
                            proc = SpectrumProcessor::new(cfg.clone()).expect("spectrum processor");
                            backlog_cap = (new_history as usize).saturating_mul(8).saturating_add(256).max(1024);
                        }
                        DspCommand::UpdateRuntime(new_cfg) => {
                            cfg = new_cfg;
                            proc.set_runtime_cfg(&cfg);
                        }
                        DspCommand::SetHistory(new_history) => {
                            backlog_cap = (new_history as usize).saturating_mul(8).saturating_add(256).max(1024);
                        }
                        DspCommand::SetPaused(value) => paused = value,
                    }
                }
                let n = ring.pop_into(&mut scratch);
                if n == 0 {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                if paused { continue; }
                let mut cols = Vec::new();
                proc.push_samples(&scratch[..n], &mut cols);
                let mut q = pending_w.lock().unwrap();
                for col in cols {
                    while q.len() >= backlog_cap { q.pop_front(); }
                    q.push_back(col);
                }
            }
        });
        let overlay_name = args.overlay.clone().unwrap_or_else(|| profile.colors.overlay.clone());
        let (overlay_lines, overlay_color, overlay_opacity, overlay_thickness) = compute_overlay(&overlay_name, &spectrum);
        Self {
            prog: SpectrogramProgram {
                pending_spectra,
                bins: spectrum_output_bins(&spectrum) as u32,
                min_history: history,
                paused: false,
                dev: SpectrogramDevConfig {
                    scroll_right_to_left: rtl,
                },
                colormap_lut,
                contrast,
                saturation,
                debug_profile: args.debug_profile,
                overlay_lines,
                overlay_color,
                overlay_opacity,
                overlay_thickness,
            },
            settings: SettingsState::new(
                true, contrast, saturation, colormap_name, profile_name, "default",
                args.overlay.clone().unwrap_or_else(|| profile.colors.overlay.clone()),
                source, &spectrum, history as f32,
            ),
            args,
            spectrum,
            fullscreen: false,
            last_click: None,
            restart_tx,
            capture_tx,
            colormaps: spektrum_core::all_colormap_names(),
            profiles: profiles::list_profile_names(),
            dsp_settings: list_dsp_settings_names(),
            overlays: list_overlay_names(),
            sources,
        }
    }
}

fn list_dsp_settings_names() -> Vec<String> {
    let mut names = profiles::list_dsp_settings_names();
    names.push("custom".to_string());
    names
}

fn list_overlay_names() -> Vec<String> {
    let mut names = vec!["none".to_string()];
    names.extend(overlay::builtin_overlay_names());
    names
}

fn compute_overlay(name: &str, spectrum: &SpectrumConfig) -> (Vec<f32>, [f32; 3], f32, f32) {
    if name == "none" || name.is_empty() { return (Vec::new(), [0.9, 0.9, 0.9], 0.0, 0.003); }
    let Some(config) = overlay::load_overlay(name) else { return (Vec::new(), [0.9, 0.9, 0.9], 0.0, 0.003); };
    let f_min = spectrum.f_min_hz;
    let f_max = spectrum.f_max_hz;
    let range = (f_max / f_min).ln();
    let exponent = spectrum.freq_scale_exp.max(0.1);
    let lines = config.lines.iter()
        .filter(|line| line.freq >= f_min && line.freq <= f_max)
        .map(|line| 1.0 - ((line.freq / f_min).ln() / range).powf(1.0 / exponent))
        .collect();
    (
        lines,
        [config.color[0] as f32 / 255.0, config.color[1] as f32 / 255.0, config.color[2] as f32 / 255.0],
        config.opacity,
        config.thickness,
    )
}

fn apply_overlay(app: &mut App, name: &str) {
    app.settings.overlay = name.to_string();
    let (lines, color, opacity, thickness) = compute_overlay(name, &app.spectrum);
    app.prog.overlay_lines = lines;
    app.prog.overlay_color = color;
    app.prog.overlay_opacity = opacity;
    app.prog.overlay_thickness = thickness;
}

fn restart_dsp(app: &mut App) {
    app.restart_tx.send(DspCommand::Restart(app.spectrum.clone(), app.prog.min_history)).ok();
    app.prog.bins = spectrum_output_bins(&app.spectrum) as u32;
    let overlay = app.settings.overlay.clone();
    apply_overlay(app, &overlay);
}

fn apply_dsp_settings(app: &mut App, name: &str) {
    if name == "custom" {
        app.settings.dsp_settings = name.to_string();
        return;
    }
    let Ok(mut spectrum) = profiles::resolve_dsp_settings(name) else { return; };
    if let Some(sample_rate) = app.args.sample_rate { spectrum.sample_rate = sample_rate; }
    app.spectrum = spectrum;
    app.settings.from_spectrum(&app.spectrum, app.prog.min_history as f32);
    app.settings.dsp_settings = name.to_string();
    restart_dsp(app);
}

fn apply_colormap(app: &mut App, name: &str) {
    if let Ok(colormap) = resolve_colormap(name) {
        app.prog.colormap_lut = Arc::new(colormap.build_lut_rgba(256));
        app.settings.colormap = name.to_string();
    }
}

fn apply_edited_colormap(app: &mut App) {
    if app.settings.colormap_stops.len() < 2 { return; }
    let mut stops = app.settings.colormap_stops.clone();
    stops.sort_by(|left, right| left.0.total_cmp(&right.0));
    let colormap = spektrum_core::Colormap::new(&app.settings.colormap, stops);
    app.prog.colormap_lut = Arc::new(colormap.build_lut_rgba(256));
}

fn apply_profile(app: &mut App, name: &str) {
    let Ok(profile) = profiles::resolve_profile(name) else { return; };
    let mut spectrum = profile.dsp.clone();
    if let Some(sample_rate) = app.args.sample_rate { spectrum.sample_rate = sample_rate; }
    app.spectrum = spectrum;
    app.prog.min_history = profile.history.unwrap_or(app.prog.min_history).max(1);
    app.prog.bins = spectrum_output_bins(&app.spectrum) as u32;
    app.prog.contrast = app.args.contrast.unwrap_or(profile.colors.contrast);
    app.prog.saturation = app.args.saturation.unwrap_or(profile.colors.saturation);
    let colormap_name = app.args.colormap.clone().unwrap_or(profile.colors.colormap);
    apply_colormap(app, &colormap_name);
    app.settings.profile = name.to_string();
    app.settings.dsp_settings = "custom".to_string();
    app.settings.contrast = app.prog.contrast;
    app.settings.saturation = app.prog.saturation;
    app.settings.from_spectrum(&app.spectrum, app.prog.min_history as f32);
    let overlay = app.args.overlay.clone().unwrap_or(profile.colors.overlay);
    apply_overlay(app, &overlay);
    if app.args.target_object.is_none() {
        if let Some(source) = profile.audio.source {
            app.capture_tx.send(source.clone()).ok();
            app.settings.source = source;
        }
    }
    restart_dsp(app);
}

fn refresh_libraries(app: &mut App) {
    app.profiles = profiles::list_profile_names();
    app.dsp_settings = list_dsp_settings_names();
    app.colormaps = spektrum_core::all_colormap_names();
}

fn current_profile(app: &App) -> profiles::Profile {
    profiles::Profile {
        dsp: app.spectrum.clone(),
        colors: profiles::ColorSettings {
            colormap: app.settings.colormap.clone(),
            contrast: app.settings.contrast,
            saturation: app.settings.saturation,
            overlay: app.settings.overlay.clone(),
        },
        audio: profiles::AudioSettings { source: Some(app.settings.source.clone()) },
        image: Some(profiles::ProfileImage { width: 800, height: 800, scroll_right_to_left: app.prog.dev.scroll_right_to_left }),
        history: Some(app.prog.min_history),
    }
}

fn apply_advanced(app: &mut App, field: DspSlider) {
    app.settings.dsp_settings = "custom".to_string();
    if field == DspSlider::History {
        app.prog.min_history = app.settings.history.max(1.0) as u32;
        app.restart_tx.send(DspCommand::SetHistory(app.prog.min_history)).ok();
        return;
    }
    match field {
        DspSlider::WindowSize => app.spectrum.window_size = app.settings.advanced.window_size,
        DspSlider::HopSize => app.spectrum.hop_size = app.settings.advanced.hop_size,
        DspSlider::LogBins => app.spectrum.log_bins = app.settings.advanced.log_bins,
        DspSlider::FMin => app.spectrum.f_min_hz = app.settings.advanced.f_min_hz,
        DspSlider::FMax => app.spectrum.f_max_hz = app.settings.advanced.f_max_hz,
        DspSlider::DbFloor => app.spectrum.db_floor = app.settings.advanced.db_floor,
        DspSlider::DbCeil => app.spectrum.db_ceil = app.settings.advanced.db_ceil,
        DspSlider::Smoothing => app.spectrum.freq_smoothing_sigma = app.settings.advanced.freq_smoothing_sigma,
        DspSlider::Gamma => app.spectrum.amplitude_gamma = app.settings.advanced.amplitude_gamma,
        DspSlider::TemporalAlpha => app.spectrum.temporal_alpha = app.settings.advanced.temporal_alpha,
        DspSlider::PeakDecay => app.spectrum.peak_hold_decay = app.settings.advanced.peak_hold_decay,
        DspSlider::CqtBins => app.spectrum.cqt_bins_per_octave = app.settings.advanced.cqt_bins_per_octave,
        DspSlider::FreqScaleExp => app.spectrum.freq_scale_exp = app.settings.advanced.freq_scale_exp,
        DspSlider::History => {}
    }
    if field.is_runtime() {
        app.restart_tx.send(DspCommand::UpdateRuntime(app.spectrum.clone())).ok();
    } else {
        restart_dsp(app);
    }
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Tick => Task::none(),
        Message::WindowEvent(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))) => {
            let now = Instant::now();
            if app.last_click.is_some_and(|previous| now.duration_since(previous) < Duration::from_millis(400)) {
                app.fullscreen = !app.fullscreen;
                app.last_click = None;
                let mode = if app.fullscreen { iced::window::Mode::Fullscreen } else { iced::window::Mode::Windowed };
                return iced::window::latest().and_then(move |id| iced::window::set_mode(id, mode));
            }
            app.last_click = Some(now);
            Task::none()
        }
        Message::WindowEvent(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))) => {
            app.settings.toggle();
            Task::none()
        }
        Message::WindowEvent(Event::Keyboard(keyboard::Event::KeyPressed { key, .. })) => {
            match key {
                keyboard::Key::Character(value) if value == "m" || value == "M" => app.settings.toggle(),
                keyboard::Key::Named(keyboard::key::Named::Escape) => app.settings.close(),
                keyboard::Key::Named(keyboard::key::Named::Space) => {
                    app.prog.paused = !app.prog.paused;
                    app.prog.pending_spectra.lock().unwrap().clear();
                    app.restart_tx.send(DspCommand::SetPaused(app.prog.paused)).ok();
                }
                _ => {}
            }
            Task::none()
        }
        Message::WindowEvent(_) => Task::none(),
        Message::Settings(message) => {
            match message {
                SettingsMessage::Toggle => app.settings.toggle(),
                SettingsMessage::Close => app.settings.close(),
                SettingsMessage::SetContrast(value) => { app.prog.contrast = value; app.settings.contrast = value; }
                SettingsMessage::SetSaturation(value) => { app.prog.saturation = value; app.settings.saturation = value; }
                SettingsMessage::SetColormap(name) => apply_colormap(app, &name),
                SettingsMessage::SetProfile(name) => apply_profile(app, &name),
                SettingsMessage::SetDspSettings(name) => apply_dsp_settings(app, &name),
                SettingsMessage::SetOverlay(name) => apply_overlay(app, &name),
                SettingsMessage::SetSource(source) => {
                    app.capture_tx.send(source.clone()).ok();
                    app.settings.source = source;
                }
                SettingsMessage::OpenManager(manager) => {
                    app.settings.library_name.clear();
                    if matches!(manager, spektrum::settings::LibraryManager::Colormaps) {
                        app.settings.colormap_stops = resolve_colormap(&app.settings.colormap).map(|map| map.stops().to_vec()).unwrap_or_default();
                    }
                    app.settings.manager = Some(manager);
                }
                SettingsMessage::CloseManager => app.settings.manager = None,
                SettingsMessage::SetLibraryName(name) => app.settings.library_name = name,
                SettingsMessage::SaveProfile => {
                    let name = if app.settings.library_name.trim().is_empty() { app.settings.profile.clone() } else { app.settings.library_name.trim().to_string() };
                    if profiles::save_user_profile(&name, &current_profile(app)).is_ok() {
                        app.settings.profile = name;
                        app.settings.library_name.clear();
                        refresh_libraries(app);
                    }
                }
                SettingsMessage::DeleteProfile => {
                    if profiles::delete_user_profile(&app.settings.profile).is_ok() {
                        app.settings.profile = "default".to_string();
                        refresh_libraries(app);
                    }
                }
                SettingsMessage::SaveDspSettings => {
                    let name = if app.settings.library_name.trim().is_empty() { app.settings.dsp_settings.clone() } else { app.settings.library_name.trim().to_string() };
                    if profiles::save_user_dsp_settings(&name, &app.spectrum).is_ok() {
                        app.settings.dsp_settings = name;
                        app.settings.library_name.clear();
                        refresh_libraries(app);
                    }
                }
                SettingsMessage::DeleteDspSettings => {
                    if profiles::delete_user_dsp_settings(&app.settings.dsp_settings).is_ok() {
                        app.settings.dsp_settings = "custom".to_string();
                        refresh_libraries(app);
                    }
                }
                SettingsMessage::SaveColormap => {
                    let name = if app.settings.library_name.trim().is_empty() { app.settings.colormap.clone() } else { app.settings.library_name.trim().to_string() };
                    let colormap = if app.settings.colormap_stops.len() >= 2 {
                        spektrum_core::Colormap::new(&name, app.settings.colormap_stops.clone())
                    } else if let Ok(colormap) = resolve_colormap(&app.settings.colormap) {
                        colormap
                    } else {
                        return Task::none();
                    };
                    if spektrum_core::colormap::save_user_colormap(&name, &colormap).is_ok() {
                        apply_colormap(app, &name);
                        app.settings.library_name.clear();
                        refresh_libraries(app);
                    }
                }
                SettingsMessage::DeleteColormap => {
                    if spektrum_core::colormap::delete_user_colormap(&app.settings.colormap).is_ok() {
                        apply_colormap(app, "magma");
                        refresh_libraries(app);
                    }
                }
                SettingsMessage::SetColorStop(index, component, value) => {
                    if let Some(stop) = app.settings.colormap_stops.get_mut(index) {
                        match component {
                            0 => stop.0 = value.clamp(0.0, 1.0),
                            1 => stop.1 = value.clamp(0.0, 1.0),
                            2 => stop.2 = value.clamp(0.0, 1.0),
                            3 => stop.3 = value.clamp(0.0, 1.0),
                            _ => {}
                        }
                        apply_edited_colormap(app);
                    }
                }
                SettingsMessage::AddColorStop => {
                    app.settings.colormap_stops.push((0.5, 0.5, 0.5, 0.5));
                    app.settings.colormap_stops.sort_by(|left, right| left.0.total_cmp(&right.0));
                    apply_edited_colormap(app);
                }
                SettingsMessage::DeleteColorStop(index) => {
                    if app.settings.colormap_stops.len() > 2 && index < app.settings.colormap_stops.len() {
                        app.settings.colormap_stops.remove(index);
                        apply_edited_colormap(app);
                    }
                }
                SettingsMessage::AdvancedSlider(field, value) => app.settings.set(field, value),
                SettingsMessage::AdvancedSliderRelease(field) => apply_advanced(app, field),
                SettingsMessage::SetWindowFn(value) => {
                    app.settings.advanced.window_fn = value;
                    app.spectrum.window_fn = value;
                    app.settings.dsp_settings = "custom".to_string();
                    restart_dsp(app);
                }
                SettingsMessage::SetBandAggregation(value) => {
                    app.settings.advanced.band_aggregation = value;
                    app.spectrum.band_aggregation = value;
                    app.settings.dsp_settings = "custom".to_string();
                    restart_dsp(app);
                }
                SettingsMessage::SetWeighting(value) => {
                    app.settings.advanced.weighting = value;
                    app.spectrum.weighting = value;
                    app.settings.dsp_settings = "custom".to_string();
                    restart_dsp(app);
                }
                SettingsMessage::SetTransform(value) => {
                    app.settings.advanced.transform = value;
                    app.spectrum.transform = value;
                    app.settings.dsp_settings = "custom".to_string();
                    restart_dsp(app);
                }
                SettingsMessage::SetCentered(value) => {
                    app.settings.advanced.centered = value;
                    app.spectrum.centered = value;
                    app.settings.dsp_settings = "custom".to_string();
                    restart_dsp(app);
                }
            }
            Task::none()
        }
    }
}

fn view(app: &App) -> Element<'_, Message> {
    let spectrogram = container(Shader::new(app.prog.clone()).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill);
    if !app.settings.open { return spectrogram.into(); }
    let menu = app.settings.view(&app.colormaps, &app.profiles, &app.dsp_settings, &app.overlays, &app.sources, app.prog.paused)
        .map(Message::Settings);
    let panel: Element<'_, Message> = container(menu)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(if app.prog.dev.scroll_right_to_left { iced::alignment::Horizontal::Left } else { iced::alignment::Horizontal::Right })
        .into();
    stack![spectrogram, panel].into()
}

fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick),
        iced::event::listen().map(Message::WindowEvent),
    ])
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let profile = args.config.as_ref().and_then(|path| profiles::load_profile(path).ok())
        .or_else(|| args.profile.as_deref().and_then(|name| profiles::resolve_profile(name).ok()))
        .unwrap_or_else(|| profiles::builtin_profile("default").unwrap());
    let image = profile.image.as_ref();
    let size = Size::new(
        args.width.unwrap_or(image.map_or(800, |config| config.width)) as f32,
        args.height.unwrap_or(image.map_or(800, |config| config.height)) as f32,
    );
    let icon = load_window_icon();
    iced::application(move || App::bootstrap(args.clone()), update, view)
        .title("vividspektrum")
        .window(iced::window::Settings {
            icon,
            ..Default::default()
        })
        .window_size(size)
        .centered()
        .subscription(subscription)
        .theme(iced::Theme::Dark)
        .run()
        .map_err(|e| anyhow::anyhow!("{e:?}"))
}

fn load_window_icon() -> Option<iced::window::Icon> {
    let png = include_bytes!("../../favicon-256.png");
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    iced::window::icon::from_rgba(img.into_raw(), w, h).ok()
}
