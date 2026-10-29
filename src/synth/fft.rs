//! `DSP/FFTwrapper.cpp` on top of realfft, with FFTW's conventions:
//! unnormalised forward r2c and backward c2r, Nyquist bin cleared before
//! the inverse transform.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

pub type C32 = Complex32;

pub struct Fft {
    size: usize,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    scratch: Vec<C32>,
    tmp_in: Vec<f32>,
    tmp_freq: Vec<C32>,
}

impl Clone for Fft {
    fn clone(&self) -> Self {
        Self {
            size: self.size,
            fwd: self.fwd.clone(),
            inv: self.inv.clone(),
            scratch: self.scratch.clone(),
            tmp_in: self.tmp_in.clone(),
            tmp_freq: self.tmp_freq.clone(),
        }
    }
}

impl std::fmt::Debug for Fft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fft({})", self.size)
    }
}

impl Fft {
    pub fn new(size: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(size);
        let inv = planner.plan_fft_inverse(size);
        let sl = fwd.get_scratch_len().max(inv.get_scratch_len());
        Self {
            size,
            fwd,
            inv,
            scratch: vec![C32::new(0.0, 0.0); sl],
            tmp_in: vec![0.0; size],
            tmp_freq: vec![C32::new(0.0, 0.0); size / 2 + 1],
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Samples (len `size`) -> `size/2+1` bins. Input is left untouched.
    pub fn smps2freqs(&mut self, smps: &[f32], freqs: &mut [C32]) {
        self.tmp_in.copy_from_slice(&smps[..self.size]);
        let _ = self.fwd.process_with_scratch(&mut self.tmp_in, &mut freqs[..self.size / 2 + 1], &mut self.scratch);
    }

    /// Bins (at least `size/2` used) -> samples (len `size`).
    pub fn freqs2smps(&mut self, freqs: &[C32], smps: &mut [f32]) {
        let h = self.size / 2;
        self.tmp_freq[..h].copy_from_slice(&freqs[..h]);
        self.tmp_freq[h] = C32::new(0.0, 0.0);
        self.tmp_freq[0].im = 0.0;
        let _ = self.inv.process_with_scratch(&mut self.tmp_freq, &mut smps[..self.size], &mut self.scratch);
    }
}
