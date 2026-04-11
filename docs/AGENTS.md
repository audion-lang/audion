# Audion Language Spec (auto-generated)

<!-- regenerate: audion spec > docs/AGENTS.md -->

## Overview

Audion is a dynamically typed, C-syntax scripting language for generative music.
It targets SuperCollider via OSC. File extension: `.au`

## Types

number (f64), string (double-quoted), bool, nil, function (first-class, closures), array (ordered key-value), object (`return this;`), namespace (`include`)

Falsy: `nil`, `false`, `0`, `""`, `[]`, empty object. Everything else truthy.

## Keywords

`fn` `let` `if` `else` `while` `loop` `for` `return` `thread` `define` `break` `continue` `this` `include` `as` `using` `true` `false` `nil`

## Operators (by precedence, low→high)

1. `= += -= *= /=`
2. `||`
3. `&&`
4. `|`
5. `^`
6. `&`
7. `== !=`
8. `< > <= >=`
9. `<< >>`
10. `+ -`
11. `* / %`
12. `- ! ~` (unary)
13. calls, `[]`, `.`

## Syntax

```
let x = 5;                          // variable
x = 10;                             // assign (creates if not found)
let a = [1, 2, "k" => 3];           // array (PHP-style ordered kv)
fn add(a, b) { return a + b; }      // function decl
let f = fn(x) { return x * 2; };    // anonymous fn / closure
synth("name", freq: 440, amp: 0.5); // named args
if (c) { } else { }                 // control flow
while (c) { }                       // while loop
loop { break; }                     // infinite loop
for (let i=0; i<10; i+=1) { }       // C-style for
thread drums { loop { wait(1); } }  // named thread
include "lib.au";                    // include file
include "lib.au" as utils;           // aliased include
using lib;                           // import namespace
// line comment  /* block comment */
```

## Objects

```
fn make_obj() {
    let val = 0;
    let get = fn() { return val; };
    return this;    // captures scope as object
}
let o = make_obj();
o.val = 42;         // field access via dot
o.get();            // method call
```

## SynthDef (define blocks)

```
define bass(freq, amp, gate, out) {
    out(out, lpf(saw(freq), freq * 4) * env(gate) * amp);
}
synth("bass", freq: 110, amp: 0.5);
```

## Builtins

**core**: `print` `bpm` `wait` `wait_ms` `synth` `free` `set` `rand` `seed` `time` `count` `push` `pop` `keys` `has_key` `remove` `mtof` `ftom` `int` `float` `bool` `exec` `hash` `eval`

**sequence**: `array_seq_binary_to_intervals` `array_seq_intervals_to_binary` `array_seq_random_correlated` `array_seq_euclidean` `array_seq_permutations` `array_seq_debruijn` `array_seq_compositions` `array_seq_partitions` `array_seq_partitions_allowed` `array_seq_partitions_m_parts` `array_seq_partitions_allowed_m_parts` `array_seq_necklaces` `array_seq_necklaces_allowed` `array_seq_necklaces_m_ones` `array_seq_necklaces_allowed_m_ones` `array_seq_markov` `array_seq_compositions_allowed` `array_seq_compositions_m_parts` `array_seq_compositions_allowed_m_parts` `array_seq_composition_random` `array_seq_composition_random_m_parts` `array_seq_cf_convergent` `array_seq_cf_sqrt` `array_seq_christoffel` `array_seq_paper_folding`

**melody**: `array_mel_debruijn_k` `array_mel_lattice_walk_square` `array_mel_lattice_walk_tri` `array_mel_lattice_walk_square_no_retrace` `array_mel_lattice_walk_square_with_stops` `array_mel_string_to_indices` `array_mel_random_walk` `array_mel_invert` `array_mel_reverse` `array_mel_subset_sample` `array_mel_lattice_to_melody` `array_mel_automaton` `array_mel_probabilistic_automaton`

**array**: `array_rand` `array_push` `array_pop` `array_next` `array_prev` `array_current` `array_end` `array_beginning` `array_key`

**buffer**: `buffer_load` `buffer_free` `buffer_alloc` `buffer_read` `buffer_stream_open` `buffer_stream_close`

**console**: `console_read` `console_read_password` `console_read_key` `console_error`

**dir**: `dir_scan` `dir_exists` `dir_create` `dir_delete`

**file**: `file_read` `file_write` `file_append` `file_exists` `file_delete`

**json**: `json_encode` `json_decode`

