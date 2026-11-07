//! `cargo run --release --example resave -- <in.xiz> <out.xiz>`: load an
//! instrument and write it back with Zynabre's serializer (plain XML).
use zynabre::synth::master::Patch;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let p = Patch::from_bytes(&std::fs::read(&a[1]).unwrap(), 1024).unwrap();
    std::fs::write(&a[2], p.xiz_of(0).to_string()).unwrap();
}
