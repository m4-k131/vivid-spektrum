//! Shared settings overlay panel used by the live spectrogram window (Linux & Windows).
use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, slider, text, text_input, Space, Tooltip};
use iced::widget::tooltip::Position;
use iced::{Alignment, Element, Length, Theme};
use spektrum_core::{BandAggregation, SpectrumConfig, Transform, Weighting, WindowFunction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspSlider {
    WindowSize,
    HopSize,
    LogBins,
    FMin,
    FMax,
    DbFloor,
    DbCeil,
    Smoothing,
    Gamma,
    TemporalAlpha,
    PeakDecay,
    CqtBins,
    FreqScaleExp,
    History,
}

impl DspSlider {
    pub fn label(self) -> &'static str {
        match self {
            DspSlider::WindowSize => "FFT window size",
            DspSlider::HopSize => "Time step / scroll speed",
            DspSlider::LogBins => "Log frequency bins",
            DspSlider::FMin => "Freq min (Hz)",
            DspSlider::FMax => "Freq max (Hz)",
            DspSlider::DbFloor => "dB floor",
            DspSlider::DbCeil => "dB ceil",
            DspSlider::Smoothing => "Freq smoothing",
            DspSlider::Gamma => "Amplitude gamma",
            DspSlider::TemporalAlpha => "Temporal alpha",
            DspSlider::PeakDecay => "Peak hold decay",
            DspSlider::CqtBins => "CQT bins / octave",
            DspSlider::FreqScaleExp => "Freq scale exp",
            DspSlider::History => "History / buffer",
        }
    }

    pub fn range(self) -> std::ops::RangeInclusive<f32> {
        match self {
            DspSlider::WindowSize => 256.0..=32768.0,
            DspSlider::HopSize => 64.0..=8192.0,
            DspSlider::LogBins => 64.0..=8192.0,
            DspSlider::FMin => 10.0..=2000.0,
            DspSlider::FMax => 2000.0..=24000.0,
            DspSlider::DbFloor => -120.0..=0.0,
            DspSlider::DbCeil => -60.0..=6.0,
            DspSlider::Smoothing => 0.0..=5.0,
            DspSlider::Gamma => 0.0..=2.0,
            DspSlider::TemporalAlpha => 0.0..=1.0,
            DspSlider::PeakDecay => 0.0..=0.999,
            DspSlider::CqtBins => 12.0..=96.0,
            DspSlider::FreqScaleExp => 0.1..=2.0,
            DspSlider::History => 100.0..=10000.0,
        }
    }

    pub fn step(self) -> f32 {
        match self {
            DspSlider::WindowSize => 64.0,
            DspSlider::HopSize => 64.0,
            DspSlider::LogBins => 64.0,
            DspSlider::FMin => 10.0,
            DspSlider::FMax => 100.0,
            DspSlider::DbFloor => 1.0,
            DspSlider::DbCeil => 1.0,
            DspSlider::Smoothing => 0.1,
            DspSlider::Gamma => 0.05,
            DspSlider::TemporalAlpha => 0.05,
            DspSlider::PeakDecay => 0.01,
            DspSlider::CqtBins => 6.0,
            DspSlider::FreqScaleExp => 0.05,
            DspSlider::History => 100.0,
        }
    }

    pub fn is_runtime(self) -> bool {
        matches!(
            self,
            DspSlider::History
                | DspSlider::HopSize
                | DspSlider::DbFloor
                | DspSlider::DbCeil
                | DspSlider::Smoothing
                | DspSlider::Gamma
                | DspSlider::TemporalAlpha
                | DspSlider::PeakDecay
        )
    }

    pub fn needs_restart(self) -> bool {
        !self.is_runtime()
    }

    pub fn tip(self) -> &'static str {
        match self {
            DspSlider::WindowSize => "FFT length in samples. Larger = better frequency resolution but more latency.",
            DspSlider::HopSize => "Samples between spectrogram columns. Smaller = smoother time/scroll.",
            DspSlider::LogBins => "Number of frequency bins on the vertical axis.",
            DspSlider::FMin => "Lowest visible frequency in Hz.",
            DspSlider::FMax => "Highest visible frequency in Hz.",
            DspSlider::DbFloor => "Magnitude mapped to the darkest color (dB).",
            DspSlider::DbCeil => "Magnitude mapped to the brightest color (dB).",
            DspSlider::Smoothing => "Frequency-domain Gaussian smoothing width. 0 = off.",
            DspSlider::Gamma => "Amplitude power curve. <1 brightens quiet parts, >1 darkens them.",
            DspSlider::TemporalAlpha => "Blend factor with previous column. 0 = no temporal smoothing.",
            DspSlider::PeakDecay => "Peak-hold decay per column. 0 = disabled.",
            DspSlider::CqtBins => "Bins per octave when using Constant-Q transform.",
            DspSlider::FreqScaleExp => "Vertical frequency scale non-linearity. <1 stretches low frequencies.",
            DspSlider::History => "Number of spectrogram columns kept in the live buffer.",
        }
    }

    fn is_integer(self) -> bool {
        matches!(
            self,
            DspSlider::WindowSize | DspSlider::HopSize | DspSlider::LogBins | DspSlider::CqtBins | DspSlider::History
        )
    }
}

