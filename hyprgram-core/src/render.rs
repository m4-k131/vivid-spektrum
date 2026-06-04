use crate::colormap;
use crate::freq_grid::{compute_grid_lines, ScaleConfig};
use crate::{CoreError, SpectrumConfig, SpectrumProcessor};
use image::{ImageBuffer, Rgb};
use rayon::prelude::*;
use std::path::Path;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SpectrogramImageConfig {
    pub spectrum: SpectrumConfig,
    pub width: u32,
    pub height: u32,
    pub scroll_right_to_left: bool,
    #[serde(default = "default_colormap_name")]
    pub colormap: String,
}

fn default_colormap_name() -> String { "viridis".into() }

impl Default for SpectrogramImageConfig {
    fn default() -> Self {
        Self {
            spectrum: SpectrumConfig::default(),
            width: 800,
            height: 200,
            scroll_right_to_left: true,
            colormap: "viridis".into(),
        }
    }
}

pub fn samples_to_spectrogram(
    samples: &[f32],
    spectrum: SpectrumConfig,
) -> Result<Vec<Vec<f32>>, CoreError> {
    let n = samples.len();
    let w = spectrum.window_size;
    let h = spectrum.hop_size;
    if n < w {
        let mut processor = SpectrumProcessor::new(spectrum)?;
        let mut columns = Vec::new();
        processor.push_samples(samples, &mut columns);
        return Ok(columns);
    }
    let num_windows = (n - w) / h + 1;
    let num_threads = rayon::current_num_threads().max(1);
    let windows_per_chunk = (num_windows + num_threads - 1) / num_threads;
    let chunks: Vec<(usize, usize)> = (0..num_windows)
        .step_by(windows_per_chunk)
        .map(|start| {
            let end = (start + windows_per_chunk).min(num_windows);
            (start, end)
        })
        .collect();
    let results: Vec<Vec<Vec<f32>>> = chunks
        .par_iter()
        .map(|&(j_start, j_end)| {
            let sample_start = j_start * h;
            let sample_end = ((j_end - 1) * h + w).min(n);
            let mut processor = SpectrumProcessor::new(spectrum.clone())
                .expect("spectrum processor");
            let mut columns = Vec::with_capacity(j_end - j_start);
            processor.push_samples(&samples[sample_start..sample_end], &mut columns);
            columns
        })
        .collect();
    let total: Vec<Vec<f32>> = results.into_iter().flatten().collect();
    Ok(total)
}

pub fn render_spectrogram_png<P: AsRef<Path>>(
    columns: &[Vec<f32>],
    config: &SpectrogramImageConfig,
    output: P,
) -> Result<(), CoreError> {
    render_spectrogram_png_with_grid(columns, config, output, None)
}

pub fn render_spectrogram_png_with_grid<P: AsRef<Path>>(
    columns: &[Vec<f32>],
    config: &SpectrogramImageConfig,
    output: P,
    scale: Option<&ScaleConfig>,
) -> Result<(), CoreError> {
    let width = config.width.max(1);
    let height = config.height.max(1);
    let bins = config.spectrum.log_bins.max(1);
    let cmap = colormap::builtin_colormap(&config.colormap)
        .unwrap_or_else(colormap::default_colormap);
    let lut = cmap.build_lut(256);
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let value = if config.scroll_right_to_left {
                let col = sample_index(x, width, columns.len());
                let bin = sample_index(height - 1 - y, height, bins);
                sample_column(columns, col, bin)
            } else {
                let col = sample_index(y, height, columns.len());
                let bin = sample_index(x, width, bins);
                sample_column(columns, col, bin)
            };
            let idx = (value.clamp(0.0, 1.0) * 255.0).round() as usize;
            img.put_pixel(x, y, Rgb(lut[idx]));
        }
    }

    if let Some(scale_config) = scale {
        let grid = compute_grid_lines(
            scale_config,
            config.spectrum.f_min_hz,
            config.spectrum.f_max_hz,
            height,
            bins,
        );
        for line in grid {
            let y = line.y_px.min(height - 1);
            for x in 0..width {
                draw_line_pixel(&mut img, x, y, &line.color, line.width);
            }
        }
    }

    img.save(output)
        .map_err(|e| CoreError::Dsp(format!("failed to write PNG: {e}")))?;
    Ok(())
}

