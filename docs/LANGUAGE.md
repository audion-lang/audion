# Audion Language Reference

## Overview

Audion is a **dynamically typed, interpreted, imperative** scripting language with C-family syntax. It talks to many things, one of the core integrations is SuperCollider's `scsynth` via OSC and Audion is designed for generative music, performance and audio-visual installations that can run forever.

The interpreter is a Rust **tree-walking interpreter**, source code is lexed into tokens, parsed into an AST, and executed directly by walking the tree. There is no bytecode compilation or JIT. This keeps the implementation simple and the startup instant, which matters for live music where you want to tweak and re-run quickly.

Audion takes design cues from several languages:
- **C** - familiar syntax, control flow, semicolons, braces
- **Lua** - lightweight embeddable feel, minimal core, easy to extend
- **Elixir** - see ROADMAP.md.
- **JavaScript** - first-class functions, closures, `let` declarations
- **PHP** - ordered key-value arrays, dynamic typing, `=>` syntax, loose comparisons

Audion goals:
- Have good naming
- Be general purpose
- Have a extensive standard library


```c
bpm(120);

fn kick() {
    synth("default", freq: 60, amp: 0.8);
    wait(1);
}

fn main() {
    thread kick_loop {
        loop { kick(); }
    }
}
```

---

## Types

Audion is dynamically typed. All values are one of:

| Type | Examples | Notes |
|------|----------|-------|
| **number** | `42`, `3.14`, `0.5` | All numbers are 64-bit floats |
| **string** | `"hello"`, `"kick"` | Double-quoted, supports escapes |
| **bool** | `true`, `false` | |
| **nil** | `nil` | Default value for uninitialized variables |
| **function** | `fn(x) { return x; }` | First-class, supports closures |
| **array** | `[1, 2, "key" => 3]` | Ordered key-value map, PHP-style |
| **object** | `return this;` | Closure-object with dot access |
| **namespace** | `include "file.au";` | Imported module scope |

### Truthiness

| Value | Truthy? |
|-------|---------|
| `nil` | false |
| `false` | false |
| `0` | false |
| `""` (empty string) | false |
| `[]` (empty array) | false |
| `<object {}>` (empty object) | false |
| Everything else | true |

---

## Variables

```c
let x;              // declares x, initialized to nil
let x = 5;          // declares x with value
x = 10;             // assignment (creates if not found)
```

### Compound Assignment

```c
x += 5;             // x = x + 5
x -= 3;             // x = x - 3
x *= 2;             // x = x * 2
x /= 2;             // x = x / 2
```

Variables are block-scoped. Inner blocks can see and modify parent variables.

---

## Arrays

PHP-style ordered key-value arrays. Keys can be strings or numbers. Values can be anything: numbers, strings, bools, functions, nested arrays.

### Creating Arrays

```c
// Auto-indexed (keys: 0, 1, 2)
let nums = ["a", "b", "c"];

// Key-value pairs with =>
let config = [
    "name" => "audion",
    "version" => 1,
    "active" => true,
];

// Mixed: auto-index and explicit keys
let mixed = ["key" => 100, 42, 99];
// mixed["key"] is 100, mixed[0] is 42, mixed[1] is 99

// Empty array
let empty = [];

// Nested arrays
let data = [
    "instruments" => ["kick", "snare", "hat"],
    "bpm" => 120,
];

// Functions as values
let handlers = [
    "double" => fn(x) { return x * 2; },
    "triple" => fn(x) { return x * 3; },
];
```

### Accessing Elements

```c
let a = ["x" => 10, "y" => 20];
a["x"];           // 10
a[0];             // nil (no key 0)

let nums = [100, 200, 300];
nums[0];          // 100
nums[2];          // 300
nums[99];         // nil (missing keys return nil)
```

### Nested Access

```c
let data = ["inner" => [10, 20, 30]];
data["inner"][1];  // 20
```

### Modifying Arrays

```c
let a = [1, 2, 3];
a[0] = 99;           // update existing key
a["new"] = "hello";  // add new key
a[1] += 10;          // compound assignment
```

### Copy Semantics

Arrays are **deep copied** on assignment. Modifying a copy does not affect the original. Array inserts are O(1), removal is O(n).

```c
let a = [1, 2, 3];
let b = a;          // b is an independent copy
b[0] = 99;
print(a[0]);        // 1 (unchanged)
```

### Array Built-in Functions

| Function | Description | Returns |
|----------|-------------|---------|
| `count(arr)` | Number of elements | number |
| `array_push(arr, value)` | Append value with next auto-index | new length |
| `array_pop(arr)` | Remove and return last element | value or nil |
| `keys(arr)` | Get all keys as an array | array |
| `has_key(arr, key)` | Check if key exists | bool |
| `remove(arr, key)` | Remove element by key | removed value or nil |
| `array_push(arr, value)` | Alias for `push()` | new length |
| `array_pop(arr)` | Alias for `pop()` | value or nil |
| `array_beginning(arr)` | Move cursor to first element | value or nil |
| `array_end(arr)` | Move cursor to last element | value or nil |
| `array_next(arr, false)` | Advance cursor, use true to loop, return value | value or nil |
| `array_prev(arr, false)` | Move cursor back, use true to loop, return value | value or nil |
| `array_current(arr)` | Return value at cursor | value or nil |
| `array_key(arr)` | Return key at cursor | value or nil |

`count()` also works on strings (returns character count) and on nested arrays:

```c
let a = ["inner" => [1, 2, 3]];
count(a);           // 1
count(a["inner"]);  // 3
count("hello");     // 5
```

```c
let a = [10, 20];
push(a, 30);          // a is now [0 => 10, 1 => 20, 2 => 30]
pop(a);               // returns 30, a is now [0 => 10, 1 => 20]
has_key(a, 0);        // true
has_key(a, "nope");   // false
remove(a, 0);         // returns 10, a is now [1 => 20]
keys(a);              // [0 => 1] (array of the remaining keys)
```

### Array Cursor Operations

Arrays have an internal cursor for sequential traversal. This is useful for sequencer-style iteration.

```c
let seq = [60, 64, 67, 72];
array_beginning(seq);         // moves cursor to first, returns 60
array_next(seq);              // advances cursor, returns 64
array_next(seq);              // returns 67
array_key(seq);               // returns 2 (the current key)
array_current(seq);           // returns 67 (without moving)
array_end(seq);               // moves cursor to last, returns 72
array_prev(seq);              // moves cursor back, returns 67
```

Cursor functions return `nil` when moving past the beginning or end of the array.

---

## Operators

### Arithmetic

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Add / string concat | `2 + 3` → `5`, `"a" + "b"` → `"ab"` |
| `-` | Subtract | `10 - 3` → `7` |
| `*` | Multiply | `4 * 5` → `20` |
| `/` | Divide | `10 / 3` → `3.333...` |
| `%` | Modulo | `10 % 3` → `1` |

String concatenation works with mixed types: `"freq: " + 440` → `"freq: 440"`.

### Comparison

| Operator | Description |
|----------|-------------|
| `==` | Equal (works on all types) |
| `!=` | Not equal |
| `<` | Less than (numbers only) |
| `>` | Greater than |
| `<=` | Less than or equal |
| `>=` | Greater than or equal |

### Logical

| Operator | Description | Short-circuits? |
|----------|-------------|-----------------|
| `&&` | AND | Yes - stops if left is falsy |
| `\|\|` | OR | Yes - stops if left is truthy |
| `!` | NOT | - |

### Precedence (lowest to highest)

1. `=`, `+=`, `-=`, `*=`, `/=`
2. `||`
3. `&&`
4. `==`, `!=`
5. `<`, `>`, `<=`, `>=`
6. `+`, `-`
7. `*`, `/`, `%`
8. `-` (unary), `!`
9. Function calls

## Bitwise Operators

Audion supports bitwise operations on numbers (converted to i64 internally):

**Binary operators:**
- `&` — bitwise AND
- `|` — bitwise OR
- `^` — bitwise XOR
- `<<` — left shift
- `>>` — right shift (arithmetic)

**Unary operator:**
- `~` — bitwise NOT

---

## Control Flow

### if / else

```c
if (x > 0) {
    print("positive");
} else if (x == 0) {
    print("zero");
} else {
    print("negative");
}
```

### while

```c
while (x < 100) {
    x += 1;
}
```

### loop (infinite)

```c
loop {
    synth("default", freq: 440);
    wait(1);
}
```

### for (C-style)

```c
for (let i = 0; i < 10; i += 1) {
    print(i);
}
```

### break / continue

```c
loop {
    if (done) { break; }
    if (skip) { continue; }
    // ...
}
```

---

## Functions

### Declaration

```c
fn greet(name) {
    print("hello", name);
}

fn add(a, b) {
    return a + b;
}
```

### Anonymous Functions (First-Class)

```c
let double = fn(x) { return x * 2; };
double(21);  // 42
```

### Closures

Functions capture their defining scope:

```c
fn make_counter() {
    let count = 0;
    return fn() {
        count += 1;
        return count;
    };
}

let c = make_counter();
c();  // 1
c();  // 2
```

### Variable Functions

```c
fn test(a, b) { return a + b; }
let something = "test";
something(10, 20);  // returns 30

// Also works with builtins
let f = "print";
f("hello world");
```

This is very useful combined with arrays for generative composition.


### Named Arguments

Any function call supports named arguments using `name: value` syntax:

```c
synth("default", freq: 440, amp: 0.5);
set(node_id, freq: 880, pan: -1);
```