fn info_icon<'a>(tip: &'a str) -> Element<'a, SettingsMessage> {
    Tooltip::new(
        text("(i)").size(10),
        container(text(tip).size(12))
            .padding(8)
            .style(container::rounded_box),
        Position::Bottom,
    )
    .into()
}

fn label_row<'a>(label: &'a str, tip: &'a str) -> Element<'a, SettingsMessage> {
    row![
        text(label).size(12),
        Space::new().width(Length::Fill),
        info_icon(tip),
    ]
    .align_y(Alignment::Center)
    .into()
}

#[derive(Debug, Clone)]
pub enum LibraryManager {
    Profiles,
    Colormaps,
    DspSettings,
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    Toggle,
    Close,
    SetContrast(f32),
    SetSaturation(f32),
    SetColormap(String),
    SetProfile(String),
    SetDspSettings(String),
    SetOverlay(String),
    OverlayShift(i32),
    SetSource(String),
    OpenManager(LibraryManager),
    CloseManager,
    SetLibraryName(String),
    SaveProfile,
    DeleteProfile,
    SaveDspSettings,
    DeleteDspSettings,
    SaveColormap,
    DeleteColormap,
    SetColorStop(usize, u8, f32),
    AddColorStop,
    DeleteColorStop(usize),
    AdvancedSlider(DspSlider, f32),
    AdvancedSliderRelease(DspSlider),
    SetWindowFn(WindowFunction),
    SetBandAggregation(BandAggregation),
    SetWeighting(Weighting),
    SetTransform(Transform),
    SetCentered(bool),
}

pub struct SettingsState {
    pub open: bool,
    pub contrast: f32,
    pub saturation: f32,
    pub colormap: String,
    pub profile: String,
    pub dsp_settings: String,
    pub overlay: String,
    pub semitone_shift: i32,
    pub source: String,
    pub width: f32,
    pub advanced: SpectrumConfig,
    pub history: f32,
    pub manager: Option<LibraryManager>,
    pub library_name: String,
    pub colormap_stops: Vec<(f32, f32, f32, f32)>,
}

