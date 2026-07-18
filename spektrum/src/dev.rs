//! Developer-facing UI defaults (spectrogram layout, etc.).
#[derive(Clone, Copy, Debug)]
pub struct SpectrogramDevConfig {
    /// When `true` (default), the waterfall scrolls along **time on the horizontal axis** (new content on the **right**, older to the **left**). When `false`, time scrolls **top to bottom** (legacy).
    pub scroll_right_to_left: bool,
}
impl Default for SpectrogramDevConfig {
    fn default() -> Self {
        Self { scroll_right_to_left: true }
    }
}

/// Texture rows used for **time** in the waterfall. For ~one STFT column per **screen pixel** along the time axis, use at least the window size on that axis (width if time scrolls horizontally, else height).
pub fn effective_spectrogram_history(requested: u32, width_px: u32, height_px: u32, scroll_right_to_left: bool) -> u32 {
    let need = if scroll_right_to_left { width_px } else { height_px };
    requested.max(need).max(1)
}