Named arguments are separated from positional arguments. Positional args come first.

---

## Objects

Functions that end with `return this;` return their local scope as an object. This provides a lightweight way to create stateful objects with methods - no class keyword needed.

### Creating Objects

```c
fn make_counter() {
    let count = 0;
    let increment = fn() {
        count += 1;
        return count;
    };
    let get = fn() {
        return count;
    };
    return this;
}

let c = make_counter();
```

`this` captures the current function's local scope as a `Value::Object`. All local variables and nested functions become fields on the object.

### Dot Access

Use `.` to access fields and call methods:

```c
c.increment();      // 1
c.increment();      // 2
c.get();            // 2
c.count;            // 2 - direct field access
```

Methods share the same environment, so `increment()` modifying `count` is visible to `get()` and to direct field access.

### Member Assignment

Object fields can be modified with `.` assignment:

```c
c.count = 100;
c.get();            // 100

// Compound assignment works too
c.count += 5;
c.count;            // 105
```

### Each Call Creates an Independent Object

```c
let c1 = make_counter();
let c2 = make_counter();
c1.increment();     // 1
c2.increment();     // 1 (independent)
c1.get();           // 1
```

### Copy Semantics

Objects are **deep copied** on assignment (same as arrays). The copy gets its own independent state, and methods inside the copy are remapped to point at the copy's environment:

```c
let c = make_counter();
c.increment();      // 1
c.increment();      // 2

let c2 = c;         // deep copy
c2.increment();     // 3
c2.get();           // 3
c.get();            // 2 (unchanged)
```

### Dot Access on Arrays

Dot access also works on arrays with string keys - `arr.key` is equivalent to `arr["key"]`:

```c
let config = ["name" => "audion", "version" => 1];
config.name;        // "audion"
config.version;     // 1
```

---

## Include & Namespaces

Use `include` to load external `.au` files. Definitions from the included file are accessible under a hierarchical namespace using `::` syntax.

### Basic Include

```c
include "math.au";

math::sin(3.14);
math::PI;
```

The namespace hierarchy is derived from the include path (without the `.au` extension). Each path component becomes a namespace level:

```c
include "lib/math/utils.au";

lib::math::utils::double(5);
```

### Include with `as` Alias

Use `as` to override the default namespace:

```c
// Single alias — useful for absolute paths or renaming
include "/usr/share/audion/lib/math.au" as math;
math::sin(3.14);

// Multi-segment alias
include "mylib.au" as vendor::mylib;
vendor::mylib::func();
```

### Include Semantics

- **Hierarchical namespaces**: The include path determines the namespace hierarchy (e.g. `"lib/math.au"` creates `lib::math`)
- **Include-once**: Including the same file twice skips re-execution, but the namespace is still installed (allowing different `as` aliases for the same file)
- **No main()**: The `main()` function of included files is not executed
- **Relative paths**: Paths are resolved relative to the including file's directory
- **Fresh scope**: Each included file runs in its own scope — only its top-level definitions are captured
- **Shared intermediates**: Multiple includes that share a prefix (e.g. `lib/a.au` and `lib/b.au`) share the intermediate `lib` namespace

### Namespace Access

Use `::` to access functions and variables from a namespace. Multi-level chaining works:

```c
include "synths.au";

synths::pad(440, 0.5);
synths::DEFAULT_AMP;
```

### `using` — Import Namespace into Local Scope

The `using` keyword copies all bindings from a namespace into the current scope, so you can call functions without the namespace prefix:

```c
include "lib/math.au" as lib::math;

using lib::math;
sin(3.14);       // no prefix needed
clamp(0.8, 0, 1);
```

If two namespaces define the same name, the later `using` wins:

```c
include "a.au" as a;   // defines fn value() { return 1; }
include "b.au" as b;   // defines fn value() { return 2; }

using a;
using b;        // overwrites a's value()
value();        // returns 2
```

`using` works in any scope — threads, functions, blocks:

```c
include "lib/helpers.au" as lib::helpers;

thread drums {
    using lib::helpers;
    play_pattern([1, 0, 1, 0]);   // from helpers
}

fn process() {
    using lib::helpers;
    return transform(data);        // from helpers
}
```

### Example: Multi-File Project

**lib/math.au**
```c
let PI = 3.14159265;

fn sin(x) {
    // ...
}

fn clamp(val, lo, hi) {
    if (val < lo) { return lo; }
    if (val > hi) { return hi; }
    return val;
}
```

**lib/synths.au**
```c
fn pad(freq, amp) {
    synth("default", freq: freq, amp: amp);
}
```

**main.au**
```c
include "lib/math.au";
include "lib/synths.au";

fn main() {
    using lib::math;
    let freq = 440 * sin(PI / 4);
    let amp = clamp(0.8, 0.0, 1.0);
    lib::synths::pad(freq, amp);
}
```

---

## Threads

Spawn concurrent OS threads with the `thread` keyword:

```c
thread my_beat {
    loop {
        synth("default", freq: 200);
        wait(1);
    }
}
```

- Each thread runs independently with its own local scope
- Threads share the global environment (functions, globals)
- The program waits for all threads to finish before exiting
- Thread errors are printed to stderr but don't crash the program

### Example: Multiple Concurrent Patterns

```c
bpm(120);

fn main() {
    thread kick {
        loop {
            synth("default", freq: 60, amp: 0.8);
            wait(1);
        }
    }

    thread hihat {
        loop {
            synth("default", freq: 6000, amp: 0.2);
            wait(0.5);
        }
    }

    thread bass {
        loop {
            let node = synth("default", freq: 110, amp: 0.4);
            wait(2);
            free(node);
        }
    }
}
```

---

## Built-in Functions

### Audio

| Function | Description | Returns |
|----------|-------------|---------|
| `synth(name, ...)` | Create a synth on scsynth | node ID (number) |
| `free(node_id)` | Free a synth node | nil |
| `set(node_id, ...)` | Modify a running synth's parameters | nil |

```c
let n = synth("default", freq: 440, amp: 0.5);  // start synth
set(n, freq: 880);                                // change freq
free(n);                                           // stop synth
```

#### `synth("default", ...)` Parameters

The `"default"` SynthDef is built into SuperCollider. It's a simple subtractive synth (sawtooth oscillator through a low-pass filter with an ADSR envelope). All parameters are passed as named arguments:

| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `freq` | `440` | 20–20000 | Frequency in Hz |
| `amp` | `0.1` | 0.0–1.0 | Output amplitude |
| `pan` | `0` | -1.0 to 1.0 | Stereo panning (left to right) |
| `gate` | `1` | 0 or 1 | Envelope gate. Set to `0` to release the note |
| `out` | `0` | 0+ | Output bus number |

```c
// Basic usage
synth("default", freq: 440, amp: 0.5);

// Full control
let n = synth("default", freq: 261.63, amp: 0.3, pan: -0.5);

// Release the note gracefully (envelope release) instead of free()
set(n, gate: 0);

// Hard stop (immediate cutoff)
free(n);
```

> **Note:** Control values are sent as 32-bit floats over OSC. Any non-number named argument is sent as `0.0`. Custom SynthDefs loaded in SuperCollider can be used by name with their own parameters - `synth()` forwards all named arguments as-is.

#### Synth Lifetime Patterns

All `env()` envelopes compile with `doneAction: 2`, meaning SuperCollider **automatically frees the node** when the envelope finishes. This affects how you should structure your synth code depending on the envelope type.

**Percussive synths (sus=0) — fire and forget**

For drums, hits, and one-shots where `env()` has `sus: 0`, the node self-destructs after `atk + rel` time. Just call `synth()` each hit — no `free()` or `set()` needed:

```c
define kick(freq, amp, gate) {
    out(0, sine(freq) * env(gate, 0.001, 0, 0.3) * amp);
}

thread drums {
    loop {
        synth("kick", freq: 60, amp: 0.9);
        wait(1);
    }
}
```

Overlapping nodes are fine — at `wait(0.25)` with a 0.93s release, you'd have ~4 nodes alive at any time, which is negligible for SuperCollider.

> **Do not** try to reuse a percussive synth node with `set(node, gate: 1)` — once `gate` goes to `0`, the envelope completes its release and the node is freed. Any subsequent `set()` calls target a node that no longer exists.

**Sustained synths (sus>0) — gate control**

For pads, drones, and basslines where `env()` has `sus > 0`, create the node once and toggle `gate` with `set()` to retrigger:

```c
define pad(freq, amp, gate) {
    out(0, reverb(saw(freq) * env(gate, 0.5, 1, 2) * amp, 0.5, 0.8, 0.3));
}

thread pad_loop {
    let n = synth("pad", freq: 220, amp: 0.3);
    loop {
        set(n, gate: 1, freq: rand(200, 400));
        wait(2);
        set(n, gate: 0);
        wait(0.5);
    }
}
```

> **Warning:** Setting `gate: 0` on a sustained synth starts its release phase. If the release completes before you set `gate: 1` again, the node is freed (same `doneAction: 2`). Keep your `wait()` during the off period shorter than the `rel` time, or use `free()` and recreate instead.

**Long-running synths — set() for parameter changes**

If a synth should play continuously and you only need to change parameters, never set `gate: 0`:

```c
thread bass {
    let n = synth("dirty_bass", freq: 110, amp: 0.4);
    loop {
        set(n, freq: rand(80, 160));
        wait(0.5);
    }
}
```

### Timing

