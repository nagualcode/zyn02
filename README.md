<div align="center">

# Zynabre

**ZynAddSubFX, rebuilt in Rust.**

*ADDsynth · SUBsynth · PADsynth · 16 parts × 16 kit items · the full Zyn effect rack · your .xiz and .xmz patches*

An Audio Unit, VST3 and CLAP instrument by **Jardim Sonoro**, written in Rust with
[truce](https://github.com/truce-audio/truce).

[![Build](https://github.com/nagualcode/zynabre/actions/workflows/build.yml/badge.svg)](https://github.com/nagualcode/zynabre/actions/workflows/build.yml)

![Zynabre](screenshots/main.png)

</div>

---

## The instrument

In 2002 Nasca Octavian Paul released [ZynAddSubFX](https://github.com/zynaddsubfx/zynaddsubfx),
a free synthesizer built on three ideas nobody else had put in one box: an
additive/subtractive engine whose oscillators are drawn harmonic by
harmonic, a subtractive engine that carves sound out of white noise with
banks of band-pass filters, and PADsynth, an algorithm that spreads every
harmonic over a band and renders the whole thing with one enormous inverse
FFT into a seamless, breathing wavetable. Two decades of musicians have
made thousands of patches for it.

Zynabre (Zyn + abre, Portuguese for "opens") is a port of that engine, module by module, from the
C++ sources (version 3.0.x) into Rust, wrapped as a modern plugin with a
new editor. It is not "Zyn-inspired": it loads the original patch files
and plays them the way ZynAddSubFX does.

## Same sound as the original — measured

The original was compiled locally and driven by a small harness through the
same note sequence as Zynabre, sample for sample (`tools/` in the history of
this repo; see *How it was checked* below).

| What | Result |
|---|---|
| All 1318 instruments in the bundled banks, instrument effects off | **862 bit-for-bit identical** (error < −40 dB, most at −120 dB) |
| The rest | statistically identical; they diverge only where the original is itself random or chaotic (see below) |
| Every effect (all 10) × every factory preset × insertion/system | **bit-identical** (zero error), except 3 Distortion presets and 4 DynFilter presets at −75 to −110 dB |
| PADsynth wavetables | magnitude spectra **identical**; phases are random by design |
| Save → reload in Zynabre | lossless on every tested patch (`cargo test`) |

Why the remaining instruments are not bit-identical:

- **PADsynth** fills its tables with random phases, a fresh set every time
  (in the original too).
- **Frequency modulation** integrates the modulator; a one-bit difference
  between FFTW (used by Zyn) and realfft (used here) in the oscillator table
  grows over a note. Envelopes and spectra stay the same.
- Some patches use Zyn's own randomness: random panning per note, the
  "Random" reverb type (comb lengths drawn at load), LFO randomness.

Faithfulness goes down to the quirks: Zyn's random generator and the order
in which notes consume it, the places where the C++ does one operation in
`double` in the middle of `float` code, the `normalize_max` bug in
PADsynth, the full-range random pan, version upgrades of old files
(volume scales, envelope dB fix for files older than 2.4.4, LFO frequency
before 3.0.4, filter parameters before 3.0.2, missing effect parameters
before 3.0.6), the pre-3.0.7 pan law.

## What is inside

```
 Part (×16) ── Kit item (×16) ──┬─ ADDsynth: 8 voices, each an OscilGen oscillator
                                │            + unison, FM / PM / ring / mix / PWM / sync,
                                │            voice filter, 5 envelopes, 3 LFOs
                                ├─ SUBsynth: up to 64 harmonics × 5 band-pass stages on noise
                                └─ PADsynth: harmonic profile → wavetables (built off the audio thread)
            └─ 3 instrument effects ─┐
 8 insertion effects ── 4 system effects (with sends) ── master volume / tuning
```

- **OscilGen**: 16 base functions + user waveform, 128 harmonics with
  magnitude/phase, oscillator filter (14 types), 17 waveshapers, spectrum
  adjust, harmonic shift, base-function and oscillator modulation,
  adaptive harmonics, resonance.
- **Filters**: analog biquads (9 types, up to 5 stages, smoothed cutoff),
  formant filter with vowel sequences, state-variable, Moog ladder, comb.
- **Effects**: Reverb, Echo, Chorus/Flanger, Phaser (classic and analog),
  AlienWah, Distortion, 8-band EQ, Dynamic Filter, and Zyn 3.0.7's
  Sympathetic (a bank of up to 228 resonating comb "strings": generic,
  piano and guitar tunings) and Reverse (reversed segments triggered by
  notes, free-running, or locked to the host's beat), with all factory
  presets.
- **Performance**: poly / mono / legato, portamento (constant or
  proportional, up/down stretch, thresholds), key and voice limits,
  sustain, velocity curves, microtonal scales and keyboard maps from .xmz.

## The editor

A compact, flat editor (1140 × 602) where every control sits on the same
grid, so all pages fit without scrolling.

- **Header**: preset browser (bank and instrument name, ◀ ▶ to step through
  the bank), the 16 parts (click to edit, double-click to switch a part
  on/off, the bar lights with activity), load/save, panic.
- **Navigation**: PART · ADD · SUB · PAD · EFFECTS · MIXER, colour-coded
  per engine like Zyn's own editor (ADDsynth orange, SUBsynth cyan,
  PADsynth violet). The dot shows which engines the kit item uses. The ADD
  voice selector (with dots on active voices) and the kit selector are
  on the same row.
- **Browser** (click the preset box): every bank in a grid, the community
  banks first, plus a search across all banks. A click loads into the
  selected part.
- **Pages**:
  **PART** (mode, volume, keys, MIDI channel, controllers, instrument
  info, kit table) ·
  **ADD** global (amplitude / frequency / filter, each with its envelope
  and LFO, an overview card per voice with its waveform, resonance) and
  per voice (voice, unison and pitch in one row, oscillator designer with
  carrier/modulator tabs, modulation tabs) ·
  **SUB** (harmonic and bandwidth bars, amplitude, bandwidth & frequency,
  filter) ·
  **PAD** (profile, spectrum, oscillator, tabbed amplitude / frequency /
  filter; tables rebuild when you release the mouse) ·
  **EFFECTS** (a rack of the 3 part, 4 system and 8 insertion slots next to
  the editor for the selected slot, with every part's send) ·
  **MIXER** (master volume, tuning, a strip per part with on/off, volume,
  pan, MIDI channel, level and system-effect sends).
- **Bottom**: the host-automatable controls, grouped by function, and the
  output meter with the count of sounding notes. Play from your DAW or a
  MIDI keyboard; the editor has no on-screen keyboard.

Colour carries meaning. Engines keep their colours, and each job has the
same colour on every page: amplitude coral, frequency blue, filter green,
modulation pink, with LFOs a lighter shade of their section. Each of the 16
parts has its own hue (part buttons, mixer strips, send knobs), and so does
each effect type (rack, editor, mixer sends). A knob at its default dims its
readout, so what shapes a patch stands out.

Knobs: drag up/down, Shift for fine, double-click for default, wheel to
nudge. Envelope graphs: drag the points. Bars: drag to paint, right-click
to reset a bar.

| | |
|---|---|
| ![ADD voice](screenshots/voice.png) | ![Browser](screenshots/browser.png) |
| ![Effects](screenshots/fx.png) | ![Mixer](screenshots/mixer.png) |

## Community presets

On top of the 1318 ZynAddSubFX instruments, Zynabre bundles **199 community
patches**, mostly well-known classic sounds, in six banks listed first in
the browser:

| Bank | Patches | Source | License |
|---|---|---|---|
| Classic Bass | 35 | TB acid bass, techno / house / club basses, Italo-disco basses, sub bass | CC0 |
| Classic Leads & Synths | 31 | super saws, 1984 synth, punch lead, Italo leads and sequences, synth brass | CC0 |
| Classic Pads & Strings | 30 | 80s pads, super saw pads, synth strings, matrix and ensemble strings | CC0 |
| Classic Bells & Voices | 21 | synth bells, crystal, choirs, space voice, solo vox | CC0 |
| Yoshimi Chip | 49 | chiptune basses, leads and effects (Chip Van Dan) | GPL-2.0+ |
| Yoshimi Companion II | 33 | B3, theatre and cathedral organs, accordion, cello, Tomita whistle, choirs (Will J. Godfrey) | GPL-2.0+ |

The four *Classic* banks come from Yingchun Soul and the
[NeoSoundFonts](https://github.com/NeoSoundFonts) project (CC0). The two
Yoshimi banks come from [Yoshimi](https://github.com/Yoshimi/yoshimi).
Every patch was rendered with Zynabre and with the original ZynAddSubFX
and kept only when both renders have the same level; about half are
bit-for-bit identical. Full credits: [`banks-community/CREDITS.md`](banks-community/CREDITS.md).

## Host parameters

The whole sound lives in the plugin state (saved as ZynAddSubFX XML, so a
DAW session can be exported and opened in the original). On top of it the
host sees a small performance layer that drives the controllers Zyn already
understands, on every part:

| Parameter | Zyn controller |
|---|---|
| Filter Cutoff, Filter Q | CC 74, CC 71 |
| Bandwidth | CC 75 |
| Mod Wheel | CC 1 |
| FM Amount | CC 76 |
| Resonance Center, Resonance Bandwidth | CC 77, CC 78 |
| Expression, Panning | CC 11, CC 10 |
| Portamento | CC 65 |
| Output | gain after the engine |
| Engine Buffer, PAD Interpolation | settings |

MIDI: notes, velocity, polyphonic aftertouch, pitch bend, every
controller above plus sustain (64), all notes off (123), all sounds off
(120), reset controllers (121). Each part listens to its own MIDI
channel, as in Zyn.

## Install

Download the zip for your system from [Releases](../../releases) and unzip
it. Every zip has the plug-ins and `zynabre-standalone`, a desktop app that
needs no DAW. Builds of every commit are on the
[Actions](../../actions/workflows/build.yml) page (sign in to GitHub to
download them).

**macOS (Apple Silicon)**: AU, VST3, CLAP.

1. Copy the bundles to `~/Library/Audio/Plug-Ins/`: `Zynabre.component` into
   `Components/`, `Zynabre.vst3` into `VST3/`, `Zynabre.clap` into `CLAP/`.
2. The bundles are ad-hoc signed, not notarized. Clear the quarantine once:
   ```sh
   xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/Components/Zynabre.component
   ```
3. Rescan plug-ins in your DAW: *Jardim Sonoro › Zynabre*.

**Windows (x64)**: VST3, CLAP. Copy `Zynabre.vst3` to
`C:\Program Files\Common Files\VST3\` and `Zynabre.clap` to
`C:\Program Files\Common Files\CLAP\`. The build is not code-signed, so
SmartScreen may warn about the standalone app.

**Linux (x64)**: VST3, CLAP. Copy `Zynabre.vst3` to `~/.vst3/` and
`Zynabre.clap` to `~/.clap/`.

## Build from source

```sh
cargo install cargo-truce
cargo truce install --au2        # build + install the Audio Unit
cargo truce install              # every format
cargo truce run                  # standalone app
cargo test --release             # save/reload round trip over the banks
cargo run --release --example render -- banks/Pads/0003-Analog\ Pad\ 1.xiz out.wav 48 55 60 64
```

`auval` passes.

| Path | What lives there (and the C++ file it follows) |
|---|---|
| `src/synth/oscilgen.rs` | `Synth/OscilGen.cpp` |
| `src/synth/adnote.rs`, `ad_params.rs` | `Synth/ADnote.cpp`, `Params/ADnoteParameters.cpp` |
| `src/synth/sub.rs` | `Synth/SUBnote.cpp`, `Params/SUBnoteParameters.cpp` |
| `src/synth/pad.rs` | `Synth/PADnote.cpp`, `Params/PADnoteParameters.cpp` |
| `src/synth/filter.rs`, `modfilter.rs` | `DSP/*Filter.cpp`, `Synth/ModFilter.cpp` |
| `src/synth/envelope.rs`, `lfo.rs` | `Synth/Envelope.cpp`, `Synth/LFO.cpp` and their params |
| `src/synth/effects.rs` | `Effects/*.cpp`, `DSP/Unison.cpp` |
| `src/synth/part.rs`, `master.rs` | `Misc/Part.cpp`, `Containers/NotePool.cpp`, `Misc/Master.cpp` |
| `src/synth/xml.rs` | `Misc/XMLwrapper.cpp` |
| `src/state.rs` | shared patch, loader / PAD builder / state writer thread, banks |
| `src/ui/` | the editor |
| `banks/`, `examples-xmz/` | the instrument banks and example setups of ZynAddSubFX |
| `banks-community/` | community banks (NeoSoundFonts CC0, Yoshimi GPL), see its CREDITS.md |

## How it was checked

ZynAddSubFX was built from source twice: as released (`-O3 -ffast-math`)
and with strict IEEE arithmetic (`-O2 -ffp-contract=off`). A C++ harness
loads an instrument into a `Part`, plays the same notes at the same block
boundaries as `examples/render.rs`, and writes raw samples; both random
generators are seeded with 0x1234 before the first note. A second harness
runs every effect preset on a fixed test signal. The comparison against the
strict build is what the table above reports; against the fast-math build
the results are the same within rounding.

Engine timing follows Zyn's plugin builds: audio is computed in blocks of
the engine buffer (256 samples by default, selectable), and MIDI lands on
block boundaries.

## Roadmap

Fidelity
- [ ] Some old Yoshimi-saved patches (file version 2.0, e.g. Will Godfrey's
  "Ghost Choir") diverge from the original from the first block, even with
  one voice and no unison; the parser is not the cause. Seven community
  patches are left out until this is fixed.

Features from ZynAddSubFX not ported yet
- [ ] MIDI Program Change / Bank Select
- [ ] NRPN
- [ ] `.scl` / `.kbm` scale import and a scale editor on the tuning page
- [ ] Undo / redo and Zyn's preset clipboard (copy / paste envelopes, LFOs, oscillators)
- [ ] User banks: save straight into a bank shown in the browser
- [ ] More host-automatable parameters

Distribution
- [x] VST3 and CLAP builds for macOS, Windows and Linux (GitHub Actions)
- [ ] Universal (Intel + Apple Silicon) build
- [ ] Notarized bundle

Editor
- [ ] Test the 0.3 editor in more DAWs (drop-downs, envelope dragging, scaling)

## Credits

ZynAddSubFX: Nasca Octavian Paul, Mark McCurry and contributors (GPL-2.0 or
later); the bundled banks keep their authors' licences (see the files in
`banks/`). Framework: truce (MIT/Apache-2.0). Zynabre is an independent port
and is not affiliated with the ZynAddSubFX project.

## License

Free software under the [GNU General Public License v3.0](LICENSE) or later
(ZynAddSubFX is GPL-2.0-or-later, which allows this).
