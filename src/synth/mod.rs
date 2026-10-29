//! The ZynAddSubFX engine, ported module by module from the C++ sources
//! (`src/` of github.com/zynaddsubfx/zynaddsubfx, version 3.0.x). Each
//! file names the original it follows; quirks of the original are kept
//! on purpose so that patches sound the same.

pub mod ad_params;
pub mod adnote;
pub mod controller;
pub mod effects;
pub mod envelope;
pub mod fft;
pub mod filter;
pub mod lfo;
pub mod master;
pub mod microtonal;
pub mod modfilter;
pub mod oscilgen;
pub mod pad;
pub mod part;
pub mod resonance;
pub mod sub;
pub mod synthnote;
pub mod util;
pub mod waveshape;
pub mod xml;

use controller::Controller;
use fft::Fft;
use filter::MAX_BUF;
use lfo::AbsTime;
use oscilgen::OscScratch;
use synthnote::PortaSlot;
use util::SynthT;

/// Everything a note needs from its surroundings for one buffer.
pub struct NoteCtx<'a> {
    pub synth: &'a SynthT,
    pub time: &'a AbsTime,
    pub ctl: &'a Controller,
    /// Parameters were edited since the previous buffer.
    pub changed: bool,
    pub const_power: bool,
    pub cubic_interpolation: bool,
    pub portas: &'a [PortaSlot],
    pub fft: &'a mut Fft,
    pub osc: &'a mut OscScratch,
}

/// Scratch buffers shared by every ADnote.
pub struct NoteScratch {
    pub tmpwave_unison: Vec<Vec<f32>>,
    pub tmpwavel: Vec<f32>,
    pub tmpwaver: Vec<f32>,
    pub bypassl: Vec<f32>,
    pub bypassr: Vec<f32>,
}

impl NoteScratch {
    pub fn new() -> Self {
        Self {
            tmpwave_unison: (0..64).map(|_| vec![0.0; MAX_BUF]).collect(),
            tmpwavel: vec![0.0; MAX_BUF],
            tmpwaver: vec![0.0; MAX_BUF],
            bypassl: vec![0.0; MAX_BUF],
            bypassr: vec![0.0; MAX_BUF],
        }
    }
}

impl Default for NoteScratch {
    fn default() -> Self {
        Self::new()
    }
}
