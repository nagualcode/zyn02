//! Zyn02 — ZynAddSubFX rebuilt in Rust with truce.
//!
//! Plugin layer: host parameters, MIDI and state. The sound engine is in
//! [`synth`] (a module-by-module port of ZynAddSubFX), the shared patch
//! and worker thread in [`state`], the editor in `ui`.
//!
//! The full sound (16 parts, kits, effects, tuning) lives in the Zyn XML
//! format, saved as the plugin's custom state. Host parameters are a
//! small performance layer on top: they drive the MIDI controllers Zyn
//! already understands (cutoff = CC74, Q = CC71, …) on every part.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use truce::prelude::*;

pub mod state;
pub mod synth;
mod ui;

use state::Shared;
use synth::master::{Engine, Event, NUM_MIDI_PARTS, Patch};

#[derive(ParamEnum)]
pub enum BufferSize {
    #[name = "64"]
    B64,
    #[name = "128"]
    B128,
    #[name = "256"]
    B256,
    #[name = "512"]
    B512,
}

#[derive(ParamEnum)]
pub enum Interp {
    Linear,
    Cubic,
}

#[derive(Params)]
pub struct ZynParams {
    #[param(id = 1, name = "Output", group = "Master", range = "linear(-60, 12)", default = 0.0, unit = "dB")]
    pub output: FloatParam,
    #[param(id = 10, name = "Filter Cutoff", short_name = "Cutoff", group = "Controllers", range = "linear(0, 127)", default = 64.0, format = "fmt_bipolar")]
    pub cutoff: FloatParam,
    #[param(id = 11, name = "Filter Q", short_name = "Q", group = "Controllers", range = "linear(0, 127)", default = 64.0, format = "fmt_bipolar")]
    pub q: FloatParam,
    #[param(id = 12, name = "Bandwidth", group = "Controllers", range = "linear(0, 127)", default = 64.0, format = "fmt_bipolar")]
    pub bandwidth: FloatParam,
    #[param(id = 13, name = "Mod Wheel", short_name = "Mod", group = "Controllers", range = "linear(0, 127)", default = 64.0, format = "fmt_bipolar")]
    pub modwheel: FloatParam,
    #[param(id = 14, name = "FM Amount", short_name = "FM", group = "Controllers", range = "linear(0, 127)", default = 127.0, format = "fmt_pct")]
    pub fmamp: FloatParam,
    #[param(id = 15, name = "Resonance Center", short_name = "Res Ctr", group = "Controllers", range = "linear(0, 127)", default = 64.0, format = "fmt_bipolar")]
    pub res_center: FloatParam,
    #[param(id = 16, name = "Resonance Bandwidth", short_name = "Res BW", group = "Controllers", range = "linear(0, 127)", default = 64.0, format = "fmt_bipolar")]
    pub res_bw: FloatParam,
    #[param(id = 17, name = "Expression", short_name = "Expr", group = "Controllers", range = "linear(0, 127)", default = 127.0, format = "fmt_pct")]
    pub expression: FloatParam,
    #[param(id = 18, name = "Panning", short_name = "Pan", group = "Controllers", range = "linear(0, 127)", default = 64.0, format = "fmt_bipolar")]
    pub pan: FloatParam,
    #[param(id = 19, name = "Portamento", short_name = "Porta", group = "Controllers", default = false)]
    pub portamento: BoolParam,
    #[param(id = 30, name = "Engine Buffer", short_name = "Buffer", group = "Settings", default = 2)]
    pub buffer: EnumParam<BufferSize>,
    #[param(id = 31, name = "PAD Interpolation", short_name = "PAD Int", group = "Settings")]
    pub interp: EnumParam<Interp>,

    #[meter]
    pub meter_l: MeterSlot,
    #[meter]
    pub meter_r: MeterSlot,

    #[skip]
    pub shared: Arc<Shared>,
}

impl ZynParams {
    fn fmt_bipolar(&self, v: f64) -> String {
        format!("{:+.0}", v - 64.0)
    }
    fn fmt_pct(&self, v: f64) -> String {
        format!("{:.0}%", v / 127.0 * 100.0)
    }
}

/// Controller params: (CC number, getter).
const CTL: [u32; 10] = [74, 71, 75, 1, 76, 77, 78, 11, 10, 65];

