//! Shared settings overlay panel used by the live spectrogram window (Linux & Windows).
use iced::widget::{button, column, container, pick_list, row, slider, text};
use iced::{Alignment, Element, Length, Theme};

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    Toggle,
    SetContrast(f32),
    SetSaturation(f32),
    SetColormap(String),
    SetProfile(String),
    SetOverlay(String),
    Close,
}

pub struct SettingsState {
    pub open: bool,
    pub contrast: f32,
    pub saturation: f32,
    pub colormap: String,
    pub profile: String,
    pub overlay: String,
    pub width: f32,
}

impl SettingsState {
    pub fn new(
        open: bool,
        contrast: f32,
        saturation: f32,
        colormap: impl Into<String>,
        profile: impl Into<String>,
        overlay: impl Into<String>,
    ) -> Self {
        Self {
            open,
            contrast,
            saturation,
            colormap: colormap.into(),
            profile: profile.into(),
            overlay: overlay.into(),
            width: 280.0,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn view<'a>(
        &'a self,
        colormaps: &'a [String],
        profiles: &'a [String],
        overlays: &'a [String],
    ) -> Element<'a, SettingsMessage> {
        let header = row![
            text("vividspektrum").size(20),
            button("X").on_press(SettingsMessage::Close),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let controls = column![
            text("Profile").size(12),
            pick_list(profiles, Some(self.profile.clone()), SettingsMessage::SetProfile),
            text("Colormap").size(12),
            pick_list(colormaps, Some(self.colormap.clone()), SettingsMessage::SetColormap),
            text("Overlay").size(12),
            pick_list(overlays, Some(self.overlay.clone()), SettingsMessage::SetOverlay),
            text(format!("Contrast {:.2}", self.contrast)).size(12),
            slider(0.0f32..=3.0f32, self.contrast, SettingsMessage::SetContrast).step(0.05),
            text(format!("Saturation {:.2}", self.saturation)).size(12),
            slider(0.0f32..=3.0f32, self.saturation, SettingsMessage::SetSaturation).step(0.05),
        ]
        .spacing(8)
        .padding(12);

        let panel = column![header, controls]
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
}