impl SettingsState {
    pub fn new(
        open: bool,
        contrast: f32,
        saturation: f32,
        colormap: impl Into<String>,
        profile: impl Into<String>,
        dsp_settings: impl Into<String>,
        overlay: impl Into<String>,
        source: impl Into<String>,
        spectrum: &SpectrumConfig,
        history: f32,
    ) -> Self {
        Self {
            open,
            contrast,
            saturation,
            colormap: colormap.into(),
            profile: profile.into(),
            dsp_settings: dsp_settings.into(),
            overlay: overlay.into(),
            semitone_shift: 0,
            source: source.into(),
            width: 280.0,
            advanced: spectrum.clone(),
            history,
            manager: None,
            library_name: String::new(),
            colormap_stops: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn from_spectrum(&mut self, spectrum: &SpectrumConfig, history: f32) {
        self.advanced = spectrum.clone();
        self.history = history;
    }

    pub fn value(&self, s: DspSlider) -> f32 {
        match s {
            DspSlider::WindowSize => self.advanced.window_size as f32,
            DspSlider::HopSize => self.advanced.hop_size as f32,
            DspSlider::LogBins => self.advanced.log_bins as f32,
            DspSlider::FMin => self.advanced.f_min_hz,
            DspSlider::FMax => self.advanced.f_max_hz,
            DspSlider::DbFloor => self.advanced.db_floor,
            DspSlider::DbCeil => self.advanced.db_ceil,
            DspSlider::Smoothing => self.advanced.freq_smoothing_sigma,
            DspSlider::Gamma => self.advanced.amplitude_gamma,
            DspSlider::TemporalAlpha => self.advanced.temporal_alpha,
            DspSlider::PeakDecay => self.advanced.peak_hold_decay,
            DspSlider::CqtBins => self.advanced.cqt_bins_per_octave as f32,
            DspSlider::FreqScaleExp => self.advanced.freq_scale_exp,
            DspSlider::History => self.history,
        }
    }

    pub fn set(&mut self, s: DspSlider, v: f32) {
        match s {
            DspSlider::WindowSize => self.advanced.window_size = v.round() as usize,
            DspSlider::HopSize => self.advanced.hop_size = v.round() as usize,
            DspSlider::LogBins => self.advanced.log_bins = v.round() as usize,
            DspSlider::FMin => self.advanced.f_min_hz = v.min(self.advanced.f_max_hz - 1.0).max(1.0),
            DspSlider::FMax => self.advanced.f_max_hz = v.max(self.advanced.f_min_hz + 1.0),
            DspSlider::DbFloor => self.advanced.db_floor = v.min(self.advanced.db_ceil - 1.0),
            DspSlider::DbCeil => self.advanced.db_ceil = v.max(self.advanced.db_floor + 1.0),
            DspSlider::Smoothing => self.advanced.freq_smoothing_sigma = v.max(0.0),
            DspSlider::Gamma => self.advanced.amplitude_gamma = v.max(0.0),
            DspSlider::TemporalAlpha => self.advanced.temporal_alpha = v.clamp(0.0, 1.0),
            DspSlider::PeakDecay => self.advanced.peak_hold_decay = v.clamp(0.0, 0.999),
            DspSlider::CqtBins => self.advanced.cqt_bins_per_octave = v.round() as u32,
            DspSlider::FreqScaleExp => self.advanced.freq_scale_exp = v.max(0.1),
            DspSlider::History => self.history = v,
        }
    }

    pub fn view<'a>(
        &'a self,
        colormaps: &'a [String],
        profiles: &'a [String],
        dsp_settings: &'a [String],
        overlays: &'a [String],
        sources: &'a [String],
        paused: bool,
    ) -> Element<'a, SettingsMessage> {
        if let Some(manager) = &self.manager {
            return self.manager_view(manager, paused);
        }
        let header = row![
            text("vividspektrum").size(20),
            Space::new().width(Length::Fill),
            button("X").on_press(SettingsMessage::Close),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let band_agg: Element<'a, SettingsMessage> = if self.advanced.transform == Transform::Stft {
            pick_list(AGGREGATIONS, Some(self.advanced.band_aggregation), SettingsMessage::SetBandAggregation).into()
        } else {
            text("CQT uses constant-Q triangular bands.").size(11).into()
        };

        let shift_text: Element<'a, SettingsMessage> = if self.semitone_shift != 0 {
            text(format!("{}{} semitones", if self.semitone_shift > 0 { "+" } else { "" }, self.semitone_shift)).size(11).into()
        } else {
            text("").into()
        };

