use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_overlay_color")]
    pub color: [u8; 3],
    #[serde(default = "default_overlay_opacity")]
    pub opacity: f32,
    #[serde(default = "default_overlay_thickness")]
    pub thickness: f32,
    pub lines: Vec<OverlayLine>,
}

fn default_overlay_color() -> [u8; 3] { [230, 230, 230] }
fn default_overlay_opacity() -> f32 { 0.6 }
fn default_overlay_thickness() -> f32 { 0.003 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayLine {
    pub freq: f32,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub thickness: Option<f32>,
}

pub fn overlays_dir() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe.parent().unwrap_or(Path::new("."));
    for candidate in [
        std::path::PathBuf::from("overlays"),
        std::path::PathBuf::from("../overlays"),
        exe_dir.join("overlays"),
        exe_dir.join("../overlays"),
        exe_dir.join("../../overlays"),
        exe_dir.join("../../../overlays"),
    ] {
        if candidate.is_dir() {
            return candidate;
        }
    }
    std::path::PathBuf::from("overlays")
}

pub fn load_overlay(name_or_path: &str) -> Option<OverlayConfig> {
    let path = Path::new(name_or_path);
    if path.exists() && path.extension().is_some_and(|e| e == "toml") {
        return load_overlay_file(path).ok();
    }
    let dir = overlays_dir();
    let file = dir.join(format!("{}.toml", name_or_path));
    if file.exists() {
        return load_overlay_file(&file).ok();
    }
    None
}

pub fn load_overlay_file(path: &Path) -> Result<OverlayConfig, crate::CoreError> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| crate::CoreError::Dsp(format!("failed to read overlay: {}", e)))?;
    let config: OverlayConfig = toml::from_str(&data)
        .map_err(|e| crate::CoreError::Dsp(format!("failed to parse overlay: {}", e)))?;
    Ok(config)
}

pub fn builtin_overlay_names() -> Vec<String> {
    let dir = overlays_dir();
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                if let Some(stem) = path.file_stem() {
                    names.push(stem.to_string_lossy().into_owned());
                }
            }
        }
    }
    names.sort();
    names
}
