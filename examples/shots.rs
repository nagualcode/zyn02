//! Offscreen renders of the editor for the README:
//! `cargo run --release --example shots -- <out_dir> [patch.xiz]`
use std::sync::atomic::Ordering;
use truce_core::export::PluginExport;
use truce_core::plugin::PluginRuntime as _;
use zynabre::Plugin;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let out = a.get(1).cloned().unwrap_or_else(|| "screenshots".into());
    let patch = a.get(2).cloned();
    std::fs::create_dir_all(&out).unwrap();
    // (file, page, voice, browse, bank, tab, fx)
    let shots: &[(&str, u8, Option<usize>, bool, usize, usize, (u8, usize))] = &[
        ("add", 1, None, false, 0, 0, (0, 0)),
        ("voice", 1, Some(0), false, 0, 0, (0, 0)),
        ("voice_filter", 1, Some(0), false, 0, 3, (0, 0)),
        ("sub", 2, None, false, 0, 0, (0, 0)),
        ("pad", 3, None, false, 0, 0, (0, 0)),
        ("part", 0, None, false, 0, 0, (0, 0)),
        ("fx", 4, None, false, 0, 0, (0, 0)),
        ("mixer", 5, None, false, 0, 0, (0, 0)),
        ("browser", 1, None, true, 0, 0, (0, 0)),
    ];
    for (name, page, voice, browse, bank, tab, fx) in shots {
        let mut plugin = Plugin::create();
        plugin.init();
        if let Some(p) = &patch {
            let sh = plugin.params().shared.clone();
            sh.load_instrument(std::fs::read(p).unwrap(), 0, "patch".into());
            let t0 = std::time::Instant::now();
            while sh.busy.load(Ordering::Acquire) > 0 && t0.elapsed().as_secs() < 20 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        zynabre::ui::debug_start(*page, *voice, *browse, *bank, *tab, *fx);
        let (px, w, h) = truce_core::screenshot::render_pixels_for_at_scale::<Plugin>(&mut plugin, 2.0);
        truce_core::screenshot::save_png(std::path::Path::new(&format!("{out}/{name}.png")), &px, w, h);
        eprintln!("{name}: {w}x{h}");
    }
}
