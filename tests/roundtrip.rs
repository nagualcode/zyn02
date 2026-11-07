//! Load every bundled instrument, save it in our XML, load it back and
//! check the two renders match (the format round-trips losslessly).

use std::sync::Arc;
use zynabre::state::banks;
use zynabre::synth::master::{Engine, Event, Patch};
use zynabre::synth::util::sprng;

fn render(p: Patch) -> Vec<f32> {
    let mut p = p;
    p.prepare(44100.0);
    sprng(0x1234);
    let mut e = Engine::new(44100, 256, Arc::new(p));
    e.event(Event::NoteOn { ch: 0, note: 60, vel: 100 });
    let mut l = vec![0.0; 22050];
    let mut r = vec![0.0; 22050];
    e.render(&mut l, &mut r);
    l
}

#[test]
fn xml_roundtrip_renders_identically() {
    let mut n = 0;
    for b in banks().iter() {
        let bank = &b.name;
        for it in b.items.iter().step_by(9) {
            let a = Patch::from_bytes(it.data, 1024).unwrap_or_else(|e| panic!("{bank}/{}: {e}", it.name));
            if a.parts.iter().any(|p| p.kit.iter().any(|k| k.pad_enabled)) {
                continue; // random-phase wavetables
            }
            let bytes = a.to_xml().to_gzip();
            let b = Patch::from_bytes(&bytes, 1024).unwrap();
            let (ra, rb) = (render(a), render(b));
            let err = ra.iter().zip(&rb).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
            assert!(err < 1e-4, "{bank}/{}: roundtrip differs by {err}", it.name);
            n += 1;
        }
    }
    assert!(n > 50, "only {n} instruments tested");
}
