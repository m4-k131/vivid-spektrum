use crate::Args;
use spektrum::dev::{effective_spectrogram_history, SpectrogramDevConfig};
use spektrum::settings::{DspSlider, SettingsMessage, SettingsState};
use spektrum::source::{spawn_dsp_thread, DspCommand, SourceSlot};
use spektrum_core::{overlay, profiles, resolve_colormap, spectrum_output_bins, SampleRing, SpectrumConfig};
use iced::keyboard;
use iced::mouse;
use iced::widget::{container, stack};
use iced::widget::shader::Shader;
use iced::{Element, Event, Length, Size, Subscription, Task};
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_SOURCES: usize = 4;

#[derive(Debug, Clone)]
enum Message {
    Tick,
    WindowEvent(Event),
    Settings(SettingsMessage),
}

pub struct App {
    sources: Vec<SourceSlot>,
    settings: SettingsState,
    args: Args,
    spectrum: SpectrumConfig,
    fullscreen: bool,
    last_click: Option<Instant>,
    colormaps: Vec<String>,
    profiles: Vec<String>,
    dsp_settings: Vec<String>,
    overlays: Vec<String>,
    source_list: Vec<String>,
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
        let img = profile.image.as_ref();
        let rtl = if args.legacy_vertical_scroll { false } else { img.map_or(true, |i| i.scroll_right_to_left) };

        let history = effective_spectrogram_history(args.history);
        let capture_target = args.target_object.clone().or_else(|| profile.audio.source.clone());
        let source_list = spektrum_core::cpal::output_device_names();
        let source = capture_target.clone().unwrap_or_else(|| "default output".to_string());

        let dev = SpectrogramDevConfig { scroll_right_to_left: rtl };
        let debug_profile = args.debug_profile;

        let (mut slot, capture_tx) = create_source_slot(
            0,
            source.clone(),
            &spectrum,
            colormap_name,
            contrast,
            saturation,
            1.0,
            history,
            dev,
            debug_profile,
        );

        let overlay_name = args.overlay.clone().unwrap_or_else(|| profile.colors.overlay.clone());
        let (overlay_lines, overlay_color, overlay_opacity, overlay_thickness) = compute_overlay(&overlay_name, &spectrum, 0);
        slot.prog.overlay_lines = overlay_lines;
        slot.prog.overlay_color = overlay_color;
        slot.prog.overlay_opacity = overlay_opacity;
        slot.prog.overlay_thickness = overlay_thickness;

        let mut settings = SettingsState::new(
            true, contrast, saturation, colormap_name, profile_name, "default",
            overlay_name.clone(), source, &spectrum, history as f32,
        );
        settings.source_labels = vec!["Source 1".to_string()];

        Self {
            sources: vec![slot],
            settings,
            args,
            spectrum,
            fullscreen: false,
            last_click: None,
            colormaps: spektrum_core::all_colormap_names(),
            profiles: profiles::list_profile_names(),
            dsp_settings: list_dsp_settings_names(),
            overlays: list_overlay_names(),
            source_list,
        }
    }
}

