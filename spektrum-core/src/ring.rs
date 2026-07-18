use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::{Arc, Mutex};

pub struct SampleRingProducer {
    prod: HeapProd<f32>,
}

pub struct SampleRingConsumer {
    cons: HeapCons<f32>,
}

unsafe impl Send for SampleRingProducer {}
unsafe impl Send for SampleRingConsumer {}

pub fn sample_ring_pair(capacity: usize) -> (SampleRingProducer, SampleRingConsumer) {
    let rb = HeapRb::<f32>::new(capacity);
    let (p, c) = rb.split();
    (SampleRingProducer { prod: p }, SampleRingConsumer { cons: c })
}

impl SampleRingProducer {
    pub fn push_interleaved(&mut self, samples: &[f32], channels: usize) -> usize {
        if channels == 0 {
            return 0;
        }
        let mut n = 0usize;
        if channels == 1 {
            for &s in samples {
                if self.prod.try_push(s).is_err() {
                    break;
                }
                n += 1;
            }
            return n;
        }
        for chunk in samples.chunks(channels) {
            let mut acc = 0.0f32;
            for &s in chunk {
                acc += s;
            }
            let m = acc / (channels as f32);
            if self.prod.try_push(m).is_err() {
                break;
            }
            n += 1;
        }
        n
    }
}

impl SampleRingConsumer {
    pub fn pop_into(&mut self, dst: &mut [f32]) -> usize {
        self.cons.pop_slice(dst)
    }
    pub fn available(&self) -> usize {
        self.cons.occupied_len()
    }
}

pub struct SampleRing {
    rb: Arc<Mutex<(HeapProd<f32>, HeapCons<f32>)>>,
}

impl SampleRing {
    pub fn new(capacity: usize) -> Self {
        let rb = HeapRb::<f32>::new(capacity);
        let (p, c) = rb.split();
        Self {
            rb: Arc::new(Mutex::new((p, c))),
        }
    }
}

impl Clone for SampleRing {
    fn clone(&self) -> Self {
        Self {
            rb: Arc::clone(&self.rb),
        }
    }
}

impl SampleRing {
    pub fn push_interleaved(&self, samples: &[f32], channels: usize) -> usize {
        if channels == 0 {
            return 0;
        }
        let mut g = self.rb.lock().unwrap();
        let (p, _) = &mut *g;
        let mut n = 0usize;
        if channels == 1 {
            for &s in samples {
                if p.try_push(s).is_err() {
                    break;
                }
                n += 1;
            }
            return n;
        }
        for chunk in samples.chunks(channels) {
            let mut acc = 0.0f32;
            for &s in chunk {
                acc += s;
            }
            let m = acc / (channels as f32);
            if p.try_push(m).is_err() {
                break;
            }
            n += 1;
        }
        n
    }
    pub fn pop_into(&self, dst: &mut [f32]) -> usize {
        let mut g = self.rb.lock().unwrap();
        let (_, c) = &mut *g;
        c.pop_slice(dst)
    }
    pub fn available(&self) -> usize {
        let g = self.rb.lock().unwrap();
        let (_, c) = &*g;
        c.occupied_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ring_is_empty() {
        let ring = SampleRing::new(1024);
        assert_eq!(ring.available(), 0);
    }

    #[test]
    fn push_pop_roundtrip_mono() {
        let ring = SampleRing::new(1024);
        let input: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let n = ring.push_interleaved(&input, 1);
        assert_eq!(n, 100);
        assert_eq!(ring.available(), 100);
        let mut output = vec![0.0f32; 200];
        let m = ring.pop_into(&mut output);
        assert_eq!(m, 100);
        for i in 0..100 {
            assert!((output[i] - i as f32).abs() < 0.001);
        }
    }

    #[test]
    fn push_interleaved_stereo_downmixes() {
        let ring = SampleRing::new(1024);
        let input: Vec<f32> = vec![1.0, 3.0, 5.0, 7.0];
        let n = ring.push_interleaved(&input, 2);
        assert_eq!(n, 2);
        let mut output = vec![0.0f32; 10];
        let m = ring.pop_into(&mut output);
        assert_eq!(m, 2);
        assert!((output[0] - 2.0).abs() < 0.001);
        assert!((output[1] - 6.0).abs() < 0.001);
    }

    #[test]
    fn push_interleaved_zero_channels() {
        let ring = SampleRing::new(1024);
        let n = ring.push_interleaved(&[1.0, 2.0, 3.0], 0);
        assert_eq!(n, 0);
        assert_eq!(ring.available(), 0);
    }

    #[test]
    fn push_until_full() {
        let ring = SampleRing::new(10);
        let input: Vec<f32> = (0..20).map(|i| i as f32).collect();
        let n = ring.push_interleaved(&input, 1);
        assert_eq!(n, 10);
        assert_eq!(ring.available(), 10);
    }

    #[test]
    fn pop_into_empty_ring() {
        let ring = SampleRing::new(1024);
        let mut output = vec![0.0f32; 10];
        let n = ring.pop_into(&mut output);
        assert_eq!(n, 0);
    }

    #[test]
    fn clone_shares_data() {
        let ring1 = SampleRing::new(1024);
        let ring2 = ring1.clone();
        ring1.push_interleaved(&[1.0, 2.0, 3.0], 1);
        assert_eq!(ring2.available(), 3);
        let mut output = vec![0.0f32; 10];
        let n = ring2.pop_into(&mut output);
        assert_eq!(n, 3);
    }
}
