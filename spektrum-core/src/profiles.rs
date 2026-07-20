use crate::{CoreError, SpectrumConfig, SpectrogramImageConfig};
use std::path::Path;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Profile {
    pub dsp: SpectrumConfig,
    pub colors: ColorSettings,
    pub audio: AudioSettings,
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
            colormap: "magma".into(),
            contrast: 1.0,
            saturation: 1.0,
            overlay: "none".into(),
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub source: Option<String>,
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
            audio: AudioSettings::default(),
            image: None,
            history: None,
        }),
        "default" => Some(Profile {
            dsp: builtin_dsp_settings("default").unwrap(),
            colors: ColorSettings::default(),
            audio: AudioSettings::default(),
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
            audio: AudioSettings::default(),
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

fn personal_dsp_settings() -> SpectrumConfig {
    SpectrumConfig {
        window_size: 8192,
        hop_size: 1024,
        sample_rate: 48000,
        log_bins: 4096,
        f_min_hz: 20.0,
        f_max_hz: 15000.0,
        db_floor: -90.0,
        db_ceil: -10.0,
        window_fn: crate::WindowFunction::BlackmanHarris,
        band_aggregation: crate::BandAggregation::Triangular,
        freq_smoothing_sigma: 1.1,
        amplitude_gamma: 0.4,
        temporal_alpha: 0.6,
        peak_hold_decay: 0.95,
        freq_scale_exp: 0.6,
        ..Default::default()
    }
}

pub fn builtin_dsp_settings(name: &str) -> Option<SpectrumConfig> {
    let mut settings = personal_dsp_settings();
    match name {
        "high-resolution" => {}
        "default" | "medium-resolution" => settings.window_size = 4096,
        "low-resolution" => settings.window_size = 2048,
        "fast-scrolling" => {
            settings.window_size = 4096;
            settings.hop_size = 512;
        }
        "wide-db-band" => {
            settings.window_size = 4096;
            settings.db_floor = -120.0;
            settings.db_ceil = -1.0;
        }
        "narrow-frequency-band" => {
            settings.window_size = 4096;
            settings.f_max_hz = 4800.0;
        }
        "dense-frequency-grid" => settings.log_bins = 8192,
        "low-frequency-cutoff" => settings.f_min_hz = 120.0,
        "hann-window" => settings.window_fn = crate::WindowFunction::Hann,
        "nearest-bands" => settings.band_aggregation = crate::BandAggregation::Nearest,
        "frequency-smoothing" => settings.freq_smoothing_sigma = 3.0,
        "bright-quiet-detail" => settings.amplitude_gamma = 0.25,
        "temporal-smoothing" => settings.temporal_alpha = 0.85,
        "peak-hold" => settings.peak_hold_decay = 0.99,
        "a-weighting" => settings.weighting = crate::Weighting::A,
        "c-weighting" => settings.weighting = crate::Weighting::C,
        "musical-cqt" => {
            settings.transform = crate::Transform::Cqt;
            settings.cqt_bins_per_octave = 48;
        }
        "low-frequency-scale" => settings.freq_scale_exp = 0.35,
        "centered-analysis" => settings.centered = true,
        _ => return None,
    }
    Some(settings)
}

pub fn builtin_dsp_settings_names() -> &'static [&'static str] {
    &[
        "default",
        "high-resolution",
        "medium-resolution",
        "low-resolution",
        "fast-scrolling",
        "wide-db-band",
        "narrow-frequency-band",
        "dense-frequency-grid",
        "low-frequency-cutoff",
        "hann-window",
        "nearest-bands",
        "frequency-smoothing",
        "bright-quiet-detail",
        "temporal-smoothing",
        "peak-hold",
        "a-weighting",
        "c-weighting",
        "musical-cqt",
        "low-frequency-scale",
        "centered-analysis",
    ]
}

pub fn user_dsp_settings_dir() -> std::path::PathBuf {
    config_dir().join("vividspektrum/dsp-settings")
}

pub fn is_builtin_dsp_settings(name: &str) -> bool {
    builtin_dsp_settings(name).is_some()
}

pub fn load_dsp_settings(path: &Path) -> Result<SpectrumConfig, CoreError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| CoreError::Dsp(format!("failed to read DSP settings: {e}")))?;
    toml::from_str(&content)
        .map_err(|e| CoreError::Dsp(format!("invalid DSP settings TOML: {e}")))
}

pub fn resolve_dsp_settings(name: &str) -> Result<SpectrumConfig, CoreError> {
    if let Some(settings) = builtin_dsp_settings(name) {
        return Ok(settings);
    }
    let path = user_dsp_settings_dir().join(format!("{name}.toml"));
    if path.exists() {
        load_dsp_settings(&path)
    } else {
        Err(CoreError::Dsp(format!("unknown DSP settings '{name}'")))
    }
}