fn ctl_values(p: &ZynParams) -> [i32; 10] {
    let r = |v: f32| v.round().clamp(0.0, 127.0) as i32;
    [
        r(p.cutoff.value()),
        r(p.q.value()),
        r(p.bandwidth.value()),
        r(p.modwheel.value()),
        r(p.fmamp.value()),
        r(p.res_center.value()),
        r(p.res_bw.value()),
        r(p.expression.value()),
        r(p.pan.value()),
        if p.portamento.value() { 127 } else { 0 },
    ]
}

pub struct Zyn {
    shared: Arc<Shared>,
    engine: Option<Box<Engine>>,
    seen_version: u64,
    sample_rate: u32,
    buffersize: usize,
    last_ctl: [i32; 10],
    held: Vec<Arc<Patch>>,
    peak: [f32; 2],
    meter_div: u32,
}

impl Default for Zyn {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            engine: None,
            seen_version: 0,
            sample_rate: 44100,
            buffersize: 256,
            last_ctl: [-1; 10],
            held: Vec::with_capacity(16),
            peak: [0.0; 2],
            meter_div: 0,
        }
    }
}

fn buffer_size(p: &ZynParams) -> usize {
    [64, 128, 256, 512][(p.buffer.index() as usize).min(3)]
}

impl Zyn {
    fn build_engine(&mut self) {
        let patch = self.shared.patch();
        self.seen_version = self.shared.version.load(Ordering::Acquire);
        let mut e = Engine::new(self.sample_rate, self.buffersize, patch);
        e.tempo = 120.0;
        self.engine = Some(Box::new(e));
        self.last_ctl = [-1; 10];
    }

    fn sync_patch(&mut self) {
        let v = self.shared.version.load(Ordering::Acquire);
        if v == self.seen_version {
            return;
        }
        let Some(p) = self.shared.try_patch() else { return };
        let load = self.shared.load_version.load(Ordering::Acquire) > self.seen_version;
        self.seen_version = v;
        if let Some(e) = self.engine.as_mut() {
            let old = e.patch.clone();
            if load {
                e.load_patch(p);
                self.last_ctl = [-1; 10];
            } else {
                e.set_patch(p);
            }
            if let Err(old) = self.shared.dispose(old) {
                if self.held.len() < self.held.capacity() {
                    self.held.push(old);
                }
            }
        }
        while let Some(o) = self.held.pop() {
            if let Err(o) = self.shared.dispose(o) {
                self.held.push(o);
                break;
            }
        }
    }

    fn handle(e: &mut Engine, body: &EventBody) {
        match *body {
            EventBody::NoteOn { channel, note, velocity, .. } => e.event(Event::NoteOn { ch: channel, note, vel: velocity }),
            EventBody::NoteOff { channel, note, .. } => e.event(Event::NoteOff { ch: channel, note }),
            EventBody::Aftertouch { channel, note, pressure, .. } => {
                e.event(Event::PolyAftertouch { ch: channel, note, vel: pressure })
            }
            EventBody::ControlChange { channel, cc, value, .. } => {
                e.event(Event::Controller { ch: channel, cc: cc as u32, val: value as i32 })
            }
            EventBody::PitchBend { channel, value, .. } => {
                e.event(Event::Controller { ch: channel, cc: 1000, val: value as i32 - 8192 })
            }
            _ => {}
        }
    }
}

impl PluginLogic for Zyn {
    type Params = ZynParams;
    type DspState = Self;

    fn bus_layouts() -> Vec<BusLayout> {
        vec![BusLayout::new().with_output("Main", ChannelConfig::Stereo)]
    }

    fn init(params: &ZynParams, cx: &InitContext) -> Self {
        let shared = params.shared.clone();
        shared.set_publisher(cx.snapshot_publisher());
        shared.flush_publish();
        Self { shared, ..Default::default() }
    }

    fn reset(state: &mut Self, params: &ZynParams, config: &AudioConfig) {
        let sr = config.sample_rate.round() as u32;
        let old_sr = state.shared.sample_rate.swap(sr, Ordering::AcqRel);
        if old_sr != sr {
            state.shared.rebuild_all();
        }
        state.sample_rate = sr;
        state.buffersize = buffer_size(params);
        state.build_engine();
    }