fn draw_line_pixel(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, x: u32, y: u32, color: &[u8; 3], width: u8) {
    let width = width.max(1);
    let half = (width / 2) as i32;
    let y_center = y as i32;

    for dy in -half..=half {
        let py = (y_center + dy) as u32;
        if py < img.height() {
            let existing = img.get_pixel(x, py);
            let blended = blend_pixel(existing.0, *color);
            img.put_pixel(x, py, Rgb(blended));
        }
    }
}

fn blend_pixel(bg: [u8; 3], fg: [u8; 3]) -> [u8; 3] {
    let alpha = 0.7f32;
    [
        (fg[0] as f32 * alpha + bg[0] as f32 * (1.0 - alpha)).round() as u8,
        (fg[1] as f32 * alpha + bg[1] as f32 * (1.0 - alpha)).round() as u8,
        (fg[2] as f32 * alpha + bg[2] as f32 * (1.0 - alpha)).round() as u8,
    ]
}

fn sample_index(pos: u32, extent: u32, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    ((pos as f32 / extent.saturating_sub(1).max(1) as f32) * (len - 1) as f32).round() as usize
}

fn sample_column(columns: &[Vec<f32>], col: usize, bin: usize) -> f32 {
    columns
        .get(col)
        .and_then(|c| c.get(bin))
        .copied()
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_index_first() {
        assert_eq!(sample_index(0, 100, 10), 0);
    }

    #[test]
    fn sample_index_last() {
        assert_eq!(sample_index(99, 100, 10), 9);
    }

    #[test]
    fn sample_index_middle() {
        let idx = sample_index(50, 100, 10);
        assert!(idx == 4 || idx == 5);
    }

    #[test]
    fn sample_index_single_element() {
        assert_eq!(sample_index(0, 100, 1), 0);
        assert_eq!(sample_index(99, 100, 1), 0);
    }

    #[test]
    fn sample_index_zero_len() {
        assert_eq!(sample_index(0, 100, 0), 0);
    }

    #[test]
    fn sample_column_returns_value() {
        let columns = vec![vec![0.5f32, 0.8], vec![0.3, 0.1]];
        assert!((sample_column(&columns, 0, 0) - 0.5).abs() < 0.001);
        assert!((sample_column(&columns, 0, 1) - 0.8).abs() < 0.001);
        assert!((sample_column(&columns, 1, 0) - 0.3).abs() < 0.001);
    }

    #[test]
    fn sample_column_out_of_bounds_returns_zero() {
        let columns = vec![vec![0.5f32]];
        assert_eq!(sample_column(&columns, 1, 0), 0.0);
        assert_eq!(sample_column(&columns, 0, 1), 0.0);
    }

    #[test]
    fn sample_column_empty_columns() {
        let columns: Vec<Vec<f32>> = vec![];
        assert_eq!(sample_column(&columns, 0, 0), 0.0);
    }

    #[test]
    fn samples_to_spectrogram_too_few_samples() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 512,
            sample_rate: 48000,
            log_bins: 64,
            ..Default::default()
        };
        let samples = vec![0.0f32; 512];
        let result = samples_to_spectrogram(&samples, cfg).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn samples_to_spectrogram_produces_columns() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 512,
            sample_rate: 48000,
            log_bins: 64,
            ..Default::default()
        };
        let samples = vec![0.0f32; 2048];
        let result = samples_to_spectrogram(&samples, cfg).unwrap();
        let expected = (2048 - 1024) / 512 + 1;
        assert_eq!(result.len(), expected);
    }

    #[test]
    fn samples_to_spectrogram_parallel_matches_sequential() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 256,
            sample_rate: 48000,
            log_bins: 64,
            ..Default::default()
        };
        let n = 4096;
        let samples: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin()).collect();
        let result = samples_to_spectrogram(&samples, cfg).unwrap();
        assert!(!result.is_empty());
        for col in &result {
            assert_eq!(col.len(), 64);
            for &v in col {
                assert!(v.is_finite() && v >= 0.0 && v <= 1.0);
            }
        }
    }

    #[test]
    fn spectrogram_image_config_default_colormap() {
        let cfg = SpectrogramImageConfig::default();
        assert_eq!(cfg.colormap, "viridis");
        assert_eq!(cfg.width, 800);
        assert_eq!(cfg.height, 200);
    }
}