fn create_source_slot(
    id: usize,
    target: String,
    spectrum: &SpectrumConfig,
    colormap_name: &str,
    contrast: f32,
    saturation: f32,
    opacity: f32,
    history: u32,
    dev: SpectrogramDevConfig,
    debug_profile: bool,
) -> (SourceSlot, mpsc::Sender<String>) {
    let colormap = resolve_colormap(colormap_name).unwrap_or_else(|error| {
        eprintln!("{error}; using default colormap");
        spektrum_core::default_colormap()
    });
    let colormap_lut = Arc::new(colormap.build_lut_rgba(256));

    let pending_spectra = Arc::new(Mutex::new(VecDeque::new()));
    let pending_w = pending_spectra.clone();
    let ring = SampleRing::new((spectrum.sample_rate as usize) * 2);
    let ring_clone = ring.clone();
    let (_audio, capture_tx) = spektrum_core::cpal::spawn_capture(Some(target.clone()), ring_clone);

    let (restart_tx, restart_rx) = mpsc::channel::<DspCommand>();
    let initial_cfg = spectrum.clone();
    let ring = std::sync::Mutex::new(ring);
    spawn_dsp_thread(
        pending_w,
        restart_rx,
        initial_cfg,
        history,
        debug_profile,
        move |scratch| {
            let ring = ring.lock().unwrap();
            ring.pop_into(scratch)
        },
    );

    let label = format!("Source {}", id + 1);
    let prog = spektrum::spectrogram::SpectrogramProgram {
        pending_spectra,
        bins: spectrum_output_bins(spectrum) as u32,
        min_history: history,
        paused: false,
        dev,
        colormap_lut,
        contrast,
        saturation,
        debug_profile,
        overlay_lines: Vec::new(),
        overlay_color: [0.9, 0.9, 0.9],
        overlay_opacity: 0.0,
        overlay_thickness: 0.003,
        opacity,
    };

    let slot = SourceSlot {
        id,
        label,
        target,
        pending_spectra: prog.pending_spectra.clone(),
        restart_tx,
        capture_tx: Some(capture_tx),
        prog,
        opacity,
        colormap_name: colormap_name.to_string(),
    };

    (slot, slot.capture_tx.clone().unwrap())
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

fn compute_overlay(name: &str, spectrum: &SpectrumConfig, semitone_shift: i32) -> (Vec<f32>, [f32; 3], f32, f32) {
    if name == "none" || name.is_empty() { return (Vec::new(), [0.9, 0.9, 0.9], 0.0, 0.003); }
    let Some(config) = overlay::load_overlay(name) else { return (Vec::new(), [0.9, 0.9, 0.9], 0.0, 0.003); };
    let shift_ratio = 2.0f32.powf(semitone_shift as f32 / 12.0);
    let f_min = spectrum.f_min_hz;
    let f_max = spectrum.f_max_hz;
    let range = (f_max / f_min).ln();
    let exponent = spectrum.freq_scale_exp.max(0.1);
    let lines = config.lines.iter()
        .map(|line| line.freq * shift_ratio)
        .filter(|freq| *freq >= f_min && *freq <= f_max)
        .map(|freq| 1.0 - ((freq / f_min).ln() / range).powf(1.0 / exponent))
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
    app.settings.semitone_shift = 0;
    if name == "none" || name.is_empty() {
        for slot in app.sources.iter_mut() {
            slot.prog.overlay_lines.clear();
            slot.prog.overlay_opacity = 0.0;
        }
        return;
    }
    let (lines, color, opacity, thickness) = compute_overlay(name, &app.spectrum, app.settings.semitone_shift);
    for slot in app.sources.iter_mut() {
        slot.prog.overlay_lines = lines.clone();
        slot.prog.overlay_color = color;
        slot.prog.overlay_opacity = opacity;
        slot.prog.overlay_thickness = thickness;
    }
}

fn restart_dsp(app: &mut App) {
    let history = app.sources.first().map_or(1, |s| s.prog.min_history);
    for slot in &app.sources {
        slot.restart_dsp(&app.spectrum, history);
    }
    for slot in app.sources.iter_mut() {
        slot.prog.bins = spectrum_output_bins(&app.spectrum) as u32;
    }
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
    let history = app.sources.first().map_or(1, |s| s.prog.min_history);
    app.settings.from_spectrum(&app.spectrum, history as f32);
    app.settings.dsp_settings = name.to_string();
    restart_dsp(app);
}

fn apply_colormap(app: &mut App, name: &str) {
    if let Ok(cm) = resolve_colormap(name) {
        let lut = Arc::new(cm.build_lut_rgba(256));
        let active = app.settings.active_source;
        if let Some(slot) = app.sources.get_mut(active) {
            slot.update_colormap(lut, name);
        }
        app.settings.colormap = name.to_string();
    }
}

fn apply_edited_colormap(app: &mut App) {
    if app.settings.colormap_stops.len() < 2 { return; }
    let mut stops = app.settings.colormap_stops.clone();
    stops.sort_by(|left, right| left.0.total_cmp(&right.0));
    let colormap = spektrum_core::Colormap::new(&app.settings.colormap, stops);
    let lut = Arc::new(colormap.build_lut_rgba(256));
    let active = app.settings.active_source;
    if let Some(slot) = app.sources.get_mut(active) {
        slot.prog.colormap_lut = lut;
    }
}

fn apply_profile(app: &mut App, name: &str) {
    let Ok(profile) = profiles::resolve_profile(name) else { return; };
    let mut spectrum = profile.dsp.clone();
    if let Some(sample_rate) = app.args.sample_rate { spectrum.sample_rate = sample_rate; }
    let history = profile.history.unwrap_or(app.sources.first().map_or(1, |s| s.prog.min_history)).max(1);
    let dev = app.sources.first().map_or(SpectrogramDevConfig::default(), |s| s.prog.dev);
    let debug_profile = app.sources.first().map_or(app.args.debug_profile, |s| s.prog.debug_profile);

    if !profile.sources.is_empty() {
        while app.sources.len() > 1 {
            app.sources.pop();
        }
        let default_target = app.source_list.first().cloned().unwrap_or_else(|| "default output".to_string());
        let n = profile.sources.len().min(MAX_SOURCES);
        while app.sources.len() < n {
            let id = app.sources.len();
            let (slot, _tx) = create_source_slot(
                id,
                default_target.clone(),
                &spectrum,
                "magma",
                1.0,
                1.0,
                0.5,
                history,
                dev,
                debug_profile,
            );
            app.sources.push(slot);
        }
        for (i, sc) in profile.sources.iter().take(n).enumerate() {
            let slot = &mut app.sources[i];
            slot.restart_dsp(&spectrum, history);
            slot.prog.bins = spectrum_output_bins(&spectrum) as u32;
            slot.prog.min_history = history;
            slot.prog.contrast = sc.contrast;
            slot.prog.saturation = sc.saturation;
            slot.prog.opacity = sc.opacity;
            if let Ok(cm) = resolve_colormap(&sc.colormap) {
                slot.prog.colormap_lut = Arc::new(cm.build_lut_rgba(256));
                slot.colormap_name = sc.colormap.clone();
            }
            if app.args.target_object.is_none() {
                if let Some(ref src) = sc.source {
                    slot.set_target(src);
                }
            }
        }
        app.settings.active_source = 0;
        app.settings.source_labels = app.sources.iter().map(|s| s.label.clone()).collect();
        let first = &profile.sources[0];
        app.settings.contrast = first.contrast;
        app.settings.saturation = first.saturation;
        app.settings.opacity = first.opacity;
        app.settings.colormap = first.colormap.clone();
    } else {
        let active = app.settings.active_source;
        let contrast = app.args.contrast.unwrap_or(profile.colors.contrast);
        let saturation = app.args.saturation.unwrap_or(profile.colors.saturation);
        let colormap_name = app.args.colormap.clone().unwrap_or(profile.colors.colormap);
        for slot in &app.sources {
            slot.restart_dsp(&spectrum, history);
        }
        for slot in app.sources.iter_mut() {
            slot.prog.bins = spectrum_output_bins(&spectrum) as u32;
            slot.prog.min_history = history;
        }
        if let Some(slot) = app.sources.get_mut(active) {
            slot.prog.contrast = contrast;
            slot.prog.saturation = saturation;
            if let Ok(cm) = resolve_colormap(&colormap_name) {
                slot.prog.colormap_lut = Arc::new(cm.build_lut_rgba(256));
                slot.colormap_name = colormap_name.clone();
            }
        }
        app.settings.contrast = contrast;
        app.settings.saturation = saturation;
        app.settings.colormap = colormap_name;
        if app.args.target_object.is_none() {
            if let Some(source) = profile.audio.source {
                if let Some(slot) = app.sources.get(active) {
                    slot.set_target(&source);
                }
                app.settings.source = source;
            }
        }
    }

    app.spectrum = spectrum;
    app.settings.profile = name.to_string();
    app.settings.dsp_settings = "custom".to_string();
    app.settings.from_spectrum(&app.spectrum, history as f32);
    let overlay = app.args.overlay.clone().unwrap_or(profile.colors.overlay);
    apply_overlay(app, &overlay);
    sync_active_source_settings(app);
}

fn refresh_libraries(app: &mut App) {
    app.profiles = profiles::list_profile_names();
    app.dsp_settings = list_dsp_settings_names();
    app.colormaps = spektrum_core::all_colormap_names();
}

fn current_profile(app: &App) -> profiles::Profile {
    let active = app.settings.active_source;
    let slot = app.sources.get(active);
    let sources: Vec<profiles::SourceConfig> = if app.sources.len() > 1 {
        app.sources.iter().map(|s| profiles::SourceConfig {
            source: Some(s.target.clone()),
            colormap: s.colormap_name.clone(),
            contrast: s.prog.contrast,
            saturation: s.prog.saturation,
            opacity: s.prog.opacity,
        }).collect()
    } else {
        Vec::new()
    };
    profiles::Profile {
        dsp: app.spectrum.clone(),
        colors: profiles::ColorSettings {
            colormap: app.settings.colormap.clone(),
            contrast: app.settings.contrast,
            saturation: app.settings.saturation,
            overlay: app.settings.overlay.clone(),
        },
        audio: profiles::AudioSettings { source: Some(app.settings.source.clone()) },
        image: Some(profiles::ProfileImage { width: 800, height: 800, scroll_right_to_left: slot.map_or(true, |s| s.prog.dev.scroll_right_to_left) }),
        history: slot.map(|s| s.prog.min_history),
        sources,
    }
}

fn apply_advanced(app: &mut App, field: DspSlider) {
    app.settings.dsp_settings = "custom".to_string();
    if field == DspSlider::History {
        let history = app.settings.history.max(1.0) as u32;
        for slot in &mut app.sources {
            slot.prog.min_history = history;
            slot.set_history(history);
        }
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
        for slot in &app.sources {
            slot.update_runtime(&app.spectrum);
        }
    } else {
        restart_dsp(app);
    }
}

fn sync_active_source_settings(app: &mut App) {
    let active = app.settings.active_source;
    if let Some(slot) = app.sources.get(active) {
        app.settings.contrast = slot.prog.contrast;
        app.settings.saturation = slot.prog.saturation;
        app.settings.opacity = slot.opacity;
        app.settings.colormap = slot.colormap_name.clone();
        app.settings.source = slot.target.clone();
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
                    let paused = app.sources.first().map_or(false, |s| s.prog.paused);
                    let new_paused = !paused;
                    for slot in &mut app.sources {
                        slot.prog.paused = new_paused;
                        slot.prog.pending_spectra.lock().unwrap().clear();
                        slot.set_paused(new_paused);
                    }
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
                SettingsMessage::SetContrast(value) => {
                    let active = app.settings.active_source;
                    if let Some(slot) = app.sources.get_mut(active) {
                        slot.update_contrast(value);
                    }
                    app.settings.contrast = value;
                }
                SettingsMessage::SetSaturation(value) => {
                    let active = app.settings.active_source;
                    if let Some(slot) = app.sources.get_mut(active) {
                        slot.update_saturation(value);
                    }
                    app.settings.saturation = value;
                }
                SettingsMessage::SetOpacity(value) => {
                    let active = app.settings.active_source;
                    if let Some(slot) = app.sources.get_mut(active) {
                        slot.update_opacity(value);
                    }
                    app.settings.opacity = value;
                }
                SettingsMessage::SetColormap(name) => apply_colormap(app, &name),
                SettingsMessage::SetProfile(name) => apply_profile(app, &name),
                SettingsMessage::SetDspSettings(name) => apply_dsp_settings(app, &name),
                SettingsMessage::SetOverlay(name) => apply_overlay(app, &name),
                SettingsMessage::OverlayShift(delta) => {
                    app.settings.semitone_shift += delta;
                    if app.settings.overlay != "none" {
                        let (lines, color, opacity, thickness) = compute_overlay(&app.settings.overlay, &app.spectrum, app.settings.semitone_shift);
                        for slot in app.sources.iter_mut() {
                            slot.prog.overlay_lines = lines.clone();
                            slot.prog.overlay_color = color;
                            slot.prog.overlay_opacity = opacity;
                            slot.prog.overlay_thickness = thickness;
                        }
                    }
                }
                SettingsMessage::SetSource(source) => {
                    let active = app.settings.active_source;
                    if let Some(slot) = app.sources.get(active) {
                        slot.set_target(&source);
                    }
                    app.settings.source = source;
                }
                SettingsMessage::AddSource => {
                    if app.sources.len() < MAX_SOURCES {
                        let id = app.sources.len();
                        let dev = app.sources.first().map(|s| s.prog.dev).unwrap_or_default();
                        let history = app.sources.first().map(|s| s.prog.min_history).unwrap_or(1);
                        let debug_profile = app.sources.first().map(|s| s.prog.debug_profile).unwrap_or(false);
                        let default_target = app.source_list.first().cloned().unwrap_or_else(|| "default output".to_string());
                        let (slot, _tx) = create_source_slot(
                            id,
                            default_target,
                            &app.spectrum,
                            "magma",
                            1.0,
                            1.0,
                            0.5,
                            history,
                            dev,
                            debug_profile,
                        );
                        app.sources.push(slot);
                        app.settings.source_labels = app.sources.iter().map(|s| s.label.clone()).collect();
                    }
                }
                SettingsMessage::RemoveSource(idx) => {
                    if app.sources.len() > 1 && idx < app.sources.len() {
                        app.sources.remove(idx);
                        for (i, slot) in app.sources.iter_mut().enumerate() {
                            slot.id = i;
                            slot.label = format!("Source {}", i + 1);
                        }
                        app.settings.source_labels = app.sources.iter().map(|s| s.label.clone()).collect();
                        if app.settings.active_source >= app.sources.len() {
                            app.settings.active_source = app.sources.len() - 1;
                        }
                        sync_active_source_settings(app);
                    }
                }
                SettingsMessage::SelectSource(idx) => {
                    if idx < app.sources.len() {
                        app.settings.active_source = idx;
                        sync_active_source_settings(app);
                    }
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
    let dev = app.sources.first().map(|s| s.prog.dev).unwrap_or_default();
    let debug_profile = app.sources.first().map(|s| s.prog.debug_profile).unwrap_or(false);
    let multi = spektrum::spectrogram::MultiSpectrogramProgram {
        sources: app.sources.iter().map(|s| s.prog.clone()).collect(),
        dev,
        debug_profile,
    };
    let spectrogram = container(Shader::new(multi).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill);
    if !app.settings.open { return spectrogram.into(); }
    let active = app.settings.active_source;
    let paused = app.sources.get(active).map_or(false, |s| s.prog.paused);
    let rtl = app.sources.get(active).map_or(true, |s| s.prog.dev.scroll_right_to_left);
    let menu = app.settings.view(&app.colormaps, &app.profiles, &app.dsp_settings, &app.overlays, &app.source_list, paused)
        .map(Message::Settings);
    let panel: Element<'_, Message> = container(menu)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(if rtl { iced::alignment::Horizontal::Left } else { iced::alignment::Horizontal::Right })
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
