use crate::Args;
use spektrum::dev::{effective_spectrogram_history, SpectrogramDevConfig};
use spektrum::settings::{DspSlider, SettingsMessage, SettingsState};
use spektrum::source::{spawn_dsp_thread, DspCommand, SourceSlot};
use spektrum_core::{overlay, profiles, resolve_colormap, sample_ring_pair, spectrum_output_bins, SpectrumConfig};
use iced::widget::{container, stack};
use iced::widget::shader::Shader;
use iced::{Element, Event, Length, Size, Subscription, Task};
use iced::keyboard;
use iced::mouse;
use std::collections::{HashMap, VecDeque};
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
    source_targets: HashMap<String, String>,
    _pw_handles: Vec<std::thread::JoinHandle<()>>,
}

impl App {
    fn bootstrap(args: Args) -> Self {
        let profile = initial_profile(&args);
        let mut spectrum = profile.dsp.clone();
        apply_spectrum_overrides(&args, &mut spectrum);
        let img = profile.image.as_ref();
        let colormap_name = args.colormap.as_deref()
            .unwrap_or(&profile.colors.colormap);
        let contrast = args.contrast.unwrap_or(profile.colors.contrast);
        let saturation = args.saturation.unwrap_or(profile.colors.saturation);

        let rtl = if args.legacy_vertical_scroll { false } else { img.is_none_or(|i| i.scroll_right_to_left) };
        let history = effective_spectrogram_history(args.history);
        let capture_target = args.target_object.first().cloned()
            .or_else(|| profile.audio.source.clone())
            .or_else(spektrum_core::pipewire::default_pulse_output_monitor)
            .or_else(spektrum_core::pipewire::default_pulse_source);
        let mut source_targets = HashMap::new();
        let mut source_list = Vec::new();
        for (index, target) in spektrum_core::pipewire::pulse_sources().into_iter().enumerate() {
            let label = source_label(index, &target);
            source_targets.insert(label.clone(), target);
            source_list.push(label);
        }
        let source_label_str = capture_target.as_deref()
            .and_then(|target| source_targets.iter().find_map(|(label, value)| (value == target).then(|| label.clone())))
            .unwrap_or_else(|| {
                let target = capture_target.clone().unwrap_or_else(|| "default input".to_string());
                let label = source_label(source_list.len(), &target);
                source_targets.insert(label.clone(), target);
                source_list.push(label.clone());
                label
            });

        let dev = SpectrogramDevConfig { scroll_right_to_left: rtl };
        let debug_profile = args.debug_profile;

        let first_target = capture_target.clone().unwrap_or_else(|| {
            source_targets.get(&source_label_str).cloned().unwrap_or_default()
        });
        let (mut slot, pw_handle) = create_source_slot(
            0,
            first_target,
            &spectrum,
            colormap_name,
            contrast,
            saturation,
            1.0,
            history,
            dev,
            debug_profile,
            &source_targets,
        );
        if let Some(target) = &capture_target {
            let target = target.clone();
            let capture_name = slot.capture_name.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    if spektrum_core::pipewire::move_capture_to_pulse_source(&target, &capture_name).is_ok() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                eprintln!("failed to select startup audio source '{target}'");
            });
        }

        let overlay_name = args.overlay.as_deref().unwrap_or(&profile.colors.overlay);
        let (overlay_lines, overlay_color, overlay_opacity, overlay_thickness) =
            compute_overlay(overlay_name, &spectrum, 0);
        slot.prog.overlay_lines = overlay_lines;
        slot.prog.overlay_color = overlay_color;
        slot.prog.overlay_opacity = overlay_opacity;
        slot.prog.overlay_thickness = overlay_thickness;

        let mut sources = vec![slot];
        let mut pw_handles = vec![pw_handle];

        for (i, extra_target) in args.target_object.iter().skip(1).enumerate() {
            let id = i + 1;
            if id >= MAX_SOURCES { break; }
            let (extra_slot, extra_pw) = create_source_slot(
                id,
                extra_target.clone(),
                &spectrum,
                "magma",
                1.0,
                1.0,
                0.5,
                history,
                dev,
                debug_profile,
                &source_targets,
            );
            sources.push(extra_slot);
            pw_handles.push(extra_pw);
        }

        let profile_name = args.config.as_ref()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .or(args.profile.clone())
            .unwrap_or_else(|| "high_quality".to_string());

        let colormaps = spektrum_core::all_colormap_names();
        let profiles = list_profile_names();
        let dsp_settings = list_dsp_settings_names();
        let overlays = list_overlay_names();

        let mut settings = SettingsState::new(
            true,
            contrast,
            saturation,
            colormap_name,
            profile_name,
            "default",
            overlay_name.to_string(),
            source_label_str,
            &spectrum,
            history as f32,
        );
        settings.source_labels = sources.iter().map(|s| s.label.clone()).collect();

        Self {
            sources,
            settings,
            args,
            spectrum,
            fullscreen: false,
            last_click: None,
            colormaps,
            profiles,
            dsp_settings,
            overlays,
            source_list,
            source_targets,
            _pw_handles: pw_handles,
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
    _source_targets: &HashMap<String, String>,
) -> (SourceSlot, std::thread::JoinHandle<()>) {
    let colormap = resolve_colormap(colormap_name).unwrap_or_else(|error| {
        eprintln!("{error}; using default colormap");
        spektrum_core::default_colormap()
    });
    let colormap_lut = Arc::new(colormap.build_lut_rgba(256));

    let pending_spectra = Arc::new(Mutex::new(VecDeque::new()));
    let pending_w = pending_spectra.clone();
    let (producer, consumer) = sample_ring_pair((spectrum.sample_rate as usize) * 2);
    let capture_name = spektrum_core::pipewire::next_capture_name(id);
    let pw_target = if target.is_empty() { None } else { Some(target.clone()) };
    let pw_handle = spektrum_core::pipewire::spawn_capture_lockfree(pw_target, producer, capture_name.clone());

    let (restart_tx, restart_rx) = mpsc::channel::<DspCommand>();
    let initial_cfg = spectrum.clone();
    let consumer = std::sync::Mutex::new(consumer);
    spawn_dsp_thread(
        pending_w,
        restart_rx,
        initial_cfg,
        history,
        debug_profile,
        move |scratch| {
            let mut consumer = consumer.lock().unwrap();
            consumer.pop_into(scratch)
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
        capture_tx: None,
        prog,
        opacity,
        colormap_name: colormap_name.to_string(),
        capture_name,
    };

    (slot, pw_handle)
}

fn initial_profile(args: &Args) -> profiles::Profile {
    if let Some(path) = &args.config {
        profiles::load_profile(path).expect("failed to load config")
    } else if let Some(name) = &args.profile {
        profiles::resolve_profile(name)
            .unwrap_or_else(|e| panic!("{}. Available: {:?}", e, profiles::list_profile_names()))
    } else {
        profiles::builtin_profile("high_quality").unwrap()
    }
}

fn load_profile_by_name(name: &str) -> Option<profiles::Profile> {
    profiles::resolve_profile(name).ok()
}

fn list_profile_names() -> Vec<String> {
    profiles::list_profile_names()
}

fn list_dsp_settings_names() -> Vec<String> {
    let mut names = profiles::list_dsp_settings_names();
    names.push("custom".to_string());
    names
}

fn source_label(index: usize, source: &str) -> String {
    let kind = if source.ends_with(".monitor") { "Output" } else { "Input" };
    let mut text = source.chars();
    let abbreviated: String = text.by_ref().take(32).collect();
    let suffix = if text.next().is_some() { "…" } else { "" };
    format!("{kind} {} · {abbreviated}{suffix}", index + 1)
}

fn list_overlay_names() -> Vec<String> {
    let mut names = vec!["none".to_string()];
    names.extend(overlay::builtin_overlay_names());
    names
}

fn apply_spectrum_overrides(args: &Args, spectrum: &mut SpectrumConfig) {
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
}

fn compute_overlay(name_or_path: &str, spectrum: &SpectrumConfig, semitone_shift: i32) -> (Vec<f32>, [f32; 3], f32, f32) {
    if name_or_path.is_empty() || name_or_path == "none" {
        return (Vec::new(), [0.9, 0.9, 0.9], 0.0, 0.003);
    }
    let cfg = match overlay::load_overlay(name_or_path) {
        Some(c) => c,
        None => return (Vec::new(), [0.9, 0.9, 0.9], 0.0, 0.003),
    };
    let shift_ratio = 2.0f32.powf(semitone_shift as f32 / 12.0);
    let f_min = spectrum.f_min_hz;
    let f_max = spectrum.f_max_hz;
    let exp = spectrum.freq_scale_exp.max(0.1);
    let log_range = (f_max / f_min).ln();
    let lines: Vec<f32> = cfg.lines.iter()
        .map(|l| l.freq * shift_ratio)
        .filter(|freq| *freq >= f_min && *freq <= f_max)
        .map(|freq| {
            let t = (freq / f_min).ln() / log_range;
            let bin_pos = t.powf(1.0 / exp);
            1.0 - bin_pos
        })
        .collect();
    let color = [cfg.color[0] as f32 / 255.0, cfg.color[1] as f32 / 255.0, cfg.color[2] as f32 / 255.0];
    (lines, color, cfg.opacity, cfg.thickness)
}

fn apply_profile(app: &mut App, name: &str) {
    let Some(profile) = load_profile_by_name(name) else { return; };
    let mut spectrum = profile.dsp.clone();
    apply_spectrum_overrides(&app.args, &mut spectrum);
    spectrum.sample_rate = app.spectrum.sample_rate;

    let img = profile.image.as_ref();
    let rtl = if app.args.legacy_vertical_scroll { false } else { img.is_none_or(|i| i.scroll_right_to_left) };
    let history = profile.history.unwrap_or(app.settings.history.max(1.0) as u32).max(1);
    let dev = SpectrogramDevConfig { scroll_right_to_left: rtl };
    let debug_profile = app.sources.first().map_or(app.args.debug_profile, |s| s.prog.debug_profile);

    if !profile.sources.is_empty() {
        while !app.sources.is_empty() {
            app.sources.pop();
            app._pw_handles.pop();
        }
        let default_output = spektrum_core::pipewire::default_pulse_output_monitor()
            .unwrap_or_default();
        let default_input = spektrum_core::pipewire::default_pulse_source()
            .unwrap_or_default();
        let n = profile.sources.len().min(MAX_SOURCES);
        for (i, sc) in profile.sources.iter().take(n).enumerate() {
            let target = if app.args.target_object.is_empty() {
                match sc.source.as_deref() {
                    Some(s) => s.to_string(),
                    None => {
                        if i == 0 { default_output.clone() }
                        else { default_input.clone() }
                    }
                }
            } else {
                app.args.target_object.get(i).cloned().unwrap_or_default()
            };
            let (slot, pw) = create_source_slot(
                i,
                target.clone(),
                &spectrum,
                &sc.colormap,
                sc.contrast,
                sc.saturation,
                sc.opacity,
                history,
                dev,
                debug_profile,
                &app.source_targets,
            );

            if !target.is_empty() {
                let capture_name = slot.capture_name.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        if spektrum_core::pipewire::move_capture_to_pulse_source(&target, &capture_name).is_ok() {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    eprintln!("failed to select audio source '{target}' for source {i}");
                });
            }

            app.sources.push(slot);
            app._pw_handles.push(pw);
        }
        app.settings.active_source = 0;
        app.settings.source_labels = app.sources.iter().map(|s| s.label.clone()).collect();
        let first = &profile.sources[0];
        app.settings.contrast = first.contrast;
        app.settings.saturation = first.saturation;
        app.settings.opacity = first.opacity;
        app.settings.colormap = first.colormap.clone();
    } else {
        while app.sources.len() > 1 {
            app.sources.pop();
            app._pw_handles.pop();
        }
        let colormap_name = app.args.colormap.as_deref()
            .unwrap_or(&profile.colors.colormap);
        let contrast = app.args.contrast.unwrap_or(profile.colors.contrast);
        let saturation = app.args.saturation.unwrap_or(profile.colors.saturation);
        let colormap = resolve_colormap(colormap_name).unwrap_or_else(|error| {
            eprintln!("{error}; using default colormap");
            spektrum_core::default_colormap()
        });

        for slot in &app.sources {
            slot.restart_dsp(&spectrum, history);
        }
        for slot in app.sources.iter_mut() {
            slot.prog.bins = spectrum_output_bins(&spectrum) as u32;
            slot.prog.min_history = history;
            slot.prog.dev.scroll_right_to_left = rtl;
        }
        let active = app.settings.active_source.min(app.sources.len() - 1);
        app.settings.active_source = active;
        if let Some(slot) = app.sources.get_mut(active) {
            slot.prog.contrast = contrast;
            slot.prog.saturation = saturation;
            slot.prog.colormap_lut = Arc::new(colormap.build_lut_rgba(256));
            slot.colormap_name = colormap_name.to_string();
        }
        app.settings.contrast = contrast;
        app.settings.saturation = saturation;
        app.settings.colormap = colormap_name.to_string();
        app.settings.source_labels = app.sources.iter().map(|s| s.label.clone()).collect();
        if app.args.target_object.is_empty() {
            if let Some(source) = profile.audio.source.as_deref() {
                move_capture_to_source(app, source, active);
            }
        }
    }

    app.spectrum = spectrum;
    app.settings.profile = name.to_string();
    app.settings.dsp_settings = "custom".to_string();
    app.settings.from_spectrum(&app.spectrum, history as f32);
    let overlay_name = app.args.overlay.clone().unwrap_or_else(|| profile.colors.overlay.clone());
    apply_overlay(app, &overlay_name);
    sync_active_source_settings(app);
}