        let controls = column![
            text("Right-click or press M to toggle this menu.").size(11),
            if paused { text("Spectrogram paused — press Space to resume.").size(12) } else { text("") },
            label_row("Profile", "A profile combines colors, overlay, audio source, and DSP settings."),
            row![
                pick_list(profiles, Some(self.profile.clone()), SettingsMessage::SetProfile).width(Length::Fill),
                button("…").on_press(SettingsMessage::OpenManager(LibraryManager::Profiles)),
            ].spacing(6),
            text("Colors · overlay · audio source · DSP").size(11),
            label_row("Colormap", "Color map used to map magnitude to color."),
            row![
                pick_list(colormaps, Some(self.colormap.clone()), SettingsMessage::SetColormap).width(Length::Fill),
                button("…").on_press(SettingsMessage::OpenManager(LibraryManager::Colormaps)),
            ].spacing(6),
            row![
                text(format!("Contrast {:.2}", self.contrast)).size(12),
                Space::new().width(Length::Fill),
                info_icon("GPU contrast. 1.0 is neutral, >1 increases, <1 decreases."),
            ]
            .align_y(Alignment::Center),
            slider(0.0f32..=3.0f32, self.contrast, SettingsMessage::SetContrast).step(0.05),
            row![
                text(format!("Saturation {:.2}", self.saturation)).size(12),
                Space::new().width(Length::Fill),
                info_icon("GPU saturation. 0 = grayscale, 1 = normal, >1 boosted."),
            ]
            .align_y(Alignment::Center),
            slider(0.0f32..=3.0f32, self.saturation, SettingsMessage::SetSaturation).step(0.05),
            label_row("Overlay", "Optional frequency-line overlays (e.g. A440, guitar tuning). + and - shift all lines by one semitone."),
            row![
                pick_list(overlays, Some(self.overlay.clone()), SettingsMessage::SetOverlay).width(Length::Fill),
                button("+").on_press(SettingsMessage::OverlayShift(1)),
                button("-").on_press(SettingsMessage::OverlayShift(-1)),
            ].spacing(6),
            shift_text,
            label_row("Audio source", "PipeWire/PulseAudio capture source saved separately from DSP settings."),
            pick_list(sources, Some(self.source.clone()), SettingsMessage::SetSource)
                .text_size(11)
                .width(Length::Fill),
            text("DSP").size(14),
            label_row("DSP settings", "Reusable DSP slider presets. Applying one does not change profile colors, overlay, or audio source."),
            row![
                pick_list(dsp_settings, Some(self.dsp_settings.clone()), SettingsMessage::SetDspSettings).width(Length::Fill),
                button("…").on_press(SettingsMessage::OpenManager(LibraryManager::DspSettings)),
            ].spacing(6),
            label_row("Window function", "Time-domain window applied before FFT. Blackman-Harris reduces leakage."),
            pick_list(WINDOW_FUNCTIONS, Some(self.advanced.window_fn), SettingsMessage::SetWindowFn),
            label_row("Transform", "STFT: equal time/frequency resolution. CQT: musical pitch spacing."),
            pick_list(TRANSFORMS, Some(self.advanced.transform), SettingsMessage::SetTransform),
            label_row("Band aggregation", "How FFT bins are combined into each log-frequency band."),
            band_agg,
            label_row("Weighting", "A/C frequency-weighting curves (IEC 61672) or no weighting."),
            pick_list(WEIGHTINGS, Some(self.advanced.weighting), SettingsMessage::SetWeighting),
            row![
                checkbox(self.advanced.centered)
                    .label("Centered window")
                    .on_toggle(SettingsMessage::SetCentered),
                Space::new().width(Length::Fill),
                info_icon("Center the FFT window. Adds latency but reduces frame-boundary artifacts."),
            ]
            .align_y(Alignment::Center),
            self.slider_row(DspSlider::WindowSize),
            self.slider_row(DspSlider::HopSize),
            if self.advanced.transform == Transform::Stft {
                self.slider_row(DspSlider::LogBins)
            } else {
                text("").into()
            },
            self.slider_row(DspSlider::FMin),
            self.slider_row(DspSlider::FMax),
            self.slider_row(DspSlider::DbFloor),
            self.slider_row(DspSlider::DbCeil),
            self.slider_row(DspSlider::Smoothing),
            self.slider_row(DspSlider::Gamma),
            self.slider_row(DspSlider::TemporalAlpha),
            self.slider_row(DspSlider::PeakDecay),
            if self.advanced.transform == Transform::Stft {
                self.slider_row(DspSlider::FreqScaleExp)
            } else {
                text("").into()
            },
            if self.advanced.transform == Transform::Cqt {
                self.slider_row(DspSlider::CqtBins)
            } else {
                text("").into()
            },
            self.slider_row(DspSlider::History),
        ]
        .spacing(8)
        .padding(12);

        let panel = column![header, scrollable(controls).height(Length::Fill)]
            .spacing(12)
            .padding(16)
            .width(Length::Fixed(self.width))
            .height(Length::Fill);

