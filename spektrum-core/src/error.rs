use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unsupported platform for this operation")]
    UnsupportedPlatform,
    #[error("pipewire error: {0}")]
    Pipewire(String),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("no default audio sink found; pass --target-object explicitly")]
    NoDefaultSink,
    #[error("dsp: {0}")]
    Dsp(String),
}

#[cfg(target_os = "linux")]
impl From<::pipewire::Error> for CoreError {
    fn from(e: ::pipewire::Error) -> Self {
        CoreError::Pipewire(e.to_string())
    }
}
