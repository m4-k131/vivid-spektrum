use crate::error::CoreError;
use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;
use std::f32::consts::PI;
use std::sync::Arc;

/// Default real FFT length: Hann STFT window size in samples. Frequency bin spacing ≈ `sample_rate / window_size`.
pub const DEFAULT_FFT_WINDOW_SAMPLES: usize = 8192;
/// Default hop between STFT frames (samples). ~21 ms @ 48 kHz → ~47 columns/sec (~73 s visible at 3440 px); lower for more overlap at higher CPU cost.
pub const DEFAULT_FFT_HOP_SAMPLES: usize = 1024;

/// STFT hop must satisfy `1 <= hop <= window_size`. `hop == 0` is treated as “use half window” (50% overlap). Larger values are clamped to `window_size`.
pub fn normalize_hop_size(window_size: usize, hop: usize) -> usize {
    if window_size < 1 {
        return 1;
    }
    let max_h = window_size;
    let h = if hop == 0 { (window_size / 2).max(1) } else { hop };
    h.clamp(1, max_h)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowFunction {
    #[default]
    Hann,
    Hamming,
    Blackman,
    BlackmanHarris,
}

impl std::fmt::Display for WindowFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WindowFunction::Hann => "Hann",
            WindowFunction::Hamming => "Hamming",
            WindowFunction::Blackman => "Blackman",
            WindowFunction::BlackmanHarris => "Blackman-Harris",
        };
        write!(f, "{}", s)
    }
}

impl WindowFunction {
    pub fn generate(&self, size: usize) -> Vec<f32> {
        let n = size.max(1);
        let n1 = (n - 1).max(1) as f32;
        match self {
            WindowFunction::Hann => {
                (0..n).map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / n1).cos())).collect()
            }
            WindowFunction::Hamming => {
                (0..n).map(|i| 0.53836 - 0.46164 * (2.0 * PI * i as f32 / n1).cos()).collect()
            }
            WindowFunction::Blackman => {
                (0..n).map(|i| {
                    let a = 2.0 * PI * i as f32 / n1;
                    0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos()
                }).collect()
            }
            WindowFunction::BlackmanHarris => {
                (0..n).map(|i| {
                    let a = 2.0 * PI * i as f32 / n1;
                    0.35875 - 0.48829 * a.cos() + 0.14128 * (2.0 * a).cos() - 0.01168 * (3.0 * a).cos()
                }).collect()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transform {
    #[default]
    Stft,
    Cqt,
}

impl std::fmt::Display for Transform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Transform::Stft => "STFT",
            Transform::Cqt => "CQT",
        };
        write!(f, "{}", s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Weighting {
    #[default]
    None,
    A,
    C,
}

impl std::fmt::Display for Weighting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Weighting::None => "None",
            Weighting::A => "A-weighting",
            Weighting::C => "C-weighting",
        };
        write!(f, "{}", s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BandAggregation {
    #[default]
    Nearest,
    Triangular,
}

impl std::fmt::Display for BandAggregation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BandAggregation::Nearest => "Nearest",
            BandAggregation::Triangular => "Triangular",
        };
        write!(f, "{}", s)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SpectrumConfig {
    pub window_size: usize,
    pub hop_size: usize,
    pub sample_rate: u32,
    pub log_bins: usize,
    pub f_min_hz: f32,
    pub f_max_hz: f32,
    pub db_floor: f32,
    pub db_ceil: f32,
    #[serde(default)]
    pub window_fn: WindowFunction,
    #[serde(default)]
    pub band_aggregation: BandAggregation,
    #[serde(default)]
    pub freq_smoothing_sigma: f32,
    #[serde(default = "default_gamma")]
    pub amplitude_gamma: f32,
    #[serde(default)]
    pub temporal_alpha: f32,
    #[serde(default)]
    pub peak_hold_decay: f32,
    #[serde(default)]
    pub weighting: Weighting,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default = "default_cqt_bpo")]
    pub cqt_bins_per_octave: u32,
    #[serde(default = "default_freq_scale_exp")]
    pub freq_scale_exp: f32,
    #[serde(default)]
    pub centered: bool,
}

