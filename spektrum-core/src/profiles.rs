use crate::{CoreError, SpectrumConfig, SpectrogramImageConfig};
use std::path::Path;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Profile {
    pub dsp: SpectrumConfig,
    pub colors: ColorSettings,
    pub image: Option<ProfileImage>,
    #[serde(default)]
    pub history: Option<u32>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ColorSettings {
    pub colormap: String,
    pub contrast: f32,
    pub saturation: f32,
    pub overlay: String,
}

impl Default for ColorSettings {
    fn default() -> Self {
        Self {
            colormap: "viridis".into(),
            contrast: 1.0,
            saturation: 1.0,
            overlay: "none".into(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProfileImage {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_scroll")]
    pub scroll_right_to_left: bool,
}

fn default_width() -> u32 { 800 }
fn default_height() -> u32 { 800 }
fn default_scroll() -> bool { true }

impl Profile {
    pub fn to_image_config(&self) -> SpectrogramImageConfig {
        let img = self.image.as_ref();
        SpectrogramImageConfig {
            spectrum: self.dsp.clone(),
            width: img.map_or(800, |i| i.width),
            height: img.map_or(800, |i| i.height),
            scroll_right_to_left: img.is_none_or(|i| i.scroll_right_to_left),
            colormap: self.colors.colormap.clone(),
            contrast: self.colors.contrast,
            saturation: self.colors.saturation,
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
            dsp: SpectrumConfig {
                window_size: 4096,
                hop_size: 512,
                sample_rate: 48000,
                log_bins: 128,
                ..Default::default()
            },
            colors: ColorSettings::default(),
            image: None,
            history: None,
        }),
        "default" => Some(Profile {
            dsp: SpectrumConfig::default(),
            colors: ColorSettings::default(),
            image: None,
            history: None,
        }),
        "high-resolution" => Some(Profile {
            dsp: SpectrumConfig {
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
            colors: ColorSettings::default(),
            image: None,
            history: None,
        }),
        _ => None,
    }
}

pub fn builtin_profile_names() -> &'static [&'static str] {
    &["laptop", "default", "high-resolution"]
}

pub fn user_profiles_dir() -> std::path::PathBuf {
    config_dir().join("vividspektrum/profiles")
}

pub fn is_builtin_profile(name: &str) -> bool {
    builtin_profile(name).is_some() || std::path::Path::new("presets").join(format!("{name}.toml")).exists()
}

pub fn save_user_profile(name: &str, profile: &Profile) -> Result<(), CoreError> {
    validate_name(name)?;
    if is_builtin_profile(name) {
        return Err(CoreError::Dsp(format!("'{name}' is a protected built-in profile")));
    }
    let dir = user_profiles_dir();
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::Dsp(format!("failed to create profile directory: {e}")))?;
    let path = dir.join(format!("{name}.toml"));
    let text = toml::to_string_pretty(profile).map_err(|e| CoreError::Dsp(format!("failed to serialize profile: {e}")))?;
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, text).map_err(|e| CoreError::Dsp(format!("failed to write profile: {e}")))?;
    std::fs::rename(temp, path).map_err(|e| CoreError::Dsp(format!("failed to save profile: {e}")))
}

pub fn delete_user_profile(name: &str) -> Result<(), CoreError> {
    validate_name(name)?;
    std::fs::remove_file(user_profiles_dir().join(format!("{name}.toml")))
        .map_err(|e| CoreError::Dsp(format!("failed to delete profile: {e}")))
}

pub fn resolve_profile(name: &str) -> Result<Profile, CoreError> {
    if let Some(p) = builtin_profile(name) {
        return Ok(p);
    }
    let user_path = user_profiles_dir().join(format!("{name}.toml"));
    if user_path.exists() {
        return load_profile(&user_path);
    }
    let path = std::path::PathBuf::from(format!("presets/{name}.toml"));
    if path.exists() { load_profile(&path) } else { Err(CoreError::Dsp(format!("unknown profile '{name}'"))) }
}

pub fn list_profile_names() -> Vec<String> {
    let mut names: Vec<String> = builtin_profile_names().iter().map(|s| s.to_string()).collect();
    for dir in [std::path::PathBuf::from("presets"), user_profiles_dir()] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|ext| ext == "toml") {
                    if let Some(s) = p.file_stem() {
                        let s = s.to_string_lossy().into_owned();
                        if !names.contains(&s) { names.push(s); }
                    }
                }
            }
        }
    }
    names.sort();
    names
}

fn config_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

fn validate_name(name: &str) -> Result<(), CoreError> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(CoreError::Dsp("names may contain only letters, numbers, '-' and '_'".to_string()));
    }
    Ok(())
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
        assert_eq!(p.dsp.window_size, 4096);
        assert_eq!(p.dsp.log_bins, 128);
    }

    #[test]
    fn high_resolution_profile_has_large_window() {
        let p = builtin_profile("high-resolution").unwrap();
        assert_eq!(p.dsp.window_size, 32768);
        assert_eq!(p.dsp.log_bins, 2048);
    }

    #[test]
    fn default_profile_matches_default_config() {
        let p = builtin_profile("default").unwrap();
        let default_cfg = SpectrumConfig::default();
        assert_eq!(p.dsp.window_size, default_cfg.window_size);
        assert_eq!(p.dsp.log_bins, 1024);
    }

    #[test]
    fn to_image_config_defaults() {
        let profile = Profile {
            dsp: SpectrumConfig::default(),
            colors: ColorSettings::default(),
            image: None,
            history: None,
        };
        let cfg = profile.to_image_config();
        assert_eq!(cfg.width, 800);
        assert_eq!(cfg.height, 800);
        assert_eq!(cfg.colormap, "viridis");
        assert!(cfg.scroll_right_to_left);
    }

    #[test]
    fn to_image_config_with_image_section() {
        let profile = Profile {
            dsp: SpectrumConfig::default(),
            colors: ColorSettings {
                colormap: "inferno".into(),
                ..Default::default()
            },
            image: Some(ProfileImage {
                width: 1920,
                height: 400,
                scroll_right_to_left: false,
            }),
            history: None,
        };
        let cfg = profile.to_image_config();
        assert_eq!(cfg.width, 1920);
        assert_eq!(cfg.height, 400);
        assert_eq!(cfg.colormap, "inferno");
        assert!(!cfg.scroll_right_to_left);
    }
}
