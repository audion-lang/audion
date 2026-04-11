# AUDION

<p align="center">
  <img src="graphics/logo.png" alt="Audion" width="280" />
</p>


Audion is a batteries-included language for creating stuff that ticks. A metronome at its core lets you sanely deal with comms and sync to and from any hardware and software. Think synthesizers, MIDI, lighting rigs and anything that speaks OSC or sits on a network. Its concurrency model is syncronised  by default too. Audion is friendly to anyone (hello Agents!) with a familiar syntax that's quick to pick up.

Audion is a capable scripting language for the stuff that doesn't need introduction: Database Queries, network requests, data analysis. It's small enough to run on some embedded systems like the Raspberry PI.

**Audion is built for installations, performance, audiovisual art, sound design and instrument prototypes.**

---

Run this example with `./audion run examples/readme.au`.

```c
bpm(128);

// define an instruent, (don't worry theres lots available already)
define tom(freq, amp, gate) {
    let sig = sine(freq) * env(gate, 0.01, 0, 0.45) * amp;
    out(0, sig);
    out(1, sig);
}

fn main() {

    thread keys {
        seed("Your time machine into the future of randomness");
        scale = [60, 64, 67, 71, 72];
        pauses = array_seq_euclidean(5, 8);
        loop {
            note = array_rand(scale);
            tuned_note = mtof(note);
            synth("tom", freq: tuned_note);
            if (array_next(pauses, true)) {
                wait(0.5);
            } else {
                wait(0.25);
            }
        }
    }

    thread bass_with_strobe {
        loop_counter = 0;
        bassline = [160, 190];
        bass_freq = array_next(bassline, true);
        loop {
            osc_send("/light/strobe", rand(0.2, 0.4));
            synth("tom", freq: bass_freq);
            wait(1);
            loop_counter += 1;
            if (loop_counter % 32 == 0) {
                osc_send("/light/strobe", rand(0.6, 0.8));
                bass_freq = array_next(bassline, true);
            }
        }
    }
}
```

---

## Why Audion

Most creative coding tools make you choose: learn a domain-specific environment that can't do proper general programming, or wrestle a general-purpose language into doing creative work. Audion has just the right amount of built-in functionality that should get you where you want: the creative space. If you don't see what you need yet, please check the [Roadmap](docs/ROADMAP.md) and if still not there, open a feature request!


- **Sound** -- full SuperCollider synthesis, custom SynthDefs, sample playback, disk streaming
- **MIDI** -- full I/O with clock sync
- **OSC** -- talk to Max/MSP patches, TouchDesigner, Resolume, Processing, or anything else
- **Network** -- TCP, UDP, HTTP/S built in. Control hardware, call APIs, stream data
- **Timing** -- BPM-aware clock with drift compensation, Ableton Link sync across devices
- **Threads** -- spawn concurrent patterns with a single keyword. They stay in time
- **Files, JSON, hashing, regex, math** -- that stuff is covered too

## Status