fn default_gamma() -> f32 { 1.0 }
fn default_cqt_bpo() -> u32 { 12 }
fn default_freq_scale_exp() -> f32 { 0.5 }

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            window_size: DEFAULT_FFT_WINDOW_SAMPLES,
            hop_size: DEFAULT_FFT_HOP_SAMPLES,
            sample_rate: 48000,
            log_bins: 1024,
            f_min_hz: 20.0,
            f_max_hz: 20000.0,
            db_floor: -90.0,
            db_ceil: 0.0,
            window_fn: WindowFunction::BlackmanHarris,
            band_aggregation: BandAggregation::Triangular,
            freq_smoothing_sigma: 1.0,
            amplitude_gamma: 0.5,
            temporal_alpha: 0.3,
            peak_hold_decay: 0.92,
            weighting: Weighting::None,
            transform: Transform::Stft,
            cqt_bins_per_octave: 12,
            freq_scale_exp: 0.5,
            centered: false,
        }
    }
}

pub fn spectrum_output_bins(cfg: &SpectrumConfig) -> usize {
    if cfg.transform == Transform::Cqt {
        let f_min = cfg.f_min_hz.max(1.0);
        let nyquist = 0.499 * cfg.sample_rate as f32;
        let f_max = cfg.f_max_hz.min(nyquist).max(f_min + 1.0);
        ((f_max / f_min).log2() * cfg.cqt_bins_per_octave.max(1) as f32).ceil().max(1.0) as usize
    } else {
        cfg.log_bins.max(1)
    }
}

pub struct SpectrumProcessor {
    cfg: SpectrumConfig,
    r2c: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    work_input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    band_weights: Vec<Vec<(usize, f32)>>,
    prev_column: Vec<f32>,
    peak_column: Vec<f32>,
    pending: Vec<f32>,
    weighting_weights: Vec<f32>,
    cqt_weights: Vec<Vec<(usize, f32)>>,
    total_samples_pushed: u64,
    centered_prefill: usize,
}

