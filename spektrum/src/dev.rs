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

/// Live texture rows used for the **time** axis. This is the number of STFT columns kept in the GPU waterfall buffer.
pub fn effective_spectrogram_history(requested: u32) -> u32 {
    requested.max(1)
}
