//! Packs the bundled ZynAddSubFX banks (`banks/`) and example setups
//! (`examples-xmz/`) into one blob that the plugin embeds, so the bank
//! browser works without any install step.

use std::io::Write;
use std::path::Path;

fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(&p, root, out);
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            if ext == "xiz" || ext == "xmz" {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push((rel, std::fs::read(&p).unwrap()));
            }
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=banks");
    println!("cargo:rerun-if-changed=examples-xmz");
    let mut files = Vec::new();
    walk(Path::new("banks"), Path::new("."), &mut files);
    walk(Path::new("examples-xmz"), Path::new("."), &mut files);
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("banks.bin");
    let mut f = std::fs::File::create(out).unwrap();
    f.write_all(&(files.len() as u32).to_le_bytes()).unwrap();
    for (name, data) in files {
        f.write_all(&(name.len() as u16).to_le_bytes()).unwrap();
        f.write_all(name.as_bytes()).unwrap();
        f.write_all(&(data.len() as u32).to_le_bytes()).unwrap();
        f.write_all(&data).unwrap();
    }
}
