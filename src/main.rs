//! Standalone entry point: run the plugin as a desktop app via
//! `cargo truce run`, no DAW needed.

use zynabre::Plugin;

fn main() {
    truce_standalone::run::<Plugin>();
}
