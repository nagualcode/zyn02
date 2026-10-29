//! Effect comparison input, mirroring the C++ reference harness.
use zyn02::synth::effects::EffectMgr;
use zyn02::synth::util::sprng;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (kind, preset, ins) = (a[1].parse().unwrap(), a[2].parse().unwrap(), a[3] != "0");
    sprng(0x1234);
    let mut em = EffectMgr::for_test(kind, preset, ins, 44100.0, 256);
    let mut out = Vec::new();
    let pi = 3.141_592_65f32;
    for b in 0..(44100 * 3 / 256) {
        let mut l = [0.0f32; 256];
        let mut r = [0.0f32; 256];
        for i in 0..256 {
            let n = b * 256 + i;
            let t = n as f32 / 44100.0;
            let env = if n < 44100 { 1.0 } else { 0.0 };
            l[i] = env * (0.3 * (2.0 * pi * 220.0 * t).sin() + 0.2 * (2.0 * pi * 1370.0 * t).sin()) + if n % 11025 == 0 { 0.8 } else { 0.0 };
            r[i] = env * (0.3 * (2.0 * pi * 330.0 * t).sin() + 0.1 * (2.0 * pi * 4100.0 * t).sin());
        }
        em.out(&mut l, &mut r);
        for i in 0..256 {
            out.extend_from_slice(&l[i].to_le_bytes());
            out.extend_from_slice(&r[i].to_le_bytes());
        }
    }
    std::fs::write(&a[4], out).unwrap();
}
