//! State shared by the editor, the host's state callbacks and the audio
//! thread.
//!
//! The authoritative sound is one `Arc<Patch>`. The editor edits copies
//! (`Arc::make_mut` clones only the touched part/kit item) and swaps
//! them in; the audio thread picks the new `Arc` up at the next block.
//! Heavy work — parsing files, building PADsynth wavetables, writing the
//! XML that the host saves — runs on a worker thread.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use truce_core::snapshot::SnapshotPublisher;

use crate::synth::fft::Fft;
use crate::synth::master::{Event, NUM_MIDI_PARTS, Patch};
use crate::synth::xml::XmlDoc;

pub const OSCILSIZE: usize = 1024;

/// f32 stored in an atomic.
#[derive(Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn max(&self, v: f32) {
        if v > self.get() {
            self.set(v);
        }
    }
}

enum Job {
    /// Replace the whole setup (host state, .xmz, or .xiz into part 0).
    LoadBytes { bytes: Arc<Vec<u8>>, publish: bool },
    /// Load an instrument file into one part.
    LoadInstrument { bytes: Arc<Vec<u8>>, part: usize, name: String },
    /// Rebuild the PADsynth tables of one kit item.
    RebuildPad { part: usize, kit: usize },
    /// Rebuild every PADsynth table (sample-rate change).
    RebuildAll,
    /// Serialize the current patch for the host.
    Publish,
}

pub struct Shared {
    patch: Mutex<Arc<Patch>>,
    /// Bumped on every change.
    pub version: AtomicU64,
    /// Version that must be applied as a fresh sound (notes cut).
    pub load_version: AtomicU64,
    trash: Mutex<Vec<Arc<Patch>>>,
    pub sample_rate: AtomicU32,
    publisher: Mutex<Option<SnapshotPublisher>>,
    /// Last serialized state (fallback for `save_state`).
    last_blob: Mutex<Arc<Vec<u8>>>,
    tx: Mutex<Option<Sender<Job>>>,
    pub busy: AtomicU32,
    pub status: Mutex<String>,
    pub gui_events: Mutex<Vec<Event>>,
    pub master_peak: [AtomicF32; 2],
    pub part_peak: Vec<AtomicF32>,
    pub part_notes: Vec<AtomicU32>,
    pub keys: Vec<AtomicBool>,
    /// Part shown in the editor.
    pub selected_part: AtomicU32,
    pub publish_pending: AtomicBool,
    publish_queued: AtomicBool,
}

impl Default for Shared {
    fn default() -> Self {
        let mut p = Patch::new(OSCILSIZE);
        p.prepare(44100.0);
        Self {
            patch: Mutex::new(Arc::new(p)),
            version: AtomicU64::new(1),
            load_version: AtomicU64::new(1),
            trash: Mutex::new(Vec::with_capacity(64)),
            sample_rate: AtomicU32::new(44100),
            publisher: Mutex::new(None),
            last_blob: Mutex::new(Arc::new(Vec::new())),
            tx: Mutex::new(None),
            busy: AtomicU32::new(0),
            status: Mutex::new(String::new()),
            gui_events: Mutex::new(Vec::with_capacity(256)),
            master_peak: [AtomicF32::default(), AtomicF32::default()],
            part_peak: (0..NUM_MIDI_PARTS * 2).map(|_| AtomicF32::default()).collect(),
            part_notes: (0..NUM_MIDI_PARTS).map(|_| AtomicU32::new(0)).collect(),
            keys: (0..128).map(|_| AtomicBool::new(false)).collect(),
            selected_part: AtomicU32::new(0),
            publish_pending: AtomicBool::new(false),
            publish_queued: AtomicBool::new(false),
        }
    }
}

impl Shared {
    /// Current patch (cheap clone of the `Arc`).
    pub fn patch(&self) -> Arc<Patch> {
        self.patch.lock().unwrap().clone()
    }

    /// Audio thread: non-blocking read.
    pub fn try_patch(&self) -> Option<Arc<Patch>> {
        self.patch.try_lock().ok().map(|p| p.clone())
    }

    /// Audio thread: hand an old patch back for deallocation elsewhere.
    pub fn dispose(&self, p: Arc<Patch>) -> Result<(), Arc<Patch>> {
        match self.trash.try_lock() {
            Ok(mut t) => {
                if t.len() < t.capacity() {
                    t.push(p);
                    Ok(())
                } else {
                    Err(p)
                }
            }
            Err(_) => Err(p),
        }
    }

    pub fn empty_trash(&self) {
        let old: Vec<_> = std::mem::take(&mut *self.trash.lock().unwrap());
        drop(old);
        self.trash.lock().unwrap().reserve(64);
    }