**link**: `link_enable` `link_disable` `link_is_enabled` `link_peers` `link_beat` `link_phase` `link_quantum` `link_play` `link_stop` `link_is_playing` `link_request_beat`

**math**: `math_abs` `math_acos` `math_acosh` `math_asin` `math_asinh` `math_atan` `math_atan2` `math_atanh` `math_ceil` `math_cos` `math_cosh` `math_cbrt` `math_deg2rad` `math_exp` `math_expm1` `math_floor` `math_fmod` `math_fract` `math_hypot` `math_is_finite` `math_is_infinite` `math_is_nan` `math_intdiv` `math_log` `math_log10` `math_log2` `math_log1p` `math_lerp` `math_max` `math_min` `math_map` `math_pi` `math_e` `math_pow` `math_rad2deg` `math_round` `math_sin` `math_sinh` `math_sqrt` `math_sign` `math_tan` `math_tanh` `math_trunc` `math_clamp`

**midi**: `midi_config` `midi_note` `midi_cc` `midi_program` `midi_out` `midi_clock` `midi_start` `midi_stop` `midi_panic` `midi_listen` `midi_bpm_sync`

**net**: `net_connect` `net_listen` `net_accept` `net_read` `net_write` `net_close` `net_http` `net_udp_bind` `net_udp_send` `net_udp_recv`

**osc**: `osc_config` `osc_send` `osc_listen` `osc_recv` `osc_close`

**os**: `os_env_get` `os_env_set` `os_env_list` `os_process_id` `os_pid` `os_process_parent_id` `os_ppid` `os_current_working_directory` `os_cwd` `os_current_working_directory_change` `os_chdir` `os_arguments` `os_args` `os_exit` `os_name` `os_hostname` `os_username` `os_home`

**string**: `str_explode` `str_join` `str_replace` `str_contains` `str_upper` `str_lower` `str_trim` `str_length` `str_substr` `str_starts_with` `str_ends_with` `str`

**date**: `date` `timestamp` `timestamp_ms`

## UGens (inside define blocks)

`sine` `saw` `square` `pulse` `tri` `blip` `var_saw` `sync_saw` `fsin_osc` `lf_par` `lf_cub` `pm_osc` `noise` `white` `pink` `brown` `gray` `clip_noise` `crackle` `lpf` `hpf` `bpf` `rlpf` `rhpf` `resonz` `moog_ff` `brf` `formlet` `lag` `leak_dc` `ringz` `one_pole` `two_pole` `ramp` `mid_eq` `slew` `env` `line` `xline` `decay` `linen` `lfo_sine` `lfo_saw` `lfo_tri` `lfo_pulse` `lfo_noise` `lfo_step` `reverb` `gverb` `delay` `delay_c` `delay_n` `delay_l` `allpass_n` `allpass_l` `allpass_c` `comb_n` `comb_c` `coin_gate` `pluck` `tanh` `atan` `wrap` `fold` `softclip` `dist` `compander` `limiter` `amplitude` `normalizer` `pitch_shift` `freq_shift` `pitch` `vibrato` `running_sum` `median` `running_max` `running_min` `peak` `zero_crossing` `latch` `gate` `pulse_count` `t_exprand` `t_irand` `sweep` `in` `out` `pan` `pan4` `splay` `balance2` `local_in` `local_out` `PlayBuf` `buf_wr` `record_buf` `local_buf` `Dust` `Impulse` `TRand` `GrainBuf` `GrainSin` `GrainFM` `grains_t` `Clip` `Wrap` `array` `array_get` `sample`

## Default SynthDef Params

`freq`=440 `amp`=0.1 `pan`=0 `gate`=1 `out`=0 `density`=20 `rate`=1 `pos`=0.5 `spray`=0.1 `gdur`=0.1 `gdur_rand`=0 `pitch_rand`=0 `width`=1 `atk`=0.01 `sus`=1 `rel`=0.3 `filt`=20000 `filt_q`=1 `cutoff`=20000 `scan_speed`=0.1 `scan_depth`=0 `lfo_rate`=1 `lfo_depth`=0 `mix`=0.5 `rmix`=0.3 `rroom`=0.5 `rdamp`=0.5 `del_time`=0.2 `del_decay`=0.5 `ratio`=1 `index`=0 `fb`=0

## Execution

- `audion run file.au` — runs file, auto-calls `main()` if defined, waits for threads
- `audion run file.au --watch` — reloads on save, caches SynthDefs
- `audion` — REPL mode (no auto main)
- Ctrl+C frees all synth nodes cleanly