| Function | Description | Returns |
|----------|-------------|---------|
| `bpm()` | Get current BPM | number |
| `bpm(n)` | Set BPM | previous BPM |
| `wait(beats)` | Sleep for N beats at current BPM | nil |
| `wait_ms(ms)` | Sleep for N milliseconds | nil |

```c
bpm(140);           // set tempo
wait(1);            // wait 1 beat (~428ms at 140 BPM)
wait(0.25);         // wait a sixteenth note
wait_ms(500);       // wait exactly 500ms regardless of BPM
```

BPM changes take effect immediately across all threads.

### Utility

| Function | Description | Returns |
|----------|-------------|---------|
| `print(...)` | Print values to stdout (space-separated) | nil |
| `rand(min, max)` | Random number in range | number |
| `rand()` | Random number 0.0 to 1.0 | number |
| `seed(value)` | Seed the PRNG for reproducible randomness | nil |
| `array_rand(arr)` | Return a random value from an array | value or nil |
| `time()` | Seconds elapsed since program start | number |
| `mtof(note)` | MIDI note to frequency (12-TET) | number |
| `mtof(note, tuning)` | MIDI note to frequency with custom tuning | number |
| `ftom(freq)` | Frequency to MIDI note (12-TET, fractional) | number |
| `ftom(freq, tuning)` | Frequency to nearest MIDI note in tuning | number |

```c
print("hello", 42, true);   // prints: hello 42 true
let freq = rand(200, 800);  // random frequency
let t = time();              // elapsed seconds
```

#### `mtof()` / `ftom()` - MIDI and Frequency Conversion

Convert between MIDI note numbers and frequencies. With no tuning argument, uses standard 12-TET (A4 = 440 Hz). Pass a Scala file path or an array of ratios for microtonal tunings.

```c
mtof(69);          // 440.0 (A4 in 12-TET)
mtof(60);          // 261.63 (middle C)
ftom(440);         // 69.0
ftom(442);         // 69.08 (fractional)

// Scala file tuning - note 69 = 440 Hz, scale degrees wrap per-period
mtof(69, "scales/just_intonation.scl");
mtof(72, "scales/just_intonation.scl");

// Array of ratios - last element is the period (2/1 = octave)
let just = [9/8, 5/4, 4/3, 3/2, 5/3, 15/8, 2/1];
mtof(69, just);    // 440.0 (degree 0 = unison)
mtof(70, just);    // 495.0 (degree 1 = 9/8)

// Use in a helper for cleaner code
fn tune(note) {
    return mtof(note, "./scales/al-farabi_dor.scl");
}
synth("bass", freq: tune(60), amp: 0.5, gate: 1);
```

Scala files are cached after the first load - safe to call `mtof` in tight loops without repeated disk reads. The `ftom` function returns fractional notes for 12-TET and the nearest integer note for custom tunings (comparing in log space).

#### `seed()` - Reproducible Randomness

`seed()` sets a thread-local PRNG seed. Once seeded, `rand()` and `array_rand()` produce a deterministic sequence - same seed, same results every time. This is scope-specific per thread, so one thread can use seeded randomness while others stay truly random.

```c
seed("abc");               // seed with a string
seed(12345);               // seed with a number
seed(false);               // disable seed, back to random

// Seeded rand produces the same sequence every run
seed(42);
print(rand(0, 100));       // always the same value
print(rand(0, 100));       // always the same second value
```

**Thread-local seeding** - each thread has its own independent seed state:

```c
fn main() {
    thread drums {
        seed("pattern_a");
        loop {
            // reproducible pattern, same every run
            let vel = rand(0.3, 0.9);
            synth("kick", amp: vel);
            wait(1);
        }
    }

    thread melody {
        // no seed - fully random every run
        loop {
            let freq = rand(200, 800);
            synth("default", freq: freq);
            wait(0.5);
        }
    }
}
```

#### `array_rand()` - Random Array Element

Returns a random value from an array. Respects `seed()` for reproducible picks. Returns `nil` for empty arrays.

```c
let notes = [60, 64, 67, 72];
let note = array_rand(notes);   // random element from the array

let samples = ["kick.wav", "snare.wav", "hat.wav"];
let pick = array_rand(samples); // random filename

// With seed for reproducibility
seed("melody");
let scale = [261, 293, 329, 349, 392, 440, 493];
let note = array_rand(scale);   // same pick every run
```

### Date / Time

| Function | Description | Returns |
|----------|-------------|---------|
| `date(fmt)` | Format current date/time | string |
| `date(fmt, timestamp)` | Format a specific unix timestamp | string |
| `timestamp()` | Current time as unix seconds | number |
| `timestamp_ms()` | Current time as unix milliseconds | number |

`date()` uses PHP-style format characters:

| Char | Description | Example |
|------|-------------|---------|
| `Y` | 4-digit year | `2025` |
| `m` | Month (01-12) | `03` |
| `d` | Day (01-31) | `07` |
| `H` | Hour 24h (00-23) | `14` |
| `i` | Minute (00-59) | `05` |
| `s` | Second (00-59) | `09` |
| `N` | Day of week (1=Mon, 7=Sun) | `5` |
| `j` | Day without leading zero | `7` |
| `n` | Month without leading zero | `3` |
| `G` | Hour 24h no leading zero | `14` |
| `g` | Hour 12h no leading zero | `2` |
| `A` | AM/PM | `PM` |
| `U` | Unix timestamp | `1709827509` |

Non-format characters are passed through as-is:

```c
date("Y-m-d");              // "2025-03-07"
date("H:i:s");              // "14:05:09"
date("Y-m-d H:i:s");        // "2025-03-07 14:05:09"

// Format a specific timestamp (like PHP)
date("Y", 0);               // "1970"
date("Y-m-d", 1700000000);  // "2023-11-14"

// Timestamps
let now = timestamp();       // 1709827509.123
let now_ms = timestamp_ms(); // 1709827509123.0
```

### Type Casts

| Function | Behavior | Returns |
|----------|----------|---------|
| `int(val)` | Number: truncate. String: parse. Bool: 0/1. Nil: 0 | number |
| `float(val)` | Number: identity. String: parse. Bool: 0.0/1.0. Nil: 0.0 | number |
| `bool(val)` | Uses standard truthiness rules | bool |
| `str(val)` | Convert any value to its string representation | string |

```c
int(3.7);          // 3
int(-3.7);         // -3
int("42");         // 42
int(true);         // 1

float("3.14");     // 3.14
float(true);       // 1

bool(1);           // true
bool(0);           // false
bool("");          // false
bool("hello");    // true

str(42);           // "42"
str(3.14);         // "3.14"
str(true);         // "true"
str(nil);          // "nil"
```

### Exec

| Function | Description | Returns |
|----------|-------------|---------|
| `exec(command, arg1, ...)` | Run an external command | array or false |

Runs a system command with optional arguments. Returns an array with `stdout`, `stderr`, and `status` keys on success, or `false` if the command doesn't exist.

```c
let result = exec("echo", "hello");
if (result) {
    print(result["stdout"]);    // "hello\n"
    print(result["status"]);    // 0
}

// Command not found
let bad = exec("nonexistent_command");
if (!bad) {
    print("command not found");
}

// Check exit status
let r = exec("test", "-d", "/tmp");
print(r["status"]);    // 0
```

### Hashing

| Function | Description | Returns |
|----------|-------------|---------|
| `hash(algorithm, data)` | Hash a string | hex string |

Supported algorithms: `"md5"`, `"sha256"`, `"sha512"`. Panics on unsupported algorithms.

```c
hash("md5", "hello");      // "5d41402abc4b2a76b9719d911017c592"
hash("sha256", "hello");   // "2cf24dba5fb0a30e26e83b2ac5b9e29e..."
hash("sha512", "hello");   // 128-character hex string
```

### Math

All math functions are prefixed with `math_` and operate on numbers (f64).

#### Rounding & Parts

| Function | Description | Returns |
|----------|-------------|---------|
| `math_round(n)` | Round to nearest integer | number |
| `math_round(n, precision)` | Round to N decimal places | number |
| `math_floor(n)` | Round down (toward negative infinity) | number |
| `math_ceil(n)` | Round up (toward positive infinity) | number |
| `math_trunc(n)` | Truncate toward zero (remove fractional part) | number |
| `math_fract(n)` | Fractional part of a number | number |
| `math_sign(n)` | Sign: -1, 0, or 1 | number |
| `math_abs(n)` | Absolute value | number |

#### Comparison & Clamping

| Function | Description | Returns |
|----------|-------------|---------|
| `math_min(a, b, ...)` | Lowest value (variadic, any number of args) | number |
| `math_max(a, b, ...)` | Highest value (variadic, any number of args) | number |
| `math_clamp(n, min, max)` | Clamp value between min and max | number |

#### Arithmetic

| Function | Description | Returns |
|----------|-------------|---------|
| `math_pow(base, exp)` | base raised to the power of exp | number |
| `math_sqrt(n)` | Square root | number |
| `math_cbrt(n)` | Cube root | number |
| `math_fmod(x, y)` | Floating point remainder of x/y | number |
| `math_intdiv(a, b)` | Integer division (truncated). Errors on division by zero | number |
| `math_hypot(x, y)` | Hypotenuse: sqrt(x^2 + y^2) | number |

#### Trigonometry