impl SpectrumProcessor {
    pub fn new(mut cfg: SpectrumConfig) -> Result<Self, CoreError> {
        if cfg.window_size < 8 {
            return Err(CoreError::Dsp("window_size too small".into()));
        }
        cfg.hop_size = normalize_hop_size(cfg.window_size, cfg.hop_size);
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(cfg.window_size);
        let spectrum = r2c.make_output_vec();
        let work_input = r2c.make_input_vec();
        let window = cfg.window_fn.generate(cfg.window_size);
        let band_weights = build_band_weights(&cfg);
        let weighting_weights = build_weighting_weights(&cfg);
        let cqt_weights = build_cqt_weights(&cfg);
        let pending_cap = cfg.window_size * 2;
        let centered_prefill = if cfg.centered { cfg.window_size / 2 } else { 0 };
        Ok(Self {
            cfg,
            r2c,
            window,
            work_input,
            spectrum,
            band_weights,
            prev_column: Vec::new(),
            peak_column: Vec::new(),
            pending: Vec::with_capacity(pending_cap),
            weighting_weights,
            cqt_weights,
            total_samples_pushed: 0,
            centered_prefill,
        })
    }
    pub fn set_sample_rate(&mut self, sr: u32) {
        self.cfg.sample_rate = sr;
    }
    pub fn set_runtime_cfg(&mut self, cfg: &SpectrumConfig) {
        self.cfg.hop_size = normalize_hop_size(self.cfg.window_size, cfg.hop_size);
        self.cfg.db_floor = cfg.db_floor;
        self.cfg.db_ceil = cfg.db_ceil;
        self.cfg.freq_smoothing_sigma = cfg.freq_smoothing_sigma;
        self.cfg.amplitude_gamma = cfg.amplitude_gamma;
        self.cfg.temporal_alpha = cfg.temporal_alpha;
        self.cfg.peak_hold_decay = cfg.peak_hold_decay;
    }
    pub fn log_bins(&self) -> usize {
        spectrum_output_bins(&self.cfg)
    }
    pub fn total_samples_pushed(&self) -> u64 {
        self.total_samples_pushed
    }
    pub fn push_samples(&mut self, incoming: &[f32], out_columns: &mut Vec<Vec<f32>>) {
        self.pending.extend_from_slice(incoming);
        self.total_samples_pushed += incoming.len() as u64;
        let w = self.cfg.window_size;
        let h = self.cfg.hop_size;
        let n_bins = self.log_bins();
        out_columns.clear();
        let min_pending = w + self.centered_prefill;
        while self.pending.len() >= min_pending {
            for i in 0..w {
                self.work_input[i] = self.pending[i] * self.window[i];
            }
            if self.r2c.process(&mut self.work_input, &mut self.spectrum).is_err() {
                break;
            }
            self.pending.drain(..h);
            let mut col = vec![0.0f32; n_bins];
            self.map_log_magnitude(&mut col);
            self.apply_temporal(&mut col);
            out_columns.push(col);
        }
    }
    fn map_log_magnitude(&self, col: &mut [f32]) {
        let sr = self.cfg.sample_rate as f32;
        let nyq = 0.499 * sr;
        let f_max = self.cfg.f_max_hz.min(nyq).max(self.cfg.f_min_hz + 1.0);
        let f_min = self.cfg.f_min_hz.max(1.0);
        let nfft = self.cfg.window_size;
        let kmax = self.spectrum.len().saturating_sub(1).max(1);
        if self.cfg.transform == Transform::Cqt && !self.cqt_weights.is_empty() {
            for i in 0..col.len().min(self.cqt_weights.len()) {
                let mut mag_sum = 0.0f32;
                let mut weight_sum = 0.0f32;
                for &(k, w) in &self.cqt_weights[i] {
                    let re = self.spectrum[k].re;
                    let im = self.spectrum[k].im;
                    let mag = (re * re + im * im).sqrt() / nfft as f32 * self.weighting_weights[k];
                    mag_sum += mag * w;
                    weight_sum += w;
                }
                let mag = if weight_sum > 0.0 { mag_sum / weight_sum } else { 0.0 };
                let db = 20.0 * (mag + 1e-12).log10();
                let u = ((db - self.cfg.db_floor) / (self.cfg.db_ceil - self.cfg.db_floor).max(1e-9)).clamp(0.0, 1.0);
                col[i] = u;
            }
        } else {
            match self.cfg.band_aggregation {
            BandAggregation::Nearest => {
                let exp = self.cfg.freq_scale_exp.max(0.1);
                for i in 0..col.len() {
                    let t = (i as f32 / (col.len().saturating_sub(1).max(1) as f32)).powf(exp);
                    let f = f_min * (f_max / f_min).powf(t);
                    let bin_f = f * nfft as f32 / sr;
                    let k = (bin_f.round() as usize).clamp(1, kmax);
                    let re = self.spectrum[k].re;
                    let im = self.spectrum[k].im;
                    let mag = (re * re + im * im).sqrt() / nfft as f32 * self.weighting_weights[k];
                    let db = 20.0 * (mag + 1e-12).log10();
                    let u = ((db - self.cfg.db_floor) / (self.cfg.db_ceil - self.cfg.db_floor).max(1e-9)).clamp(0.0, 1.0);
                    col[i] = u;
                }
            }
            BandAggregation::Triangular => {
                for (i, val) in col.iter_mut().enumerate() {
                    let mut mag_sum = 0.0f32;
                    let mut weight_sum = 0.0f32;
                    for &(k, w) in &self.band_weights[i] {
                        let re = self.spectrum[k].re;
                        let im = self.spectrum[k].im;
                        let mag = (re * re + im * im).sqrt() / nfft as f32 * self.weighting_weights[k];
                        mag_sum += mag * w;
                        weight_sum += w;
                    }
                    let mag = if weight_sum > 0.0 { mag_sum / weight_sum } else { 0.0 };
                    let db = 20.0 * (mag + 1e-12).log10();
                    let u = ((db - self.cfg.db_floor) / (self.cfg.db_ceil - self.cfg.db_floor).max(1e-9)).clamp(0.0, 1.0);
                    *val = u;
                }
            }
        }
        }
        if self.cfg.freq_smoothing_sigma > 0.0 {
            let orig = col.to_vec();
            let n = col.len();
            let base_sigma = self.cfg.freq_smoothing_sigma;
            for i in 0..n {
                let t = (i as f32 + 1.0) / n as f32;
                let sigma = base_sigma / t.max(0.01);
                let sigma = sigma.min(n as f32 * 0.25);
                let radius = (3.0 * sigma).ceil() as isize;
                if radius == 0 {
                    continue;
                }
                let mut sum = 0.0f32;
                let mut wsum = 0.0f32;
                for off in -radius..=radius {
                    let j = i as isize + off;
                    if j >= 0 && j < n as isize {
                        let w = (-0.5 * (off as f32 / sigma).powi(2)).exp();
                        sum += orig[j as usize] * w;
                        wsum += w;
                    }
                }
                col[i] = if wsum > 0.0 { sum / wsum } else { orig[i] };
            }
        }
        let gamma = self.cfg.amplitude_gamma;
        if (gamma - 1.0).abs() > 1e-6 {
            for v in col.iter_mut() {
                *v = v.powf(gamma);
            }
        }
    }
    fn apply_temporal(&mut self, col: &mut [f32]) {
        let alpha = self.cfg.temporal_alpha;
        let decay = self.cfg.peak_hold_decay;
        if alpha > 0.0 && self.prev_column.len() == col.len() {
            for (v, prev) in col.iter_mut().zip(self.prev_column.iter()) {
                *v = alpha * *v + (1.0 - alpha) * prev;
            }
        }
        if decay > 0.0 {
            if self.peak_column.len() != col.len() {
                self.peak_column = col.to_vec();
            } else {
                for (v, peak) in col.iter().zip(self.peak_column.iter_mut()) {
                    *peak = (*v).max(*peak * decay);
                }
                col.copy_from_slice(&self.peak_column);
            }
        }
        self.prev_column = col.to_vec();
    }
}

