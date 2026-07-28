use crate::spectrogram::SpectrogramProgram;
use spektrum_core::SpectrumConfig;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};

#[derive(Debug, Clone)]
pub enum DspCommand {
    Restart(SpectrumConfig, u32),
    UpdateRuntime(SpectrumConfig),
    SetHistory(u32),
    SetPaused(bool),
}

pub struct SourceSlot {
    pub id: usize,
    pub label: String,
    pub target: String,
    pub pending_spectra: Arc<Mutex<VecDeque<Vec<f32>>>>,
    pub restart_tx: mpsc::Sender<DspCommand>,
    pub capture_tx: Option<mpsc::Sender<String>>,
    pub prog: SpectrogramProgram,
    pub opacity: f32,
    pub colormap_name: String,
    pub capture_name: String,
}

impl SourceSlot {
    pub fn restart_dsp(&self, spectrum: &SpectrumConfig, history: u32) {
        self.restart_tx.send(DspCommand::Restart(spectrum.clone(), history)).ok();
    }

    pub fn update_runtime(&self, spectrum: &SpectrumConfig) {
        self.restart_tx.send(DspCommand::UpdateRuntime(spectrum.clone())).ok();
    }

    pub fn set_history(&self, history: u32) {
        self.restart_tx.send(DspCommand::SetHistory(history)).ok();
    }

    pub fn set_paused(&self, paused: bool) {
        self.restart_tx.send(DspCommand::SetPaused(paused)).ok();
    }

    pub fn set_target(&self, target: &str) {
        if let Some(tx) = &self.capture_tx {
            tx.send(target.to_string()).ok();
        }
    }

    pub fn update_colormap(&mut self, lut: Arc<Vec<[u8; 4]>>, name: &str) {
        self.prog.colormap_lut = lut;
        self.colormap_name = name.to_string();
    }

    pub fn update_contrast(&mut self, v: f32) {
        self.prog.contrast = v;
    }

    pub fn update_saturation(&mut self, v: f32) {
        self.prog.saturation = v;
    }

    pub fn update_opacity(&mut self, v: f32) {
        self.opacity = v;
        self.prog.opacity = v;
    }
}

pub fn spawn_dsp_thread(
    pending_spectra: Arc<Mutex<VecDeque<Vec<f32>>>>,
    restart_rx: mpsc::Receiver<DspCommand>,
    initial_cfg: SpectrumConfig,
    history: u32,
    debug_profile: bool,
    pop_fn: impl Fn(&mut [f32]) -> usize + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut scratch = vec![0.0f32; 65536];
        let mut prof_last = std::time::Instant::now();
        let mut prof_dsp_us: u64 = 0;
        let mut prof_cols: u64 = 0;
        let mut prof_samples: u64 = 0;

        let mut cfg = initial_cfg;
        let mut backlog_cap = (history as usize).saturating_mul(8).saturating_add(256).max(1024);
        let mut proc = spektrum_core::SpectrumProcessor::new(cfg.clone()).expect("spectrum processor");
        let mut paused = false;

        loop {
            while let Ok(cmd) = restart_rx.try_recv() {
                match cmd {
                    DspCommand::Restart(new_cfg, new_history) => {
                        cfg = new_cfg;
                        proc = spektrum_core::SpectrumProcessor::new(cfg.clone()).expect("spectrum processor");
                        backlog_cap = (new_history as usize).saturating_mul(8).saturating_add(256).max(1024);
                        if debug_profile {
                            eprintln!("[profile] DSP restart: fft={} hop={} bins={}", cfg.window_size, cfg.hop_size, cfg.log_bins);
                        }
                    }
                    DspCommand::UpdateRuntime(new_cfg) => {
                        cfg = new_cfg;
                        proc.set_runtime_cfg(&cfg);
                    }
                    DspCommand::SetHistory(new_history) => {
                        backlog_cap = (new_history as usize).saturating_mul(8).saturating_add(256).max(1024);
                    }
                    DspCommand::SetPaused(value) => {
                        paused = value;
                        if !paused {
                            proc = spektrum_core::SpectrumProcessor::new(cfg.clone()).expect("spectrum processor");
                        }
                    }
                }
            }

            let n = pop_fn(&mut scratch);
            if n == 0 {
                std::thread::sleep(std::time::Duration::from_micros(500));
                continue;
            }
            if paused {
                continue;
            }
            let t0 = std::time::Instant::now();
            let mut cols = Vec::new();
            proc.push_samples(&scratch[..n], &mut cols);
            let dsp_elapsed = t0.elapsed();
            let mut q = pending_spectra.lock().unwrap();
            for c in &cols {
                while q.len() >= backlog_cap {
                    q.pop_front();
                }
                q.push_back(c.clone());
            }
            drop(q);
            if debug_profile {
                prof_dsp_us += dsp_elapsed.as_micros() as u64;
                prof_cols += cols.len() as u64;
                prof_samples += n as u64;
                let elapsed = prof_last.elapsed();
                if elapsed >= std::time::Duration::from_secs(1) {
                    let secs = elapsed.as_secs_f64();
                    eprintln!(
                        "[profile] DSP: {:.1}ms/sec total | {:.2}ms/col avg | cols/sec: {:.0} | samples/sec: {:.0}",
                        prof_dsp_us as f64 / 1000.0 / secs,
                        if prof_cols > 0 { prof_dsp_us as f64 / prof_cols as f64 / 1000.0 } else { 0.0 },
                        prof_cols as f64 / secs,
                        prof_samples as f64 / secs,
                    );
                    prof_last = std::time::Instant::now();
                    prof_dsp_us = 0;
                    prof_cols = 0;
                    prof_samples = 0;
                }
            }
        }
    });
}