    fn store(&self, p: Arc<Patch>, load: bool) {
        let mut g = self.patch.lock().unwrap();
        *g = p;
        let v = self.version.fetch_add(1, Ordering::AcqRel) + 1;
        if load {
            self.load_version.store(v, Ordering::Release);
        }
        drop(g);
        self.request_publish();
    }

    /// Editor: modify the patch. `f` gets a private copy.
    pub fn edit(&self, f: impl FnOnce(&mut Patch)) {
        let mut g = self.patch.lock().unwrap();
        let mut p = (*g).clone();
        f(Arc::make_mut(&mut p));
        *g = p;
        self.version.fetch_add(1, Ordering::AcqRel);
        drop(g);
        self.request_publish();
    }

    /// Editor: replace the patch wholesale as a new sound.
    pub fn replace(&self, p: Patch) {
        self.store(Arc::new(p), true);
    }

    fn sender(self: &Arc<Self>) -> Sender<Job> {
        let mut g = self.tx.lock().unwrap();
        if let Some(tx) = g.as_ref() {
            return tx.clone();
        }
        let (tx, rx) = channel();
        let me = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("zyn02-worker".into())
            .spawn(move || worker(me, rx))
            .expect("worker thread");
        *g = Some(tx.clone());
        tx
    }

    fn send(self: &Arc<Self>, j: Job) {
        let _ = self.sender().send(j);
    }

    pub fn set_publisher(self: &Arc<Self>, p: Option<SnapshotPublisher>) {
        *self.publisher.lock().unwrap() = p;
        self.request_publish();
    }

    pub fn request_publish(&self) {
        self.publish_pending.store(true, Ordering::Release);
    }

    /// Called from the editor timer / worker to flush a pending publish.
    pub fn flush_publish(self: &Arc<Self>) {
        if self.publish_pending.load(Ordering::Acquire) && !self.publish_queued.swap(true, Ordering::AcqRel) {
            self.send(Job::Publish);
        }
    }

    pub fn last_blob(&self) -> Arc<Vec<u8>> {
        self.last_blob.lock().unwrap().clone()
    }

    /// Serialize synchronously (fallback save path).
    pub fn serialize_now(&self) -> Vec<u8> {
        self.patch().to_xml().to_gzip()
    }

    /// Host restore or file drop: replace everything (async).
    pub fn load_bytes(self: &Arc<Self>, bytes: Vec<u8>, publish: bool) {
        *self.last_blob.lock().unwrap() = Arc::new(bytes.clone());
        self.busy.fetch_add(1, Ordering::AcqRel);
        self.set_status("Loading…");
        self.send(Job::LoadBytes { bytes: Arc::new(bytes), publish });
    }

    pub fn load_instrument(self: &Arc<Self>, bytes: Vec<u8>, part: usize, name: String) {
        self.busy.fetch_add(1, Ordering::AcqRel);
        self.set_status(&format!("Loading {name}…"));
        self.send(Job::LoadInstrument { bytes: Arc::new(bytes), part, name });
    }

    pub fn rebuild_pad(self: &Arc<Self>, part: usize, kit: usize) {
        self.busy.fetch_add(1, Ordering::AcqRel);
        self.send(Job::RebuildPad { part, kit });
    }

    pub fn rebuild_all(self: &Arc<Self>) {
        self.busy.fetch_add(1, Ordering::AcqRel);
        self.send(Job::RebuildAll);
    }

    pub fn set_status(&self, s: &str) {
        *self.status.lock().unwrap() = s.to_string();
    }

    pub fn push_gui_event(&self, e: Event) {
        if let Ok(mut q) = self.gui_events.lock() {
            if q.len() < q.capacity() {
                q.push(e);
            }
        }
    }
}