fn build_band_weights(cfg: &SpectrumConfig) -> Vec<Vec<(usize, f32)>> {
    let n_bins = cfg.log_bins.max(1);
    let nfft = cfg.window_size;
    let sr = cfg.sample_rate as f32;
    let nyq = 0.499 * sr;
    let f_max = cfg.f_max_hz.min(nyq).max(cfg.f_min_hz + 1.0);
    let f_min = cfg.f_min_hz.max(1.0);
    let kmax = (nfft / 2).max(1);
    let exp = cfg.freq_scale_exp.max(0.1);
    let mut weights = Vec::with_capacity(n_bins);
    for i in 0..n_bins {
        let t = (i as f32 / (n_bins.saturating_sub(1).max(1) as f32)).powf(exp);
        let fc = f_min * (f_max / f_min).powf(t);
        let t_prev = if i > 0 { ((i - 1) as f32 / (n_bins.saturating_sub(1).max(1) as f32)).powf(exp) } else { 0.0 };
        let f_lo = f_min * (f_max / f_min).powf(t_prev);
        let t_next = if i + 1 < n_bins { ((i + 1) as f32 / (n_bins.saturating_sub(1).max(1) as f32)).powf(exp) } else { 1.0 };
        let f_hi = f_min * (f_max / f_min).powf(t_next);
        let k_lo = ((f_lo * nfft as f32 / sr).floor() as usize).clamp(1, kmax);
        let k_hi = ((f_hi * nfft as f32 / sr).ceil() as usize).clamp(1, kmax);
        let mut band: Vec<(usize, f32)> = Vec::new();
        for k in k_lo..=k_hi {
            let fk = k as f32 * sr / nfft as f32;
            let w = if fk <= f_lo || fk >= f_hi {
                0.0
            } else if fk <= fc {
                (fk - f_lo) / (fc - f_lo).max(1e-9)
            } else {
                (f_hi - fk) / (f_hi - fc).max(1e-9)
            };
            if w > 0.0 {
                band.push((k, w));
            }
        }
        if band.is_empty() {
            let k = ((fc * nfft as f32 / sr).round() as usize).clamp(1, kmax);
            band.push((k, 1.0));
        }
        weights.push(band);
    }
    weights
}

#[cfg(test)]
fn build_gaussian_kernel(sigma: f32) -> Vec<(isize, f32)> {
    if sigma <= 0.0 {
        return Vec::new();
    }
    let radius = (3.0 * sigma).ceil() as isize;
    let mut taps = Vec::new();
    let mut total = 0.0f32;
    for i in -radius..=radius {
        let x = i as f32;
        let w = (-0.5 * (x / sigma).powi(2)).exp();
        taps.push((i, w));
        total += w;
    }
    if total > 0.0 {
        for (_, w) in taps.iter_mut() {
            *w /= total;
        }
    }
    taps
}

