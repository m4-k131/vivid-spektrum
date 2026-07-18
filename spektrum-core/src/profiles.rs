use crate::{CoreError, SpectrumConfig, SpectrogramImageConfig};
use std::path::Path;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Profile {
    pub spectrum: SpectrumConfig,
    pub image: Option<ProfileImage>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProfileImage {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_scroll")]
    pub scroll_right_to_left: bool,
    #[serde(default = "default_colormap")]
    pub colormap: String,
    #[serde(default = "default_contrast")]
    pub contrast: f32,
    #[serde(default = "default_saturation")]
    pub saturation: f32,
}

fn default_width() -> u32 { 800 }
fn default_height() -> u32 { 200 }
fn default_scroll() -> bool { true }
fn default_colormap() -> String { "viridis".into() }
fn default_contrast() -> f32 { 1.0 }
fn default_saturation() -> f32 { 1.0 }

impl Profile {
    pub fn to_image_config(&self) -> SpectrogramImageConfig {
        let img = self.image.as_ref();
        SpectrogramImageConfig {
            spectrum: self.spectrum.clone(),
            width: img.map_or(800, |i| i.width),
            height: img.map_or(200, |i| i.height),
            scroll_right_to_left: img.is_none_or(|i| i.scroll_right_to_left),
            colormap: img.map_or_else(|| "viridis".into(), |i| i.colormap.clone()),
            contrast: img.map_or(1.0, |i| i.contrast),
            saturation: img.map_or(1.0, |i| i.saturation),
        }
    }
}

pub fn load_profile(path: &Path) -> Result<Profile, CoreError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| CoreError::Dsp(format!("failed to read profile: {e}")))?;
    toml::from_str(&content)
        .map_err(|e| CoreError::Dsp(format!("invalid profile TOML: {e}")))
}

pub fn builtin_profile(name: &str) -> Option<Profile> {
    match name {
        "laptop" => Some(Profile {
            spectrum: SpectrumConfig {
                window_size: 4096,
                hop_size: 512,
                sample_rate: 48000,
                log_bins: 128,
                ..Default::default()
            },
            image: None,
        }),
        "default" => Some(Profile {
            spectrum: SpectrumConfig::default(),
            image: None,
        }),
        "high-resolution" => Some(Profile {
            spectrum: SpectrumConfig {
                window_size: 32768,
                hop_size: 128,
                sample_rate: 48000,
                log_bins: 2048,
                db_floor: -100.0,
                freq_smoothing_sigma: 1.5,
                amplitude_gamma: 0.4,
                temporal_alpha: 0.4,
                peak_hold_decay: 0.95,
                ..Default::default()
            },
            image: None,
        }),
        _ => None,
    }
}

pub fn builtin_profile_names() -> &'static [&'static str] {
    &["laptop", "default", "high-resolution"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_profiles_exist() {
        for name in builtin_profile_names() {
            let p = builtin_profile(name);
            assert!(p.is_some(), "profile '{name}' should exist");
        }
    }

    #[test]
    fn unknown_profile_returns_none() {
        assert!(builtin_profile("nonexistent").is_none());
    }

    #[test]
    fn laptop_profile_has_smaller_window() {
        let p = builtin_profile("laptop").unwrap();
        assert_eq!(p.spectrum.window_size, 4096);
        assert_eq!(p.spectrum.log_bins, 128);
    }

    #[test]
    fn high_resolution_profile_has_large_window() {
        let p = builtin_profile("high-resolution").unwrap();
        assert_eq!(p.spectrum.window_size, 32768);
        assert_eq!(p.spectrum.log_bins, 2048);
    }

    #[test]
    fn default_profile_matches_default_config() {
        let p = builtin_profile("default").unwrap();
        let default_cfg = SpectrumConfig::default();
        assert_eq!(p.spectrum.window_size, default_cfg.window_size);
        assert_eq!(p.spectrum.log_bins, 1024);
    }

    #[test]
    fn to_image_config_defaults() {
        let profile = Profile {
            spectrum: SpectrumConfig::default(),
            image: None,
        };
        let cfg = profile.to_image_config();
        assert_eq!(cfg.width, 800);
        assert_eq!(cfg.height, 200);
        assert_eq!(cfg.colormap, "viridis");
        assert!(cfg.scroll_right_to_left);
    }

    #[test]
    fn to_image_config_with_image_section() {
        let profile = Profile {
            spectrum: SpectrumConfig::default(),
            image: Some(ProfileImage {
                width: 1920,
                height: 400,
                scroll_right_to_left: false,
                colormap: "inferno".into(),
                contrast: 1.0,
                saturation: 1.0,
            }),
        };
        let cfg = profile.to_image_config();
        assert_eq!(cfg.width, 1920);
        assert_eq!(cfg.height, 400);
        assert_eq!(cfg.colormap, "inferno");
        assert!(!cfg.scroll_right_to_left);
    }
}