| Function | Description | Returns |
|----------|-------------|---------|
| `math_sin(n)` | Sine (radians) | number |
| `math_cos(n)` | Cosine (radians) | number |
| `math_tan(n)` | Tangent (radians) | number |
| `math_asin(n)` | Arc sine | number |
| `math_acos(n)` | Arc cosine | number |
| `math_atan(n)` | Arc tangent | number |
| `math_atan2(y, x)` | Arc tangent of y/x (quadrant-aware) | number |

#### Hyperbolic

| Function | Description | Returns |
|----------|-------------|---------|
| `math_sinh(n)` | Hyperbolic sine | number |
| `math_cosh(n)` | Hyperbolic cosine | number |
| `math_tanh(n)` | Hyperbolic tangent | number |
| `math_asinh(n)` | Inverse hyperbolic sine | number |
| `math_acosh(n)` | Inverse hyperbolic cosine | number |
| `math_atanh(n)` | Inverse hyperbolic tangent | number |

#### Logarithms & Exponentials

| Function | Description | Returns |
|----------|-------------|---------|
| `math_log(n)` | Natural logarithm (base e) | number |
| `math_log(n, base)` | Logarithm with specified base | number |
| `math_log2(n)` | Base-2 logarithm | number |
| `math_log10(n)` | Base-10 logarithm | number |
| `math_log1p(n)` | ln(1 + n), accurate near zero | number |
| `math_exp(n)` | e raised to the power n | number |
| `math_expm1(n)` | e^n - 1, accurate near zero | number |

#### Angle Conversion

| Function | Description | Returns |
|----------|-------------|---------|
| `math_deg2rad(n)` | Degrees to radians | number |
| `math_rad2deg(n)` | Radians to degrees | number |

#### Interpolation & Mapping

| Function | Description | Returns |
|----------|-------------|---------|
| `math_lerp(a, b, t)` | Linear interpolation: a + (b - a) * t | number |
| `math_map(value, in_min, in_max, out_min, out_max)` | Remap value from one range to another | number |

#### Constants

| Function | Description | Returns |
|----------|-------------|---------|
| `math_pi()` | Pi (3.141592653589793) | number |
| `math_e()` | Euler's number (2.718281828459045) | number |

#### Type Checks

| Function | Description | Returns |
|----------|-------------|---------|
| `math_is_finite(n)` | Check if number is finite | bool |
| `math_is_infinite(n)` | Check if number is infinite | bool |
| `math_is_nan(n)` | Check if number is NaN | bool |

```c
math_round(3.7);           // 4
math_round(3.14159, 2);    // 3.14
math_floor(4.9);           // 4
math_ceil(4.1);            // 5
math_abs(-42);             // 42
math_sqrt(9);              // 3
math_pow(2, 10);           // 1024
math_min(3, 1, 4, 1, 5);  // 1
math_max(3, 1, 4, 1, 5);  // 5
math_clamp(150, 0, 127);  // 127
math_log(1);               // 0
math_log(8, 2);            // 3 (log base 2 of 8)

// Interpolation
math_lerp(0, 100, 0.5);   // 50
math_map(64, 0, 127, 0, 1);  // ~0.504 (MIDI to normalized)
```

#### Musical Applications

These functions are particularly useful for music programming, synthesis, and generative composition:

| Use Case | Example | Description |
|----------|---------|-------------|
| **MIDI velocity clamping** | `math_clamp(vel, 0, 127)` | Keep velocity in valid MIDI range |
| **MIDI to normalized** | `math_map(cc_val, 0, 127, 0.0, 1.0)` | Map a CC value (0-127) to a 0.0-1.0 float |
| **Frequency range mapping** | `math_map(knob, 0, 127, 200, 8000)` | Map a MIDI knob to a frequency range |
| **Crossfade** | `math_lerp(amp_a, amp_b, mix)` | Smooth crossfade between two amplitudes |
| **LFO depth scaling** | `math_lerp(400, 800, math_sin(t))` | Map a sine LFO (-1..1) to a filter cutoff range |
| **BPM to ms** | `math_round(60000 / bpm())` | Convert current BPM to milliseconds per beat |
| **Octave from ratio** | `math_log2(freq / 440)` | How many octaves above/below A4 |
| **Cents between freqs** | `math_log2(f2 / f1) * 1200` | Interval in cents between two frequencies |
| **Equal-power pan** | `math_sin(math_deg2rad(pos * 45))` | Equal-power panning curve from position |
| **Euclidean distance** | `math_hypot(x2 - x1, y2 - y1)` | Distance in 2D space (spatialisation) |
| **Waveshaping** | `math_tanh(signal * drive)` | Soft-clip distortion using hyperbolic tangent |
| **Phase wrapping** | `math_fmod(phase, math_pi() * 2)` | Keep phase angle within 0..2pi |
| **Beat quantize** | `math_ceil(time() * bpm() / 60)` | Round up to the next beat boundary |
| **Probability gate** | `if (rand() < math_pow(0.5, n)) { ... }` | Exponential probability decay (halves each step) |
| **dB to amplitude** | `math_pow(10, db / 20)` | Convert decibels to linear amplitude |
| **Amplitude to dB** | `20 * math_log10(amp)` | Convert linear amplitude to decibels |
| **Semitones to ratio** | `math_pow(2, semitones / 12)` | Pitch ratio from semitone interval |
| **Sigmoid curve** | `1 / (1 + math_exp(-x))` | S-curve for smooth transitions and envelopes |

```c
// Example: generative melody with math functions
bpm(120);
seed("melody");

fn main() {
    thread melody {
        let scale = [0, 2, 4, 5, 7, 9, 11]; // major scale degrees
        let t = 0;
        loop {
            // Pick a scale degree, convert to frequency
            let degree = array_rand(scale);
            let octave = math_floor(rand(0, 3));      // 0, 1, or 2
            let semitone = 60 + degree + octave * 12;
            let freq = 440 * math_pow(2, (semitone - 69) / 12);

            // Velocity with sine-wave contour over time
            let vel_norm = math_lerp(0.3, 0.9, (math_sin(t * 0.1) + 1) / 2);
            let vel = math_round(math_map(vel_norm, 0, 1, 40, 120));

            synth("default", freq: freq, amp: vel / 127);
            let dur = array_rand([0.25, 0.5, 0.5, 1]);
            wait(dur);
            t += dur;
        }
    }
}
```

### Strings

| Function | Description | Returns |
|----------|-------------|---------|
| `str_explode(delimiter, string)` | Split string by delimiter | array of strings |
| `str_join(glue, array)` | Join array elements with glue string | string |
| `str_replace(needle, replacement, haystack)` | Replace occurrences in string | string |
| `str_contains(needle, haystack)` | Check if string contains substring | bool |
| `str_upper(string)` | Convert to uppercase | string |
| `str_lower(string)` | Convert to lowercase | string |
| `str_trim(string)` | Remove leading/trailing whitespace | string |
| `str_length(string)` | Character count (Unicode-safe) | number |
| `str_substr(string, start)` | Substring from start index to end | string |
| `str_substr(string, start, length)` | Substring with length limit | string |
| `str_starts_with(prefix, string)` | Check if string starts with prefix | bool |
| `str_ends_with(suffix, string)` | Check if string ends with suffix | bool |
| `count(string)` | Character count | number |

`str_explode`, `str_replace`, and `str_contains` support both plain string and regex patterns. Wrap the pattern in `/`, `{}`, or `%%` delimiters to use regex:

```c
// Plain string split
let parts = str_explode(",", "a,b,c");         // ["a", "b", "c"]
let words = str_explode(" ", "hello world");    // ["hello", "world"]

// Regex split — /pattern/, {pattern}, or %%pattern%%
let csv = str_explode("/[,;]+/", "a,,b;c");    // ["a", "b", "c"]
let ws  = str_explode("{\\s+}", "a  b\tc");     // ["a", "b", "c"]
let nums = str_explode("%%\\d+%%", "x1y2z");   // ["x", "y", "z"]
```

`str_join` converts non-string values via their display representation:

```c
str_join(", ", ["a", "b", "c"]);   // "a, b, c"
str_join("-", [1, 2, 3]);          // "1-2-3"
str_join("", ["h","e","l","l","o"]); // "hello"
```

```c
// Replace — literal and regex
str_replace("world", "Rust", "hello world");     // "hello Rust"
str_replace("/[0-9]+/", "X", "abc123def456");    // "abcXdefX"

// Contains — literal and regex
str_contains("lo", "hello");          // true
str_contains("/[0-9]+/", "abc123");   // true

// Case conversion
str_upper("hello");       // "HELLO"
str_lower("HELLO");       // "hello"

// Trim, length, substring
str_trim("  hello  ");    // "hello"
str_length("hello");      // 5
str_substr("hello world", 6);      // "world"
str_substr("hello world", 0, 5);   // "hello"

// Prefix/suffix checks
str_starts_with("hel", "hello");   // true
str_ends_with("llo", "hello");     // true
```

### Arrays

| Function | Description | Returns |
|----------|-------------|---------|
| `count(arr)` | Number of elements (also works on strings) | number |
| `push(arr, value)` | Append with next auto-index | new length (number) |
| `pop(arr)` | Remove and return last element | value or nil |
| `keys(arr)` | All keys as a new array | array |
| `has_key(arr, key)` | Check if key exists | bool |
| `remove(arr, key)` | Remove by key, return removed value | value or nil |

### File IO