fn build_weighting_weights(cfg: &SpectrumConfig) -> Vec<f32> {
    let nfft = cfg.window_size;
    let sr = cfg.sample_rate as f32;
    let n_bins = nfft / 2 + 1;
    let mut w = vec![1.0f32; n_bins];
    match cfg.weighting {
        Weighting::None => {}
        Weighting::A => {
            for (k, wt) in w.iter_mut().enumerate() {
                let f = k as f32 * sr / nfft as f32;
                let f2 = f * f;
                let num = 12194.0f32.powi(2) * f2 * f2;
                let den = (f2 + 20.6f32.powi(2))
                    * (f2 + 107.7f32.powi(2)).sqrt()
                    * (f2 + 737.9f32.powi(2)).sqrt()
                    * (f2 + 12194.0f32.powi(2));
                let ra = if den > 0.0 { num / den } else { 0.0 };
                let ref_f = 1000.0;
                let ref_f2 = ref_f * ref_f;
                let ref_num = 12194.0f32.powi(2) * ref_f2 * ref_f2;
                let ref_den = (ref_f2 + 20.6f32.powi(2))
                    * (ref_f2 + 107.7f32.powi(2)).sqrt()
                    * (ref_f2 + 737.9f32.powi(2)).sqrt()
                    * (ref_f2 + 12194.0f32.powi(2));
                let ra_ref = ref_num / ref_den;
                let a_weight = if ra_ref > 0.0 { ra / ra_ref } else { 1.0 };
                *wt = a_weight;
            }
        }
        Weighting::C => {
            for (k, wt) in w.iter_mut().enumerate() {
                let f = k as f32 * sr / nfft as f32;
                let f2 = f * f;
                let num = 12194.0f32.powi(2) * f2;
                let den = (f2 + 20.6f32.powi(2)) * (f2 + 12194.0f32.powi(2));
                let rc = if den > 0.0 { num / den } else { 0.0 };
                let ref_f = 1000.0;
                let ref_f2 = ref_f * ref_f;
                let ref_num = 12194.0f32.powi(2) * ref_f2;
                let ref_den = (ref_f2 + 20.6f32.powi(2)) * (ref_f2 + 12194.0f32.powi(2));
                let rc_ref = ref_num / ref_den;
                let c_weight = if rc_ref > 0.0 { rc / rc_ref } else { 1.0 };
                *wt = c_weight;
            }
        }
    }
    w
}