fn worker(me: std::sync::Weak<Shared>, rx: Receiver<Job>) {
    let mut fft = Fft::new(OSCILSIZE);
    while let Ok(job) = rx.recv() {
        let Some(sh) = me.upgrade() else { break };
        let sr = sh.sample_rate.load(Ordering::Relaxed) as f32;
        match job {
            Job::LoadBytes { bytes, publish } => {
                match Patch::from_bytes(&bytes, OSCILSIZE) {
                    Ok(mut p) => {
                        p.prepare(sr);
                        sh.store(Arc::new(p), true);
                        sh.set_status("");
                    }
                    Err(e) => sh.set_status(&format!("Load failed: {e}")),
                }
                if !publish {
                    sh.publish_pending.store(false, Ordering::Release);
                    if let Some(p) = sh.publisher.lock().unwrap().as_ref() {
                        p.publish(bytes.to_vec());
                    }
                }
                sh.busy.fetch_sub(1, Ordering::AcqRel);
            }
            Job::LoadInstrument { bytes, part, name } => {
                let res = XmlDoc::parse(&bytes).and_then(|doc| {
                    let mut p = (*sh.patch()).clone();
                    if doc.root.branch("MASTER").is_some() {
                        Patch::from_xmz(&doc, OSCILSIZE)
                    } else {
                        p.load_xiz(&doc, part)?;
                        Arc::make_mut(&mut p.parts[part]).enabled = true;
                        Ok(p)
                    }
                });
                match res {
                    Ok(mut p) => {
                        let pp = Arc::make_mut(&mut p.parts[part]);
                        pp.prepare(&mut fft, sr);
                        if pp.name.is_empty() {
                            pp.name = name;
                        }
                        for (i, q) in p.parts.iter_mut().enumerate() {
                            if i != part {
                                let has_empty_pad = q.kit.iter().any(|k| k.pad_enabled && k.pad.as_ref().map(|x| x.samples.is_empty()).unwrap_or(false));
                                if has_empty_pad {
                                    Arc::make_mut(q).prepare(&mut fft, sr);
                                }
                            }
                        }
                        sh.store(Arc::new(p), true);
                        sh.set_status("");
                    }
                    Err(e) => sh.set_status(&format!("Load failed: {e}")),
                }
                sh.busy.fetch_sub(1, Ordering::AcqRel);
            }
            Job::RebuildPad { part, kit } => {
                // Coalesce repeated requests for the same item.
                let snapshot = sh.patch();
                if let Some(pad) = snapshot.parts.get(part).and_then(|p| p.kit.get(kit)).and_then(|k| k.pad.clone()) {
                    let mut np = (*pad).clone();
                    np.apply_parameters(sr);
                    let np = Arc::new(np);
                    sh.edit(|p| {
                        let pp = Arc::make_mut(&mut p.parts[part]);
                        if let Some(slot) = pp.kit[kit].pad.as_mut() {
                            // Only install if nothing else changed meanwhile.
                            let mut cur = (**slot).clone();
                            cur.samples = np.samples.clone();
                            *slot = Arc::new(cur);
                        }
                    });
                }
                sh.busy.fetch_sub(1, Ordering::AcqRel);
            }
            Job::RebuildAll => {
                let mut p = (*sh.patch()).clone();
                p.prepare(sr);
                sh.store(Arc::new(p), false);
                sh.busy.fetch_sub(1, Ordering::AcqRel);
            }
            Job::Publish => {
                // Debounce bursts of edits (knob drags).
                std::thread::sleep(Duration::from_millis(30));
                sh.publish_queued.store(false, Ordering::Release);
            }
        }
        if sh.publish_pending.swap(false, Ordering::AcqRel) {
            let blob = sh.serialize_now();
            *sh.last_blob.lock().unwrap() = Arc::new(blob.clone());
            if let Some(p) = sh.publisher.lock().unwrap().as_ref() {
                p.publish(blob);
            }
        }
        sh.empty_trash();
    }
}

// ---------------------------------------------------------------------------
// Embedded banks
// ---------------------------------------------------------------------------

static BANK_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/banks.bin"));

pub struct BankEntry {
    pub name: String,
    pub data: &'static [u8],
}

/// Bank name -> instruments (sorted), from the embedded blob.
pub fn banks() -> &'static BTreeMap<String, Vec<BankEntry>> {
    static B: OnceLock<BTreeMap<String, Vec<BankEntry>>> = OnceLock::new();
    B.get_or_init(|| {
        let mut m: BTreeMap<String, Vec<BankEntry>> = BTreeMap::new();
        let b = BANK_BLOB;
        if b.len() < 4 {
            return m;
        }
        let n = u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize;
        let mut i = 4;
        for _ in 0..n {
            let nl = u16::from_le_bytes(b[i..i + 2].try_into().unwrap()) as usize;
            i += 2;
            let name = String::from_utf8_lossy(&b[i..i + nl]).into_owned();
            i += nl;
            let dl = u32::from_le_bytes(b[i..i + 4].try_into().unwrap()) as usize;
            i += 4;
            let data = &b[i..i + dl];
            i += dl;
            let mut parts = name.split('/');
            let top = parts.next().unwrap_or("");
            let (bank, file) = if top == "banks" {
                (parts.next().unwrap_or("").to_string(), parts.collect::<Vec<_>>().join("/"))
            } else {
                ("Example Setups".to_string(), parts.collect::<Vec<_>>().join("/"))
            };
            let pretty = pretty_name(&file);
            m.entry(bank).or_default().push(BankEntry { name: pretty, data });
        }
        m
    })
}

/// "0042-Soft Arpeggio4.xiz" -> "Soft Arpeggio4".
pub fn pretty_name(file: &str) -> String {
    let stem = file.rsplit('/').next().unwrap_or(file);
    let stem = stem.trim_end_matches(".xiz").trim_end_matches(".xmz");
    match stem.split_once('-') {
        Some((num, rest)) if num.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
        _ => stem.to_string(),
    }
}