    fn process(
        state: &mut Self,
        params: &ZynParams,
        buffer: &mut AudioBuffer,
        events: &EventList,
        context: &mut ProcessContext,
    ) -> ProcessStatus {
        if state.engine.is_none() || buffer_size(params) != state.buffersize {
            state.buffersize = buffer_size(params);
            state.build_engine();
        }
        state.sync_patch();
        let Some(eng) = state.engine.as_mut() else { return ProcessStatus::Normal };
        eng.cubic = params.interp.index() == 1;
        if context.transport.tempo > 0.0 {
            eng.tempo = context.transport.tempo as f32;
        }
        eng.playing = context.transport.playing;
        if eng.playing {
            eng.beat = context.transport.position_beats;
        }

        // Host parameters -> controllers on every MIDI channel.
        let vals = ctl_values(params);
        for (i, v) in vals.iter().enumerate() {
            if *v != state.last_ctl[i] {
                state.last_ctl[i] = *v;
                for ch in 0..16u8 {
                    eng.event(Event::Controller { ch, cc: CTL[i], val: *v });
                }
            }
        }
        // Virtual keyboard from the editor.
        if let Ok(mut q) = state.shared.gui_events.try_lock() {
            for ev in q.drain(..) {
                eng.event(ev);
            }
        }

        let n = buffer.num_samples();
        let out_gain = if params.output.value() <= -59.9 { 0.0 } else { 10f32.powf(params.output.value() / 20.0) };
        let mut pos = 0;
        let mut next = 0;
        let mut tl = [0.0f32; 1024];
        let mut tr = [0.0f32; 1024];
        while pos < n {
            while let Some(ev) = events.get(next) {
                if ev.sample_offset as usize > pos {
                    break;
                }
                Self::handle(eng, &ev.body);
                next += 1;
            }
            let mut end = n;
            if let Some(ev) = events.get(next) {
                end = (ev.sample_offset as usize).clamp(pos + 1, n);
            }
            end = end.min(pos + 1024);
            let len = end - pos;
            eng.render(&mut tl[..len], &mut tr[..len]);
            let ch = buffer.num_output_channels();
            for i in 0..len {
                let (l, r) = (tl[i] * out_gain, tr[i] * out_gain);
                state.peak[0] = state.peak[0].max(l.abs());
                state.peak[1] = state.peak[1].max(r.abs());
                if ch > 1 {
                    buffer.output(0)[pos + i] = l;
                    buffer.output(1)[pos + i] = r;
                } else if ch == 1 {
                    buffer.output(0)[pos + i] = 0.5 * (l + r);
                }
            }
            pos = end;
        }
        while let Some(ev) = events.get(next) {
            Self::handle(eng, &ev.body);
            next += 1;
        }

        // Meters.
        let decay = (-(n as f64) / (0.3 * context.sample_rate)).exp() as f32;
        context.set_meter(ZynParamsParamId::MeterL, state.peak[0]);
        context.set_meter(ZynParamsParamId::MeterR, state.peak[1]);
        state.peak[0] *= decay;
        state.peak[1] *= decay;
        state.meter_div = state.meter_div.wrapping_add(1);
        if state.meter_div % 4 == 0 {
            let (m, parts) = eng.take_peaks();
            let sh = &state.shared;
            sh.master_peak[0].max(m[0]);
            sh.master_peak[1].max(m[1]);
            for (i, p) in parts.iter().enumerate().take(NUM_MIDI_PARTS) {
                sh.part_peak[i * 2].max(p[0]);
                sh.part_peak[i * 2 + 1].max(p[1]);
                sh.part_notes[i].store(eng.active_notes(i) as u32, Ordering::Relaxed);
            }
            let pr = eng.part_rt(sh.selected_part.load(Ordering::Relaxed) as usize % NUM_MIDI_PARTS);
            for k in 0..128 {
                sh.keys[k].store(pr.active_notes[k], Ordering::Relaxed);
            }
        }
        ProcessStatus::Normal
    }

    /// State is published off the audio thread by the worker (see
    /// `state.rs`); this is only the fallback before the first publish.
    fn save_state(state: &Self) -> Vec<u8> {
        let b = state.shared.last_blob();
        if b.is_empty() { state.shared.serialize_now() } else { b.to_vec() }
    }

    fn snapshot_into(_state: &Self, _buf: &mut Vec<u8>) -> bool {
        false
    }

    fn load_state(state: &mut Self, data: &[u8]) -> Result<(), StateLoadError> {
        if data.is_empty() {
            return Ok(());
        }
        state.shared.load_bytes(data.to_vec(), false);
        Ok(())
    }

    fn tail(_state: &Self) -> u32 {
        u32::MAX
    }

    fn editor(params: Arc<ZynParams>) -> Box<dyn Editor> {
        ui::editor(params)
    }
}

truce::plugin! {
    logic: Zyn,
    params: ZynParams,
}