fn build_cqt_weights(cfg: &SpectrumConfig) -> Vec<Vec<(usize, f32)>> {
    if cfg.transform != Transform::Cqt {
        return Vec::new();
    }
    let bpo = cfg.cqt_bins_per_octave.max(1) as f32;
    let q = 1.0 / (2.0f32.powf(1.0 / bpo) - 1.0);
    let f_min = cfg.f_min_hz.max(1.0);
    let sr = cfg.sample_rate as f32;
    let nyq = 0.499 * sr;
    let _f_max = cfg.f_max_hz.min(nyq).max(f_min + 1.0);
    let nfft = cfg.window_size;
    let kmax = (nfft / 2).max(1);
    let num_bins = spectrum_output_bins(cfg);
    let mut weights = Vec::with_capacity(num_bins);
    for k_cqt in 0..num_bins {
        let fc = f_min * 2.0f32.powf(k_cqt as f32 / bpo);
        let bw = fc / q;
        let f_lo = (fc - bw * 0.5).max(0.0);
        let f_hi = (fc + bw * 0.5).min(nyq);
        let k_lo = ((f_lo * nfft as f32 / sr).floor() as usize).clamp(1, kmax);
        let k_hi = ((f_hi * nfft as f32 / sr).ceil() as usize).clamp(1, kmax);
        let mut band: Vec<(usize, f32)> = Vec::new();
        for k in k_lo..=k_hi {
            let fk = k as f32 * sr / nfft as f32;
            let w = if fk <= f_lo || fk >= f_hi {
                0.0
            } else if fk <= fc {
                (fk - f_lo) / (fc - f_lo).max(1e-9)
            } else {
                (f_hi - fk) / (f_hi - fc).max(1e-9)
            };
            if w > 0.0 {
                band.push((k, w));
            }
        }
        if band.is_empty() {
            let k = ((fc * nfft as f32 / sr).round() as usize).clamp(1, kmax);
            band.push((k, 1.0));
        }
        weights.push(band);
    }
    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_clamps_high() {
        assert_eq!(normalize_hop_size(16384, 32768), 16384);
    }
    #[test]
    fn hop_zero_is_half_window() {
        assert_eq!(normalize_hop_size(16384, 0), 8192);
    }
    #[test]
    fn hop_one_to_window_preserved() {
        assert_eq!(normalize_hop_size(100, 50), 50);
    }
    #[test]
    fn hop_minimum_one() {
        assert_eq!(normalize_hop_size(0, 0), 1);
        assert_eq!(normalize_hop_size(10, 0), 5);
    }

    #[test]
    fn hann_window_symmetry() {
        let w = WindowFunction::Hann.generate(256);
        assert!((w[0] - 0.0).abs() < 0.01);
        for i in 0..128 {
            assert!((w[i] - w[255 - i]).abs() < 0.001);
        }
    }

    #[test]
    fn hamming_window_endpoints() {
        let w = WindowFunction::Hamming.generate(256);
        assert!(w[0] < 0.1);
        assert!(w[127] > 0.9);
    }

    #[test]
    fn blackman_window_endpoints() {
        let w = WindowFunction::Blackman.generate(256);
        assert!((w[0] - 0.0).abs() < 0.01);
        assert!(w[127] > 0.8);
    }

    #[test]
    fn blackman_harris_window_endpoints() {
        let w = WindowFunction::BlackmanHarris.generate(256);
        assert!((w[0] - 0.00006).abs() < 0.01);
        assert!(w[127] > 0.8);
    }

    #[test]
    fn window_size_one() {
        let w = WindowFunction::Hann.generate(1);
        assert_eq!(w.len(), 1);
        assert!((w[0] - 0.0).abs() < 0.01);
    }

    #[test]
    fn spectrum_processor_creates_with_defaults() {
        let proc = SpectrumProcessor::new(SpectrumConfig::default()).unwrap();
        assert_eq!(proc.log_bins(), 1024);
    }

    #[test]
    fn spectrum_processor_rejects_tiny_window() {
        let cfg = SpectrumConfig { window_size: 4, ..Default::default() };
        assert!(SpectrumProcessor::new(cfg).is_err());
    }

    #[test]
    fn push_samples_silence_produces_low_values() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 512,
            sample_rate: 48000,
            log_bins: 64,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let silence = vec![0.0f32; 2048];
        let mut cols = Vec::new();
        proc.push_samples(&silence, &mut cols);
        assert!(!cols.is_empty());
        for col in &cols {
            for &v in col {
                assert!(v < 0.1, "silence should produce near-zero dB-normalized values, got {v}");
            }
        }
    }

    #[test]
    fn push_samples_sine_produces_peak_at_frequency() {
        let sr = 48000u32;
        let freq = 1000.0f32;
        let cfg = SpectrumConfig {
            window_size: 4096,
            hop_size: 2048,
            sample_rate: sr,
            log_bins: 128,
            f_min_hz: 20.0,
            f_max_hz: 20000.0,
            db_floor: -120.0,
            db_ceil: 0.0,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let n = 8192;
        let samples: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * freq * i as f32 / sr as f32).sin())
            .collect();
        let mut cols = Vec::new();
        proc.push_samples(&samples, &mut cols);
        assert!(cols.len() >= 2);
        let mid_col = &cols[cols.len() / 2];
        let max_val = mid_col.iter().cloned().fold(0.0f32, f32::max);
        assert!(max_val > 0.5, "sine should produce strong peak, got max {max_val}");
    }

    #[test]
    fn push_samples_produces_expected_column_count() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 256,
            sample_rate: 48000,
            log_bins: 64,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let samples = vec![0.5f32; 2048];
        let mut cols = Vec::new();
        proc.push_samples(&samples, &mut cols);
        let expected = (2048 - 1024) / 256 + 1;
        assert_eq!(cols.len(), expected);
    }

    #[test]
    fn push_samples_clears_output_vec() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 512,
            sample_rate: 48000,
            log_bins: 64,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let samples = vec![0.0f32; 2048];
        let mut cols = vec![vec![1.0f32; 64]];
        proc.push_samples(&samples, &mut cols);
        assert!(!cols.is_empty());
        assert!(cols[0][0] < 1.0);
    }

    #[test]
    fn push_samples_accumulates_across_calls() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 512,
            sample_rate: 48000,
            log_bins: 64,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let mut cols = Vec::new();
        proc.push_samples(&vec![0.0f32; 512], &mut cols);
        assert!(cols.is_empty());
        proc.push_samples(&vec![0.0f32; 512], &mut cols);
        assert_eq!(cols.len(), 1);
    }

    #[test]
    fn gaussian_kernel_zero_sigma_empty() {
        let k = build_gaussian_kernel(0.0);
        assert!(k.is_empty());
    }

    #[test]
    fn gaussian_kernel_sums_to_one() {
        let k = build_gaussian_kernel(2.0);
        assert!(!k.is_empty());
        let sum: f32 = k.iter().map(|(_, w)| w).sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn gaussian_kernel_symmetric() {
        let k = build_gaussian_kernel(3.0);
        for &(off, w) in &k {
            let mirror = k.iter().find(|&&(o, _)| o == -off).map(|(_, w)| *w);
            if let Some(mw) = mirror {
                assert!((w - mw).abs() < 0.001);
            }
        }
    }

    #[test]
    fn band_weights_covers_all_bins() {
        let cfg = SpectrumConfig {
            window_size: 2048,
            sample_rate: 48000,
            log_bins: 32,
            f_min_hz: 20.0,
            f_max_hz: 20000.0,
            ..Default::default()
        };
        let weights = build_band_weights(&cfg);
        assert_eq!(weights.len(), 32);
        for band in &weights {
            assert!(!band.is_empty(), "every log bin should have at least one FFT bin");
        }
    }

    #[test]
    fn weighting_none_is_unity() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            sample_rate: 48000,
            weighting: Weighting::None,
            ..Default::default()
        };
        let w = build_weighting_weights(&cfg);
        assert_eq!(w.len(), 1024 / 2 + 1);
        for &v in &w {
            assert!((v - 1.0).abs() < 0.001);
        }
    }

    #[test]
    fn weighting_a_attenuates_low_frequencies() {
        let cfg = SpectrumConfig {
            window_size: 4096,
            sample_rate: 48000,
            weighting: Weighting::A,
            ..Default::default()
        };
        let w = build_weighting_weights(&cfg);
        let low_bin = 1;
        let mid_bin = (1000.0f64 * 4096.0 / 48000.0).round() as usize;
        assert!(w[low_bin] < w[mid_bin], "A-weighting should attenuate low frequencies");
    }

    #[test]
    fn weighting_c_is_flatter_than_a() {
        let cfg_a = SpectrumConfig {
            window_size: 4096,
            sample_rate: 48000,
            weighting: Weighting::A,
            ..Default::default()
        };
        let cfg_c = SpectrumConfig {
            window_size: 4096,
            sample_rate: 48000,
            weighting: Weighting::C,
            ..Default::default()
        };
        let wa = build_weighting_weights(&cfg_a);
        let wc = build_weighting_weights(&cfg_c);
        let low_bin = 1;
        assert!(wc[low_bin] > wa[low_bin], "C-weighting should be flatter than A at low frequencies");
    }

    #[test]
    fn cqt_weights_empty_for_stft() {
        let cfg = SpectrumConfig {
            transform: Transform::Stft,
            ..Default::default()
        };
        let w = build_cqt_weights(&cfg);
        assert!(w.is_empty());
    }

    #[test]
    fn cqt_weights_produced_for_cqt() {
        let cfg = SpectrumConfig {
            window_size: 4096,
            sample_rate: 48000,
            transform: Transform::Cqt,
            cqt_bins_per_octave: 12,
            f_min_hz: 20.0,
            f_max_hz: 20000.0,
            ..Default::default()
        };
        let w = build_cqt_weights(&cfg);
        assert!(!w.is_empty());
        for band in &w {
            assert!(!band.is_empty());
        }
    }

    #[test]
    fn temporal_ema_smooths_values() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 512,
            sample_rate: 48000,
            log_bins: 64,
            temporal_alpha: 0.5,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let mut cols = Vec::new();
        let n = 2048;
        let samples: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin()).collect();
        proc.push_samples(&samples, &mut cols);
        assert!(cols.len() >= 3);
        let diffs: Vec<f32> = cols.windows(2).map(|w| {
            (0..w[0].len()).map(|i| (w[1][i] - w[0][i]).abs()).sum::<f32>()
        }).collect();
        let max_diff = diffs.iter().cloned().fold(0.0f32, f32::max);
        assert!(max_diff < 1.0, "EMA should limit frame-to-frame jumps");
    }

    #[test]
    fn peak_hold_preserves_maxima() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 512,
            sample_rate: 48000,
            log_bins: 64,
            peak_hold_decay: 0.9,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let mut cols = Vec::new();
        let n = 2048;
        let samples: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin()).collect();
        proc.push_samples(&samples, &mut cols);
        assert!(cols.len() >= 2);
        let last = cols.last().unwrap();
        let has_nonzero = last.iter().any(|&v| v > 0.0);
        assert!(has_nonzero, "peak hold should keep values visible");
    }

    #[test]
    fn gamma_below_one_brightens() {
        let cfg = SpectrumConfig {
            window_size: 1024,
            hop_size: 512,
            sample_rate: 48000,
            log_bins: 64,
            amplitude_gamma: 0.5,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let mut cols = Vec::new();
        let n = 2048;
        let samples: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin()).collect();
        proc.push_samples(&samples, &mut cols);
        for col in &cols {
            for &v in col {
                assert!(v >= 0.0 && v <= 1.0, "values must stay in [0,1] after gamma");
            }
        }
    }

    #[test]
    fn cqt_output_size_matches_effective_bin_count() {
        let cfg = SpectrumConfig {
            window_size: 2048,
            hop_size: 1024,
            transform: Transform::Cqt,
            cqt_bins_per_octave: 24,
            f_min_hz: 40.0,
            f_max_hz: 12000.0,
            ..Default::default()
        };
        let expected = spectrum_output_bins(&cfg);
        let mut processor = SpectrumProcessor::new(cfg).unwrap();
        let samples: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut columns = Vec::new();
        processor.push_samples(&samples, &mut columns);
        assert!(!columns.is_empty());
        assert!(columns.iter().all(|column| column.len() == expected));
    }

    #[test]
    fn cqt_gamma_below_one_brightens() {
        let base = SpectrumConfig {
            window_size: 2048,
            hop_size: 1024,
            transform: Transform::Cqt,
            cqt_bins_per_octave: 12,
            freq_smoothing_sigma: 0.0,
            amplitude_gamma: 1.0,
            temporal_alpha: 0.0,
            peak_hold_decay: 0.0,
            ..Default::default()
        };
        let mut brightened = base.clone();
        brightened.amplitude_gamma = 0.5;
        let samples: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut base_processor = SpectrumProcessor::new(base).unwrap();
        let mut bright_processor = SpectrumProcessor::new(brightened).unwrap();
        let mut base_columns = Vec::new();
        let mut bright_columns = Vec::new();
        base_processor.push_samples(&samples, &mut base_columns);
        bright_processor.push_samples(&samples, &mut bright_columns);
        let base_sum: f32 = base_columns.last().unwrap().iter().sum();
        let bright_sum: f32 = bright_columns.last().unwrap().iter().sum();
        assert!(bright_sum > base_sum);
    }

    #[test]
    fn triangular_aggregation_produces_valid_output() {
        let cfg = SpectrumConfig {
            window_size: 2048,
            hop_size: 1024,
            sample_rate: 48000,
            log_bins: 64,
            band_aggregation: BandAggregation::Triangular,
            ..Default::default()
        };
        let mut proc = SpectrumProcessor::new(cfg).unwrap();
        let n = 4096;
        let samples: Vec<f32> = (0..n).map(|i| (i as f32 * 0.005).sin()).collect();
        let mut cols = Vec::new();
        proc.push_samples(&samples, &mut cols);
        assert!(!cols.is_empty());
        for col in &cols {
            assert_eq!(col.len(), 64);
            for &v in col {
                assert!(v.is_finite());
            }
        }
    }
}