        container(panel)
            .width(Length::Fixed(self.width))
            .height(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(iced::Color::from_rgb8(20, 20, 25).into()),
                ..Default::default()
            })
            .into()
    }

    fn manager_view<'a>(&'a self, manager: &LibraryManager, paused: bool) -> Element<'a, SettingsMessage> {
        let (title, current, save, delete) = match manager {
            LibraryManager::Profiles => ("Manage profiles", &self.profile, SettingsMessage::SaveProfile, SettingsMessage::DeleteProfile),
            LibraryManager::Colormaps => ("Manage colormaps", &self.colormap, SettingsMessage::SaveColormap, SettingsMessage::DeleteColormap),
            LibraryManager::DspSettings => ("Manage DSP settings", &self.dsp_settings, SettingsMessage::SaveDspSettings, SettingsMessage::DeleteDspSettings),
        };
        let mut body = column![
            row![text(title).size(20), Space::new().width(Length::Fill), button("Back").on_press(SettingsMessage::CloseManager)]
                .align_y(Alignment::Center),
            text(format!("Selected: {current}")).size(12),
            text("Right-click or press M to toggle this menu.").size(11),
            if paused { text("Spectrogram paused — press Space to resume.").size(12) } else { text("") },
            text("Built-ins are protected. Enter a new name to save a copy.").size(12),
            text_input("new name (optional)", &self.library_name).on_input(SettingsMessage::SetLibraryName),
        ];
        let protected = match manager {
            LibraryManager::Profiles => spektrum_core::profiles::is_builtin_profile(current),
            LibraryManager::Colormaps => spektrum_core::colormap::is_builtin_colormap(current),
            LibraryManager::DspSettings => current == "custom" || spektrum_core::profiles::is_builtin_dsp_settings(current),
        };
        let save_selected = if protected {
            button("Built-in is protected")
        } else {
            button("Save selected user item").on_press(save.clone())
        };
        let delete_selected = if protected {
            button("Built-in cannot be deleted")
        } else {
            button("Delete selected user item").on_press(delete)
        };
        body = body.push(save_selected).push(button("Save as user copy").on_press(save)).push(delete_selected);
        if matches!(manager, LibraryManager::Colormaps) {
            body = body.push(button("Add color stop").on_press(SettingsMessage::AddColorStop));
            for (index, &(position, r, g, b)) in self.colormap_stops.iter().enumerate() {
                body = body.push(column![
                    row![
                        text(format!("Stop {}", index + 1)).size(12),
                        Space::new().width(Length::Fill),
                        container(Space::new().width(Length::Fixed(18.0)).height(Length::Fixed(18.0)))
                            .style(move |_theme: &Theme| container::Style {
                                background: Some(iced::Color::from_rgb(r, g, b).into()),
                                border: iced::Border { color: iced::Color::from_rgb8(180, 180, 180), width: 1.0, radius: 3.0.into() },
                                ..Default::default()
                            }),
                        button("Remove").on_press(SettingsMessage::DeleteColorStop(index)),
                    ].spacing(6).align_y(Alignment::Center),
                    text(format!("Position {:.2}", position)).size(11),
                    slider(0.0..=1.0, position, move |value| SettingsMessage::SetColorStop(index, 0, value)).step(0.01),
                    text(format!("Red {:.2}", r)).size(11),
                    slider(0.0..=1.0, r, move |value| SettingsMessage::SetColorStop(index, 1, value)).step(0.01),
                    text(format!("Green {:.2}", g)).size(11),
                    slider(0.0..=1.0, g, move |value| SettingsMessage::SetColorStop(index, 2, value)).step(0.01),
                    text(format!("Blue {:.2}", b)).size(11),
                    slider(0.0..=1.0, b, move |value| SettingsMessage::SetColorStop(index, 3, value)).step(0.01),
                ].spacing(4));
            }
        }
        let body = scrollable(body.spacing(12).padding(16)).height(Length::Fill).width(Length::Fixed(self.width));
        container(body)
            .width(Length::Fixed(self.width))
            .height(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(iced::Color::from_rgb8(20, 20, 25).into()),
                ..Default::default()
            })
            .into()
    }

    fn slider_row<'b>(&'b self, s: DspSlider) -> Element<'b, SettingsMessage> {
        let value = self.value(s);
        let label = format_value(s, value);
        column![
            row![
                text(format!("{}: {}", s.label(), label)).size(12),
                Space::new().width(Length::Fill),
                info_icon(s.tip()),
            ]
            .align_y(Alignment::Center),
            slider(s.range(), value, move |v| SettingsMessage::AdvancedSlider(s, v))
                .step(s.step())
                .on_release(SettingsMessage::AdvancedSliderRelease(s))
        ]
        .spacing(4)
        .into()
    }
}

const WINDOW_FUNCTIONS: &[WindowFunction] = &[
    WindowFunction::Hann,
    WindowFunction::Hamming,
    WindowFunction::Blackman,
    WindowFunction::BlackmanHarris,
];

const TRANSFORMS: &[Transform] = &[Transform::Stft, Transform::Cqt];

const AGGREGATIONS: &[BandAggregation] = &[BandAggregation::Nearest, BandAggregation::Triangular];

const WEIGHTINGS: &[Weighting] = &[Weighting::None, Weighting::A, Weighting::C];

fn format_value(s: DspSlider, v: f32) -> String {
    if s.is_integer() {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v)
    }
}