pub fn list_dsp_settings_names() -> Vec<String> {
    let mut names: Vec<String> = builtin_dsp_settings_names().iter().map(|s| s.to_string()).collect();
    if let Ok(entries) = std::fs::read_dir(user_dsp_settings_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                if let Some(name) = path.file_stem() {
                    let name = name.to_string_lossy().into_owned();
                    if !names.contains(&name) { names.push(name); }
                }
            }
        }
    }
    names.sort();
    names
}

pub fn save_user_dsp_settings(name: &str, settings: &SpectrumConfig) -> Result<(), CoreError> {
    validate_name(name)?;
    if is_builtin_dsp_settings(name) {
        return Err(CoreError::Dsp(format!("'{name}' is a protected built-in DSP settings preset")));
    }
    let dir = user_dsp_settings_dir();
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::Dsp(format!("failed to create DSP settings directory: {e}")))?;
    let path = dir.join(format!("{name}.toml"));
    let text = toml::to_string_pretty(settings).map_err(|e| CoreError::Dsp(format!("failed to serialize DSP settings: {e}")))?;
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, text).map_err(|e| CoreError::Dsp(format!("failed to write DSP settings: {e}")))?;
    std::fs::rename(temp, path).map_err(|e| CoreError::Dsp(format!("failed to save DSP settings: {e}")))
}

pub fn delete_user_dsp_settings(name: &str) -> Result<(), CoreError> {
    validate_name(name)?;
    std::fs::remove_file(user_dsp_settings_dir().join(format!("{name}.toml")))
        .map_err(|e| CoreError::Dsp(format!("failed to delete DSP settings: {e}")))
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
    fn dsp_settings_variants_inherit_personal_baseline() {
        let personal = builtin_dsp_settings("high-resolution").unwrap();
        let medium = builtin_dsp_settings("medium-resolution").unwrap();
        let default = builtin_dsp_settings("default").unwrap();
        let fast = builtin_dsp_settings("fast-scrolling").unwrap();
        let wide = builtin_dsp_settings("wide-db-band").unwrap();
        let narrow = builtin_dsp_settings("narrow-frequency-band").unwrap();
        assert_eq!(medium.window_size, 4096);
        assert_eq!(default.window_size, medium.window_size);
        assert_eq!(default.hop_size, medium.hop_size);
        assert_eq!(default.log_bins, medium.log_bins);
        assert_eq!(fast.hop_size, 512);
        assert_eq!(wide.db_floor, -120.0);
        assert_eq!(wide.db_ceil, -1.0);
        assert_eq!(narrow.f_max_hz, 4800.0);
        assert_eq!(narrow.db_floor, personal.db_floor);
        assert_eq!(narrow.db_ceil, personal.db_ceil);
    }

    #[test]
    fn dsp_settings_showcase_remaining_controls() {
        assert_eq!(builtin_dsp_settings("dense-frequency-grid").unwrap().log_bins, 8192);
        assert_eq!(builtin_dsp_settings("low-frequency-cutoff").unwrap().f_min_hz, 120.0);
        assert_eq!(builtin_dsp_settings("hann-window").unwrap().window_fn, crate::WindowFunction::Hann);
        assert_eq!(builtin_dsp_settings("nearest-bands").unwrap().band_aggregation, crate::BandAggregation::Nearest);
        assert_eq!(builtin_dsp_settings("frequency-smoothing").unwrap().freq_smoothing_sigma, 3.0);
        assert_eq!(builtin_dsp_settings("bright-quiet-detail").unwrap().amplitude_gamma, 0.25);
        assert_eq!(builtin_dsp_settings("temporal-smoothing").unwrap().temporal_alpha, 0.85);
        assert_eq!(builtin_dsp_settings("peak-hold").unwrap().peak_hold_decay, 0.99);
        assert_eq!(builtin_dsp_settings("a-weighting").unwrap().weighting, crate::Weighting::A);
        assert_eq!(builtin_dsp_settings("c-weighting").unwrap().weighting, crate::Weighting::C);
        assert_eq!(builtin_dsp_settings("musical-cqt").unwrap().transform, crate::Transform::Cqt);
        assert_eq!(builtin_dsp_settings("low-frequency-scale").unwrap().freq_scale_exp, 0.35);
        assert!(builtin_dsp_settings("centered-analysis").unwrap().centered);
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
    fn default_profile_uses_default_dsp_settings() {
        let profile = builtin_profile("default").unwrap();
        let settings = builtin_dsp_settings("default").unwrap();
        assert_eq!(profile.dsp.window_size, settings.window_size);
        assert_eq!(profile.dsp.hop_size, settings.hop_size);
        assert_eq!(profile.dsp.log_bins, settings.log_bins);
    }

    #[test]
    fn to_image_config_defaults() {
        let profile = Profile {
            dsp: SpectrumConfig::default(),
            colors: ColorSettings::default(),
            audio: AudioSettings::default(),
            image: None,
            history: None,
        };
        let cfg = profile.to_image_config();
        assert_eq!(cfg.width, 800);
        assert_eq!(cfg.height, 800);
        assert_eq!(cfg.colormap, "magma");
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
            audio: AudioSettings::default(),
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