| Function | Description | Returns |
|----------|-------------|---------|
| `file_read(path)` | Read entire file as text | string or false |
| `file_read(path, offset)` | Read from character offset to end | string or false |
| `file_read(path, offset, length)` | Read `length` characters from offset | string or false |
| `file_write(path, data)` | Write data to file (overwrites) | bytes written (number) or false |
| `file_append(path, data)` | Append data to file (creates if missing) | bytes written (number) or false |
| `file_exists(path)` | Check if file exists | bool |
| `file_delete(path)` | Delete a file | bool |
| `file_size(path)` | Get file size in bytes | number or false |

Returns `false` on failure, which is falsy - convenient for error checks:

```c
let data = file_read("config.txt");
if (data) {
    print("loaded:", data);
} else {
    print("could not read file");
}

file_write("output.txt", "hello audion");
file_append("log.txt", "event happened\n");

if (file_exists("temp.txt")) {
    file_delete("temp.txt");
}
```

### Binary File IO

| Function | Description | Returns |
|----------|-------------|---------|
| `file_read_bytes(path)` | Read entire file as raw bytes | bytes or false |
| `file_read_bytes(path, offset)` | Read from byte offset to end | bytes or false |
| `file_read_bytes(path, offset, length)` | Read `length` bytes from offset | bytes or false |
| `file_write_bytes(path, bytes)` | Write bytes to file (overwrites) | bytes written (number) or false |
| `bytes_len(bytes)` | Get length of a byte buffer | number |
| `bytes_get(bytes, index)` | Get byte at index (0-255), supports negative indexing | number or nil |
| `bytes_slice(bytes, start, length?)` | Extract a portion of bytes | bytes |
| `bytes_to_array(bytes)` | Convert bytes to array of numbers (0-255) | array |
| `array_to_bytes(array)` | Convert array of numbers to bytes (clamped 0-255) | bytes |

The `bytes` type is a dedicated binary data type for working with raw file contents. Use it to read and parse binary formats like MIDI files, images, or any non-text data:

```c
// read a binary file
let data = file_read_bytes("sample.mid");
if (data) {
    print("read", bytes_len(data), "bytes");

    // inspect individual bytes
    let first = bytes_get(data, 0);
    let last  = bytes_get(data, -1);

    // slice a header
    let header = bytes_slice(data, 0, 4);

    // convert to array for processing
    let arr = bytes_to_array(header);
    print("first four bytes:", arr);
}

// create bytes from an array and write to disk
let raw = array_to_bytes([72, 101, 108, 108, 111]);
file_write_bytes("out.bin", raw);
```

### Directory IO

| Function | Description | Returns |
|----------|-------------|---------|
| `dir_scan(path)` | List filenames in a directory | array of strings, or false |
| `dir_exists(path)` | Check if directory exists | bool |
| `dir_create(path)` | Create directory (recursive) | bool |
| `dir_delete(path)` | Delete directory and contents | bool |

