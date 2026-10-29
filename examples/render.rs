//! Offline renderer: `cargo run --release --example render -- <patch.xiz|xmz> [out.wav] [notes...]`
//! Plays a short chord/arpeggio through the engine and writes a WAV, so
//! patches can be checked without a DAW.

use std::sync::Arc;

use zyn02::synth::master::{Engine, Event, Patch};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: render <file.xiz|xmz> [out.wav] [note ...]");
    let out = args.get(2).cloned().unwrap_or_else(|| "render.wav".into());
    let notes: Vec<u8> = if args.len() > 3 { args[3..].iter().map(|s| s.parse().unwrap()).collect() } else { vec![48, 55, 60, 64] };
    let sr = 44100u32;
    let bytes = std::fs::read(path).expect("read patch");
    let t0 = std::time::Instant::now();
    let mut patch = Patch::from_bytes(&bytes, 1024).expect("parse patch");
    patch.prepare(sr as f32);
    eprintln!("loaded+prepared in {:?}", t0.elapsed());
    if let Ok(path) = std::env::var("PADDUMP") {
        let mut f = Vec::new();
        if let Some(pad) = patch.parts[0].kit[0].pad.as_ref() {
            for smp in pad.samples.iter() {
                f.extend_from_slice(&(smp.size as i32).to_le_bytes());
                f.extend_from_slice(&smp.basefreq.to_le_bytes());
                for v in &smp.smp[..smp.size] {
                    f.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        std::fs::write(path, f).unwrap();
        return;
    }
    let mut eng = Engine::new(sr, 256, Arc::new(patch));
    let raw = std::env::var("RAW_PART").is_ok();
    zyn02::synth::util::sprng(0x1234);
    if raw {
        eng.debug_part = Some((Vec::new(), Vec::new()));
    }
    let total = sr as usize * 4;
    let mut l = vec![0.0f32; total];
    let mut r = vec![0.0f32; total];
    let block = 256;
    let t1 = std::time::Instant::now();
    let mut pos = 0;
    let mut k = 0usize;
    let mut off = false;
    while pos < total {
        let t = pos as f32 / sr as f32;
        for (i, n) in notes.iter().enumerate() {
            if t >= 0.25 * i as f32 && i >= k {
                eng.event(Event::NoteOn { ch: 0, note: *n, vel: 100 });
                k = i + 1;
            }
        }
        if !off && t >= 2.5 {
            for n in notes.iter() {
                eng.event(Event::NoteOff { ch: 0, note: *n });
            }
            off = true;
        }
        let e = (pos + block).min(total);
        let (a, b) = (&mut l[pos..e], &mut r[pos..e]);
        eng.render(a, b);
        pos = e;
    }
    let el = t1.elapsed();
    let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, v| m.max(v.abs()));
    let rms = (l.iter().map(|v| v * v).sum::<f32>() / total as f32).sqrt();
    eprintln!("rendered 4 s in {:?} ({:.1}x realtime) peak {:.3} rms {:.4} nan={}", el, 4.0 / el.as_secs_f32(), peak, rms, l.iter().any(|v| !v.is_finite()));
    if raw {
        let (dl, dr) = eng.debug_part.take().unwrap();
        let mut f = Vec::new();
        for i in 0..dl.len().min(total) {
            f.extend_from_slice(&dl[i].to_le_bytes());
            f.extend_from_slice(&dr[i].to_le_bytes());
        }
        std::fs::write(format!("{out}.raw"), f).unwrap();
    }
    let spec = hound::WavSpec { channels: 2, sample_rate: sr, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(&out, spec).unwrap();
    for i in 0..total {
        w.write_sample(l[i]).unwrap();
        w.write_sample(r[i]).unwrap();
    }
    w.finalize().unwrap();
}