fn move_capture_to_source(app: &mut App, target: &str, source_index: usize) {
    let capture_name = app.sources.get(source_index).map(|s| s.capture_name.clone());
    let Some(capture_name) = capture_name else { return; };
    match spektrum_core::pipewire::move_capture_to_pulse_source(target, &capture_name) {
        Ok(()) => {
            let source = app.source_targets.iter()
                .find_map(|(label, value)| (value == target).then(|| label.clone()))
                .unwrap_or_else(|| {
                    let label = source_label(app.source_list.len(), target);
                    app.source_targets.insert(label.clone(), target.to_string());
                    app.source_list.push(label.clone());
                    label
                });
            app.settings.source = source;
            if let Some(slot) = app.sources.get_mut(source_index) {
                slot.target = target.to_string();
            }
        }
        Err(e) => eprintln!("failed to change audio source: {e}"),
    }
}

fn apply_overlay(app: &mut App, name: &str) {
    app.settings.overlay = name.to_string();
    app.settings.semitone_shift = 0;
    if name == "none" {
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

fn current_profile(app: &App) -> profiles::Profile {
    let active = app.settings.active_source;
    let slot = app.sources.get(active);
    let sources: Vec<profiles::SourceConfig> = if app.sources.len() > 1 {
        app.sources.iter().map(|s| profiles::SourceConfig {
            source: app.source_targets.get(&s.target).cloned(),
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
        audio: profiles::AudioSettings {
            source: app.source_targets.get(&app.settings.source).cloned(),
        },
        image: Some(profiles::ProfileImage {
            width: 800,
            height: 800,
            scroll_right_to_left: slot.map_or(true, |s| s.prog.dev.scroll_right_to_left),
        }),
        history: slot.map(|s| s.prog.min_history),
        sources,
    }
}

fn refresh_libraries(app: &mut App) {
    app.profiles = list_profile_names();
    app.dsp_settings = list_dsp_settings_names();
    app.colormaps = spektrum_core::all_colormap_names();
}

fn apply_dsp_settings(app: &mut App, name: &str) {
    if name == "custom" {
        app.settings.dsp_settings = name.to_string();
        return;
    }
    let Ok(mut spectrum) = profiles::resolve_dsp_settings(name) else { return; };
    apply_spectrum_overrides(&app.args, &mut spectrum);
    spectrum.sample_rate = app.spectrum.sample_rate;
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
    if app.settings.colormap_stops.len() < 2 {
        return;
    }
    let mut stops = app.settings.colormap_stops.clone();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let colormap = spektrum_core::Colormap::new(&app.settings.colormap, stops);
    let lut = Arc::new(colormap.build_lut_rgba(256));
    let active = app.settings.active_source;
    if let Some(slot) = app.sources.get_mut(active) {
        slot.prog.colormap_lut = lut;
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

    let mut new = app.spectrum.clone();
    copy_spectrum_field(&mut new, &app.settings.advanced, field);
    app.spectrum = new;

    if field.is_runtime() {
        for slot in &app.sources {
            slot.update_runtime(&app.spectrum);
        }
    } else {
        let history = app.sources.first().map_or(1, |s| s.prog.min_history);
        for slot in &mut app.sources {
            slot.restart_dsp(&app.spectrum, history);
            slot.prog.bins = spectrum_output_bins(&app.spectrum) as u32;
        }
    }

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

fn copy_spectrum_field(dst: &mut SpectrumConfig, src: &SpectrumConfig, field: DspSlider) {
    match field {
        DspSlider::WindowSize => dst.window_size = src.window_size,
        DspSlider::HopSize => dst.hop_size = src.hop_size,
        DspSlider::LogBins => dst.log_bins = src.log_bins,
        DspSlider::FMin => dst.f_min_hz = src.f_min_hz,
        DspSlider::FMax => dst.f_max_hz = src.f_max_hz,
        DspSlider::DbFloor => dst.db_floor = src.db_floor,
        DspSlider::DbCeil => dst.db_ceil = src.db_ceil,
        DspSlider::Smoothing => dst.freq_smoothing_sigma = src.freq_smoothing_sigma,
        DspSlider::Gamma => dst.amplitude_gamma = src.amplitude_gamma,
        DspSlider::TemporalAlpha => dst.temporal_alpha = src.temporal_alpha,
        DspSlider::PeakDecay => dst.peak_hold_decay = src.peak_hold_decay,
        DspSlider::CqtBins => dst.cqt_bins_per_octave = src.cqt_bins_per_octave,
        DspSlider::FreqScaleExp => dst.freq_scale_exp = src.freq_scale_exp,
        DspSlider::History => {}
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

fn sync_active_source_settings(app: &mut App) {
    let active = app.settings.active_source;
    if let Some(slot) = app.sources.get(active) {
        app.settings.contrast = slot.prog.contrast;
        app.settings.saturation = slot.prog.saturation;
        app.settings.opacity = slot.opacity;
        app.settings.colormap = slot.colormap_name.clone();
        let target_label = app.source_targets.iter()
            .find_map(|(label, value)| (value == &slot.target).then(|| label.clone()))
            .unwrap_or_else(|| slot.target.clone());
        app.settings.source = target_label;
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
        Message::WindowEvent(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))) => {
            app.settings.toggle();
            Task::none()
        }
        Message::WindowEvent(Event::Keyboard(keyboard::Event::KeyPressed { key, .. })) => {
            match key {
                keyboard::Key::Character(c) if c == "m" || c == "M" => {
                    app.settings.toggle();
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    app.settings.close();
                }
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
        Message::Settings(msg) => {
            match msg {
                SettingsMessage::Toggle => app.settings.toggle(),
                SettingsMessage::Close => app.settings.close(),
                SettingsMessage::SetContrast(v) => {
                    let active = app.settings.active_source;
                    if let Some(slot) = app.sources.get_mut(active) {
                        slot.update_contrast(v);
                    }
                    app.settings.contrast = v;
                }
                SettingsMessage::SetSaturation(v) => {
                    let active = app.settings.active_source;
                    if let Some(slot) = app.sources.get_mut(active) {
                        slot.update_saturation(v);
                    }
                    app.settings.saturation = v;
                }
                SettingsMessage::SetOpacity(v) => {
                    let active = app.settings.active_source;
                    if let Some(slot) = app.sources.get_mut(active) {
                        slot.update_opacity(v);
                    }
                    app.settings.opacity = v;
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
                SettingsMessage::OpenManager(manager) => {
                    app.settings.library_name.clear();
                    if matches!(manager, spektrum::settings::LibraryManager::Colormaps) {
                        app.settings.colormap_stops = resolve_colormap(&app.settings.colormap)
                            .map(|map| map.stops().to_vec())
                            .unwrap_or_default();
                    }
                    app.settings.manager = Some(manager);
                }
                SettingsMessage::CloseManager => { app.settings.manager = None; app.settings.error_msg = None; }
                SettingsMessage::SetLibraryName(name) => { app.settings.library_name = name; app.settings.error_msg = None; }
                SettingsMessage::SaveProfile => {
                    let name = if app.settings.library_name.trim().is_empty() {
                        app.settings.profile.clone()
                    } else {
                        app.settings.library_name.trim().to_string()
                    };
                    if let Err(e) = profiles::save_user_profile(&name, &current_profile(app)) {
                        app.settings.error_msg = Some(format!("{e}"));
                    } else {
                        app.settings.profile = name;
                        app.settings.library_name.clear();
                        refresh_libraries(app);
                    }
                }
                SettingsMessage::DeleteProfile => {
                    let name = app.settings.profile.clone();
                    match profiles::delete_user_profile(&name) {
                        Ok(()) => {
                            app.settings.profile = "high_quality".to_string();
                            refresh_libraries(app);
                        }
                        Err(e) => eprintln!("failed to delete profile: {e}"),
                    }
                }
                SettingsMessage::SaveDspSettings => {
                    let name = if app.settings.library_name.trim().is_empty() {
                        app.settings.dsp_settings.clone()
                    } else {
                        app.settings.library_name.trim().to_string()
                    };
                    if let Err(e) = profiles::save_user_dsp_settings(&name, &app.spectrum) {
                        app.settings.error_msg = Some(format!("{e}"));
                    } else {
                        app.settings.dsp_settings = name;
                        app.settings.library_name.clear();
                        refresh_libraries(app);
                    }
                }
                SettingsMessage::DeleteDspSettings => {
                    let name = app.settings.dsp_settings.clone();
                    match profiles::delete_user_dsp_settings(&name) {
                        Ok(()) => {
                            app.settings.dsp_settings = "custom".to_string();
                            refresh_libraries(app);
                        }
                        Err(e) => eprintln!("failed to delete DSP settings: {e}"),
                    }
                }
                SettingsMessage::SaveColormap => {
                    let name = if app.settings.library_name.trim().is_empty() {
                        app.settings.colormap.clone()
                    } else {
                        app.settings.library_name.trim().to_string()
                    };
                    let colormap = if app.settings.colormap_stops.len() >= 2 {
                        spektrum_core::Colormap::new(&name, app.settings.colormap_stops.clone())
                    } else {
                        match resolve_colormap(&app.settings.colormap) {
                            Ok(colormap) => colormap,
                            Err(e) => {
                                eprintln!("failed to load colormap: {e}");
                                return Task::none();
                            }
                        }
                    };
                    match spektrum_core::colormap::save_user_colormap(&name, &colormap) {
                            Ok(()) => {
                                apply_colormap(app, &name);
                                app.settings.library_name.clear();
                                refresh_libraries(app);
                            }
                            Err(e) => app.settings.error_msg = Some(format!("{e}")),
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
                    app.settings.colormap_stops.sort_by(|a, b| a.0.total_cmp(&b.0));
                    apply_edited_colormap(app);
                }
                SettingsMessage::DeleteColorStop(index) => {
                    if app.settings.colormap_stops.len() > 2 && index < app.settings.colormap_stops.len() {
                        app.settings.colormap_stops.remove(index);
                        apply_edited_colormap(app);
                    }
                }
                SettingsMessage::DeleteColormap => {
                    let name = app.settings.colormap.clone();
                    match spektrum_core::colormap::delete_user_colormap(&name) {
                        Ok(()) => {
                            apply_colormap(app, "magma");
                            refresh_libraries(app);
                        }
                        Err(e) => eprintln!("failed to delete colormap: {e}"),
                    }
                }
                SettingsMessage::SetSource(source) => {
                    let Some(target) = app.source_targets.get(&source).cloned() else {
                        return Task::none();
                    };
                    let active = app.settings.active_source;
                    move_capture_to_source(app, &target, active);
                }
                SettingsMessage::AddSource => {
                    if app.sources.len() < MAX_SOURCES {
                        let id = app.sources.len();
                        let dev = app.sources.first().map(|s| s.prog.dev).unwrap_or_default();
                        let history = app.sources.first().map(|s| s.prog.min_history).unwrap_or(1);
                        let debug_profile = app.sources.first().map(|s| s.prog.debug_profile).unwrap_or(false);
                        let default_target = spektrum_core::pipewire::default_pulse_source()
                            .unwrap_or_else(|| app.source_targets.values().next().cloned().unwrap_or_default());
                        let (slot, pw) = create_source_slot(
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
                            &app.source_targets,
                        );
                        app.sources.push(slot);
                        app._pw_handles.push(pw);
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
                SettingsMessage::AdvancedSlider(field, value) => app.settings.set(field, value),
                SettingsMessage::AdvancedSliderRelease(field) => apply_advanced(app, field),
                SettingsMessage::SetWindowFn(w) => {
                    app.settings.dsp_settings = "custom".to_string();
                    app.settings.advanced.window_fn = w;
                    app.spectrum.window_fn = w;
                    restart_dsp(app);
                }
                SettingsMessage::SetBandAggregation(a) => {
                    app.settings.dsp_settings = "custom".to_string();
                    app.settings.advanced.band_aggregation = a;
                    app.spectrum.band_aggregation = a;
                    restart_dsp(app);
                }
                SettingsMessage::SetWeighting(w) => {
                    app.settings.dsp_settings = "custom".to_string();
                    app.settings.advanced.weighting = w;
                    app.spectrum.weighting = w;
                    restart_dsp(app);
                }
                SettingsMessage::SetTransform(t) => {
                    app.settings.dsp_settings = "custom".to_string();
                    app.settings.advanced.transform = t;
                    app.spectrum.transform = t;
                    restart_dsp(app);
                }
                SettingsMessage::SetCentered(c) => {
                    app.settings.dsp_settings = "custom".to_string();
                    app.settings.advanced.centered = c;
                    app.spectrum.centered = c;
                    restart_dsp(app);
                }
                SettingsMessage::SetSharedBg(v) => {
                    app.settings.shared_bg = v;
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
        shared_bg: app.settings.shared_bg,
    };
    let spectrogram = container(
        Shader::new(multi).width(Length::Fill).height(Length::Fill)
    )
    .width(Length::Fill)
    .height(Length::Fill);

    if !app.settings.open {
        return spectrogram.into();
    }

    let active = app.settings.active_source;
    let paused = app.sources.get(active).map_or(false, |s| s.prog.paused);
    let rtl = app.sources.get(active).map_or(true, |s| s.prog.dev.scroll_right_to_left);
    let menu = app.settings.view(&app.colormaps, &app.profiles, &app.dsp_settings, &app.overlays, &app.source_list, paused)
        .map(Message::Settings);
    let panel: Element<'_, Message> = container(menu)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(if rtl {
            iced::alignment::Horizontal::Left
        } else {
            iced::alignment::Horizontal::Right
        })
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
    let profile_for_size = if let Some(path) = &args.config {
        profiles::load_profile(path).ok()
    } else if let Some(name) = &args.profile {
        profiles::builtin_profile(name)
    } else {
        profiles::builtin_profile("high_quality")
    };
    let img = profile_for_size.as_ref().and_then(|p| p.image.as_ref());
    let win_w = args.width.unwrap_or(img.map_or(800, |i| i.width));
    let win_h = args.height.unwrap_or(img.map_or(800, |i| i.height));
    let size = Size::new(win_w as f32, win_h as f32);
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
        .title("vividspektrum")
        .window(iced::window::Settings {
            platform_specific: iced::window::settings::PlatformSpecific {
                application_id: "vividspektrum".to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
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