Returns `false` on failure (e.g. path doesn't exist for `dir_scan`). `dir_exists` returns `false` for regular files.

```c
// List files in a directory
let files = dir_scan("samples/");
if (files) {
    for (let i = 0; i < count(files); i += 1) {
        print(files[i]);
    }
} else {
    print("directory not found");
}

// Create nested directories
dir_create("output/renders/2024");

// Check and clean up
if (dir_exists("temp")) {
    dir_delete("temp");
}
```

### JSON

| Function | Description | Returns |
|----------|-------------|---------|
| `json_encode(value)` | Convert a value to a JSON string | string or false |
| `json_decode(string)` | Parse a JSON string into a value | value or false |

`json_encode()` converts Audion values to JSON:
- Sequential integer-keyed arrays (`[1, 2, 3]`) become JSON arrays
- String-keyed arrays (`["name" => "audion"]`) become JSON objects
- Numbers, strings, bools map directly; `nil` becomes `null`
- Functions, objects, and namespaces become `null`

`json_decode()` parses JSON into Audion values:
- JSON objects become string-keyed arrays (accessible with `["key"]` or `.key`)
- JSON arrays become integer-keyed arrays (accessible with `[0]`, `[1]`, etc.)
- `null` becomes `nil`

Returns `false` on failure (invalid JSON, encoding error).

```c
// Encode an array to JSON
let data = ["name" => "audion", "bpm" => 120, "notes" => [60, 64, 67]];
let json = json_encode(data);
file_write("config.json", json);
// config.json: {"name":"audion","bpm":120,"notes":[60,64,67]}

// Decode JSON back into an array
let raw = file_read("config.json");
let config = json_decode(raw);
print(config["name"]);       // audion
print(config.bpm);           // 120
print(config["notes"][0]);   // 60

// Handle invalid JSON gracefully
let bad = json_decode("not json");
if (!bad) {
    print("parse failed");
}
```

### Networking - TCP

Handle-based TCP networking using `std::net`. All I/O is blocking - use `thread` for concurrent connections.

| Function | Description | Returns |
|----------|-------------|---------|
| `net_connect(host, port)` | Open a TCP connection | handle (number) or false |
| `net_listen(host, port)` | Bind a TCP server | handle (number) or false |
| `net_accept(handle)` | Accept incoming connection (blocks) | new handle (number) or false |
| `net_read(handle)` | Read available data (up to 8192 bytes) | string or false |
| `net_read(handle, max)` | Read up to `max` bytes | string or false |
| `net_write(handle, data)` | Send data | bytes written (number) or false |
| `net_close(handle)` | Close connection or listener | true |

Returns `false` on failure - same pattern as file I/O. `net_read()` returns an empty string `""` when the remote side closes the connection (falsy, so `if (data) {...}` catches it).

```c
// TCP client - connect and exchange data
let h = net_connect("example.com", 80);
if (h) {
    net_write(h, "GET / HTTP/1.0\r\nHost: example.com\r\n\r\n");
    let response = net_read(h);
    print(response);
    net_close(h);
}

// TCP server - accept connections in a thread
let server = net_listen("0.0.0.0", 9000);
thread accept_loop {
    loop {
        let client = net_accept(server);
        if (client) {
            let data = net_read(client);
            print("received:", data);
            net_write(client, "ok\n");
            net_close(client);
        }
    }
}
```

### Networking - HTTP

HTTP/HTTPS client via `net_http()`. Supports GET, POST, PUT, DELETE, PATCH, HEAD. HTTPS works out of the box.

| Function | Description | Returns |
|----------|-------------|---------|
| `net_http(method, url)` | HTTP request | response array or false |
| `net_http(method, url, body)` | With request body | response array or false |
| `net_http(method, url, body, headers)` | With body and custom headers | response array or false |

Returns an array on success: `["status" => 200, "body" => "...", "headers" => ["Content-Type" => "text/html"]]`. Returns `false` on connection error. HTTP error statuses (4xx, 5xx) still return the response array - only network failures return `false`.

Pass `nil` as body when you only need custom headers without a body.

```c
// Simple GET
let resp = net_http("GET", "https://httpbin.org/get");
if (resp) {
    print("status:", resp.status);
    print("body:", resp.body);
}

// POST JSON to an API
let payload = json_encode(["name" => "audion", "bpm" => 120]);
let resp = net_http("POST", "https://httpbin.org/post", payload,
    ["Content-Type" => "application/json"]);
if (resp) {
    let data = json_decode(resp.body);
    print(data);
}

// GET with custom headers, no body
let resp = net_http("GET", "https://api.example.com/data", nil,
    ["Authorization" => "Bearer my_token"]);

// Check for HTTP errors
let resp = net_http("GET", "https://example.com/missing");
if (resp && resp.status >= 400) {
    print("HTTP error:", resp.status);
}
```

### Networking - UDP

Handle-based UDP networking using `std::net::UdpSocket`. UDP is connectionless and non-blocking - messages may be lost or arrive out of order.

**What UDP is good for:**
- **Real-time audio/video streaming** - low latency is more important than reliability
- **Game networking** - fast position updates where dropped packets don't matter
- **Sensor data** - continuous streams where the latest value matters most
- **Broadcast/multicast** - send to multiple receivers simultaneously
- **OSC (Open Sound Control)** - music/media control protocols (see `osc_*` functions)
- **Discovery protocols** - finding devices on the network
- **High-frequency telemetry** - metrics, logging, monitoring where occasional loss is acceptable

**When NOT to use UDP:**
- File transfers or anything requiring guaranteed delivery (use TCP: `net_connect`)
- HTTP APIs or web services (use `net_http`)
- Chat messages or text where every message must arrive in order

| Function | Description | Returns |
|----------|-------------|---------|
| `net_udp_bind(port)` | Bind UDP socket to all interfaces on port | handle (number) or false |
| `net_udp_bind(host, port)` | Bind UDP socket to specific interface | handle (number) or false |
| `net_udp_send(handle, host, port, data)` | Send data to a specific address | bytes sent (number) or false |
| `net_udp_recv(handle)` | Receive data (non-blocking, up to 8192 bytes) | array or nil |
| `net_udp_recv(handle, max)` | Receive up to `max` bytes | array or nil |
| `net_close(handle)` | Close UDP socket | true |

**Notes:**
- `net_udp_bind()` creates a non-blocking socket
- `net_udp_recv()` returns `nil` when no data is available (non-blocking)
- On success, `net_udp_recv()` returns: `["data" => string, "host" => sender_ip, "port" => sender_port]`
- UDP is unreliable - packets may be dropped, duplicated, or reordered
- Use `net_close()` (same as TCP) to close UDP sockets

```c
// UDP sender - send a message and exit
let sock = net_udp_bind(0);  // bind to any available port
net_udp_send(sock, "127.0.0.1", 9000, "Hello UDP!");
net_close(sock);

// UDP receiver - listen for messages
let sock = net_udp_bind(9000);  // bind to specific port
loop {
    let msg = net_udp_recv(sock);
    if (msg) {
        print("from", msg.host, ":", msg.port, "->", msg.data);
    }
    wait_ms(10);  // avoid busy-waiting
}

// Echo server - receive and reply
let server = net_udp_bind("0.0.0.0", 8888);
thread echo_loop {
    loop {
        let msg = net_udp_recv(server);
        if (msg) {
            print("received:", msg.data);
            // Reply back to sender
            net_udp_send(server, msg.host, msg.port, "echo: " + msg.data);
        }
        wait_ms(5);
    }
}

// Send to multiple destinations (multicast pattern)
let sock = net_udp_bind(0);
let targets = [
    ["host" => "192.168.1.100", "port" => 9000],
    ["host" => "192.168.1.101", "port" => 9000],
    ["host" => "192.168.1.102", "port" => 9000]
];
for (let i = 0; i < count(targets); i += 1) {
    let t = targets[i];
    net_udp_send(sock, t.host, t.port, "sync");
}
```

### MIDI Output

Configure a MIDI output port at startup, then send notes, CC, program changes, and clock sync to external hardware or software.

| Function | Description | Returns |
|----------|-------------|---------|
| `midi_config()` | List available MIDI output ports (prints and returns array) | array of port name strings |
| `midi_config(name)` | Connect to port matching name (substring match) | bool |
| `midi_config(index)` | Connect to port by index number | bool |
| `midi_note(note, vel)` | Note on (vel 0 = note off). Default channel 1 | nil |
| `midi_note(note, vel, ch)` | Note on/off with explicit channel (1-16) | nil |
| `midi_cc(cc, val)` | Control change. Default channel 1 | nil |
| `midi_cc(cc, val, ch)` | Control change with channel (1-16) | nil |
| `midi_program(program)` | Program change. Default channel 1 | nil |
| `midi_program(program, ch)` | Program change with channel (1-16) | nil |
| `midi_out(b1, b2)` | Send raw 2-byte MIDI message | nil |
| `midi_out(b1, b2, b3)` | Send raw 3-byte MIDI message | nil |
| `midi_clock()` | Send single MIDI clock tick (0xF8) | nil |
| `midi_start()` | Send MIDI Start (0xFA) | nil |
| `midi_stop()` | Send MIDI Stop (0xFC) | nil |
| `midi_panic()` | All notes off on all 16 channels | nil |

Channels are **1-16** in Audion (human-friendly). If omitted, defaults to channel 1. Calling any `midi_*` function before `midi_config()` silently does nothing.

```c
// List ports and connect
midi_config();             // prints: 0: IAC Driver Bus 1
midi_config("IAC");        // connect by name
midi_config(0);            // or connect by index

// Send notes
midi_note(60, 100);        // C4 note on, velocity 100
wait(1);
midi_note(60, 0);          // note off (velocity 0)

// CC and program change
midi_cc(1, 64);            // mod wheel to 50%
midi_program(5);           // select program 5
midi_program(0, 10);       // program 0 on channel 10

// Raw message
midi_out(144, 60, 127);    // note on ch1 (0x90=144), C4, vel 127
```

#### MIDI Clock Sync

Send 24 PPQN (pulses per quarter note) to sync external gear with your BPM. The `wait(1/24)` call uses Audion's drift-compensated clock, keeping external gear tightly in sync.

```c
bpm(120);

fn main() {
    midi_config("IAC Driver");

    thread clock_out {
        midi_start();
        loop {
            midi_clock();
            wait(1/24);   // 24 ticks per beat
        }
    }

    // Your music threads here...
}
```

Ctrl+C automatically sends all-notes-off (`midi_panic()`) on shutdown.

### MIDI Input

Listen for incoming MIDI messages and respond with custom code. Perfect for building controllers, arpeggiators, MIDI effects, or syncing to external clock sources.

| Function | Description | Returns |
|----------|-------------|---------|
| `midi_listen(port, callback)` | Listen for MIDI input on port, call callback(event) for each message. **Blocks** until shutdown - wrap in a `thread` block | nil |
| `midi_bpm_sync(port, enable)` | Auto-sync BPM to incoming MIDI clock (24 PPQN). `true` = enable, `false` = disable and restore previous BPM | bool |

#### Event Structure

MIDI events are passed to your callback as arrays with the event type as the first element and named parameters:

```c
["note_on", "note" => 60, "vel" => 100, "channel" => 1]
["note_off", "note" => 60, "vel" => 64, "channel" => 1]
["cc", "num" => 74, "value" => 127, "channel" => 1]
["program", "num" => 5, "channel" => 1]
["clock"]        // MIDI clock tick (24 per beat)
["start"]        // MIDI Start message
["stop"]         // MIDI Stop message
["continue"]     // MIDI Continue message
```

Channels are **1-16** (human-friendly). Access event data using array keys: `event["note"]`, `event["vel"]`, `event["channel"]`, etc.

#### Basic MIDI Input

```c
thread midi_input {
    midi_listen(0, fn(event) {
        let type = event[0];

        if (type == "note_on") {
            let note = event["note"];
            let vel = event["vel"];
            print("Note:", note, "Velocity:", vel);
            synth("default", freq: mtof(note), amp: vel / 127.0);
        } else if (type == "cc") {
            print("CC", event["num"], "=", event["value"]);
        }
    });
}

print("MIDI listener running in background");
// Program continues...
```

**Important**: `midi_listen()` blocks until shutdown, so always wrap it in a `thread` block to keep your program responsive.

#### Arpeggiator Example

```c
let held_notes = [];

thread midi_input {
    midi_listen(0, fn(event) {
        if (event[0] == "note_on" && event["vel"] > 0) {
            push(held_notes, event["note"]);
        } else if (event[0] == "note_off") {
            // Remove note from array
            let i = 0;
            loop {
                if (i >= count(held_notes)) { break; }
                if (held_notes[i] == event["note"]) {
                    remove(held_notes, i);
                    break;
                }
                i += 1;
            }
        }
    });
}

thread arpeggiator {
    let index = 0;
    loop {
        if (count(held_notes) > 0) {
            synth("default", freq: mtof(held_notes[index]));
            index = (index + 1) % count(held_notes);
        }
        wait(0.25);  // 16th notes
    }
}
```

#### Clock Sync (Simple)

For musicians who just want to sync to incoming MIDI clock:

```c
bpm(120);  // fallback BPM
midi_bpm_sync(0, true);  // Enable sync to port 0

thread kick {
    loop {
        synth("default", freq: 60);
        wait(1);  // Automatically follows MIDI clock!
    }
}

// Later, to stop syncing:
// midi_bpm_sync(0, false);  // Reverts to BPM(120)
```

#### Clock Sync (Advanced)

For hackers who want full control over clock timing:

```c
let clock_count = 0;
let pulse_times = [];

thread midi_input {
    midi_listen(0, fn(event) {
        if (event[0] == "clock") {
            clock_count += 1;
            push(pulse_times, time());

            // Custom BPM calculation with smoothing
            if (count(pulse_times) > 24) {
                let elapsed = pulse_times[23] - pulse_times[0];
                let bpm = 60.0 / elapsed;
                bpm(bpm);
                pulse_times = [];  // Reset
            }

            // Weird human stuff: add swing, latency compensation, etc.
            if (clock_count % 2 == 0) {
                // Delay every other pulse for swing
                wait_ms(10);
            }
        } else if (event[0] == "start") {
            clock_count = 0;
            pulse_times = [];
            print("Clock started");
        }
    });
}
```

#### MIDI Controller Mapping

```c
let cutoff = 1000;
let resonance = 0.5;
let synth_id = 0;

thread midi_input {
    midi_listen(0, fn(event) {
        if (event[0] == "note_on") {
            synth_id = synth("mysynth",
                freq: mtof(event["note"]),
                cutoff: cutoff,
                res: resonance
            );
        } else if (event[0] == "cc") {
            if (event["num"] == 74) {
                // Mod wheel controls cutoff
                cutoff = event["value"] * 40;  // 0-5080 Hz
                if (synth_id > 0) {
                    set(synth_id, "cutoff", cutoff);
                }
            } else if (event["num"] == 71) {
                // Filter resonance
                resonance = event["value"] / 127.0;
                if (synth_id > 0) {
                    set(synth_id, "res", resonance);
                }
            }
        }
    });
}
```

**Port Numbers**: Use `0` for the first MIDI input device, `1` for the second, etc. Port ordering depends on your system configuration.

### OSC Protocol

Send and receive arbitrary OSC (Open Sound Control) messages to/from any OSC-compatible software: Max/MSP, TouchDesigner, Processing, another audion instance, etc.

| Function | Description | Returns |
|----------|-------------|---------|
| `osc_config()` | Show current target address | string or nil |
| `osc_config(addr)` | Set target address for sending (e.g. `"127.0.0.1:9000"`) | bool |
| `osc_send(path, ...)` | Send OSC message with any number of arguments | bool |
| `osc_listen(port)` | Start listening for incoming OSC on a UDP port | bool |
| `osc_recv()` | Non-blocking receive. Returns next message or nil | array or nil |
| `osc_close()` | Close the listener socket | nil |
| `osc_close("sender")` | Close the sender socket and clear target | nil |
| `osc_close("all")` | Close both listener and sender | nil |

Calling `osc_send()` before `osc_config()` returns `false`. `osc_recv()` returns `nil` if no message is available.

Arguments are automatically converted: numbers become OSC ints (if integer-valued) or floats, strings become OSC strings, booleans become OSC bools, nil becomes OSC nil.

Received messages are returned as arrays: `["/address", arg1, arg2, ...]`.

```c
// Configure target and send
osc_config("127.0.0.1:9000");
osc_send("/synth/freq", 440);
osc_send("/synth/note", 60, 100, 0.25);
osc_send("/label", "hello from audion");

// Listen for incoming messages
osc_listen(8000);

thread listener {
    loop {
        let msg = osc_recv();
        if (msg) {
            print("received:", msg[0]);  // address
        }
        wait_ms(10);
    }
}
```

### Buffers

| Function | Description | Returns |
|----------|-------------|---------|
| `buffer_load(path)` | Allocate a buffer and read a sound file into it | buffer ID (number) |
| `buffer_free(buf_id)` | Free a buffer on scsynth | nil |
| `buffer_alloc(frames, channels?)` | Allocate an empty buffer (channels defaults to 1) | buffer ID (number) |
| `buffer_read(buf_id, path)` | Read a sound file into an existing buffer | nil |

`buffer_load()` is the simplest way to load a sample - it allocates and reads in one step. Pass the returned buffer ID to synths via the `bufnum` named argument, and swap samples live with `set()`.

Relative paths are resolved from the current working directory.

```c
// Load a sample and play it
let buf = buffer_load("samples/kick.wav");
let node = synth("sampler", bufnum: buf);

// Swap the sample on a running synth
let buf2 = buffer_load("samples/snare.wav");
set(node, bufnum: buf2);
buffer_free(buf);  // free the old buffer

// For advanced use (streaming, DiskIn): allocate empty, then read
let cache = buffer_alloc(65536, 2);  // 65536 frames, stereo
buffer_read(cache, "long_recording.wav");
```

#### Double-Buffering Pattern

Load samples in a background thread while another thread plays, then swap:

```c
let current_buf = buffer_load("samples/start.wav");
let node = synth("sampler", bufnum: current_buf);

thread loader {
    let dir = "samples/";
    loop {
        let files = dir_scan(dir);
        let idx = rand(0, count(files));
        let new_buf = buffer_load(dir + files[idx]);
        wait(0.25);  // give scsynth time to load
        let old = current_buf;
        current_buf = new_buf;
        set(node, bufnum: current_buf);
        buffer_free(old);
        wait(4);  // swap every 4 beats
    }
}
```

### Streaming Large Sound Files

For sound files too large to load entirely into memory (long ambient recordings, field recordings, backing tracks), use **disk streaming**. Instead of loading the whole file, scsynth reads it from disk in small chunks via a cache buffer.

#### Streaming Functions

| Function | Description | Returns |
|----------|-------------|---------|
| `buffer_stream_open(path)` | Allocate a 65536-frame cache buffer, cue the file for streaming | buffer ID (number) |
| `buffer_stream_close(buf_id)` | Close the file handle and free the cache buffer | nil |

`buffer_stream_open()` detects the channel count from the file header automatically. The cache buffer is 65536 frames (~1.5 seconds at 44.1kHz), which must be a power of 2 for DiskIn.

```c
// Open a long file for streaming
let buf = buffer_stream_open("field_recording.wav");

// ... use buf with a streaming SynthDef (see stream_disk UGen below) ...

// When done, close the file handle and free the buffer
buffer_stream_close(buf);
```

#### Streaming UGens for `define` Blocks

| UGen | Description |
|------|-------------|
| `stream_disk(bufnum)` | Stream audio from disk (DiskIn) |
| `stream_disk_variable_rate(bufnum, rate)` | Stream with variable playback rate (VDiskIn) |

Both accept optional named arguments:

| Property | Default | Description |
|----------|---------|-------------|
| `channels` | 2 | Number of audio channels (must match the file) |
| `loop` | 0 | Loop flag (0=stop at end, 1=loop) |

```c
// Simple stereo disk streamer
define streamer(bufnum, amp, gate, out) {
    out(out, stream_disk(bufnum) * env(gate, 0.5, 1, 2) * amp);
}

// Mono streamer
define mono_streamer(bufnum, amp, gate, out) {
    out(out, pan(0, stream_disk(bufnum, channels: 1)) * env(gate, 0.5, 1, 2) * amp);
}

// Looping streamer
define loop_streamer(bufnum, amp, gate, out) {
    out(out, stream_disk(bufnum, loop: 1) * env(gate, 0.5, 1, 2) * amp);
}

// Variable-rate streamer (for pitch shifting / time stretching)
define varspeed(bufnum, rate, amp, gate, out) {
    out(out, stream_disk_variable_rate(bufnum, rate) * env(gate, 0.5, 1, 2) * amp);
}
```

#### Complete Streaming Example

```c
// Play a long ambient recording with reverb, forever

define ambient(bufnum, amp, gate, out) {
    out(out, reverb(
        stream_disk(bufnum, loop: 1) * env(gate, 2, 1, 4) * amp,
        0.6, 0.9, 0.4
    ));
}

fn main() {
    let buf = buffer_stream_open("nature_sounds_2hr.wav");
    let node = synth("ambient", bufnum: buf, amp: 0.3);

    // Let it play forever - Ctrl+C to stop
    loop {
        wait(60);
    }

    // Cleanup (reached on shutdown)
    free(node);
    buffer_stream_close(buf);
}
```

#### Streaming vs Loading

| | `buffer_load()` | `buffer_stream_open()` |
|---|---|---|
| **Memory** | Entire file in RAM | 65536-frame cache (~1.5s) |
| **Best for** | Short samples, drums, one-shots | Long recordings, ambience, backing tracks |
| **Seeking** | Random access via `sample()` | Sequential playback only |
| **UGen** | `sample()` in `define` | `stream_disk()` in `define` |

> **Note:** Disk streaming is sequential - no scrubbing or seeking. It's designed for long textures and backing tracks that play straight through. For samples you need to pitch-shift, loop precisely, or trigger from arbitrary positions, use `buffer_load()` + `sample()` instead.

---

## Comments

```c
// Line comment - everything until end of line

/* Block comment
   spans multiple lines */
```

---

## String Escapes

| Escape | Character |
|--------|-----------|
| `\n` | Newline |
| `\t` | Tab |
| `\\` | Backslash |
| `\"` | Double quote |

---

## Semicolons

**Required after:**
- Expression statements: `x + 1;`
- Variable declarations: `let x = 5;`
- Return / break / continue: `return 42;`
- Include statements: `include "file.au";`

**Not required after:**
- Function declarations: `fn f() { }`
- Thread blocks: `thread t { }`
- Control structures: `if (...) { }`, `while (...) { }`, `loop { }`

---

## Program Execution

### File Mode

```bash
audion run file.au
audion run file.au --server 127.0.0.1:57110 --bpm 140
```

1. All top-level statements execute in order
2. If a `main()` function is defined, it is called automatically
3. The program waits for all spawned threads to finish
4. Ctrl+C cleanly shuts down (frees all synth nodes)

### REPL Mode

```bash
audion
```

```
audion v0.1.0 - type 'exit' or Ctrl+D to quit
connected to scsynth at 127.0.0.1:57110
>> synth("default", freq: 440);
1000
>> free(1000);
>> bpm(90);
120
>>
```

- State persists across lines
- Multi-line input: unbalanced `{` continues on the next line with `.. ` prompt
- Spawned threads keep running in the background
- Type `exit`, `quit`, or Ctrl+D to quit

---

## CLI Reference

```
audion                                    # Start REPL
audion run <file.au>                      # Run a file
audion run <file.au> --server <host:port> # Custom scsynth address
audion run <file.au> --bpm <number>       # Set initial BPM
audion --help                             # Show help
audion --version                          # Show version
```

---

## Custom SynthDefs with `define`

Create your own synths directly in audion using the `define` keyword. Audion generates SuperCollider code and compiles it via `sclang`.

### Syntax

```c
define name(params...) {
    expression;
}
```

The body is a single expression using UGen functions in nesting/composition style:

```c
define bass(freq, amp, pan, gate, out) {
    out(out, pan(pan, lpf(saw(freq), freq * 4) * env(gate) * amp));
}

// Now use it like any built-in synth:
synth("bass", freq: 110, amp: 0.5);
```

### Available UGens

#### Oscillators

| UGen | Description |
|------|-------------|
| `sine(freq)` | Sine wave |
| `saw(freq)` | Sawtooth wave |
| `square(freq)` / `pulse(freq, width)` | Square/pulse wave (width default 0.5) |
| `tri(freq)` | Triangle wave |
| `noise()` | White noise |
| `pink()` | Pink noise |
| `brown()` | Brown noise |

#### Sample Playback

| UGen | Description |
|------|-------------|
| `sample("file.wav")` | Play a WAV/AIFF/FLAC sample |
| `sample("file.wav", root: 60, ...)` | Play with named properties |

The `sample()` UGen loads a sound file into a SuperCollider buffer and plays it back using `PlayBuf` or `BufRd` (for precise loop points). It works like any other signal source - wrap it in filters, effects, and envelopes to shape the sound.

**Named properties:**

| Property | Default | Description |
|----------|---------|-------------|
| `root` | 60 | MIDI note the sample was recorded at |
| `vel_lo` | 0 | Velocity range low (0-127) |
| `vel_hi` | 127 | Velocity range high (0-127) |
| `key_lo` | 0 | Key range low (MIDI note) |
| `key_hi` | 127 | Key range high (MIDI note) |
| `loop` | 0 | Loop flag (0=off, 1=on) |
| `loop_start` | 0 | Loop start point in frames |
| `loop_end` | 0 | Loop end point in frames (0 = end of file) |
| `detune` | 0 | Detune in cents |
| `start` | 0 | Playback start position in frames |

Pitch is automatically calculated from `freq` relative to the sample's `root` note. A `vel` parameter is auto-added to the SynthDef when velocity ranges are used.

**Multi-layer instruments** - stack multiple `sample()` calls with `+` to create layered sounds. Each layer can have its own effects, envelope, and velocity/key mapping:

```c
define piano(freq, amp, gate, vel) {
    out(0, pan(0,
        // Soft layer: velocities 0-80
        lpf(sample("piano_soft.wav", root: 60, vel_lo: 0, vel_hi: 80), freq * 4)
            * env(gate, 0.1, 1, 0.5) * amp
        +
        // Hard layer: velocities 81-127
        sample("piano_hard.wav", root: 60, vel_lo: 81, vel_hi: 127)
            * env(gate, 0.01, 1, 0.3) * amp
    ));
}

synth("piano", freq: 440, amp: 0.8, vel: 100);
```

**Looping samples:**

```c
// Loop the entire file
define pad(freq, amp, gate) {
    out(0, sample("texture.wav", root: 48, loop: 1) * env(gate, 0.5, 1, 2) * amp);
}

// Loop between specific frame positions
define string(freq, amp, gate) {
    out(0, sample("violin.wav", root: 60, loop: 1, loop_start: 44100, loop_end: 132300)
        * env(gate, 0.1, 1, 0.8) * amp);
}
```

#### Filters

| UGen | Description |
|------|-------------|
| `lpf(sig, cutoff)` | Low-pass filter |
| `hpf(sig, cutoff)` | High-pass filter |
| `bpf(sig, freq, rq)` | Band-pass filter |
| `rlpf(sig, cutoff, rq)` | Resonant low-pass filter |
| `rhpf(sig, cutoff, rq)` | Resonant high-pass filter |

#### Envelope

| UGen | Description |
|------|-------------|
| `env(gate)` | ASR envelope (defaults: atk=0.01, sus=1, rel=0.3) |
| `env(gate, atk, sus, rel)` | ASR envelope with custom times. Use sus=0 for percussive envelopes |

#### LFOs (control rate)

| UGen | Description |
|------|-------------|
| `lfo_sine(freq)` | Sine LFO, bipolar -1 to +1 (SinOsc.kr) |
| `lfo_saw(freq)` | Sawtooth LFO (LFSaw.kr) |
| `lfo_tri(freq)` | Triangle LFO (LFTri.kr) |
| `lfo_pulse(freq, width)` | Pulse/square LFO, 0 or 1 (LFPulse.kr) |
| `lfo_noise(freq)` | Smooth random LFO (LFNoise1.kr) |
| `lfo_step(freq)` | Stepped random LFO (LFNoise0.kr) |

LFOs run at control rate (`.kr`) - use them to modulate filter cutoffs, panning, mix levels, etc.

#### Effects

| UGen | Description |
|------|-------------|
| `reverb(sig, mix, room, damp)` | Reverb (FreeVerb) |
| `delay(sig, time, decay)` | Delay (CombL) |
| `dist(sig, amount)` | Distortion (clip) |

#### Output

| UGen | Description |
|------|-------------|
| `out(bus, sig)` | Write signal to output bus |
| `pan(pos, sig)` | Stereo panning (-1 left, +1 right) |

### Signal math

You can use `*`, `+`, `-`, `/` between signals:

```c
define pad(freq, amp, gate, out) {
    out(out, pan(0,
        (sine(freq) + saw(freq * 0.5)) * 0.5 * env(gate) * amp
    ));
}
```

### Local variables with `let`

Use `let` inside `define` blocks to store intermediate signals. This avoids duplicating expressions and lets you build complex signal graphs (reverbs, delays, parallel effects chains):

```c
define schroeder_verb(freq, amp, gate, decay) {
    let sig = saw(freq) * env(gate) * amp;
    let comb_mix = CombC(sig, 0.035, 0.035, decay)
                 + CombC(sig, 0.040, 0.040, decay)
                 + CombC(sig, 0.045, 0.045, decay)
                 + CombC(sig, 0.050, 0.050, decay);
    let verb = AllpassC(AllpassC(comb_mix, 0.02, 0.02, 0.1), 0.03, 0.03, 0.1);
    out(0, sig + (verb * 0.3));
}
```

Each `let` becomes a SuperCollider `var` declaration - the signal is computed once and reused everywhere it's referenced.

### Parameter defaults

Parameters are automatically assigned sensible defaults based on their name:

| Name | Default |
|------|---------|
| `freq` | 440 |
| `amp` | 0.1 |
| `pan` | 0 |
| `gate` | 1 |
| `out` | 0 |
| anything else | 0 |

### Examples

```c
// Simple sine with envelope
define simple_sine(freq, amp, gate, out) {
    out(out, sine(freq) * env(gate) * amp);
}

// Detuned saw pad with filter and reverb
define lush_pad(freq, amp, gate, out) {
    out(out, reverb(
        lpf(saw(freq) + saw(freq * 1.01), freq * 6) * env(gate, 0.5, 1, 2) * amp,
        0.5, 0.8, 0.3
    ));
}

// Percussive kick (sus=0 for percussive envelope)
define kick(freq, amp, gate, out) {
    out(out, sine(freq) * env(gate, 0.001, 0, 0.3) * amp);
}

// Distorted bass
define dirty_bass(freq, amp, gate, out) {
    out(out, dist(lpf(saw(freq), 800), 3) * env(gate) * amp);
}

// Custom reverb with LFO-modulated filter and parallel comb delays
define shimmer(freq, amp, gate) {
    let sig = lpf(saw(freq), 2000 + lfo_sine(0.3) * 1000) * env(gate, 0.1, 1, 3) * amp;
    let wet = CombC(sig, 0.05, 0.035, 4)
            + CombC(sig, 0.05, 0.042, 3.5)
            + CombC(sig, 0.05, 0.048, 3.8);
    let verb = AllpassC(AllpassC(wet * 0.925, 0.902, 0.917, 0.91), 0.02, 0.013, 0.1);
    out(0, pan(lfo_tri(0.1), sig + verb));
}
```

> **Note:** `define` requires `sclang` to be installed (comes with SuperCollider). Unknown UGen names are passed through to SuperCollider as-is, so you can use any SC UGen by its class name.

---

## Memory Model

### Ownership & Cleanup

Audion uses **reference counting** for memory management (Rust's `Arc` - atomic reference counting). There is no garbage collector. Memory is freed deterministically when the last reference to a value goes out of scope.

In practice, this means:
- Local variables are cleaned up when their block/function exits
- Closures keep their captured scope alive as long as the closure exists
- Arrays and objects are reference-counted internally, and deep-copied on assignment (see below)

There are no manual memory operations - no `malloc`, `free`, `new`, or `delete`. You don't need to think about memory.

### Value Semantics

| Type | Assignment behavior |
|------|-------------------|
| number, string, bool, nil | **Copied** (cheap, immutable primitives) |
| array | **Deep copied** - independent copy, changes don't affect original |
| object | **Deep copied** - independent copy with remapped methods |
| function | **Shared** - closures share their captured environment via reference |

Deep copy on assignment (PHP-style) means you never get accidental aliasing bugs:

```c
let a = [1, 2, 3];
let b = a;          // b is a full independent copy
b[0] = 99;
print(a[0]);        // 1 - unchanged
```

### Scope Chain

Variables are resolved through a **scope chain**. Each block, function call, or thread creates a child scope that can see its parent:

```
global scope
  └─ function scope
       └─ if/while/for block scope
            └─ nested block scope
```

- `let x = 5;` creates a variable in the **current** scope
- `x = 5;` (without `let`) walks **up** the chain to find and update `x`, or creates it in the current scope if not found
- Inner scopes can read and modify parent variables (this is how closures work)

### Thread Memory

Each thread gets a **child scope** of the scope where it was spawned. Threads share access to the global environment (functions, top-level variables) through the scope chain, protected by mutexes.

```c
let shared = 0;

fn main() {
    thread worker {
        shared = 42;    // modifies parent scope
    }
}
```

### No Circular References

Because arrays and objects are deep-copied on assignment, circular references cannot occur in user code. This means reference counting is always sufficient - there is no need for a cycle-detecting garbage collector.

---

## Requirements

- **SuperCollider**: scsynth must be running separately
  ```bash
  scsynth -u 57110
  ```
- The `"default"` SynthDef is available out of the box in SuperCollider
- `sclang` is required for `define` blocks (included with SuperCollider)

---

## Complete Example

```c
// generative_rhythm.au - an ever-changing drum pattern

bpm(120);

fn play(freq, amp, dur) {
    let n = synth("default", freq: freq, amp: amp);
    wait(dur);
    free(n);
}

fn main() {
    print("starting generative rhythm at", bpm(), "BPM");

    thread kicks {
        loop {
            play(60, 0.9, 0.5);
            wait(0.5);
        }
    }

    thread hats {
        loop {
            let vel = rand(0.1, 0.4);
            play(8000, vel, 0.1);
            wait(0.25);
        }
    }

    thread melody {
        loop {
            let notes = rand(200, 800);
            let dur = rand(0.25, 1.0);
            play(notes, 0.3, dur);
        }
    }
}
```
