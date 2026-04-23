# AUDION ROADMAP

This is a list of features that will be added, the order is not definite and this list may be incomplete. If you have an idea please open a feature request. We will make our best effort to respond quickly. The idea behind Audion is to be small, useful and most of all: fun. Programming (and therefore creating) can be fun, don't beleive anyone that tells you otherwise.

### error handling
has been improved but we may want to *catch* errors in situations like API calls etc.

### binding to libusb
rusb - Rust binding to libusb, which works across multiple platforms

### more array functions
sorting, reducing, mapping

### for a in array
the c with counter is cumbersome to write some of the time, and sometimes we don't need the counter
no looping over objects/functions, should be an error
syntax `for a in iterable` not foreach keyword, avoid many keywords in the language

### yields in generators
be able to pause generation, lazy evaluation

### nice sound library
doing sound design is cool but it may not be your priority, we need the basics readymade
- analog synths (basics: saw/supersaw/tri/square) additive, subtractive [√ see examples]
- digital synths (fm/vector/granular/wavetable) [√ see examples]
- effect chains (reverbs/delays/compressors/filters/eq)

### more generators and modifiers
sequence/melodic generators and modifiers

### AI model invocation/training features
there are some nice crates but things are not very stable right now, ongoing

### UI/window
- will be implemented as separate binary, see audion-window repo

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

### JIT (TBD)

### User-defined types and more oop (TBD)
- inheritance seems likely