Audion is currently in a beta phase, breaking changes are unlikely at this point. The language focus is on: good naming and features. You can download a precompiled binary and bundled working examples [Nightly releases](https://github.com/audion-lang/audion/releases/tag/nightly). Supercollider should be installed separately, Audion does not depend on supercollider.

## Programming in the AI-Native Age.

Audion has a small, consistent syntax with clear naming conventions and no hidden magic. AI tools can learn to write Audion fluently using a tiny help file, `./audion spec` creates the docs/AGENTS.md file. Since the language is concise, you can describe complex audiovisual behavior in a few lines of natural language and get working code back. Use AI as a collaborator: describe the piece you want to build, iterate on the code together, and perform it live.

I recommend you try write it yourself. The language is designed to stay out of your way and be enjoyable to use and hack with.

AI models generalize poorly and creativity often involves making connections between seemingly unrelated concepts, ideas, or experiences. Your role is vital in this process.

---

## Quick Start

- If you want to use Audion with Supercollider, download Supercollider and run it [SuperCollider](https://supercollider.github.io/) (run and "boot server")
- Download [Nightly release](https://github.com/audion-lang/audion/releases/tag/nightly) then open a terminal and `./audion run ./examples/readme.au`


## Documentation

Full language reference, builtin function docs, and SynthDef guide:

- [Language Reference](docs/LANGUAGE.md)


### Watch Mode (Live Coding)

```bash
audion run piece.au --watch
```

Edit your file, save, and Audion reloads instantly. SynthDef compilation is cached -- only changed definitions recompile.

---

## The Language in 60 Seconds

```c
// Variables, types, the usual
let name = "audion";
let bpm_val = 140;
let active = true;
let notes = [60, 64, 67, 72];
let config = ["key" => "value", "nested" => [1, 2, 3]];

// Functions are first-class
let double = fn(x) { return x * 2; };

// Closures become objects
fn make_synth_voice() {
    let freq = 440;
    let set_freq = fn(f) { freq = f; };
    let get_freq = fn() { return freq; };
    return this;
}
let voice = make_synth_voice();
voice.set_freq(880);

// Threads run concurrently and stay in time
bpm(120);
thread drums {
    loop {
        synth("default", freq: 60, amp: 0.8);
        wait(1);
    }
}

// Include and namespace system
include "lib/effects.au";
using lib::effects;

// Talk to the outside world
osc_config("127.0.0.1:9000");
osc_send("/dmx/channel/1", 255);
let resp = net_http("GET", "https://api.example.com/data");
```

---

## Custom Sound Design

Build synthesizers directly in Audion. The `define` block compiles to SuperCollider SynthDefs:

```c
define shimmer(freq, amp, gate) {
    let sig = lpf(saw(freq) + saw(freq * 1.01), 2000 + lfo_sine(0.3) * 1000);
    let wet = delay(sig, 0.035, 4) + delay(sig, 0.042, 3.5);
    out(0, pan(lfo_tri(0.1), sig + wet * 0.3) * env(gate, 0.1, 1, 3) * amp);
}

// Sampler with velocity layers
define piano(freq, amp, gate, vel) {
    out(0, pan(0,
        sample("piano_soft.wav", root: 60, vel_lo: 0, vel_hi: 80)
            * env(gate, 0.1, 1, 0.5) * amp
        +
        sample("piano_hard.wav", root: 60, vel_lo: 81, vel_hi: 127)
            * env(gate, 0.01, 1, 0.3) * amp
    ));
}
```

Oscillators, filters, envelopes, LFOs, effects, and sample playback, all composable with arithmetic. Unknown UGen names pass through to SuperCollider directly, so every SC UGen is available.

---

## Beyond Audio

Audion is a complete scripting language. File I/O, JSON, regex, HTTP clients, TCP/UDP servers, process execution, environment variables, hashing, it's all there. No external dependencies needed at the language level.

```c
// Read config, call an API, write results
let config = json_decode(file_read("config.json"));
let resp = net_http("GET", config["api_url"], nil,
    ["Authorization" => "Bearer " + config["token"]]);
let data = json_decode(resp.body);
file_write("output.json", json_encode(data));

// Run system commands
let result = exec("ffmpeg", "-i", "input.mp4", "-ss", "00:01:00", "frame.png");
```

---

### Want to help Build?

- [Rust](https://www.rust-lang.org/tools/install) (to build from source)
- [SuperCollider](https://supercollider.github.io/) (for audio synthesis -- `scsynth` and `sclang`)

```bash
git clone https://github.com/audion-lang/audion.git
cd audion
cargo build --release

# Start SuperCollider's audio server
scsynth -u 57110

# Run a file
./target/release/audion run your_piece.au

# Or drop into the REPL
./target/release/audion
```

### System Info / Known Limitations / Technical Debt
see docs/SYSTEM_INFORMATION.md

---

## License

GPL-3.0-or-later


🎵