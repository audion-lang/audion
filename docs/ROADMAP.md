# AUDION ROADMAP

This is a list of features that will be added, the order is not definite and this list may be incomplete. If you have an idea please open a feature request. We will make our best effort to respond quickly. The idea behind Audion is to be small, useful and most of all: fun. Programming (and therefore creating) can be fun, don't beleive anyone that tells you otherwise.

### error handling
try/catch is done [√] — a script can choose per call site whether a failing call kills its thread (default) or is caught (`try { } catch (e) { }`). Still open: anything beyond that, e.g. structured retry/backoff helpers for API calls.

### Embedded Subset
- export a subset of audion to C for defined targets: teensy/daisy/others
- define workflow: prototype in audion, export audion subset to C, compile to target

### binding to libusb
rusb - Rust binding to libusb, which works across multiple platforms

### more array functions
sorting, reducing, mapping

### yields in generators
be able to pause generation, lazy evaluation

### nice sound library
doing sound design is cool but it may not be your priority, we need the basics readymade
- analog synths (basics: saw/supersaw/tri/square) additive, subtractive [√ see examples]
- digital synths (fm/vector/granular/wavetable) [√ see examples]
- effect chains (reverbs/delays/compressors/filters) [√] — FreeVerb, CombC/L/N, Compander, LPF/HPF/BPF all available in `define` blocks. Dedicated shelf/peak EQ UGens (`BLowShelf`/`BHiShelf`/`BPeakEQ`) still missing, see the Butterworth `B*` gap below.

### more generators and modifiers
large library already built [√] — ~60 `array_seq_*`/`array_mel_*` generators (euclidean, markov, permutations, de Bruijn, necklaces, partitions, lattice walks, automata, etc.), used for example by `drum_sequencer()`'s euclidean+markov mutation. Keep adding more as needed.

### AI model invocation/training features
lightweight, built-in models already landed [√] (`src/ml.rs`, `ml_*` builtins) — Markov chain train/generate/next (used by `midi_markov.au` and `drum_sequencer()`'s mutation), weighted_choice, softmax, entropy, normalize, and a full Kalman filter/smoother set (filter, smooth, state, update, predict, used for anticipation in `breaks_seq.au`). Still open: invoking/training heavier external models (the unstable-crates problem from before).

### UI/window
built directly into the `audion` binary [√] (`--ui` flag, `src/ui/`), not a separate binary/repo:
- widgets (slider, knob, button, toggle, dropdown, text, array, ...) backed by `eframe`
- `.aui` TOML config for layout/widget defaults, auto-saved and reloaded
- `ui.three.*` facade for 3D canvases, backed by a real GPU renderer (`three.rs`/`three_gpu.rs`/`three_loader.rs`)
- still open: accessibility pass (keyboard nav, screen reader annotations)

### distributed runtime (TBD)
- because yes: sync and run multiple PCs for an artwork
- already possible with ableton sync, but we dive deeper into:
```
threads with keywords
thread (global) abc {} // runs on all nodes
thread (one) abc {} // runs on one node, does not matter which
thread (tag:tag_value, tagb:value2) abc {}  // runs on tagged nodes
shared (global) { } // shared scope
shared (tag:tag_value) { } // shared scope by tag only
```

### queue (TBD)
- defaults to simple FIFO, optionally configurable by userland closure

### website


### SFZ Instrument Support
- `sfz_load("piano.sfz")` → parse SFZ file in Rust, auto-generate multi-layer SynthDef with all sample mappings
- Maps SFZ regions to existing `sample()` UGen properties (`root`, `vel_lo`, `vel_hi`, `key_lo`, `key_hi`)
- Registers a synth name matching the SFZ filename: `synth("piano", freq: 440, vel: 100)`
- Alternative: link sfizz C library via FFI for full SFZ compliance (bigger dependency)

### more UGens for `define`
`define` blocks build SynthDefs directly (`src/dsp/` + `synthdef.rs`, no sclang — see git history around the vibelang-dsp integration). Coverage is good for the workhorse categories (oscillators, filters, envelopes, effects, dynamics, grain playback) but two whole capability classes are still missing, worth prioritizing over the smaller incremental gaps:
- **spectral (FFT/PV_\*)**: `FFT`/`IFFT` plus the phase-vocoder `PV_*` family (`PV_MagAbove`, `PV_BinShift`, `PV_Diffuser`, etc.) — zero coverage today, no spectral processing possible at all
- **demand-rate sequencing**: `Dseq`, `Dser`, `Drand`, `Dwhite`, `Demand`, `DemandEnvGen`, `TDuty`, etc. — pattern/sequence generators at the UGen level (audion has its own sequencing story elsewhere, but not this)

Smaller, mechanical (same one-match-arm-per-UGen pattern as the existing ~127) gaps if/when needed: triggers (`Trig`, `Done`, `FreeSelf`, `PulseDivider`, `SendTrig`, `Schmidt`, `ToggleFF`, ...), more oscillators (`Klang`, `Gendy1-3`, `VOsc`/`VOsc3`, `Formant`), more filters (`Hilbert`, `OneZero`/`TwoZero`, the Butterworth `B*` family, `DetectSilence`), convolution (`Convolution`/`2`/`2L`/`3`, `PartConv`), buffer introspection (`BufFrames`, `BufDur`, `BufChannels`, ...), control (`MouseX`/`MouseY`/`MouseButton`, `Select`, `Timer`), info (`SampleRate`, `NumRunningSynths`, ...), panning (`PanB`, `DecodeB2`, `PanAz`, ...), bufdelays, conversion, random (`Rand`/`IRand`/`LinRand`), physical, envelopes.

`Klank` also has a best-effort/unverified port (see comment in `synthdef.rs`) — needs a real scsynth test if anyone actually uses it.

### JIT (TBD)

### User-defined types and more oop (TBD)
- inheritance seems likely





