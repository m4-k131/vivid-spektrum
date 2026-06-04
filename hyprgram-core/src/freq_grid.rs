use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleSource {
    Generated {
        root_note: String,
        root_freq: f32,
        scale_type: String,
        octaves: OctaveRange,
    },
    Custom {
        lines: Vec<FreqLine>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OctaveRange {
    pub start: i32,
    pub end: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FreqLine {
    pub freq: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LineStyles {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<StyleDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub octave: Option<StyleDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<StyleDef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StyleDef {
    pub color: [u8; 3],
    pub width: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScaleConfig {
    pub name: String,
    pub source: ScaleSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub styles: Option<LineStyles>,
}

#[derive(Clone, Debug)]
pub struct GridLine {
    pub y_px: u32,
    pub freq_hz: f32,
    pub label: Option<String>,
    pub color: [u8; 3],
    pub width: u8,
}

impl ScaleConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, CoreError> {
        let data = std::fs::read_to_string(path.as_ref())
            .map_err(|e| CoreError::Dsp(format!("failed to read scale config: {}", e)))?;
        let config: ScaleConfig = serde_json::from_str(&data)
            .map_err(|e| CoreError::Dsp(format!("failed to parse scale config: {}", e)))?;
        Ok(config)
    }

    pub fn to_lines(&self) -> Vec<FreqLine> {
        match &self.source {
            ScaleSource::Generated { root_note, root_freq, scale_type, octaves } => {
                generate_scale_lines(root_note, *root_freq, scale_type, octaves, &self.styles)
            }
            ScaleSource::Custom { lines } => lines.clone(),
        }
    }
}

fn generate_scale_lines(
    root_note: &str,
    root_freq: f32,
    scale_type: &str,
    octaves: &OctaveRange,
    styles: &Option<LineStyles>,
) -> Vec<FreqLine> {
    let intervals = match scale_type {
        "chromatic" => (0..12).map(|i| i as f32).collect::<Vec<_>>(),
        "major" => vec![0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0],
        "minor" => vec![0.0, 2.0, 3.0, 5.0, 7.0, 8.0, 10.0],
        "pentatonic_major" => vec![0.0, 2.0, 4.0, 7.0, 9.0],
        "pentatonic_minor" => vec![0.0, 3.0, 5.0, 7.0, 10.0],
        _ => (0..12).map(|i| i as f32).collect(),
    };

    let root_semitones = note_to_semitones(root_note).unwrap_or(69) as f32;
    let root_a4_offset = root_freq / 440.0_f32;
    let style_default = StyleDef { color: [150, 150, 150], width: 1 };
    let style_root = styles.as_ref().and_then(|s| s.root.clone()).unwrap_or(StyleDef { color: [255, 100, 100], width: 2 });
    let style_octave = styles.as_ref().and_then(|s| s.octave.clone()).unwrap_or(StyleDef { color: [200, 200, 200], width: 1 });

    let mut lines = Vec::new();

    for oct in octaves.start..=octaves.end {
        for (_, &interval) in intervals.iter().enumerate() {
            let semitones = (oct * 12) as f32 + interval - (root_semitones - 69.0);
            let freq = 440.0 * root_a4_offset * 2.0_f32.powf(semitones / 12.0);
            let note_name = semitones_to_note(((root_semitones as i32 + interval as i32) % 12) as u8);
            let full_note = format!("{}{}", note_name, oct);

            let is_root = interval == 0.0;
            let style = if is_root { &style_root } else { &style_default };

            lines.push(FreqLine {
                freq,
                label: Some(full_note),
                color: Some(style.color),
                width: Some(style.width),
                style: if is_root { Some("root".to_string()) } else { None },
            });
        }
    }

    for oct in (octaves.start..=octaves.end).step_by(1) {
        let semitones = (oct * 12) as f32 - (root_semitones - 69.0);
        let freq = 440.0 * root_a4_offset * 2.0_f32.powf(semitones / 12.0);
        if lines.iter().any(|l| (l.freq - freq).abs() < 0.1) {
            continue;
        }
        let note_name = semitones_to_note((root_semitones as i32 % 12) as u8);
        let full_note = format!("{}{}", note_name, oct);

        lines.push(FreqLine {
            freq,
            label: Some(full_note),
            color: Some(style_octave.color),
            width: Some(style_octave.width),
            style: Some("octave".to_string()),
        });
    }

    lines.sort_by(|a, b| a.freq.partial_cmp(&b.freq).unwrap());
    lines
}

fn note_to_semitones(note: &str) -> Option<u8> {
    let note = note.to_uppercase();
    let base = match note.chars().next()? {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let accidental = if note.contains('#') || note.contains("SHARP") {
        1
    } else if note.contains('B') || note.contains("FLAT") {
        -1
    } else {
        0
    };
    Some((base as i8 + accidental) as u8)
}

fn semitones_to_note(semitones: u8) -> &'static str {
    match semitones % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        _ => "?",
    }
}

pub fn compute_grid_lines(
    config: &ScaleConfig,
    f_min_hz: f32,
    f_max_hz: f32,
    height_px: u32,
    log_bins: usize,
) -> Vec<GridLine> {
    let lines = config.to_lines();
    let mut result = Vec::new();

    for line in lines {
        if line.freq < f_min_hz || line.freq > f_max_hz {
            continue;
        }

        let y_px = freq_to_y_px(line.freq, f_min_hz, f_max_hz, height_px, log_bins);
        if y_px >= height_px {
            continue;
        }

        let color = line.color.unwrap_or([150, 150, 150]);
        let width = line.width.unwrap_or(1);

        result.push(GridLine {
            y_px,
            freq_hz: line.freq,
            label: line.label,
            color,
            width,
        });
    }

    result.sort_by(|a, b| a.y_px.cmp(&b.y_px));
    result.dedup_by(|a, b| (a.y_px as i32 - b.y_px as i32).abs() < 2);

    result
}

fn freq_to_y_px(freq: f32, f_min: f32, f_max: f32, height: u32, _log_bins: usize) -> u32 {
    let t = (freq.log10() - f_min.log10()) / (f_max.log10() - f_min.log10());
    let bin_f = t * (height.saturating_sub(1).max(1) as f32);
    (height.saturating_sub(1) as f32 - bin_f).round().clamp(0.0, height.saturating_sub(1) as f32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_semitones_roundtrip() {
        assert_eq!(note_to_semitones("A"), Some(9));
        assert_eq!(note_to_semitones("C"), Some(0));
        assert_eq!(note_to_semitones("F#"), Some(6));
        assert_eq!(note_to_semitones("Bb"), Some(10));
    }

    #[test]
    fn generated_chromatic_has_12_per_octave() {
        let config = ScaleConfig {
            name: "test".to_string(),
            source: ScaleSource::Generated {
                root_note: "C".to_string(),
                root_freq: 261.63,
                scale_type: "chromatic".to_string(),
                octaves: OctaveRange { start: 4, end: 4 },
            },
            styles: None,
        };
        let lines = config.to_lines();
        assert!(lines.len() >= 12, "expected at least 12 lines, got {}", lines.len());
    }

    #[test]
    fn freq_to_y_is_monotonic() {
        let f_min = 20.0;
        let f_max = 20000.0;
        let height = 256;
        let log_bins = 256;

        let y_100 = freq_to_y_px(100.0, f_min, f_max, height, log_bins);
        let y_1000 = freq_to_y_px(1000.0, f_min, f_max, height, log_bins);
        let y_10000 = freq_to_y_px(10000.0, f_min, f_max, height, log_bins);

        assert!(y_100 > y_1000, "lower freq should be higher Y");
        assert!(y_1000 > y_10000, "lower freq should be higher Y");
    }
}
