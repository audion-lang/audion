// Copyright (C) 2025-2026 Aleksandr Bogdanov
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
//

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

// ---------------------------------------------------------------------------
// Assert counters — global across all threads, reset each run
// ---------------------------------------------------------------------------

static ASSERT_PASS: AtomicUsize = AtomicUsize::new(0);
static ASSERT_FAIL: AtomicUsize = AtomicUsize::new(0);

/// Reset assert counters (call before each file run in watch mode).
pub fn reset_assert_stats() {
    ASSERT_PASS.store(0, Ordering::Relaxed);
    ASSERT_FAIL.store(0, Ordering::Relaxed);
}

/// Print assert stats if any asserts ran. Returns true if there were failures.
pub fn print_assert_stats() -> bool {
    let pass = ASSERT_PASS.load(Ordering::Relaxed);
    let fail = ASSERT_FAIL.load(Ordering::Relaxed);
    if pass == 0 && fail == 0 {
        return false;
    }
    let total = pass + fail;
    if fail == 0 {
        eprintln!("assert: {} / {} passed", pass, total);
    } else {
        eprintln!("assert: {} passed, {} failed / {} total", pass, fail, total);
    }
    fail > 0
}

// ---------------------------------------------------------------------------
// Scala file cache — parsed .scl tunings keyed by absolute path
// ---------------------------------------------------------------------------

fn scala_cache() -> &'static Mutex<HashMap<String, Vec<f64>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<f64>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

use serde_json;
use digest::Digest;

// ---------------------------------------------------------------------------
// Thread-local seeded PRNG (xorshift64) for seed() / rand() / array_rand()
// ---------------------------------------------------------------------------

thread_local! {
    static SEEDED_RNG: RefCell<Option<u64>> = RefCell::new(None);
}

fn xorshift64(state: u64) -> u64 {
    let mut s = state;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    s
}

fn seeded_random_f64() -> Option<f64> {
    SEEDED_RNG.with(|cell| {
        let mut rng = cell.borrow_mut();
        if let Some(state) = *rng {
            let next = xorshift64(state);
            *rng = Some(next);
            Some((next as f64) / (u64::MAX as f64))
        } else {
            None
        }
    })
}

thread_local! {
    static UNSEEDED_RNG: RefCell<u64> = RefCell::new(0);
}

pub fn random_f64() -> f64 {
    if let Some(val) = seeded_random_f64() {
        val
    } else {
        UNSEEDED_RNG.with(|cell| {
            let mut state = *cell.borrow();
            if state == 0 {
                let dur = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap();
                state = dur.as_nanos() as u64;
                if state == 0 { state = 1; }
            }
            state = xorshift64(state);
            *cell.borrow_mut() = state;
            (state as f64) / (u64::MAX as f64)
        })
    }
}

fn hash_seed(input: &str) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in input.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    if h == 0 { 1 } else { h }
}

use crate::clock::Clock;
use crate::dmx::DmxClient;
use crate::error::{AudionError, Result};
use crate::midi::MidiClient;
use crate::osc::OscClient;
use crate::osc_protocol::OscProtocolClient;
use crate::value::{AudionArray, Value};

// ---------------------------------------------------------------------------
// Network handle store — TCP streams and listeners keyed by integer handle ID
// ---------------------------------------------------------------------------

enum NetHandle {
    Stream(TcpStream),
    Listener(TcpListener),
    UdpSocket(UdpSocket),
}

static NEXT_NET_HANDLE: AtomicU64 = AtomicU64::new(1);

fn net_handles() -> &'static Mutex<HashMap<u64, Arc<Mutex<NetHandle>>>> {
    static HANDLES: OnceLock<Mutex<HashMap<u64, Arc<Mutex<NetHandle>>>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn store_handle(handle: NetHandle) -> u64 {
    let id = NEXT_NET_HANDLE.fetch_add(1, Ordering::Relaxed);
    net_handles().lock().unwrap().insert(id, Arc::new(Mutex::new(handle)));
    id
}

fn get_handle(id: u64) -> Option<Arc<Mutex<NetHandle>>> {
    net_handles().lock().unwrap().get(&id).cloned()
}

fn remove_handle(id: u64) {
    net_handles().lock().unwrap().remove(&id);
}

// ---------------------------------------------------------------------------
// File stream handle store
// ---------------------------------------------------------------------------

enum FileHandle {
    Read(std::io::BufReader<std::fs::File>),
    Write(std::fs::File),
    Append(std::fs::File),
}

static NEXT_FILE_HANDLE: AtomicU64 = AtomicU64::new(1);

fn file_handles() -> &'static Mutex<HashMap<u64, Arc<Mutex<FileHandle>>>> {
    static HANDLES: OnceLock<Mutex<HashMap<u64, Arc<Mutex<FileHandle>>>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn store_file_handle(handle: FileHandle) -> u64 {
    let id = NEXT_FILE_HANDLE.fetch_add(1, Ordering::Relaxed);
    file_handles().lock().unwrap().insert(id, Arc::new(Mutex::new(handle)));
    id
}

fn get_file_handle(id: u64) -> Option<Arc<Mutex<FileHandle>>> {
    file_handles().lock().unwrap().get(&id).cloned()
}

fn remove_file_handle(id: u64) {
    file_handles().lock().unwrap().remove(&id);
}

/// Single source of truth for all builtin function names.
/// Used by Interpreter::new() for registration and available for introspection.
pub const BUILTIN_NAMES: &[&str] = &[
    "print", "bpm", "wait", "wait_ms",
    "synth", "free", "set",
    "rand", "seed", "array_rand", "time",
    "count", "push", "pop", "keys", "has_key", "remove",
    "file_read", "file_write", "file_append", "file_exists", "file_delete", "file_size",
    "file_open", "file_line", "file_read_chunk", "file_write_handle", "file_seek", "file_tell", "file_close",
    "dir_scan", "dir_exists", "dir_create", "dir_delete",
    "json_encode", "json_decode",
    "buffer_load", "buffer_free", "buffer_alloc", "buffer_read", "buffer_query",
    "buffer_stream_open", "buffer_stream_close",
    "record_start", "record_stop", "record_path",
    "net_connect", "net_listen", "net_accept",
    "net_read", "net_write", "net_close", "net_http",
    "net_udp_bind", "net_udp_send", "net_udp_recv",
    "mtof", "ftom",
    "midi_config", "midi_note", "midi_cc", "midi_program",
    "midi_out", "midi_clock", "midi_start", "midi_stop", "midi_panic",
    "midi_listen", "midi_bpm_sync",
    "midi_read", "midi_write",
    "osc_config", "osc_send", "osc_listen", "osc_recv", "osc_close",
    "array_push", "array_pop",
    "array_cycle", "array_rotate", "array_chunk",
    "array_next", "array_prev", "array_current",
    "array_end", "array_beginning", "array_key",
    "str_explode", "str_join",
    "str_replace", "str_contains",
    "str_upper", "str_lower", "str_trim", "str_length",
    "str_substr", "str_starts_with", "str_ends_with",
    "date", "timestamp", "timestamp_ms",
    "int", "float", "bool", "str",
    "hex", "bin", "oct",
    "exec",
    "hash",
    "math_abs", "math_acos", "math_acosh", "math_asin", "math_asinh",
    "math_atan", "math_atan2", "math_atanh",
    "math_ceil", "math_cos", "math_cosh", "math_cbrt",
    "math_deg2rad", "math_exp", "math_expm1",
    "math_floor", "math_fmod", "math_fract",
    "math_hypot", "math_is_finite", "math_is_infinite", "math_is_nan", "math_intdiv",
    "math_log", "math_log10", "math_log2", "math_log1p", "math_lerp",
    "math_max", "math_min", "math_map",
    "math_pi", "math_e", "math_pow",
    "math_rad2deg", "math_round",
    "math_sin", "math_sinh", "math_sqrt", "math_sign",
    "math_tan", "math_tanh", "math_trunc",
    "math_clamp", "math_wrap", "math_fold",
    "console_read", "console_read_password", "console_read_key", "console_error",
    "os_env_get", "os_env_set", "os_env_list",
    "os_process_id", "os_pid",
    "os_process_parent_id", "os_ppid",
    "os_current_working_directory", "os_cwd",
    "os_current_working_directory_change", "os_chdir",
    "os_arguments", "os_args",
    "os_exit",
    "os_name", "os_hostname", "os_username", "os_home",
    "array_seq_binary_to_intervals", "array_seq_intervals_to_binary",
    "array_seq_random_correlated",
    "array_seq_euclidean",
    "array_seq_permutations",
    "array_seq_debruijn",
    "array_seq_compositions",
    "array_seq_partitions", "array_seq_partitions_allowed",
    "array_seq_partitions_m_parts", "array_seq_partitions_allowed_m_parts",
    "array_seq_necklaces", "array_seq_necklaces_allowed",
    "array_seq_necklaces_m_ones", "array_seq_necklaces_allowed_m_ones",
    "array_seq_markov",
    "array_seq_compositions_allowed", "array_seq_compositions_m_parts",
    "array_seq_compositions_allowed_m_parts",
    "array_seq_composition_random", "array_seq_composition_random_m_parts",
    "array_seq_cf_convergent", "array_seq_cf_sqrt",
    "array_seq_christoffel", "array_seq_paper_folding",
    "array_mel_debruijn_k",
    "array_mel_lattice_walk_square", "array_mel_lattice_walk_tri",
    "array_mel_lattice_walk_square_no_retrace", "array_mel_lattice_walk_square_with_stops",
    "array_mel_string_to_indices",
    "array_mel_random_walk",
    "array_mel_invert", "array_mel_reverse",
    "array_mel_subset_sample",
    "array_mel_lattice_to_melody",
    "array_mel_automaton",
    "array_mel_probabilistic_automaton",
    "eval",
    "link_enable", "link_disable", "link_is_enabled", "link_peers",
    "link_beat", "link_phase",
    "link_quantum",
    "link_play", "link_stop", "link_is_playing",
    "link_request_beat",
    "sqlite_open", "sqlite_close", "sqlite_exec",
    "sqlite_query", "sqlite_tables", "sqlite_table_exists",
    "file_read_bytes", "file_write_bytes",
    "bytes_len", "bytes_get", "bytes_slice",
    "bytes_to_array", "array_to_bytes",
    "dmx_connect", "dmx_universe",
    "dmx_set", "dmx_set_range",
    "dmx_send", "dmx_blackout",
    "assert",
];

/// Resolve a potentially relative path against the source file's base directory.
/// Absolute paths are returned unchanged.
fn resolve_path(base: &std::path::Path, path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        path.to_string()
    } else {
        base.join(path).to_string_lossy().to_string()
    }
}

pub fn call_builtin(
    name: &str,
    args: &[Value],
    named_args: &[(String, Value)],
    osc: &Arc<OscClient>,
    midi: &Arc<MidiClient>,
    dmx: &Arc<DmxClient>,
    osc_protocol: &Arc<OscProtocolClient>,
    clock: &Arc<Clock>,
    env: &Arc<Mutex<crate::environment::Environment>>,
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
    base_path: &std::path::Path,
) -> Result<Value> {
    match name {
        "print" => builtin_print(args),
        "bpm" => builtin_bpm(args, clock),
        "wait" => builtin_wait(args, clock),
        "wait_ms" => builtin_wait_ms(args, clock),
        "synth" => builtin_synth(args, named_args, osc),
        "free" => builtin_free(args, osc),
        "set" => builtin_set(args, named_args, osc),
        "rand" => builtin_rand(args),
        "seed" => builtin_seed(args),
        "array_rand" => builtin_array_rand(args),
        "time" => builtin_time(clock),
        "count" => builtin_count(args),
        "push" => builtin_push(args),
        "pop" => builtin_pop(args),
        "keys" => builtin_keys(args),
        "has_key" => builtin_has_key(args),
        "remove" => builtin_remove(args),
        "file_read" => builtin_file_read(args),
        "file_write" => builtin_file_write(args),
        "file_append" => builtin_file_append(args),
        "file_exists" => builtin_file_exists(args),
        "file_delete" => builtin_file_delete(args),
        "file_size" => builtin_file_size(args),
        "file_open" => builtin_file_open(args),
        "file_line" => builtin_file_line(args),
        "file_read_chunk" => builtin_file_read_chunk(args),
        "file_write_handle" => builtin_file_write_handle(args),
        "file_seek" => builtin_file_seek(args),
        "file_tell" => builtin_file_tell(args),
        "file_close" => builtin_file_close(args),
        "dir_scan" => builtin_dir_scan(args),
        "dir_exists" => builtin_dir_exists(args),
        "dir_create" => builtin_dir_create(args),
        "dir_delete" => builtin_dir_delete(args),
        "json_encode" => builtin_json_encode(args),
        "json_decode" => builtin_json_decode(args),
        "buffer_load" => builtin_buffer_load(args, osc),
        "buffer_free" => builtin_buffer_free(args, osc),
        "buffer_alloc" => builtin_buffer_alloc(args, osc),
        "buffer_query" => builtin_buffer_query(args, osc),
        "buffer_read" => builtin_buffer_read(args, osc),
        "buffer_stream_open" => builtin_buffer_stream_open(args, osc),
        "buffer_stream_close" => builtin_buffer_stream_close(args, osc),
        "record_start" => builtin_record_start(args, osc, base_path),
        "record_stop" => builtin_record_stop(osc),
        "record_path" => builtin_record_path(osc),
        "net_connect" => builtin_net_connect(args),
        "net_listen" => builtin_net_listen(args),
        "net_accept" => builtin_net_accept(args),
        "net_read" => builtin_net_read(args),
        "net_write" => builtin_net_write(args),
        "net_close" => builtin_net_close(args),
        "net_http" => builtin_net_http(args),
        "net_udp_bind" => builtin_net_udp_bind(args),
        "net_udp_send" => builtin_net_udp_send(args),
        "net_udp_recv" => builtin_net_udp_recv(args),
        "mtof" => builtin_mtof(args, base_path),
        "ftom" => builtin_ftom(args, base_path),
        "midi_config" => builtin_midi_config(args, midi),
        "midi_note" => builtin_midi_note(args, midi),
        "midi_cc" => builtin_midi_cc(args, midi),
        "midi_program" => builtin_midi_program(args, midi),
        "midi_out" => builtin_midi_out(args, midi),
        "midi_clock" => builtin_midi_clock(midi),
        "midi_start" => builtin_midi_start(midi),
        "midi_stop" => builtin_midi_stop(midi),
        "midi_panic" => builtin_midi_panic(midi),
        "midi_listen" => builtin_midi_listen(args, midi, dmx, osc, osc_protocol, clock, env, shutdown, base_path),
        "midi_bpm_sync" => builtin_midi_bpm_sync(args, midi, clock, env, shutdown),
        "midi_read" => builtin_midi_read(args, base_path),
        "midi_write" => builtin_midi_write(args, base_path),
        "osc_config" => builtin_osc_config(args, osc_protocol),
        "osc_send" => builtin_osc_send(args, osc_protocol),
        "osc_listen" => builtin_osc_listen(args, osc_protocol),
        "osc_recv" => builtin_osc_recv(osc_protocol),
        "osc_close" => builtin_osc_close(args, osc_protocol),
        "array_push" => builtin_push(args),
        "array_pop" => builtin_pop(args),
        "array_cycle" | "array_rotate" => builtin_array_cycle(args),
        "array_chunk" => builtin_array_chunk(args),
        "array_next" => builtin_array_next(args),
        "array_prev" => builtin_array_prev(args),
        "array_current" => builtin_array_current(args),
        "array_end" => builtin_array_end(args),
        "array_beginning" => builtin_array_beginning(args),
        "array_key" => builtin_array_key(args),
        "str_explode" => builtin_str_explode(args),
        "str_join" => builtin_str_join(args),
        "str_replace" => crate::strings::builtin_str_replace(args),
        "str_contains" => crate::strings::builtin_str_contains(args),
        "str_upper" => crate::strings::builtin_str_upper(args),
        "str_lower" => crate::strings::builtin_str_lower(args),
        "str_trim" => crate::strings::builtin_str_trim(args),
        "str_length" => crate::strings::builtin_str_length(args),
        "str_substr" => crate::strings::builtin_str_substr(args),
        "str_starts_with" => crate::strings::builtin_str_starts_with(args),
        "str_ends_with" => crate::strings::builtin_str_ends_with(args),
        "date" => builtin_date(args),
        "timestamp" => builtin_timestamp(args),
        "timestamp_ms" => builtin_timestamp_ms(args),
        "int" => builtin_int(args),
        "float" => builtin_float(args),
        "bool" => builtin_bool(args),
        "str" => builtin_str(args),
        "hex" => builtin_hex(args),
        "bin" => builtin_bin(args),
        "oct" => builtin_oct(args),
        "exec" => builtin_exec(args),
        "hash" => builtin_hash(args),
        "math_abs" => crate::math::builtin_math_abs(args),
        "math_acos" => crate::math::builtin_math_acos(args),
        "math_acosh" => crate::math::builtin_math_acosh(args),
        "math_asin" => crate::math::builtin_math_asin(args),
        "math_asinh" => crate::math::builtin_math_asinh(args),
        "math_atan" => crate::math::builtin_math_atan(args),
        "math_atan2" => crate::math::builtin_math_atan2(args),
        "math_atanh" => crate::math::builtin_math_atanh(args),
        "math_ceil" => crate::math::builtin_math_ceil(args),
        "math_cos" => crate::math::builtin_math_cos(args),
        "math_cosh" => crate::math::builtin_math_cosh(args),
        "math_deg2rad" => crate::math::builtin_math_deg2rad(args),
        "math_exp" => crate::math::builtin_math_exp(args),
        "math_expm1" => crate::math::builtin_math_expm1(args),
        "math_floor" => crate::math::builtin_math_floor(args),
        "math_fmod" => crate::math::builtin_math_fmod(args),
        "math_hypot" => crate::math::builtin_math_hypot(args),
        "math_is_finite" => crate::math::builtin_math_is_finite(args),
        "math_is_infinite" => crate::math::builtin_math_is_infinite(args),
        "math_is_nan" => crate::math::builtin_math_is_nan(args),
        "math_log" => crate::math::builtin_math_log(args),
        "math_log10" => crate::math::builtin_math_log10(args),
        "math_log2" => crate::math::builtin_math_log2(args),
        "math_log1p" => crate::math::builtin_math_log1p(args),
        "math_max" => crate::math::builtin_math_max(args),
        "math_min" => crate::math::builtin_math_min(args),
        "math_pi" => crate::math::builtin_math_pi(args),
        "math_e" => crate::math::builtin_math_e(args),
        "math_pow" => crate::math::builtin_math_pow(args),
        "math_rad2deg" => crate::math::builtin_math_rad2deg(args),
        "math_round" => crate::math::builtin_math_round(args),
        "math_sin" => crate::math::builtin_math_sin(args),
        "math_sinh" => crate::math::builtin_math_sinh(args),
        "math_sqrt" => crate::math::builtin_math_sqrt(args),
        "math_tan" => crate::math::builtin_math_tan(args),
        "math_tanh" => crate::math::builtin_math_tanh(args),
        "math_sign" => crate::math::builtin_math_sign(args),
        "math_clamp" => crate::math::builtin_math_clamp(args),
        "math_wrap" => crate::math::builtin_math_wrap(args),
        "math_fold" => crate::math::builtin_math_fold(args),
        "math_intdiv" => crate::math::builtin_math_intdiv(args),
        "math_lerp" => crate::math::builtin_math_lerp(args),
        "math_map" => crate::math::builtin_math_map(args),
        "math_trunc" => crate::math::builtin_math_trunc(args),
        "math_fract" => crate::math::builtin_math_fract(args),
        "math_cbrt" => crate::math::builtin_math_cbrt(args),
        "console_read" => builtin_console_read(),
        "console_read_password" => builtin_console_read_password(args),
        "console_read_key" => builtin_console_read_key(),
        "console_error" => builtin_console_error(args),
        "os_env_get" => builtin_os_env_get(args),
        "os_env_set" => builtin_os_env_set(args),
        "os_env_list" => builtin_os_env_list(),
        "os_process_id" | "os_pid" => builtin_os_process_id(),
        "os_process_parent_id" | "os_ppid" => builtin_os_process_parent_id(),
        "os_current_working_directory" | "os_cwd" => builtin_os_cwd(),
        "os_current_working_directory_change" | "os_chdir" => builtin_os_chdir(args),
        "os_arguments" | "os_args" => builtin_os_arguments(env),
        "os_exit" => builtin_os_exit(args),
        "os_name" => builtin_os_name(),
        "os_hostname" => builtin_os_hostname(),
        "os_username" => builtin_os_username(),
        "os_home" => builtin_os_home(),
        "array_seq_binary_to_intervals" => crate::sequences::array_seq_binary_to_intervals(args),
        "array_seq_intervals_to_binary" => crate::sequences::array_seq_intervals_to_binary(args),
        "array_seq_random_correlated" => crate::sequences::array_seq_random_correlated(args),
        "array_seq_euclidean" => crate::sequences::array_seq_euclidean(args),
        "array_seq_permutations" => crate::sequences::array_seq_permutations(args),
        "array_seq_debruijn" => crate::sequences::array_seq_debruijn(args),
        "array_seq_compositions" => crate::sequences::array_seq_compositions(args),
        "array_seq_partitions" => crate::sequences::array_seq_partitions(args),
        "array_seq_partitions_allowed" => crate::sequences::array_seq_partitions_allowed(args),
        "array_seq_partitions_m_parts" => crate::sequences::array_seq_partitions_m_parts(args),
        "array_seq_partitions_allowed_m_parts" => crate::sequences::array_seq_partitions_allowed_m_parts(args),
        "array_seq_necklaces" => crate::sequences::array_seq_necklaces(args),
        "array_seq_necklaces_allowed" => crate::sequences::array_seq_necklaces_allowed(args),
        "array_seq_necklaces_m_ones" => crate::sequences::array_seq_necklaces_m_ones(args),
        "array_seq_necklaces_allowed_m_ones" => crate::sequences::array_seq_necklaces_allowed_m_ones(args),
        "array_seq_markov" => crate::sequences::array_seq_markov(args),
        "array_seq_compositions_allowed" => crate::sequences::array_seq_compositions_allowed(args),
        "array_seq_compositions_m_parts" => crate::sequences::array_seq_compositions_m_parts(args),
        "array_seq_compositions_allowed_m_parts" => crate::sequences::array_seq_compositions_allowed_m_parts(args),
        "array_seq_composition_random" => crate::sequences::array_seq_composition_random(args),
        "array_seq_composition_random_m_parts" => crate::sequences::array_seq_composition_random_m_parts(args),
        "array_seq_cf_convergent" => crate::sequences::array_seq_cf_convergent(args),
        "array_seq_cf_sqrt" => crate::sequences::array_seq_cf_sqrt(args),
        "array_seq_christoffel" => crate::sequences::array_seq_christoffel(args),
        "array_seq_paper_folding" => crate::sequences::array_seq_paper_folding(args),
        "array_mel_debruijn_k" => crate::melodies::array_mel_debruijn_k(args),
        "array_mel_lattice_walk_square" => crate::melodies::array_mel_lattice_walk_square(args),
        "array_mel_lattice_walk_tri" => crate::melodies::array_mel_lattice_walk_tri(args),
        "array_mel_lattice_walk_square_no_retrace" => crate::melodies::array_mel_lattice_walk_square_no_retrace(args),
        "array_mel_lattice_walk_square_with_stops" => crate::melodies::array_mel_lattice_walk_square_with_stops(args),
        "array_mel_string_to_indices" => crate::melodies::array_mel_string_to_indices(args),
        "array_mel_random_walk" => crate::melodies::array_mel_random_walk(args),
        "array_mel_invert" => crate::melodies::array_mel_invert(args),
        "array_mel_reverse" => crate::melodies::array_mel_reverse(args),
        "array_mel_subset_sample" => crate::melodies::array_mel_subset_sample(args),
        "array_mel_lattice_to_melody" => crate::melodies::array_mel_lattice_to_melody(args),
        "array_mel_automaton" => crate::melodies::array_mel_automaton(args),
        "array_mel_probabilistic_automaton" => crate::melodies::array_mel_probabilistic_automaton(args),
        "eval" => builtin_eval(args, env),
        "link_enable" => builtin_link_enable(args, clock),
        "link_disable" => builtin_link_disable(clock),
        "link_is_enabled" => builtin_link_is_enabled(clock),
        "link_peers" => builtin_link_peers(clock),
        "link_beat" => builtin_link_beat(clock),
        "link_phase" => builtin_link_phase(clock),
        "link_quantum" => builtin_link_quantum(args, clock),
        "link_play" => builtin_link_play(clock),
        "link_stop" => builtin_link_stop(clock),
        "link_is_playing" => builtin_link_is_playing(clock),
        "link_request_beat" => builtin_link_request_beat(args, clock),
        "sqlite_open" => crate::sqlite::builtin_sqlite_open(args),
        "sqlite_close" => crate::sqlite::builtin_sqlite_close(args),
        "sqlite_exec" => crate::sqlite::builtin_sqlite_exec(args),
        "sqlite_query" => crate::sqlite::builtin_sqlite_query(args),
        "sqlite_tables" => crate::sqlite::builtin_sqlite_tables(args),
        "sqlite_table_exists" => crate::sqlite::builtin_sqlite_table_exists(args),
        "file_read_bytes" => builtin_file_read_bytes(args),
        "file_write_bytes" => builtin_file_write_bytes(args),
        "bytes_len" => builtin_bytes_len(args),
        "bytes_get" => builtin_bytes_get(args),
        "bytes_slice" => builtin_bytes_slice(args),
        "bytes_to_array" => builtin_bytes_to_array(args),
        "array_to_bytes" => builtin_array_to_bytes(args),
        "dmx_connect" => builtin_dmx_connect(args, dmx),
        "dmx_universe" => builtin_dmx_universe(args, dmx),
        "dmx_set" => builtin_dmx_set(args, dmx),
        "dmx_set_range" => builtin_dmx_set_range(args, dmx),
        "dmx_send" => builtin_dmx_send(dmx),
        "dmx_blackout" => builtin_dmx_blackout(dmx),
        "assert" => builtin_assert(args),
        _ => Err(AudionError::RuntimeError {
            msg: format!("unknown builtin '{}'", name),
        }),
    }
}


fn builtin_print(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    println!("{}", parts.join(" "));
    Ok(Value::Nil)
}

fn builtin_bpm(args: &[Value], clock: &Arc<Clock>) -> Result<Value> {
    if args.is_empty() {
        return Ok(Value::Number(clock.get_bpm()));
    }
    let bpm = require_number("bpm", &args[0])?;
    let old = clock.get_bpm();
    clock.set_bpm(bpm);
    Ok(Value::Number(old))
}

fn builtin_wait(args: &[Value], clock: &Arc<Clock>) -> Result<Value> {
    let beats = require_number("wait", args.first().unwrap_or(&Value::Number(1.0)))?;
    clock.wait_beats(beats);
    Ok(Value::Nil)
}

fn builtin_wait_ms(args: &[Value], clock: &Arc<Clock>) -> Result<Value> {
    let ms = require_number("wait_ms", args.first().unwrap_or(&Value::Number(0.0)))?;
    clock.wait_ms(ms);
    Ok(Value::Nil)
}

fn builtin_synth(
    args: &[Value],
    named_args: &[(String, Value)],
    osc: &Arc<OscClient>,
) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "synth() requires a SynthDef name as first argument".to_string(),
        });
    }
    let def_name = match &args[0] {
        Value::String(s) => s.clone(),
        other => {
            return Err(AudionError::RuntimeError {
                msg: format!("synth() first argument must be a string, got {}", other.type_name()),
            })
        }
    };

    let node_id = osc.synth_new(&def_name, named_args);
    Ok(Value::Number(node_id as f64))
}

fn builtin_free(args: &[Value], osc: &Arc<OscClient>) -> Result<Value> {
    let node_id = require_number("free", args.first().unwrap_or(&Value::Nil))?;
    osc.node_free(node_id as i32);
    Ok(Value::Nil)
}

fn builtin_set(
    args: &[Value],
    named_args: &[(String, Value)],
    osc: &Arc<OscClient>,
) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "set() requires a node ID as first argument".to_string(),
        });
    }
    let node_id = require_number("set", &args[0])? as i32;
    osc.node_set(node_id, named_args);
    Ok(Value::Nil)
}

fn builtin_rand(args: &[Value]) -> Result<Value> {
    let min = if !args.is_empty() {
        require_number("rand", &args[0])?
    } else {
        0.0
    };
    let max = if args.len() > 1 {
        require_number("rand", &args[1])?
    } else {
        1.0
    };

    let t = random_f64();
    let val = min + t * (max - min);

    // If both args are integers, return an integer result
    let both_ints = args.len() >= 2
        && is_integer(min)
        && is_integer(max);
    if both_ints {
        Ok(Value::Number((val as i64) as f64))
    } else {
        Ok(Value::Number(val))
    }
}

fn builtin_seed(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "seed() requires an argument".to_string(),
        });
    }
    match &args[0] {
        Value::Bool(false) | Value::Nil => {
            SEEDED_RNG.with(|cell| *cell.borrow_mut() = None);
            Ok(Value::Nil)
        }
        Value::Number(n) => {
            let s = if *n == 0.0 { 1u64 } else { (*n as u64) | 1 };
            SEEDED_RNG.with(|cell| *cell.borrow_mut() = Some(s));
            Ok(Value::Nil)
        }
        Value::String(s) => {
            SEEDED_RNG.with(|cell| *cell.borrow_mut() = Some(hash_seed(s)));
            Ok(Value::Nil)
        }
        other => Err(AudionError::RuntimeError {
            msg: format!("seed() expected number, string, or false, got {}", other.type_name()),
        }),
    }
}

fn builtin_array_rand(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_rand() requires an array argument".to_string(),
        });
    }
    let arr = require_array("array_rand", &args[0])?;
    let guard = arr.lock().unwrap();
    let entries = guard.entries();
    if entries.is_empty() {
        return Ok(Value::Nil);
    }
    let t = random_f64();
    let idx = (t * entries.len() as f64) as usize;
    let idx = idx.min(entries.len() - 1);
    Ok(entries[idx].1.clone())
}

fn builtin_time(clock: &Arc<Clock>) -> Result<Value> {
    Ok(Value::Number(clock.elapsed_secs()))
}

fn require_array(fn_name: &str, val: &Value) -> Result<Arc<Mutex<AudionArray>>> {
    match val {
        Value::Array(arr) => Ok(arr.clone()),
        other => Err(AudionError::RuntimeError {
            msg: format!("{}() expected array, got {}", fn_name, other.type_name()),
        }),
    }
}

fn builtin_count(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "count() requires an argument".to_string(),
        });
    }
    match &args[0] {
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            Ok(Value::Number(guard.len() as f64))
        }
        Value::String(s) => Ok(Value::Number(s.len() as f64)),
        other => Err(AudionError::RuntimeError {
            msg: format!("count() expected array or string, got {}", other.type_name()),
        }),
    }
}

fn builtin_push(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "push() requires 2 arguments: push(array, value)".to_string(),
        });
    }
    let arr = require_array("push", &args[0])?;
    let val = args[1].clone();
    let mut guard = arr.lock().unwrap();
    let new_len = guard.push_auto(val);
    Ok(Value::Number(new_len as f64))
}

fn builtin_pop(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "pop() requires an argument".to_string(),
        });
    }
    let arr = require_array("pop", &args[0])?;
    let mut guard = arr.lock().unwrap();
    match guard.pop() {
        Some((_, v)) => Ok(v),
        None => Ok(Value::Nil),
    }
}

fn builtin_array_cycle(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_cycle(array, n) requires at least 1 argument".to_string(),
        });
    }
    let arr = require_array("array_cycle", &args[0])?;
    let n: i64 = if args.len() >= 2 {
        require_number("array_cycle", &args[1])? as i64
    } else {
        1
    };

    let guard = arr.lock().unwrap();
    let entries = guard.entries();

    if entries.is_empty() {
        drop(guard);
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let len = entries.len();
    let keys: Vec<Value> = entries.iter().map(|(k, _)| k.clone()).collect();
    let mut values: Vec<Value> = entries.iter().map(|(_, v)| v.clone()).collect();
    drop(guard);

    // normalise n into [0, len) so rotate never panics
    let n = ((n % len as i64) + len as i64) as usize % len;
    // positive n → each value shifts one slot forward (rotate_right)
    // negative n was flipped to a positive rotate_left equivalent above
    values.rotate_right(n);

    let mut result = AudionArray::new();
    for (key, val) in keys.into_iter().zip(values.into_iter()) {
        result.set(key, val);
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

fn builtin_array_chunk(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_chunk(array, size) requires 2 arguments".to_string(),
        });
    }
    let arr = require_array("array_chunk", &args[0])?;
    let size = require_number("array_chunk", &args[1])? as usize;
    if size == 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_chunk: size must be greater than 0".to_string(),
        });
    }

    let guard = arr.lock().unwrap();
    let values: Vec<Value> = guard.entries().iter().map(|(_, v)| v.clone()).collect();
    drop(guard);

    let mut result = AudionArray::new();
    for (i, chunk) in values.chunks(size).enumerate() {
        let mut chunk_arr = AudionArray::new();
        for (j, val) in chunk.iter().enumerate() {
            chunk_arr.set(Value::Number(j as f64), val.clone());
        }
        result.set(Value::Number(i as f64), Value::Array(Arc::new(Mutex::new(chunk_arr))));
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

fn builtin_keys(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "keys() requires an argument".to_string(),
        });
    }
    let arr = require_array("keys", &args[0])?;
    let guard = arr.lock().unwrap();
    let mut result = AudionArray::new();
    for (k, _) in guard.entries().iter() {
        result.push_auto(k.clone());
    }
    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

fn builtin_has_key(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "has_key() requires 2 arguments: has_key(array, key)".to_string(),
        });
    }
    let arr = require_array("has_key", &args[0])?;
    let guard = arr.lock().unwrap();
    Ok(Value::Bool(guard.get(&args[1]).is_some()))
}

fn builtin_remove(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "remove() requires 2 arguments: remove(array, key)".to_string(),
        });
    }
    let arr = require_array("remove", &args[0])?;
    let mut guard = arr.lock().unwrap();
    match guard.remove(&args[1]) {
        Some(v) => Ok(v),
        None => Ok(Value::Nil),
    }
}

fn require_string(fn_name: &str, val: &Value) -> Result<String> {
    match val {
        Value::String(s) => Ok(s.clone()),
        other => Err(AudionError::RuntimeError {
            msg: format!("{}() expected string, got {}", fn_name, other.type_name()),
        }),
    }
}

// file_read("path") → string or false
// file_read("path", offset, length) → partial string or false
fn builtin_file_read(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "file_read() requires a file path as first argument".to_string(),
        });
    }
    let path = require_string("file_read", &args[0])?;

    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Ok(Value::Bool(false)),
    };

    // Optional offset and length for partial reads
    if args.len() >= 3 {
        let offset = require_number("file_read", &args[1])? as usize;
        let length = require_number("file_read", &args[2])? as usize;
        let slice: String = contents.chars().skip(offset).take(length).collect();
        return Ok(Value::String(slice));
    } else if args.len() >= 2 {
        let offset = require_number("file_read", &args[1])? as usize;
        let slice: String = contents.chars().skip(offset).collect();
        return Ok(Value::String(slice));
    }

    Ok(Value::String(contents))
}

// file_write("path", data) → bytes written or false
fn builtin_file_write(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "file_write() requires 2 arguments: file_write(path, data)".to_string(),
        });
    }
    let path = require_string("file_write", &args[0])?;
    let data = args[1].to_string();

    match std::fs::write(&path, &data) {
        Ok(()) => Ok(Value::Number(data.len() as f64)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// file_append("path", data) → bytes written or false
fn builtin_file_append(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "file_append() requires 2 arguments: file_append(path, data)".to_string(),
        });
    }
    let path = require_string("file_append", &args[0])?;
    let data = args[1].to_string();

    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => match file.write_all(data.as_bytes()) {
            Ok(()) => Ok(Value::Number(data.len() as f64)),
            Err(_) => Ok(Value::Bool(false)),
        },
        Err(_) => Ok(Value::Bool(false)),
    }
}

// file_exists("path") → bool
fn builtin_file_exists(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "file_exists() requires a file path as first argument".to_string(),
        });
    }
    let path = require_string("file_exists", &args[0])?;
    Ok(Value::Bool(std::path::Path::new(&path).exists()))
}

// file_delete("path") → bool
fn builtin_file_delete(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "file_delete() requires a file path as first argument".to_string(),
        });
    }
    let path = require_string("file_delete", &args[0])?;
    Ok(Value::Bool(std::fs::remove_file(&path).is_ok()))
}

// file_size("path") → number (bytes) or false
fn builtin_file_size(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "file_size() requires a file path as first argument".to_string(),
        });
    }
    let path = require_string("file_size", &args[0])?;
    match std::fs::metadata(&path) {
        Ok(meta) => Ok(Value::Number(meta.len() as f64)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// ---------------------------------------------------------------------------
// Handle-based streaming file I/O
// ---------------------------------------------------------------------------

// file_open(path, mode) → handle (number) or false
// mode: "r" (read), "w" (write/truncate), "a" (append)
// Returns a numeric handle for use with file_line, file_read_chunk, file_write_handle,
// file_seek, file_tell, file_close.
fn builtin_file_open(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "file_open() requires 2 arguments: file_open(path, mode)".to_string(),
        });
    }
    let path = require_string("file_open", &args[0])?;
    let mode = require_string("file_open", &args[1])?;
    match mode.as_str() {
        "r" => match std::fs::File::open(&path) {
            Ok(f) => {
                let id = store_file_handle(FileHandle::Read(std::io::BufReader::new(f)));
                Ok(Value::Number(id as f64))
            }
            Err(_) => Ok(Value::Bool(false)),
        },
        "w" => match std::fs::File::create(&path) {
            Ok(f) => {
                let id = store_file_handle(FileHandle::Write(f));
                Ok(Value::Number(id as f64))
            }
            Err(_) => Ok(Value::Bool(false)),
        },
        "a" => match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            Ok(f) => {
                let id = store_file_handle(FileHandle::Append(f));
                Ok(Value::Number(id as f64))
            }
            Err(_) => Ok(Value::Bool(false)),
        },
        other => Err(AudionError::RuntimeError {
            msg: format!("file_open() unknown mode {:?}: use \"r\", \"w\", or \"a\"", other),
        }),
    }
}

// file_line(handle) → Bytes (raw bytes up to and including \n) or false (EOF or error)
// Binary safe: uses read_until so no UTF-8 assumption.
fn builtin_file_line(args: &[Value]) -> Result<Value> {
    use std::io::BufRead;
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "file_line() requires a handle argument".to_string(),
        });
    }
    let id = require_number("file_line", &args[0])? as u64;
    let handle_arc = match get_file_handle(id) {
        Some(h) => h,
        None => return Err(AudionError::RuntimeError {
            msg: format!("file_line(): invalid handle {}", id),
        }),
    };
    let mut guard = handle_arc.lock().unwrap();
    match &mut *guard {
        FileHandle::Read(reader) => {
            let mut buf = Vec::new();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => Ok(Value::Bool(false)), // EOF
                Ok(_) => Ok(Value::Bytes(buf)),
                Err(_) => Ok(Value::Bool(false)),
            }
        }
        _ => Err(AudionError::RuntimeError {
            msg: "file_line() requires a handle opened in read mode".to_string(),
        }),
    }
}

// file_read_chunk(handle, size) → Bytes (up to `size` bytes) or false (EOF or error)
// Binary safe.
fn builtin_file_read_chunk(args: &[Value]) -> Result<Value> {
    use std::io::Read;
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "file_read_chunk() requires 2 arguments: file_read_chunk(handle, size)".to_string(),
        });
    }
    let id = require_number("file_read_chunk", &args[0])? as u64;
    let size = require_number("file_read_chunk", &args[1])? as usize;
    let handle_arc = match get_file_handle(id) {
        Some(h) => h,
        None => return Err(AudionError::RuntimeError {
            msg: format!("file_read_chunk(): invalid handle {}", id),
        }),
    };
    let mut guard = handle_arc.lock().unwrap();
    match &mut *guard {
        FileHandle::Read(reader) => {
            let mut buf = vec![0u8; size];
            match reader.read(&mut buf) {
                Ok(0) => Ok(Value::Bool(false)), // EOF
                Ok(n) => {
                    buf.truncate(n);
                    Ok(Value::Bytes(buf))
                }
                Err(_) => Ok(Value::Bool(false)),
            }
        }
        _ => Err(AudionError::RuntimeError {
            msg: "file_read_chunk() requires a handle opened in read mode".to_string(),
        }),
    }
}

// file_write_handle(handle, data) → number of bytes written or false
// Accepts string or bytes. Works on write and append handles.
fn builtin_file_write_handle(args: &[Value]) -> Result<Value> {
    use std::io::Write;
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "file_write_handle() requires 2 arguments: file_write_handle(handle, data)".to_string(),
        });
    }
    let id = require_number("file_write_handle", &args[0])? as u64;
    let data: Vec<u8> = match &args[1] {
        Value::String(s) => s.as_bytes().to_vec(),
        Value::Bytes(b) => b.clone(),
        other => return Err(AudionError::RuntimeError {
            msg: format!("file_write_handle(): expected string or bytes, got {}", other.type_name()),
        }),
    };
    let handle_arc = match get_file_handle(id) {
        Some(h) => h,
        None => return Err(AudionError::RuntimeError {
            msg: format!("file_write_handle(): invalid handle {}", id),
        }),
    };
    let mut guard = handle_arc.lock().unwrap();
    let file: &mut dyn Write = match &mut *guard {
        FileHandle::Write(f) => f,
        FileHandle::Append(f) => f,
        FileHandle::Read(_) => return Err(AudionError::RuntimeError {
            msg: "file_write_handle() requires a handle opened in write or append mode".to_string(),
        }),
    };
    match file.write_all(&data) {
        Ok(()) => Ok(Value::Number(data.len() as f64)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// file_seek(handle, offset) → bool
// Seeks to absolute byte offset. Works on all handle types.
fn builtin_file_seek(args: &[Value]) -> Result<Value> {
    use std::io::Seek;
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "file_seek() requires 2 arguments: file_seek(handle, offset)".to_string(),
        });
    }
    let id = require_number("file_seek", &args[0])? as u64;
    let offset = require_number("file_seek", &args[1])? as u64;
    let handle_arc = match get_file_handle(id) {
        Some(h) => h,
        None => return Err(AudionError::RuntimeError {
            msg: format!("file_seek(): invalid handle {}", id),
        }),
    };
    let mut guard = handle_arc.lock().unwrap();
    // BufReader::seek discards the internal buffer, so it is safe to use here.
    let result = match &mut *guard {
        FileHandle::Read(r) => r.seek(std::io::SeekFrom::Start(offset)),
        FileHandle::Write(f) => f.seek(std::io::SeekFrom::Start(offset)),
        FileHandle::Append(f) => f.seek(std::io::SeekFrom::Start(offset)),
    };
    Ok(Value::Bool(result.is_ok()))
}

// file_tell(handle) → number (current byte position) or false
fn builtin_file_tell(args: &[Value]) -> Result<Value> {
    use std::io::Seek;
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "file_tell() requires a handle argument".to_string(),
        });
    }
    let id = require_number("file_tell", &args[0])? as u64;
    let handle_arc = match get_file_handle(id) {
        Some(h) => h,
        None => return Err(AudionError::RuntimeError {
            msg: format!("file_tell(): invalid handle {}", id),
        }),
    };
    let mut guard = handle_arc.lock().unwrap();
    let result = match &mut *guard {
        FileHandle::Read(r) => r.stream_position(),
        FileHandle::Write(f) => f.stream_position(),
        FileHandle::Append(f) => f.stream_position(),
    };
    match result {
        Ok(pos) => Ok(Value::Number(pos as f64)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// file_close(handle) → nil
// Closes and drops the file handle. Flushing happens automatically on drop.
fn builtin_file_close(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "file_close() requires a handle argument".to_string(),
        });
    }
    let id = require_number("file_close", &args[0])? as u64;
    remove_file_handle(id);
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Binary file I/O and byte manipulation
// ---------------------------------------------------------------------------

// file_read_bytes("path") → Bytes or false
// file_read_bytes("path", offset) → read from byte offset to end
// file_read_bytes("path", offset, length) → read `length` bytes from offset
fn builtin_file_read_bytes(args: &[Value]) -> Result<Value> {
    use std::io::{Read, Seek, SeekFrom};

    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "file_read_bytes() requires a file path as first argument".to_string(),
        });
    }
    let path = require_string("file_read_bytes", &args[0])?;

    if args.len() >= 2 {
        let offset = require_number("file_read_bytes", &args[1])? as u64;
        let mut file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return Ok(Value::Bool(false)),
        };
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return Ok(Value::Bool(false));
        }
        if args.len() >= 3 {
            let length = require_number("file_read_bytes", &args[2])? as usize;
            let mut buf = vec![0u8; length];
            match file.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    return Ok(Value::Bytes(buf));
                }
                Err(_) => return Ok(Value::Bool(false)),
            }
        } else {
            let mut buf = Vec::new();
            match file.read_to_end(&mut buf) {
                Ok(_) => return Ok(Value::Bytes(buf)),
                Err(_) => return Ok(Value::Bool(false)),
            }
        }
    }

    match std::fs::read(&path) {
        Ok(bytes) => Ok(Value::Bytes(bytes)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// file_write_bytes("path", bytes) → bytes written or false
fn builtin_file_write_bytes(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "file_write_bytes() requires 2 arguments: file_write_bytes(path, bytes)".to_string(),
        });
    }
    let path = require_string("file_write_bytes", &args[0])?;
    let bytes = require_bytes("file_write_bytes", &args[1])?;
    match std::fs::write(&path, &bytes) {
        Ok(()) => Ok(Value::Number(bytes.len() as f64)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// bytes_len(bytes) → number
fn builtin_bytes_len(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "bytes_len() requires a bytes argument".to_string(),
        });
    }
    let bytes = require_bytes("bytes_len", &args[0])?;
    Ok(Value::Number(bytes.len() as f64))
}

// bytes_get(bytes, index) → number (0-255) or nil
fn builtin_bytes_get(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "bytes_get() requires 2 arguments: bytes_get(bytes, index)".to_string(),
        });
    }
    let bytes = require_bytes("bytes_get", &args[0])?;
    let idx = require_number("bytes_get", &args[1])? as i64;
    let idx = if idx < 0 { bytes.len() as i64 + idx } else { idx } as usize;
    match bytes.get(idx) {
        Some(&b) => Ok(Value::Number(b as f64)),
        None => Ok(Value::Nil),
    }
}

// bytes_slice(bytes, start, length?) → Bytes
fn builtin_bytes_slice(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "bytes_slice() requires at least 2 arguments: bytes_slice(bytes, start, length?)".to_string(),
        });
    }
    let bytes = require_bytes("bytes_slice", &args[0])?;
    let start = require_number("bytes_slice", &args[1])? as usize;
    let start = start.min(bytes.len());
    let end = if args.len() >= 3 {
        let len = require_number("bytes_slice", &args[2])? as usize;
        (start + len).min(bytes.len())
    } else {
        bytes.len()
    };
    Ok(Value::Bytes(bytes[start..end].to_vec()))
}

// bytes_to_array(bytes) → array of numbers (0-255)
fn builtin_bytes_to_array(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "bytes_to_array() requires a bytes argument".to_string(),
        });
    }
    let bytes = require_bytes("bytes_to_array", &args[0])?;
    let arr = AudionArray::new();
    let arr = Arc::new(Mutex::new(arr));
    {
        let mut guard = arr.lock().unwrap();
        for &b in &bytes {
            guard.push_auto(Value::Number(b as f64));
        }
    }
    Ok(Value::Array(arr))
}

// array_to_bytes(array) → Bytes (each element clamped to 0-255)
fn builtin_array_to_bytes(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_to_bytes() requires an array argument".to_string(),
        });
    }
    let arr = require_array("array_to_bytes", &args[0])?;
    let guard = arr.lock().unwrap();
    let mut bytes = Vec::with_capacity(guard.len());
    for (_, val) in guard.entries() {
        match val {
            Value::Number(n) => bytes.push((*n as i64).clamp(0, 255) as u8),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_to_bytes() requires all array values to be numbers".to_string(),
                });
            }
        }
    }
    Ok(Value::Bytes(bytes))
}

fn require_bytes(fn_name: &str, val: &Value) -> Result<Vec<u8>> {
    match val {
        Value::Bytes(b) => Ok(b.clone()),
        other => Err(AudionError::RuntimeError {
            msg: format!("{}() expected bytes, got {}", fn_name, other.type_name()),
        }),
    }
}

// dir_scan("path") → array of filenames or false
fn builtin_dir_scan(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "dir_scan() requires a directory path as first argument".to_string(),
        });
    }
    let path = require_string("dir_scan", &args[0])?;

    let entries = match std::fs::read_dir(&path) {
        Ok(rd) => rd,
        Err(_) => return Ok(Value::Bool(false)),
    };

    let mut arr = AudionArray::new();
    for entry in entries {
        if let Ok(entry) = entry {
            if let Some(name) = entry.file_name().to_str() {
                arr.push_auto(Value::String(name.to_string()));
            }
        }
    }
    Ok(Value::Array(Arc::new(Mutex::new(arr))))
}

// dir_exists("path") → bool
fn builtin_dir_exists(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "dir_exists() requires a directory path as first argument".to_string(),
        });
    }
    let path = require_string("dir_exists", &args[0])?;
    let p = std::path::Path::new(&path);
    Ok(Value::Bool(p.exists() && p.is_dir()))
}

// dir_create("path") → bool (recursive)
fn builtin_dir_create(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "dir_create() requires a directory path as first argument".to_string(),
        });
    }
    let path = require_string("dir_create", &args[0])?;
    Ok(Value::Bool(std::fs::create_dir_all(&path).is_ok()))
}

// dir_delete("path") → bool
fn builtin_dir_delete(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "dir_delete() requires a directory path as first argument".to_string(),
        });
    }
    let path = require_string("dir_delete", &args[0])?;
    Ok(Value::Bool(std::fs::remove_dir_all(&path).is_ok()))
}

// json_encode(value) → string
// Converts an Audion value to a JSON string.
// Arrays with all-integer sequential keys (0,1,2,...) become JSON arrays.
// Arrays with string keys (or mixed) become JSON objects.
// Returns false on failure.
fn builtin_json_encode(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "json_encode() requires an argument".to_string(),
        });
    }
    let json_val = value_to_json(&args[0]);
    match serde_json::to_string(&json_val) {
        Ok(s) => Ok(Value::String(s)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// json_decode(string) → array/value
// Parses a JSON string into Audion values.
// JSON objects become string-keyed arrays.
// JSON arrays become integer-keyed arrays.
// Returns false on parse failure.
fn builtin_json_decode(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "json_decode() requires a string argument".to_string(),
        });
    }
    let s = require_string("json_decode", &args[0])?;
    match serde_json::from_str::<serde_json::Value>(&s) {
        Ok(json_val) => Ok(json_to_value(&json_val)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::Number(n) => {
            if *n == (*n as i64) as f64 && n.is_finite() {
                serde_json::Value::Number(serde_json::Number::from(*n as i64))
            } else if n.is_finite() {
                serde_json::Number::from_f64(*n)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Nil => serde_json::Value::Null,
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            let entries = guard.entries();
            let is_list = entries.iter().enumerate().all(|(i, (k, _))| {
                matches!(k, Value::Number(n) if *n == i as f64)
            });
            if is_list {
                let items: Vec<serde_json::Value> =
                    entries.iter().map(|(_, v)| value_to_json(v)).collect();
                serde_json::Value::Array(items)
            } else {
                let mut map = serde_json::Map::new();
                for (k, v) in entries.iter() {
                    let key = match k {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    map.insert(key, value_to_json(v));
                }
                serde_json::Value::Object(map)
            }
        }
        // Functions, objects, namespaces → null
        _ => serde_json::Value::Null,
    }
}

fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            Value::Number(n.as_f64().unwrap_or(0.0))
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            let mut result = AudionArray::new();
            for v in arr.iter() {
                result.push_auto(json_to_value(v));
            }
            Value::Array(Arc::new(Mutex::new(result)))
        }
        serde_json::Value::Object(map) => {
            let mut result = AudionArray::new();
            for (k, v) in map.iter() {
                result.set(Value::String(k.clone()), json_to_value(v));
            }
            Value::Array(Arc::new(Mutex::new(result)))
        }
    }
}

// buffer_load("path.wav") → buffer ID (number)
// Allocates a buffer on scsynth and reads the sound file into it.
fn builtin_buffer_load(args: &[Value], osc: &Arc<OscClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "buffer_load() requires a file path as first argument".to_string(),
        });
    }
    let path = require_string("buffer_load", &args[0])?;
    let abs_path = if std::path::Path::new(&path).is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|d| d.join(&path).to_string_lossy().to_string())
            .unwrap_or(path)
    };
    let buf_id = osc.buffer_alloc_read(&abs_path);
    Ok(Value::Number(buf_id as f64))
}

// buffer_free(buf_id) → nil
// Frees a buffer on scsynth.
fn builtin_buffer_free(args: &[Value], osc: &Arc<OscClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "buffer_free() requires a buffer ID as first argument".to_string(),
        });
    }
    let buf_id = require_number("buffer_free", &args[0])? as i32;
    osc.buffer_free(buf_id);
    Ok(Value::Nil)
}

// buffer_query(buf_id) → [num_frames, num_channels, sample_rate] or nil
// Sends /b_query to scsynth and waits for the /b_info reply (500ms timeout).
fn builtin_buffer_query(args: &[Value], osc: &Arc<OscClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "buffer_query() requires a buffer ID".to_string(),
        });
    }
    let buf_id = require_number("buffer_query", &args[0])? as i32;
    match osc.buffer_query(buf_id) {
        Some((frames, chans, sr)) => {
            let mut arr = AudionArray::new();
            arr.push_auto(Value::Number(frames as f64));
            arr.push_auto(Value::Number(chans as f64));
            arr.push_auto(Value::Number(sr as f64));
            Ok(Value::Array(Arc::new(Mutex::new(arr))))
        }
        None => Ok(Value::Nil),
    }
}

// buffer_alloc(num_frames, num_channels) → buffer ID (number)
// Allocates an empty buffer on scsynth (useful for streaming/DiskIn).
fn builtin_buffer_alloc(args: &[Value], osc: &Arc<OscClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "buffer_alloc() requires num_frames as first argument".to_string(),
        });
    }
    let num_frames = require_number("buffer_alloc", &args[0])? as i32;
    let num_channels = if args.len() > 1 {
        require_number("buffer_alloc", &args[1])? as i32
    } else {
        1
    };
    let buf_id = osc.buffer_alloc(num_frames, num_channels);
    Ok(Value::Number(buf_id as f64))
}

// buffer_read(buf_id, "path.wav") → nil
// Reads a sound file into an existing buffer (closes file handle after reading).
fn builtin_buffer_read(args: &[Value], osc: &Arc<OscClient>) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "buffer_read() requires 2 arguments: buffer_read(buf_id, path)".to_string(),
        });
    }
    let buf_id = require_number("buffer_read", &args[0])? as i32;
    let path = require_string("buffer_read", &args[1])?;
    let abs_path = if std::path::Path::new(&path).is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|d| d.join(&path).to_string_lossy().to_string())
            .unwrap_or(path)
    };
    osc.buffer_read_close(buf_id, &abs_path);
    Ok(Value::Nil)
}

// buffer_stream_open("path.wav") → buffer ID (number)
// Allocates a small cache buffer (65536 frames), cues the file for DiskIn streaming,
// and leaves the file handle open. Returns the buffer ID.
fn builtin_buffer_stream_open(args: &[Value], osc: &Arc<OscClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "buffer_stream_open() requires a file path as first argument".to_string(),
        });
    }
    let path = require_string("buffer_stream_open", &args[0])?;
    let abs_path = if std::path::Path::new(&path).is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|d| d.join(&path).to_string_lossy().to_string())
            .unwrap_or(path)
    };

    let num_channels = crate::sampler::detect_channels(std::path::Path::new(&abs_path)) as i32;
    // Allocate the cache buffer, wait for scsynth to process it, then cue the file
    let buf_id = osc.buffer_alloc(65536, num_channels);
    std::thread::sleep(std::time::Duration::from_millis(100));
    osc.buffer_read(buf_id, &abs_path); // leaveOpen=1 for DiskIn streaming
    Ok(Value::Number(buf_id as f64))
}

// buffer_stream_close(buf_id) → nil
// Closes the open file handle and frees the cache buffer.
fn builtin_buffer_stream_close(args: &[Value], osc: &Arc<OscClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "buffer_stream_close() requires a buffer ID as first argument".to_string(),
        });
    }
    let buf_id = require_number("buffer_stream_close", &args[0])? as i32;
    osc.buffer_close(buf_id);
    osc.buffer_free(buf_id);
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Recording builtins
// ---------------------------------------------------------------------------

// record_start() → path string
// record_start("/path/to/file.wav") → path string
// Compiles and loads the audion_diskout SynthDef (once per session), allocates
// a stereo buffer opened for writing, and starts a DiskOut synth at the tail
// of the default group so it captures all audio. Auto-generates a timestamped
// filename in the source file's directory if no path is given.
fn builtin_record_start(args: &[Value], osc: &Arc<OscClient>, base_path: &std::path::Path) -> Result<Value> {
    let path = if !args.is_empty() {
        let p = require_string("record_start", &args[0])?;
        resolve_path(base_path, &p)
    } else {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        base_path.join(format!("recording_{}.wav", ts)).to_string_lossy().to_string()
    };

    // Compile the DiskOut SynthDef once per session; cache bytes in OscClient
    if !osc.has_recording_synthdef() {
        let out_dir = crate::sclang::synthdef_output_dir();
        let sc_code = format!(
            "SynthDef(\\audion_diskout, {{ |bufnum=0|\n\tDiskOut.ar(bufnum, In.ar(0, 2));\n}}).writeDefFile(\"{}\");\n0.exit;\n",
            out_dir
        );
        let compiled = crate::sclang::compile_synthdef("audion_diskout", &sc_code)?;
        osc.set_recording_synthdef(compiled);
    }

    // Alloc buffer + open for writing (atomic via completion message)
    let buf_id = osc.buffer_alloc_for_recording(&path);

    // Load SynthDef with /s_new embedded as completion — synth is created only
    // after scsynth has fully registered the SynthDef, avoiding "not found" errors
    let node_id = osc.load_recording_synthdef_then_synth(
        "audion_diskout",
        &[("bufnum".to_string(), Value::Number(buf_id as f64))],
    );

    osc.set_recording_state(path.clone(), node_id, buf_id);
    Ok(Value::String(path))
}

// record_stop() → path string (the file that was recorded), or nil
fn builtin_record_stop(osc: &Arc<OscClient>) -> Result<Value> {
    match osc.take_recording_state() {
        Some((node_id, buf_id, path)) => {
            osc.node_free(node_id);
            osc.buffer_close(buf_id);
            osc.buffer_free(buf_id);
            Ok(Value::String(path))
        }
        None => Ok(Value::Nil),
    }
}

// record_path() → string or nil
// Returns the path of the current or last recording.
fn builtin_record_path(osc: &Arc<OscClient>) -> Result<Value> {
    match osc.get_recording_path() {
        Some(path) => Ok(Value::String(path)),
        None => Ok(Value::Nil),
    }
}

// ---------------------------------------------------------------------------
// Networking builtins — TCP
// ---------------------------------------------------------------------------

// net_connect(host, port) → handle (number) or false
fn builtin_net_connect(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "net_connect() requires 2 arguments: net_connect(host, port)".to_string(),
        });
    }
    let host = require_string("net_connect", &args[0])?;
    let port = require_number("net_connect", &args[1])? as u16;

    match TcpStream::connect(format!("{}:{}", host, port)) {
        Ok(stream) => {
            let id = store_handle(NetHandle::Stream(stream));
            Ok(Value::Number(id as f64))
        }
        Err(_) => Ok(Value::Bool(false)),
    }
}

// net_listen(host, port) → handle (number) or false
fn builtin_net_listen(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "net_listen() requires 2 arguments: net_listen(host, port)".to_string(),
        });
    }
    let host = require_string("net_listen", &args[0])?;
    let port = require_number("net_listen", &args[1])? as u16;

    match TcpListener::bind(format!("{}:{}", host, port)) {
        Ok(listener) => {
            let id = store_handle(NetHandle::Listener(listener));
            Ok(Value::Number(id as f64))
        }
        Err(_) => Ok(Value::Bool(false)),
    }
}

// net_accept(listener_handle) → stream handle (number) or false
fn builtin_net_accept(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "net_accept() requires a listener handle as first argument".to_string(),
        });
    }
    let id = require_number("net_accept", &args[0])? as u64;
    let handle = match get_handle(id) {
        Some(h) => h,
        None => return Ok(Value::Bool(false)),
    };

    let locked = handle.lock().unwrap();
    match &*locked {
        NetHandle::Listener(listener) => match listener.accept() {
            Ok((stream, _addr)) => {
                drop(locked);
                let new_id = store_handle(NetHandle::Stream(stream));
                Ok(Value::Number(new_id as f64))
            }
            Err(_) => Ok(Value::Bool(false)),
        },
        _ => Err(AudionError::RuntimeError {
            msg: "net_accept() requires a listener handle".to_string(),
        }),
    }
}

// net_read(handle) → string or false
// net_read(handle, max_bytes) → string or false
fn builtin_net_read(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "net_read() requires a handle as first argument".to_string(),
        });
    }
    let id = require_number("net_read", &args[0])? as u64;
    let max_bytes = if args.len() > 1 {
        require_number("net_read", &args[1])? as usize
    } else {
        8192
    };

    let handle = match get_handle(id) {
        Some(h) => h,
        None => return Ok(Value::Bool(false)),
    };

    let mut locked = handle.lock().unwrap();
    match &mut *locked {
        NetHandle::Stream(stream) => {
            let mut buf = vec![0u8; max_bytes];
            match stream.read(&mut buf) {
                Ok(0) => Ok(Value::String(String::new())),
                Ok(n) => Ok(Value::String(String::from_utf8_lossy(&buf[..n]).to_string())),
                Err(_) => Ok(Value::Bool(false)),
            }
        }
        _ => Err(AudionError::RuntimeError {
            msg: "net_read() requires a stream handle".to_string(),
        }),
    }
}

// net_write(handle, data) → bytes written (number) or false
fn builtin_net_write(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "net_write() requires 2 arguments: net_write(handle, data)".to_string(),
        });
    }
    let id = require_number("net_write", &args[0])? as u64;
    let data = args[1].to_string();

    let handle = match get_handle(id) {
        Some(h) => h,
        None => return Ok(Value::Bool(false)),
    };

    let mut locked = handle.lock().unwrap();
    match &mut *locked {
        NetHandle::Stream(stream) => match stream.write_all(data.as_bytes()) {
            Ok(()) => Ok(Value::Number(data.len() as f64)),
            Err(_) => Ok(Value::Bool(false)),
        },
        _ => Err(AudionError::RuntimeError {
            msg: "net_write() requires a stream handle".to_string(),
        }),
    }
}

// net_close(handle) → true
fn builtin_net_close(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "net_close() requires a handle as first argument".to_string(),
        });
    }
    let id = require_number("net_close", &args[0])? as u64;
    remove_handle(id);
    Ok(Value::Bool(true))
}

// ---------------------------------------------------------------------------
// Networking builtins — HTTP (via ureq)
// ---------------------------------------------------------------------------

// net_http(method, url) → response array or false
// net_http(method, url, body) → response array or false
// net_http(method, url, body, headers) → response array or false
//
// Returns: ["status" => 200, "body" => "...", "headers" => ["Content-Type" => "text/html"]]
fn builtin_net_http(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "net_http() requires at least 2 arguments: net_http(method, url)".to_string(),
        });
    }
    let method = require_string("net_http", &args[0])?.to_uppercase();
    let url = require_string("net_http", &args[1])?;

    let body: Option<String> = if args.len() > 2 && args[2] != Value::Nil {
        Some(args[2].to_string())
    } else {
        None
    };

    // Extract headers from optional 4th argument (key=>value array)
    let headers: Vec<(String, String)> = if args.len() > 3 {
        if let Value::Array(arr) = &args[3] {
            let guard = arr.lock().unwrap();
            guard.entries()
                .iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        Value::String(s) => s.clone(),
                        _ => return None,
                    };
                    let val = v.to_string();
                    Some((key, val))
                })
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let mut req = match method.as_str() {
        "GET" => ureq::get(&url),
        "POST" => ureq::post(&url),
        "PUT" => ureq::put(&url),
        "DELETE" => ureq::delete(&url),
        "PATCH" => ureq::patch(&url),
        "HEAD" => ureq::head(&url),
        other => {
            return Err(AudionError::RuntimeError {
                msg: format!("net_http() unsupported method '{}'", other),
            });
        }
    };

    for (key, val) in &headers {
        req = req.set(key, val);
    }

    let result = if let Some(body_str) = body {
        req.send_string(&body_str)
    } else {
        req.call()
    };

    match result {
        Ok(resp) => Ok(ureq_response_to_value(resp.status(), resp)),
        Err(ureq::Error::Status(code, resp)) => Ok(ureq_response_to_value(code, resp)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn ureq_response_to_value(status: u16, resp: ureq::Response) -> Value {
    let mut headers = AudionArray::new();
    for name in resp.headers_names() {
        if let Some(val) = resp.header(&name) {
            headers.set(Value::String(name), Value::String(val.to_string()));
        }
    }
    let body_str = resp.into_string().unwrap_or_default();

    let mut result = AudionArray::new();
    result.set(Value::String("status".to_string()), Value::Number(status as f64));
    result.set(Value::String("body".to_string()), Value::String(body_str));
    result.set(Value::String("headers".to_string()), Value::Array(Arc::new(Mutex::new(headers))));
    Value::Array(Arc::new(Mutex::new(result)))
}

// ---------------------------------------------------------------------------
// Networking builtins — UDP
// ---------------------------------------------------------------------------

// net_udp_bind(port) → handle (number) or false
// net_udp_bind(host, port) → handle (number) or false
fn builtin_net_udp_bind(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "net_udp_bind() requires at least 1 argument: net_udp_bind(port) or net_udp_bind(host, port)".to_string(),
        });
    }

    let addr = if args.len() == 1 {
        // Single argument: port only, bind to all interfaces (0.0.0.0)
        let port = require_number("net_udp_bind", &args[0])? as u16;
        format!("0.0.0.0:{}", port)
    } else {
        // Two arguments: host and port
        let host = require_string("net_udp_bind", &args[0])?;
        let port = require_number("net_udp_bind", &args[1])? as u16;
        format!("{}:{}", host, port)
    };

    match UdpSocket::bind(&addr) {
        Ok(socket) => {
            // Set non-blocking for recv operations
            if socket.set_nonblocking(true).is_err() {
                return Ok(Value::Bool(false));
            }
            let id = store_handle(NetHandle::UdpSocket(socket));
            Ok(Value::Number(id as f64))
        }
        Err(_) => Ok(Value::Bool(false)),
    }
}

// net_udp_send(handle, host, port, data) → bytes sent (number) or false
fn builtin_net_udp_send(args: &[Value]) -> Result<Value> {
    if args.len() < 4 {
        return Err(AudionError::RuntimeError {
            msg: "net_udp_send() requires 4 arguments: net_udp_send(handle, host, port, data)".to_string(),
        });
    }
    let id = require_number("net_udp_send", &args[0])? as u64;
    let host = require_string("net_udp_send", &args[1])?;
    let port = require_number("net_udp_send", &args[2])? as u16;
    let data = args[3].to_string();

    let handle = match get_handle(id) {
        Some(h) => h,
        None => return Ok(Value::Bool(false)),
    };

    let locked = handle.lock().unwrap();
    match &*locked {
        NetHandle::UdpSocket(socket) => {
            let addr = format!("{}:{}", host, port);
            match socket.send_to(data.as_bytes(), &addr) {
                Ok(n) => Ok(Value::Number(n as f64)),
                Err(_) => Ok(Value::Bool(false)),
            }
        }
        _ => Err(AudionError::RuntimeError {
            msg: "net_udp_send() requires a UDP socket handle".to_string(),
        }),
    }
}

// net_udp_recv(handle) → string or nil
// net_udp_recv(handle, max_bytes) → string or nil
// Non-blocking receive. Returns nil if no data available.
// Returns ["data" => string, "host" => host, "port" => port] on success.
fn builtin_net_udp_recv(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "net_udp_recv() requires a handle as first argument".to_string(),
        });
    }
    let id = require_number("net_udp_recv", &args[0])? as u64;
    let max_bytes = if args.len() > 1 {
        require_number("net_udp_recv", &args[1])? as usize
    } else {
        8192
    };

    let handle = match get_handle(id) {
        Some(h) => h,
        None => return Ok(Value::Nil),
    };

    let locked = handle.lock().unwrap();
    match &*locked {
        NetHandle::UdpSocket(socket) => {
            let mut buf = vec![0u8; max_bytes];
            match socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let mut result = AudionArray::new();
                    result.set(Value::String("data".to_string()), Value::String(data));
                    result.set(Value::String("host".to_string()), Value::String(addr.ip().to_string()));
                    result.set(Value::String("port".to_string()), Value::Number(addr.port() as f64));
                    Ok(Value::Array(Arc::new(Mutex::new(result))))
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available (non-blocking)
                    Ok(Value::Nil)
                }
                Err(_) => Ok(Value::Bool(false)),
            }
        }
        _ => Err(AudionError::RuntimeError {
            msg: "net_udp_recv() requires a UDP socket handle".to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Scala (.scl) parser and tuning helpers for mtof / ftom
// ---------------------------------------------------------------------------

/// Parse a single Scala pitch value into a frequency ratio.
/// - Contains `/` → ratio (e.g. "3/2" → 1.5)
/// - Contains `.` → cents (e.g. "700.0" → 2^(700/1200))
/// - Plain integer → cents (e.g. "702" → 2^(702/1200))
fn parse_scala_pitch(s: &str) -> Result<f64> {
    let token = s.split_whitespace().next().unwrap_or(s);
    if token.contains('/') {
        let parts: Vec<&str> = token.split('/').collect();
        if parts.len() != 2 {
            return Err(AudionError::RuntimeError {
                msg: format!("mtof(): invalid ratio '{}'", token),
            });
        }
        let num: f64 = parts[0].trim().parse().map_err(|_| AudionError::RuntimeError {
            msg: format!("mtof(): invalid ratio '{}'", token),
        })?;
        let den: f64 = parts[1].trim().parse().map_err(|_| AudionError::RuntimeError {
            msg: format!("mtof(): invalid ratio '{}'", token),
        })?;
        Ok(num / den)
    } else {
        let cents: f64 = token.parse().map_err(|_| AudionError::RuntimeError {
            msg: format!("mtof(): invalid pitch value '{}'", token),
        })?;
        Ok(2.0_f64.powf(cents / 1200.0))
    }
}

/// Parse a Scala .scl file into a Vec<f64> of ratios.
/// Format: `!` lines are comments, first non-comment = description,
/// second non-comment = note count N, then N pitch lines.
/// The last ratio is the period (usually 2/1).
fn parse_scala_file(path: &str, base_path: &std::path::Path) -> Result<Vec<f64>> {
    let resolved = resolve_path(base_path, path);

    // Check cache first (using resolved absolute path as key)
    {
        let cache = scala_cache().lock().unwrap();
        if let Some(ratios) = cache.get(&resolved) {
            return Ok(ratios.clone());
        }
    }

    let contents = std::fs::read_to_string(&resolved).map_err(|e| AudionError::RuntimeError {
        msg: format!("mtof(): cannot read scala file '{}': {}", path, e),
    })?;

    let mut non_comment: Vec<&str> = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('!') && !trimmed.is_empty() {
            non_comment.push(trimmed);
        }
    }

    if non_comment.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: format!("mtof(): invalid scala file '{}'", path),
        });
    }

    // non_comment[0] = description (skip), non_comment[1] = note count
    let num_notes: usize = non_comment[1].trim().parse().map_err(|_| AudionError::RuntimeError {
        msg: format!("mtof(): invalid note count in scala file '{}'", path),
    })?;

    let mut ratios = Vec::with_capacity(num_notes);
    for i in 0..num_notes {
        let idx = i + 2;
        if idx >= non_comment.len() {
            break;
        }
        ratios.push(parse_scala_pitch(non_comment[idx])?);
    }

    // Store in cache
    {
        let mut cache = scala_cache().lock().unwrap();
        cache.insert(resolved, ratios.clone());
    }

    Ok(ratios)
}

/// Extract tuning ratios from the second argument to mtof/ftom.
/// Accepts a Scala file path (string) or an array of ratio numbers.
fn get_tuning_ratios(fn_name: &str, val: &Value, base_path: &std::path::Path) -> Result<Vec<f64>> {
    match val {
        Value::String(path) => parse_scala_file(path, base_path),
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            let mut ratios = Vec::with_capacity(guard.len());
            for (_, v) in guard.entries().iter() {
                match v {
                    Value::Number(n) => ratios.push(*n),
                    other => {
                        return Err(AudionError::RuntimeError {
                            msg: format!(
                                "{}(): tuning array contains non-number: {}",
                                fn_name,
                                other.type_name()
                            ),
                        });
                    }
                }
            }
            Ok(ratios)
        }
        other => Err(AudionError::RuntimeError {
            msg: format!(
                "{}(): second argument must be a scala file path or array of ratios, got {}",
                fn_name,
                other.type_name()
            ),
        }),
    }
}

// mtof(note)              → 12-TET
// mtof(note, "scale.scl") → Scala tuning
// mtof(note, [ratios...]) → array of ratios (last = period)
fn builtin_mtof(args: &[Value], base_path: &std::path::Path) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "mtof() requires a MIDI note number as first argument".to_string(),
        });
    }
    let note = require_number("mtof", &args[0])?;

    if args.len() < 2 {
        // Standard 12-TET
        let freq = 440.0 * 2.0_f64.powf((note - 69.0) / 12.0);
        return Ok(Value::Number(freq));
    }

    let ratios = get_tuning_ratios("mtof", &args[1], base_path)?;
    let n = ratios.len();
    if n == 0 {
        return Err(AudionError::RuntimeError {
            msg: "mtof(): tuning is empty".to_string(),
        });
    }

    let period = ratios[n - 1]; // last entry is the period (e.g. 2.0 for octave)
    let base_freq = 440.0;      // reference = MIDI 69 = A4
    let offset = (note - 69.0).round() as i32;
    let n_i = n as i32;

    let octave = offset.div_euclid(n_i);
    let degree = offset.rem_euclid(n_i) as usize;

    let degree_ratio = if degree == 0 {
        1.0
    } else {
        ratios[degree - 1]
    };

    let freq = base_freq * period.powi(octave) * degree_ratio;
    Ok(Value::Number(freq))
}

// ftom(freq)              → 12-TET (fractional MIDI note)
// ftom(freq, "scale.scl") → nearest MIDI note in tuning
// ftom(freq, [ratios...]) → nearest MIDI note in tuning
fn builtin_ftom(args: &[Value], base_path: &std::path::Path) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "ftom() requires a frequency as first argument".to_string(),
        });
    }
    let freq = require_number("ftom", &args[0])?;
    if freq <= 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ftom(): frequency must be positive".to_string(),
        });
    }

    if args.len() < 2 {
        // Standard 12-TET
        let note = 69.0 + 12.0 * (freq / 440.0).log2();
        return Ok(Value::Number(note));
    }

    let ratios = get_tuning_ratios("ftom", &args[1], base_path)?;
    let n = ratios.len();
    if n == 0 {
        return Err(AudionError::RuntimeError {
            msg: "ftom(): tuning is empty".to_string(),
        });
    }

    let period = ratios[n - 1];
    let log_period = period.ln();
    let log_ratio = (freq / 440.0).ln();

    // Which period are we in?
    let octave = (log_ratio / log_period).floor() as i32;
    let log_within = log_ratio - octave as f64 * log_period;

    // Find closest degree in log space (degree 0 = ln(1) = 0)
    let mut best_degree: usize = 0;
    let mut best_dist = log_within.abs();

    for (i, &r) in ratios.iter().enumerate().take(n - 1) {
        let dist = (log_within - r.ln()).abs();
        if dist < best_dist {
            best_dist = dist;
            best_degree = i + 1;
        }
    }

    // Check if closer to period (= degree 0 of next octave)
    let dist_to_period = (log_within - log_period).abs();
    if dist_to_period < best_dist {
        let note = 69.0 + ((octave + 1) * n as i32) as f64;
        return Ok(Value::Number(note));
    }

    let note = 69.0 + (octave * n as i32 + best_degree as i32) as f64;
    Ok(Value::Number(note))
}

// ---------------------------------------------------------------------------
// MIDI builtins
// ---------------------------------------------------------------------------

// midi_config() → array of port names (also prints them)
// midi_config("name") → connect by name substring match → bool
// midi_config(index) → connect by port index → bool
fn builtin_midi_config(args: &[Value], midi: &Arc<MidiClient>) -> Result<Value> {
    if args.is_empty() {
        let ports = MidiClient::list_ports();
        if ports.is_empty() {
            println!("no MIDI output ports found");
        } else {
            for (i, name) in ports.iter().enumerate() {
                println!("  {}: {}", i, name);
            }
        }
        let mut arr = AudionArray::new();
        for name in ports.iter() {
            arr.push_auto(Value::String(name.clone()));
        }
        return Ok(Value::Array(Arc::new(Mutex::new(arr))));
    }

    match &args[0] {
        Value::String(name) => {
            let ok = midi.connect(name);
            if ok {
                println!("midi: connected to '{}'", name);
            }
            Ok(Value::Bool(ok))
        }
        Value::Number(n) => {
            let idx = *n as usize;
            let ports = MidiClient::list_ports();
            let port_name = ports.get(idx).cloned().unwrap_or_default();
            let ok = midi.connect_by_index(idx);
            if ok {
                println!("midi: connected to '{}'", port_name);
            }
            Ok(Value::Bool(ok))
        }
        other => Err(AudionError::RuntimeError {
            msg: format!(
                "midi_config() expected string or number, got {}",
                other.type_name()
            ),
        }),
    }
}

// midi_note(note, velocity) — default channel 1
// midi_note(note, velocity, channel) — channel 1-16
// velocity 0 sends note off
fn builtin_midi_note(args: &[Value], midi: &Arc<MidiClient>) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "midi_note() requires 2 arguments: midi_note(note, velocity)".to_string(),
        });
    }
    let note = require_number("midi_note", &args[0])? as u8;
    let vel = require_number("midi_note", &args[1])? as u8;
    let ch = if args.len() > 2 {
        (require_number("midi_note", &args[2])? as u8).saturating_sub(1)
    } else {
        0
    };
    if vel == 0 {
        midi.note_off(ch, note);
    } else {
        midi.note_on(ch, note, vel);
    }
    Ok(Value::Nil)
}

// midi_cc(controller, value) — default channel 1
// midi_cc(controller, value, channel) — channel 1-16
fn builtin_midi_cc(args: &[Value], midi: &Arc<MidiClient>) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "midi_cc() requires 2 arguments: midi_cc(controller, value)".to_string(),
        });
    }
    let cc = require_number("midi_cc", &args[0])? as u8;
    let val = require_number("midi_cc", &args[1])? as u8;
    let ch = if args.len() > 2 {
        (require_number("midi_cc", &args[2])? as u8).saturating_sub(1)
    } else {
        0
    };
    midi.cc(ch, cc, val);
    Ok(Value::Nil)
}

// midi_program(program) — default channel 1
// midi_program(program, channel) — channel 1-16
fn builtin_midi_program(args: &[Value], midi: &Arc<MidiClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "midi_program() requires a program number as first argument".to_string(),
        });
    }
    let prog = require_number("midi_program", &args[0])? as u8;
    let ch = if args.len() > 1 {
        (require_number("midi_program", &args[1])? as u8).saturating_sub(1)
    } else {
        0
    };
    midi.program_change(ch, prog);
    Ok(Value::Nil)
}

// midi_out(byte1, byte2) — 2-byte message
// midi_out(byte1, byte2, byte3) — 3-byte message
fn builtin_midi_out(args: &[Value], midi: &Arc<MidiClient>) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "midi_out() requires at least 2 arguments: midi_out(status, data1)".to_string(),
        });
    }
    let b1 = require_number("midi_out", &args[0])? as u8;
    let b2 = require_number("midi_out", &args[1])? as u8;
    if args.len() > 2 {
        let b3 = require_number("midi_out", &args[2])? as u8;
        midi.send(&[b1, b2, b3]);
    } else {
        midi.send(&[b1, b2]);
    }
    Ok(Value::Nil)
}

// midi_clock() — send single MIDI clock tick (0xF8)
fn builtin_midi_clock(midi: &Arc<MidiClient>) -> Result<Value> {
    midi.clock_tick();
    Ok(Value::Nil)
}

// midi_start() — send MIDI Start (0xFA)
fn builtin_midi_start(midi: &Arc<MidiClient>) -> Result<Value> {
    midi.start();
    Ok(Value::Nil)
}

// midi_stop() — send MIDI Stop (0xFC)
fn builtin_midi_stop(midi: &Arc<MidiClient>) -> Result<Value> {
    midi.stop();
    Ok(Value::Nil)
}

// midi_panic() — all notes off on all 16 channels
fn builtin_midi_panic(midi: &Arc<MidiClient>) -> Result<Value> {
    midi.panic();
    Ok(Value::Nil)
}

// midi_read("file.mid") → array of event arrays
// Each event: ["note_on", "note" => 60, "vel" => 100, "tick" => 0, "track" => 0, "channel" => 1]
fn builtin_midi_read(args: &[Value], base_path: &std::path::Path) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "midi_read() requires a file path as first argument".to_string(),
        });
    }
    let path_str = require_string("midi_read", &args[0])?;
    let path = resolve_path(base_path, &path_str);

    let data = std::fs::read(&path).map_err(|e| AudionError::RuntimeError {
        msg: format!("midi_read(): cannot read '{}': {}", path, e),
    })?;

    let smf = midly::Smf::parse(&data).map_err(|e| AudionError::RuntimeError {
        msg: format!("midi_read(): failed to parse MIDI file: {}", e),
    })?;

    // Extract PPQ from timing header
    let ppq = match smf.header.timing {
        midly::Timing::Metrical(t) => t.as_int() as f64,
        midly::Timing::Timecode(fps, sub) => (fps.as_f32() * sub as f32) as f64,
    };

    let mut events = AudionArray::new();
    events.set(Value::String("ppq".to_string()), Value::Number(ppq));

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut absolute_tick: u64 = 0;
        for event in track {
            absolute_tick += event.delta.as_int() as u64;

            let maybe_ev: Option<AudionArray> = match &event.kind {
                midly::TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int() as f64 + 1.0;
                    let tick = absolute_tick as f64;
                    let tr = track_idx as f64;
                    match message {
                        midly::MidiMessage::NoteOn { key, vel } => {
                            let mut arr = AudionArray::new();
                            let t = if vel.as_int() == 0 { "note_off" } else { "note_on" };
                            arr.push_auto(Value::String(t.to_string()));
                            arr.set(Value::String("note".to_string()), Value::Number(key.as_int() as f64));
                            arr.set(Value::String("vel".to_string()), Value::Number(vel.as_int() as f64));
                            arr.set(Value::String("tick".to_string()), Value::Number(tick));
                            arr.set(Value::String("track".to_string()), Value::Number(tr));
                            arr.set(Value::String("channel".to_string()), Value::Number(ch));
                            Some(arr)
                        }
                        midly::MidiMessage::NoteOff { key, vel } => {
                            let mut arr = AudionArray::new();
                            arr.push_auto(Value::String("note_off".to_string()));
                            arr.set(Value::String("note".to_string()), Value::Number(key.as_int() as f64));
                            arr.set(Value::String("vel".to_string()), Value::Number(vel.as_int() as f64));
                            arr.set(Value::String("tick".to_string()), Value::Number(tick));
                            arr.set(Value::String("track".to_string()), Value::Number(tr));
                            arr.set(Value::String("channel".to_string()), Value::Number(ch));
                            Some(arr)
                        }
                        midly::MidiMessage::Controller { controller, value } => {
                            let mut arr = AudionArray::new();
                            arr.push_auto(Value::String("cc".to_string()));
                            arr.set(Value::String("num".to_string()), Value::Number(controller.as_int() as f64));
                            arr.set(Value::String("value".to_string()), Value::Number(value.as_int() as f64));
                            arr.set(Value::String("tick".to_string()), Value::Number(tick));
                            arr.set(Value::String("track".to_string()), Value::Number(tr));
                            arr.set(Value::String("channel".to_string()), Value::Number(ch));
                            Some(arr)
                        }
                        midly::MidiMessage::ProgramChange { program } => {
                            let mut arr = AudionArray::new();
                            arr.push_auto(Value::String("program".to_string()));
                            arr.set(Value::String("num".to_string()), Value::Number(program.as_int() as f64));
                            arr.set(Value::String("tick".to_string()), Value::Number(tick));
                            arr.set(Value::String("track".to_string()), Value::Number(tr));
                            arr.set(Value::String("channel".to_string()), Value::Number(ch));
                            Some(arr)
                        }
                        midly::MidiMessage::PitchBend { bend } => {
                            let mut arr = AudionArray::new();
                            arr.push_auto(Value::String("pitchbend".to_string()));
                            arr.set(Value::String("value".to_string()), Value::Number(bend.as_int() as f64));
                            arr.set(Value::String("tick".to_string()), Value::Number(tick));
                            arr.set(Value::String("track".to_string()), Value::Number(tr));
                            arr.set(Value::String("channel".to_string()), Value::Number(ch));
                            Some(arr)
                        }
                        _ => None,
                    }
                }
                midly::TrackEventKind::Meta(meta) => match meta {
                    midly::MetaMessage::Tempo(us) => {
                        let bpm = 60_000_000.0 / us.as_int() as f64;
                        let mut arr = AudionArray::new();
                        arr.push_auto(Value::String("tempo".to_string()));
                        arr.set(Value::String("bpm".to_string()), Value::Number(bpm));
                        arr.set(Value::String("tick".to_string()), Value::Number(absolute_tick as f64));
                        arr.set(Value::String("track".to_string()), Value::Number(track_idx as f64));
                        Some(arr)
                    }
                    _ => None,
                },
                _ => None,
            };

            if let Some(arr) = maybe_ev {
                events.push_auto(Value::Array(Arc::new(Mutex::new(arr))));
            }
        }
    }

    Ok(Value::Array(Arc::new(Mutex::new(events))))
}

// midi_write("file.mid", events) → true or false
// events: array of event arrays as returned by midi_read()
// Writes a single-track Format-0 MIDI file at 480 PPQN.
// Each event needs: type (first positional value), tick, and type-specific fields.
fn builtin_midi_write(args: &[Value], base_path: &std::path::Path) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "midi_write() requires 2 arguments: midi_write(path, events)".to_string(),
        });
    }
    let path_str = require_string("midi_write", &args[0])?;
    let path = resolve_path(base_path, &path_str);
    let events_arc = require_array("midi_write", &args[1])?;
    let events_guard = events_arc.lock().unwrap();

    const PPQN: u16 = 480;

    // Collect (absolute_tick, TrackEvent) pairs
    let mut raw: Vec<(u64, midly::TrackEvent<'static>)> = Vec::new();

    for (_, ev_val) in events_guard.entries() {
        let arr_arc = match ev_val {
            Value::Array(a) => a.clone(),
            _ => continue,
        };
        let arr = arr_arc.lock().unwrap();

        let ev_type = match arr.get(&Value::Number(0.0)) {
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };

        let tick = match arr.get(&Value::String("tick".to_string())) {
            Some(Value::Number(n)) => *n as u64,
            _ => 0,
        };

        let channel_num = match arr.get(&Value::String("channel".to_string())) {
            Some(Value::Number(n)) => ((*n as u8).saturating_sub(1)) & 0x0F,
            _ => 0,
        };
        let channel = midly::num::u4::from(channel_num);

        let message: Option<midly::MidiMessage> = match ev_type.as_str() {
            "note_on" => {
                let note = match arr.get(&Value::String("note".to_string())) {
                    Some(Value::Number(n)) => midly::num::u7::from(*n as u8 & 0x7F),
                    _ => continue,
                };
                let vel = match arr.get(&Value::String("vel".to_string())) {
                    Some(Value::Number(n)) => midly::num::u7::from(*n as u8 & 0x7F),
                    _ => midly::num::u7::from(64u8),
                };
                Some(midly::MidiMessage::NoteOn { key: note, vel })
            }
            "note_off" => {
                let note = match arr.get(&Value::String("note".to_string())) {
                    Some(Value::Number(n)) => midly::num::u7::from(*n as u8 & 0x7F),
                    _ => continue,
                };
                let vel = match arr.get(&Value::String("vel".to_string())) {
                    Some(Value::Number(n)) => midly::num::u7::from(*n as u8 & 0x7F),
                    _ => midly::num::u7::from(0u8),
                };
                Some(midly::MidiMessage::NoteOff { key: note, vel })
            }
            "cc" => {
                let num = match arr.get(&Value::String("num".to_string())) {
                    Some(Value::Number(n)) => midly::num::u7::from(*n as u8 & 0x7F),
                    _ => continue,
                };
                let val = match arr.get(&Value::String("value".to_string())) {
                    Some(Value::Number(n)) => midly::num::u7::from(*n as u8 & 0x7F),
                    _ => continue,
                };
                Some(midly::MidiMessage::Controller { controller: num, value: val })
            }
            "program" => {
                let num = match arr.get(&Value::String("num".to_string())) {
                    Some(Value::Number(n)) => midly::num::u7::from(*n as u8 & 0x7F),
                    _ => continue,
                };
                Some(midly::MidiMessage::ProgramChange { program: num })
            }
            "pitchbend" => {
                let val = match arr.get(&Value::String("value".to_string())) {
                    Some(Value::Number(n)) => midly::num::u14::from(*n as u16 & 0x3FFF),
                    _ => continue,
                };
                Some(midly::MidiMessage::PitchBend { bend: midly::PitchBend(val) })
            }
            _ => None,
        };

        if let Some(msg) = message {
            raw.push((tick, midly::TrackEvent {
                delta: midly::num::u28::from(0u32), // placeholder, fixed below
                kind: midly::TrackEventKind::Midi { channel, message: msg },
            }));
        }
    }

    // Sort by absolute tick and convert to delta ticks
    raw.sort_by_key(|(t, _)| *t);

    let mut track: Vec<midly::TrackEvent<'static>> = Vec::with_capacity(raw.len() + 1);
    let mut prev_tick: u64 = 0;
    for (abs_tick, mut ev) in raw {
        let delta = (abs_tick - prev_tick).min(u32::MAX as u64) as u32;
        ev.delta = midly::num::u28::from(delta);
        track.push(ev);
        prev_tick = abs_tick;
    }

    // End of track
    track.push(midly::TrackEvent {
        delta: midly::num::u28::from(0u32),
        kind: midly::TrackEventKind::Meta(midly::MetaMessage::EndOfTrack),
    });

    let smf = midly::Smf {
        header: midly::Header {
            format: midly::Format::SingleTrack,
            timing: midly::Timing::Metrical(midly::num::u15::from(PPQN)),
        },
        tracks: vec![track],
    };

    let mut buf = Vec::new();
    if smf.write_std(&mut buf).is_err() {
        return Ok(Value::Bool(false));
    }

    match std::fs::write(&path, &buf) {
        Ok(()) => Ok(Value::Bool(true)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// Helper: Parse MIDI message bytes into Audion event array
fn midi_message_to_event(msg: &[u8]) -> Value {
    if msg.is_empty() {
        return Value::Nil;
    }

    let status = msg[0];

    // System Real-Time messages (0xF8-0xFF)
    match status {
        0xF8 => {
            // Clock
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String("clock".to_string()));
            return Value::Array(Arc::new(Mutex::new(arr)));
        }
        0xFA => {
            // Start
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String("start".to_string()));
            return Value::Array(Arc::new(Mutex::new(arr)));
        }
        0xFC => {
            // Stop
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String("stop".to_string()));
            return Value::Array(Arc::new(Mutex::new(arr)));
        }
        0xFB => {
            // Continue
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String("continue".to_string()));
            return Value::Array(Arc::new(Mutex::new(arr)));
        }
        _ => {}
    }

    // Channel messages (need at least 2 bytes)
    if msg.len() < 2 {
        return Value::Nil;
    }

    let status_type = status & 0xF0;
    let channel = (status & 0x0F) + 1; // Convert to 1-16

    match status_type {
        0x90 => {
            // Note On
            let note = msg[1];
            let vel = msg.get(2).copied().unwrap_or(0);
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String("note_on".to_string()));
            arr.set(Value::String("note".to_string()), Value::Number(note as f64));
            arr.set(Value::String("vel".to_string()), Value::Number(vel as f64));
            arr.set(Value::String("channel".to_string()), Value::Number(channel as f64));
            Value::Array(Arc::new(Mutex::new(arr)))
        }
        0x80 => {
            // Note Off
            let note = msg[1];
            let vel = msg.get(2).copied().unwrap_or(0);
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String("note_off".to_string()));
            arr.set(Value::String("note".to_string()), Value::Number(note as f64));
            arr.set(Value::String("vel".to_string()), Value::Number(vel as f64));
            arr.set(Value::String("channel".to_string()), Value::Number(channel as f64));
            Value::Array(Arc::new(Mutex::new(arr)))
        }
        0xB0 => {
            // Control Change
            let num = msg[1];
            let value = msg.get(2).copied().unwrap_or(0);
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String("cc".to_string()));
            arr.set(Value::String("num".to_string()), Value::Number(num as f64));
            arr.set(Value::String("value".to_string()), Value::Number(value as f64));
            arr.set(Value::String("channel".to_string()), Value::Number(channel as f64));
            Value::Array(Arc::new(Mutex::new(arr)))
        }
        0xC0 => {
            // Program Change
            let num = msg[1];
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String("program".to_string()));
            arr.set(Value::String("num".to_string()), Value::Number(num as f64));
            arr.set(Value::String("channel".to_string()), Value::Number(channel as f64));
            Value::Array(Arc::new(Mutex::new(arr)))
        }
        _ => Value::Nil,
    }
}

// Helper: Call a user function from within a builtin
fn call_user_function(
    func: &Value,
    args: Vec<Value>,
    env: &Arc<Mutex<crate::environment::Environment>>,
    osc: &Arc<OscClient>,
    midi: &Arc<MidiClient>,
    dmx: &Arc<DmxClient>,
    osc_protocol: &Arc<OscProtocolClient>,
    clock: &Arc<Clock>,
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
    base_path: &std::path::Path,
) -> Result<Value> {
    match func {
        Value::Function { params, body, closure, .. } => {
            if args.len() != params.len() {
                return Err(AudionError::RuntimeError {
                    msg: format!("callback expected {} arguments, got {}", params.len(), args.len()),
                });
            }

            // Create child environment from closure
            let call_env = Arc::new(Mutex::new(crate::environment::Environment::new_child(closure.clone())));
            {
                let mut e = call_env.lock().unwrap();
                for (param, val) in params.iter().zip(args.iter()) {
                    e.define(param.name.clone(), val.clone());
                }
            }

            // Create temporary interpreter to execute the callback
            let synthdef_cache = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
            let mut interp = crate::interpreter::Interpreter::new_for_thread(
                call_env,
                osc.clone(),
                midi.clone(),
                dmx.clone(),
                osc_protocol.clone(),
                clock.clone(),
                shutdown.clone(),
                false,
                synthdef_cache,
                base_path.to_path_buf(),
            );

            match interp.exec_stmt(body)? {
                crate::interpreter::ControlFlow::Return(v) => Ok(v),
                _ => Ok(Value::Nil),
            }
        }
        _ => Err(AudionError::RuntimeError {
            msg: "expected function as callback".to_string(),
        }),
    }
}

// midi_listen(port, callback) — blocks, calls callback(event) for each MIDI message
fn builtin_midi_listen(
    args: &[Value],
    midi: &Arc<MidiClient>,
    dmx: &Arc<DmxClient>,
    osc: &Arc<OscClient>,
    osc_protocol: &Arc<OscProtocolClient>,
    clock: &Arc<Clock>,
    env: &Arc<Mutex<crate::environment::Environment>>,
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
    base_path: &std::path::Path,
) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "midi_listen() requires 2 arguments: midi_listen(port, callback)".to_string(),
        });
    }

    let port = require_number("midi_listen", &args[0])? as usize;
    let callback = args[1].clone();

    // Verify callback is a function
    if !matches!(callback, Value::Function { .. }) {
        return Err(AudionError::RuntimeError {
            msg: "midi_listen() second argument must be a function".to_string(),
        });
    }

    // Open MIDI input
    use midir::MidiInput;
    let midi_in = MidiInput::new("audion-listen").map_err(|e| AudionError::RuntimeError {
        msg: format!("Failed to create MIDI input: {}", e),
    })?;

    let in_ports = midi_in.ports();
    if port >= in_ports.len() {
        return Err(AudionError::RuntimeError {
            msg: format!("MIDI input port {} not found (available: {})", port, in_ports.len()),
        });
    }

    let in_port = &in_ports[port];

    // Clone everything we need for the callback
    let callback = callback.clone();
    let env = env.clone();
    let osc = osc.clone();
    let midi = midi.clone();
    let dmx = dmx.clone();
    let osc_protocol = osc_protocol.clone();
    let clock = clock.clone();
    let shutdown_callback = shutdown.clone();
    let shutdown_loop = shutdown.clone();
    let base_path = base_path.to_path_buf();

    // Create the connection with callback
    let _conn = midi_in
        .connect(
            in_port,
            "audion-listen-callback",
            move |_timestamp, message, _| {
                // Check shutdown flag
                if shutdown_callback.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }

                // Parse MIDI message to event
                let event = midi_message_to_event(message);
                if matches!(event, Value::Nil) {
                    return; // Skip unrecognized messages
                }

                // Call user callback
                let _ = call_user_function(
                    &callback,
                    vec![event],
                    &env,
                    &osc,
                    &midi,
                    &dmx,
                    &osc_protocol,
                    &clock,
                    &shutdown_callback,
                    &base_path,
                );
            },
            (),
        )
        .map_err(|e| AudionError::RuntimeError {
            msg: format!("Failed to connect to MIDI input: {}", e),
        })?;

    // Block until shutdown
    loop {
        if shutdown_loop.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(Value::Nil)
}

// midi_bpm_sync(port, enable) — auto-sync BPM to MIDI clock
fn builtin_midi_bpm_sync(
    args: &[Value],
    midi: &Arc<MidiClient>,
    clock: &Arc<Clock>,
    env: &Arc<Mutex<crate::environment::Environment>>,
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "midi_bpm_sync() requires 2 arguments: midi_bpm_sync(port, enable)".to_string(),
        });
    }

    let port = require_number("midi_bpm_sync", &args[0])? as usize;
    let enable = match &args[1] {
        Value::Bool(b) => *b,
        _ => return Err(AudionError::RuntimeError {
            msg: "midi_bpm_sync() second argument must be a boolean".to_string(),
        }),
    };

    if enable {
        // Check if already enabled
        if midi.sync_enabled.load(Ordering::Relaxed) {
            return Ok(Value::Bool(true));
        }

        // Store current BPM
        let current = clock.get_bpm();
        *midi.previous_bpm.lock().unwrap() = Some(current);

        // Spawn sync thread
        let midi_clone = midi.clone();
        let clock_clone = clock.clone();
        let shutdown_clone = shutdown.clone();

        let handle = std::thread::Builder::new()
            .name("midi-bpm-sync".to_string())
            .spawn(move || {
                use midir::MidiInput;
                use std::time::Instant;

                let midi_in = match MidiInput::new("audion-bpm-sync") {
                    Ok(m) => m,
                    Err(_) => {
                        eprintln!("midi_bpm_sync: failed to create MIDI input");
                        return;
                    }
                };

                let in_ports = midi_in.ports();
                if port >= in_ports.len() {
                    eprintln!("midi_bpm_sync: port {} not found", port);
                    return;
                }

                let in_port = &in_ports[port];

                // Track clock timing
                let clock_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
                let first_clock_time = Arc::new(Mutex::new(Option::<Instant>::None));
                let clock_count_clone = clock_count.clone();
                let first_clock_time_clone = first_clock_time.clone();
                let clock_ref = clock_clone.clone();
                let shutdown_callback = shutdown_clone.clone();
                let shutdown_loop = shutdown_clone;

                let _conn = midi_in.connect(
                    in_port,
                    "audion-bpm-sync",
                    move |_timestamp, message, _| {
                        if shutdown_callback.load(Ordering::Relaxed) {
                            return;
                        }

                        // Check for clock message (0xF8)
                        if message.len() > 0 && message[0] == 0xF8 {
                            let count = clock_count_clone.fetch_add(1, Ordering::Relaxed);

                            if count == 0 {
                                // First clock - record start time
                                *first_clock_time_clone.lock().unwrap() = Some(Instant::now());
                            } else if count >= 24 {
                                // Completed one beat (24 PPQN)
                                if let Some(start_time) = *first_clock_time_clone.lock().unwrap() {
                                    let elapsed = start_time.elapsed().as_secs_f64();
                                    let bpm = 60.0 / elapsed;

                                    // Update global BPM
                                    clock_ref.set_bpm(bpm);

                                    // Reset for next beat
                                    clock_count_clone.store(0, Ordering::Relaxed);
                                    *first_clock_time_clone.lock().unwrap() = None;
                                }
                            }
                        }
                    },
                    (),
                );

                if _conn.is_err() {
                    eprintln!("midi_bpm_sync: failed to connect");
                    return;
                }

                // Keep connection alive
                loop {
                    if shutdown_loop.load(Ordering::Relaxed) || !midi_clone.sync_enabled.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            })
            .expect("failed to spawn midi_bpm_sync thread");

        *midi.sync_thread.lock().unwrap() = Some(handle);
        midi.sync_enabled.store(true, Ordering::Relaxed);

        Ok(Value::Bool(true))
    } else {
        // Disable sync
        midi.sync_enabled.store(false, Ordering::Relaxed);

        // Wait for thread to finish
        if let Some(handle) = midi.sync_thread.lock().unwrap().take() {
            let _ = handle.join();
        }

        // Restore previous BPM
        if let Some(prev_bpm) = *midi.previous_bpm.lock().unwrap() {
            clock.set_bpm(prev_bpm);
        }

        Ok(Value::Bool(false))
    }
}

// ---------------------------------------------------------------------------
// OSC protocol builtins
// ---------------------------------------------------------------------------

// osc_config() → current target string or nil
// osc_config("host:port") → connect to target → bool
fn builtin_osc_config(args: &[Value], osc_protocol: &Arc<OscProtocolClient>) -> Result<Value> {
    if args.is_empty() {
        return match osc_protocol.get_target() {
            Some(t) => Ok(Value::String(t)),
            None => Ok(Value::Nil),
        };
    }

    let addr = require_string("osc_config", &args[0])?;
    let ok = osc_protocol.connect(&addr);
    if ok {
        println!("osc: target set to '{}'", addr);
    }
    Ok(Value::Bool(ok))
}

// osc_send("/address", arg1, arg2, ...) → bool
fn builtin_osc_send(args: &[Value], osc_protocol: &Arc<OscProtocolClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "osc_send() requires an OSC address as first argument".to_string(),
        });
    }
    let address = require_string("osc_send", &args[0])?;
    let osc_args: Vec<rosc::OscType> = args[1..]
        .iter()
        .map(|v| OscProtocolClient::value_to_osc(v))
        .collect();
    let ok = osc_protocol.send(&address, osc_args);
    Ok(Value::Bool(ok))
}

// osc_listen(port) → bool
// osc_listen(port, scsynth_addr) → bool  registers with scsynth for SendReply/SendTrig
fn builtin_osc_listen(args: &[Value], osc_protocol: &Arc<OscProtocolClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "osc_listen() requires a port number as first argument".to_string(),
        });
    }
    let port = require_number("osc_listen", &args[0])? as u16;
    let ok = osc_protocol.listen(port);
    if ok {
        println!("osc: listening on port {}", port);
        if let Some(Value::String(addr)) = args.get(1) {
            let notified = osc_protocol.notify_scsynth(addr);
            println!("osc: notify scsynth at {} — {}", addr, if notified { "ok" } else { "failed" });
        }
    }
    Ok(Value::Bool(ok))
}

// osc_recv() → array ["/address", arg1, arg2, ...] or nil
fn builtin_osc_recv(osc_protocol: &Arc<OscProtocolClient>) -> Result<Value> {
    match osc_protocol.recv() {
        Some((addr, args)) => {
            let mut arr = AudionArray::new();
            arr.push_auto(Value::String(addr));
            for arg in args {
                arr.push_auto(arg);
            }
            Ok(Value::Array(Arc::new(Mutex::new(arr))))
        }
        None => Ok(Value::Nil),
    }
}

// osc_close() → nil (close listener)
// osc_close("sender") → nil (close sender)
// osc_close("all") → nil (close both)
fn builtin_osc_close(args: &[Value], osc_protocol: &Arc<OscProtocolClient>) -> Result<Value> {
    if args.is_empty() {
        osc_protocol.close_listener();
        return Ok(Value::Nil);
    }

    match &args[0] {
        Value::String(s) if s == "sender" => {
            osc_protocol.close_sender();
        }
        Value::String(s) if s == "all" => {
            osc_protocol.close_listener();
            osc_protocol.close_sender();
        }
        _ => {
            osc_protocol.close_listener();
        }
    }
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Array cursor builtins
// ---------------------------------------------------------------------------

fn builtin_array_next(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_next() requires an array argument".to_string(),
        });
    }
    let arr = require_array("array_next", &args[0])?;
    let wrap = args.get(1).map_or(true, |v| v.is_truthy());
    let mut guard = arr.lock().unwrap();
    match guard.cursor_next(wrap) {
        Some((_, v)) => Ok(v.clone()),
        None => Ok(Value::Nil),
    }
}

fn builtin_array_prev(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_prev() requires an array argument".to_string(),
        });
    }
    let arr = require_array("array_prev", &args[0])?;
    let wrap = args.get(1).map_or(false, |v| v.is_truthy());
    let mut guard = arr.lock().unwrap();
    match guard.cursor_prev(wrap) {
        Some((_, v)) => Ok(v.clone()),
        None => Ok(Value::Nil),
    }
}

fn builtin_array_current(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_current() requires an array argument".to_string(),
        });
    }
    let arr = require_array("array_current", &args[0])?;
    let guard = arr.lock().unwrap();
    match guard.cursor_current() {
        Some((_, v)) => Ok(v.clone()),
        None => Ok(Value::Nil),
    }
}

fn builtin_array_end(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_end() requires an array argument".to_string(),
        });
    }
    let arr = require_array("array_end", &args[0])?;
    let mut guard = arr.lock().unwrap();
    match guard.cursor_end() {
        Some((_, v)) => Ok(v.clone()),
        None => Ok(Value::Nil),
    }
}

fn builtin_array_beginning(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_beginning() requires an array argument".to_string(),
        });
    }
    let arr = require_array("array_beginning", &args[0])?;
    let mut guard = arr.lock().unwrap();
    match guard.cursor_beginning() {
        Some((_, v)) => Ok(v.clone()),
        None => Ok(Value::Nil),
    }
}

fn builtin_array_key(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_key() requires an array argument".to_string(),
        });
    }
    let arr = require_array("array_key", &args[0])?;
    let guard = arr.lock().unwrap();
    match guard.cursor_key() {
        Some(k) => Ok(k.clone()),
        None => Ok(Value::Nil),
    }
}

fn is_integer(n: f64) -> bool {
    n.is_finite() && n == (n as i64) as f64
}

fn require_number(fn_name: &str, val: &Value) -> Result<f64> {
    match val {
        Value::Number(n) => Ok(*n),
        other => Err(AudionError::RuntimeError {
            msg: format!("{}() expected number, got {}", fn_name, other.type_name()),
        }),
    }
}

// ---------------------------------------------------------------------------
// String functions — str_explode, str_join
// ---------------------------------------------------------------------------

/// Check if a delimiter string uses regex delimiters: /pattern/, {pattern}, %%pattern%%
pub(crate) fn extract_regex_pattern(s: &str) -> Option<&str> {
    if s.len() >= 2 && s.starts_with('/') && s.ends_with('/') {
        Some(&s[1..s.len() - 1])
    } else if s.len() >= 2 && s.starts_with('{') && s.ends_with('}') {
        Some(&s[1..s.len() - 1])
    } else if s.len() >= 4 && s.starts_with("%%") && s.ends_with("%%") {
        Some(&s[2..s.len() - 2])
    } else {
        None
    }
}

fn builtin_str_explode(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "str_explode() requires 2 arguments: str_explode(delimiter, string)".to_string(),
        });
    }
    let delimiter = match &args[0] {
        Value::String(s) => s.clone(),
        other => {
            return Err(AudionError::RuntimeError {
                msg: format!(
                    "str_explode() first argument must be string, got {}",
                    other.type_name()
                ),
            })
        }
    };
    let string = match &args[1] {
        Value::String(s) => s.clone(),
        other => {
            return Err(AudionError::RuntimeError {
                msg: format!(
                    "str_explode() second argument must be string, got {}",
                    other.type_name()
                ),
            })
        }
    };

    let parts: Vec<&str> = if let Some(pattern) = extract_regex_pattern(&delimiter) {
        let re = regex::Regex::new(pattern).map_err(|e| AudionError::RuntimeError {
            msg: format!("str_explode() invalid regex: {}", e),
        })?;
        re.split(&string).collect()
    } else {
        string.split(&*delimiter).collect()
    };

    let mut arr = AudionArray::new();
    for part in parts {
        arr.push_auto(Value::String(part.to_string()));
    }
    Ok(Value::Array(std::sync::Arc::new(std::sync::Mutex::new(arr))))
}

fn builtin_str_join(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "str_join() requires 2 arguments: str_join(glue, array)".to_string(),
        });
    }
    let glue = match &args[0] {
        Value::String(s) => s.clone(),
        other => {
            return Err(AudionError::RuntimeError {
                msg: format!(
                    "str_join() first argument must be string, got {}",
                    other.type_name()
                ),
            })
        }
    };
    let arr = match &args[1] {
        Value::Array(a) => a.clone(),
        other => {
            return Err(AudionError::RuntimeError {
                msg: format!(
                    "str_join() second argument must be array, got {}",
                    other.type_name()
                ),
            })
        }
    };

    let guard = arr.lock().unwrap();
    let parts: Vec<String> = guard.entries().iter().map(|(_, v)| format!("{}", v)).collect();
    Ok(Value::String(parts.join(&glue)))
}

// ---------------------------------------------------------------------------
// Date/Time functions — date, timestamp, timestamp_ms
// ---------------------------------------------------------------------------

fn builtin_date(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "date() requires a format string argument".to_string(),
        });
    }
    let fmt = require_string("date", &args[0])?;
    let now = if args.len() >= 2 {
        match &args[1] {
            Value::Number(ts) => chrono::DateTime::from_timestamp(*ts as i64, 0)
                .ok_or_else(|| AudionError::RuntimeError {
                    msg: format!("date() invalid timestamp: {}", ts),
                })?
                .with_timezone(&chrono::Local),
            other => return Err(AudionError::RuntimeError {
                msg: format!("date() second argument must be a number (unix timestamp), got {}", other.type_name()),
            }),
        }
    } else {
        chrono::Local::now()
    };

    let mut result = String::new();
    for ch in fmt.chars() {
        match ch {
            'Y' => result.push_str(&now.format("%Y").to_string()),
            'm' => result.push_str(&now.format("%m").to_string()),
            'd' => result.push_str(&now.format("%d").to_string()),
            'H' => result.push_str(&now.format("%H").to_string()),
            'i' => result.push_str(&now.format("%M").to_string()),
            's' => result.push_str(&now.format("%S").to_string()),
            'N' => result.push_str(&now.format("%u").to_string()),
            'j' => result.push_str(&now.format("%-d").to_string()),
            'n' => result.push_str(&now.format("%-m").to_string()),
            'G' => result.push_str(&now.format("%-H").to_string()),
            'A' => result.push_str(&now.format("%p").to_string()),
            'g' => result.push_str(&now.format("%-I").to_string()),
            'U' => result.push_str(&now.timestamp().to_string()),
            other => result.push(other),
        }
    }
    Ok(Value::String(result))
}

fn builtin_timestamp(_args: &[Value]) -> Result<Value> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    Ok(Value::Number(secs as f64))
}

fn builtin_timestamp_ms(_args: &[Value]) -> Result<Value> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    Ok(Value::Number(ms as f64))
}

// ---------------------------------------------------------------------------
// Type casts — int, float, bool, str
// ---------------------------------------------------------------------------

fn parse_int_string(s: &str) -> f64 {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        let clean: String = hex.chars().filter(|&c| c != '_').collect();
        return u64::from_str_radix(&clean, 16).unwrap_or(0) as f64;
    }
    if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        let clean: String = bin.chars().filter(|&c| c != '_').collect();
        return u64::from_str_radix(&clean, 2).unwrap_or(0) as f64;
    }
    if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        let clean: String = oct.chars().filter(|&c| c != '_').collect();
        return u64::from_str_radix(&clean, 8).unwrap_or(0) as f64;
    }
    let clean: String = s.chars().filter(|&c| c != '_').collect();
    clean.parse::<f64>().unwrap_or(0.0).trunc()
}

fn builtin_int(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "int() requires an argument".to_string(),
        });
    }
    let result = match &args[0] {
        Value::Number(n) => n.trunc(),
        Value::String(s) => parse_int_string(s),
        Value::Bool(b) => if *b { 1.0 } else { 0.0 },
        Value::Nil => 0.0,
        other => return Err(AudionError::RuntimeError {
            msg: format!("int() cannot convert {} to int", other.type_name()),
        }),
    };
    Ok(Value::Number(result))
}

fn builtin_float(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "float() requires an argument".to_string(),
        });
    }
    let result = match &args[0] {
        Value::Number(n) => *n,
        Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        Value::Bool(b) => if *b { 1.0 } else { 0.0 },
        Value::Nil => 0.0,
        other => return Err(AudionError::RuntimeError {
            msg: format!("float() cannot convert {} to float", other.type_name()),
        }),
    };
    Ok(Value::Number(result))
}

fn builtin_bool(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "bool() requires an argument".to_string(),
        });
    }
    Ok(Value::Bool(args[0].is_truthy()))
}

fn builtin_str(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "str() requires an argument".to_string(),
        });
    }
    Ok(Value::String(args[0].to_string()))
}

// hex(n)         → "ff"       lowercase hex string, no prefix
// hex(n, width)  → "00ff"     zero-padded to width characters
fn builtin_hex(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "hex() requires a number argument".to_string(),
        });
    }
    let n = require_number("hex", &args[0])? as u64;
    let width = if args.len() > 1 {
        require_number("hex", &args[1])? as usize
    } else {
        0
    };
    let s = if width > 0 {
        format!("{:0>width$x}", n, width = width)
    } else {
        format!("{:x}", n)
    };
    Ok(Value::String(s))
}

// bin(n)         → "1010"     binary string, no prefix
// bin(n, width)  → "00001010" zero-padded to width characters
fn builtin_bin(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "bin() requires a number argument".to_string(),
        });
    }
    let n = require_number("bin", &args[0])? as u64;
    let width = if args.len() > 1 {
        require_number("bin", &args[1])? as usize
    } else {
        0
    };
    let s = if width > 0 {
        format!("{:0>width$b}", n, width = width)
    } else {
        format!("{:b}", n)
    };
    Ok(Value::String(s))
}

// oct(n)         → "377"
// oct(n, width)  → "0377"     zero-padded
fn builtin_oct(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "oct() requires a number argument".to_string(),
        });
    }
    let n = require_number("oct", &args[0])? as u64;
    let width = if args.len() > 1 {
        require_number("oct", &args[1])? as usize
    } else {
        0
    };
    let s = if width > 0 {
        format!("{:0>width$o}", n, width = width)
    } else {
        format!("{:o}", n)
    };
    Ok(Value::String(s))
}

// ---------------------------------------------------------------------------
// Exec — run external commands
// ---------------------------------------------------------------------------

fn builtin_exec(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "exec() requires a command as first argument".to_string(),
        });
    }
    let command = require_string("exec", &args[0])?;
    let cmd_args: Vec<String> = args[1..].iter().map(|a| a.to_string()).collect();

    let output = std::process::Command::new(&command)
        .args(&cmd_args)
        .output();

    match output {
        Ok(out) => {
            let mut result = AudionArray::new();
            result.set(
                Value::String("stdout".to_string()),
                Value::String(String::from_utf8_lossy(&out.stdout).to_string()),
            );
            result.set(
                Value::String("stderr".to_string()),
                Value::String(String::from_utf8_lossy(&out.stderr).to_string()),
            );
            result.set(
                Value::String("status".to_string()),
                Value::Number(out.status.code().unwrap_or(-1) as f64),
            );
            Ok(Value::Array(Arc::new(Mutex::new(result))))
        }
        Err(_) => Ok(Value::Bool(false)),
    }
}

// ---------------------------------------------------------------------------
// Hash — md5, sha256, sha512
// ---------------------------------------------------------------------------

fn builtin_hash(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "hash() requires 2 arguments: hash(algorithm, data)".to_string(),
        });
    }
    let algorithm = require_string("hash", &args[0])?;
    let data = args[1].to_string();

    let hex = match algorithm.as_str() {
        "md5" => {
            let mut hasher = md5::Md5::new();
            hasher.update(data.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        "sha256" => {
            let mut hasher = sha2::Sha256::new();
            hasher.update(data.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        "sha512" => {
            let mut hasher = sha2::Sha512::new();
            hasher.update(data.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        other => {
            return Err(AudionError::RuntimeError {
                msg: format!("hash() unsupported algorithm '{}'. Use: md5, sha256, sha512", other),
            });
        }
    };
    Ok(Value::String(hex))
}

// ---------------------------------------------------------------------------
// Console I/O functions
// ---------------------------------------------------------------------------

fn builtin_console_read() -> Result<Value> {
    use std::io::{self, BufRead};
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line).map_err(|e| {
        AudionError::RuntimeError {
            msg: format!("console_read() failed: {}", e),
        }
    })?;
    // Remove trailing newline
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    Ok(Value::String(line))
}

fn builtin_console_read_password(args: &[Value]) -> Result<Value> {
    let prompt = if args.is_empty() {
        ""
    } else {
        &require_string("console_read_password", &args[0])?
    };

    let password = if prompt.is_empty() {
        rpassword::read_password().map_err(|e| {
            AudionError::RuntimeError {
                msg: format!("console_read_password() failed: {}", e),
            }
        })?
    } else {
        rpassword::prompt_password(prompt).map_err(|e| {
            AudionError::RuntimeError {
                msg: format!("console_read_password() failed: {}", e),
            }
        })?
    };

    Ok(Value::String(password))
}

fn builtin_console_read_key() -> Result<Value> {
    use crossterm::{
        event::{self, Event, KeyCode},
        terminal::{disable_raw_mode, enable_raw_mode},
    };

    enable_raw_mode().map_err(|e| {
        AudionError::RuntimeError {
            msg: format!("console_read_key() failed to enable raw mode: {}", e),
        }
    })?;

    let result = loop {
        match event::read() {
            Ok(Event::Key(key_event)) => {
                let key_str = match key_event.code {
                    KeyCode::Char(c) => c.to_string(),
                    KeyCode::Enter => "\n".to_string(),
                    KeyCode::Tab => "\t".to_string(),
                    KeyCode::Backspace => "\x08".to_string(),
                    KeyCode::Delete => "\x7f".to_string(),
                    KeyCode::Esc => "\x1b".to_string(),
                    KeyCode::Up => "UP".to_string(),
                    KeyCode::Down => "DOWN".to_string(),
                    KeyCode::Left => "LEFT".to_string(),
                    KeyCode::Right => "RIGHT".to_string(),
                    KeyCode::Home => "HOME".to_string(),
                    KeyCode::End => "END".to_string(),
                    KeyCode::PageUp => "PAGEUP".to_string(),
                    KeyCode::PageDown => "PAGEDOWN".to_string(),
                    KeyCode::F(n) => format!("F{}", n),
                    _ => continue,
                };
                break Ok(Value::String(key_str));
            }
            Ok(_) => continue,
            Err(e) => {
                break Err(AudionError::RuntimeError {
                    msg: format!("console_read_key() failed: {}", e),
                });
            }
        }
    };

    let _ = disable_raw_mode();
    result
}

fn builtin_console_error(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    eprintln!("{}", parts.join(" "));
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// OS Environment functions
// ---------------------------------------------------------------------------

fn builtin_os_env_get(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "os_env_get() requires a variable name".to_string(),
        });
    }
    let name = require_string("os_env_get", &args[0])?;
    match std::env::var(&name) {
        Ok(val) => Ok(Value::String(val)),
        Err(_) => Ok(Value::Nil),
    }
}

fn builtin_os_env_set(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "os_env_set() requires name and value".to_string(),
        });
    }
    let name = require_string("os_env_set", &args[0])?;
    let value = require_string("os_env_set", &args[1])?;
    std::env::set_var(&name, &value);
    Ok(Value::Nil)
}

fn builtin_os_env_list() -> Result<Value> {
    let mut arr = AudionArray::new();
    for (key, value) in std::env::vars() {
        arr.set(Value::String(key), Value::String(value));
    }
    Ok(Value::Array(Arc::new(Mutex::new(arr))))
}

// ---------------------------------------------------------------------------
// OS Process functions
// ---------------------------------------------------------------------------

fn builtin_os_process_id() -> Result<Value> {
    Ok(Value::Number(std::process::id() as f64))
}

fn builtin_os_process_parent_id() -> Result<Value> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, false);

    let pid = sysinfo::Pid::from_u32(std::process::id());

    if let Some(process) = sys.process(pid) {
        if let Some(parent_pid) = process.parent() {
            return Ok(Value::Number(parent_pid.as_u32() as f64));
        }
    }

    Ok(Value::Nil)
}

fn builtin_os_exit(args: &[Value]) -> Result<Value> {
    let code = if args.is_empty() {
        0
    } else {
        require_number("os_exit", &args[0])? as i32
    };
    std::process::exit(code);
}

// ---------------------------------------------------------------------------
// OS Directory functions
// ---------------------------------------------------------------------------

fn builtin_os_cwd() -> Result<Value> {
    match std::env::current_dir() {
        Ok(path) => Ok(Value::String(path.to_string_lossy().to_string())),
        Err(e) => Err(AudionError::RuntimeError {
            msg: format!("os_cwd() failed: {}", e),
        }),
    }
}

fn builtin_os_chdir(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "os_chdir() requires a path".to_string(),
        });
    }
    let path = require_string("os_chdir", &args[0])?;
    std::env::set_current_dir(&path).map_err(|e| {
        AudionError::RuntimeError {
            msg: format!("os_chdir() failed: {}", e),
        }
    })?;
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// OS Arguments function
// ---------------------------------------------------------------------------

fn builtin_os_arguments(env: &Arc<Mutex<crate::environment::Environment>>) -> Result<Value> {
    // Try to get the __ARGS__ variable from the environment
    match env.lock().unwrap().get("__ARGS__") {
        Some(val) => Ok(val),
        None => {
            // Return empty array if no args were set
            Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))))
        }
    }
}

// ---------------------------------------------------------------------------
// OS System Info functions
// ---------------------------------------------------------------------------

fn builtin_os_name() -> Result<Value> {
    Ok(Value::String(std::env::consts::OS.to_string()))
}

fn builtin_os_hostname() -> Result<Value> {
    match hostname::get() {
        Ok(name) => Ok(Value::String(name.to_string_lossy().to_string())),
        Err(e) => Err(AudionError::RuntimeError {
            msg: format!("os_hostname() failed: {}", e),
        }),
    }
}

fn builtin_os_username() -> Result<Value> {
    Ok(Value::String(whoami::username()))
}

fn builtin_os_home() -> Result<Value> {
    match dirs::home_dir() {
        Some(path) => Ok(Value::String(path.to_string_lossy().to_string())),
        None => Ok(Value::Nil),
    }
}

// ---------------------------------------------------------------------------
// eval() - Execute Audion code from a string
// ---------------------------------------------------------------------------

// Note: This is a helper function that needs access to Interpreter components.
// It will be properly implemented by passing the necessary context.
fn builtin_eval(args: &[Value], env: &Arc<Mutex<crate::environment::Environment>>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "eval() requires a code string".to_string(),
        });
    }
    let _code = require_string("eval", &args[0])?;

    // eval() requires special handling in the interpreter
    // This is a placeholder - the actual implementation is in interpreter.rs
    Err(AudionError::RuntimeError {
        msg: "eval() is not yet fully implemented".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Ableton Link builtins
// ---------------------------------------------------------------------------

fn builtin_link_enable(args: &[Value], clock: &Arc<Clock>) -> Result<Value> {
    if args.is_empty() || matches!(&args[0], Value::Bool(true)) {
        clock.link_enable();
        Ok(Value::Bool(true))
    } else {
        clock.link_disable();
        Ok(Value::Bool(false))
    }
}

fn builtin_link_disable(clock: &Arc<Clock>) -> Result<Value> {
    clock.link_disable();
    Ok(Value::Nil)
}

fn builtin_link_is_enabled(clock: &Arc<Clock>) -> Result<Value> {
    Ok(Value::Bool(clock.link_is_enabled()))
}

fn builtin_link_peers(clock: &Arc<Clock>) -> Result<Value> {
    Ok(Value::Number(clock.link_num_peers() as f64))
}

fn builtin_link_beat(clock: &Arc<Clock>) -> Result<Value> {
    Ok(Value::Number(clock.link_beat()))
}

fn builtin_link_phase(clock: &Arc<Clock>) -> Result<Value> {
    Ok(Value::Number(clock.link_phase()))
}

fn builtin_link_quantum(args: &[Value], clock: &Arc<Clock>) -> Result<Value> {
    if args.is_empty() {
        Ok(Value::Number(clock.get_quantum()))
    } else {
        match &args[0] {
            Value::Number(n) => {
                clock.set_quantum(*n);
                Ok(Value::Number(*n))
            }
            _ => Err(AudionError::RuntimeError {
                msg: "link_quantum() expects a number".to_string(),
            }),
        }
    }
}

fn builtin_link_play(clock: &Arc<Clock>) -> Result<Value> {
    clock.link_play();
    Ok(Value::Nil)
}

fn builtin_link_stop(clock: &Arc<Clock>) -> Result<Value> {
    clock.link_stop();
    Ok(Value::Nil)
}

fn builtin_link_is_playing(clock: &Arc<Clock>) -> Result<Value> {
    Ok(Value::Bool(clock.link_is_playing()))
}

fn builtin_link_request_beat(args: &[Value], clock: &Arc<Clock>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "link_request_beat() requires a beat number".to_string(),
        });
    }
    match &args[0] {
        Value::Number(n) => {
            clock.link_request_beat(*n);
            Ok(Value::Nil)
        }
        _ => Err(AudionError::RuntimeError {
            msg: "link_request_beat() expects a number".to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// DMX builtins (Art-Net over UDP)
// ---------------------------------------------------------------------------

// dmx_connect(host) → bool          connect to host:6454, universe 0
// dmx_connect(host, port) → bool    connect to host:port
fn builtin_dmx_connect(args: &[Value], dmx: &Arc<DmxClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "dmx_connect() requires a host argument".to_string(),
        });
    }
    let host = match &args[0] {
        Value::String(s) => s.clone(),
        other => {
            return Err(AudionError::RuntimeError {
                msg: format!("dmx_connect() expected string host, got {}", other.type_name()),
            })
        }
    };
    let port = if args.len() > 1 {
        require_number("dmx_connect", &args[1])? as u16
    } else {
        6454
    };
    Ok(Value::Bool(dmx.connect(&host, port)))
}

// dmx_universe(n) — set Art-Net universe (0-based)
fn builtin_dmx_universe(args: &[Value], dmx: &Arc<DmxClient>) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "dmx_universe() requires a universe number".to_string(),
        });
    }
    let u = require_number("dmx_universe", &args[0])? as u16;
    dmx.set_universe(u);
    Ok(Value::Nil)
}

// dmx_set(channel, value) — channel 1–512, value 0–255
fn builtin_dmx_set(args: &[Value], dmx: &Arc<DmxClient>) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "dmx_set() requires 2 arguments: dmx_set(channel, value)".to_string(),
        });
    }
    let channel = require_number("dmx_set", &args[0])? as usize;
    let value = require_number("dmx_set", &args[1])? as u8;
    if channel < 1 || channel > 512 {
        return Err(AudionError::RuntimeError {
            msg: format!("dmx_set() channel must be 1–512, got {}", channel),
        });
    }
    dmx.set_channel(channel - 1, value);
    Ok(Value::Nil)
}

// dmx_set_range(start, array) — set channels starting at start (1-indexed)
fn builtin_dmx_set_range(args: &[Value], dmx: &Arc<DmxClient>) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "dmx_set_range() requires 2 arguments: dmx_set_range(start_channel, values)".to_string(),
        });
    }
    let start = require_number("dmx_set_range", &args[0])? as usize;
    if start < 1 || start > 512 {
        return Err(AudionError::RuntimeError {
            msg: format!("dmx_set_range() start channel must be 1–512, got {}", start),
        });
    }
    match &args[1] {
        Value::Array(arr) => {
            let arr_guard = arr.lock().unwrap();
            let values: Vec<u8> = arr_guard
                .entries()
                .iter()
                .map(|(_, v)| match v {
                    Value::Number(n) => *n as u8,
                    _ => 0,
                })
                .collect();
            drop(arr_guard);
            dmx.set_range(start - 1, &values);
            Ok(Value::Nil)
        }
        other => Err(AudionError::RuntimeError {
            msg: format!("dmx_set_range() expected array of values, got {}", other.type_name()),
        }),
    }
}

// dmx_send() → bool — transmit current channel buffer
fn builtin_dmx_send(dmx: &Arc<DmxClient>) -> Result<Value> {
    Ok(Value::Bool(dmx.send()))
}

// dmx_blackout() — zero all channels and transmit
fn builtin_dmx_blackout(dmx: &Arc<DmxClient>) -> Result<Value> {
    dmx.blackout();
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// assert(condition, message, stop_execution)
// ---------------------------------------------------------------------------

fn builtin_assert(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "assert() requires at least one argument".to_string(),
        });
    }

    let condition = args[0].is_truthy();

    let message: Option<String> = args.get(1).and_then(|v| match v {
        Value::Nil => None,
        Value::String(s) => Some(s.clone()),
        other => Some(format!("{}", other)),
    });

    let stop_execution = args.get(2).map(|v| v.is_truthy()).unwrap_or(false);

    if condition {
        ASSERT_PASS.fetch_add(1, Ordering::Relaxed);
    } else {
        ASSERT_FAIL.fetch_add(1, Ordering::Relaxed);
        if let Some(msg) = message {
            eprintln!("assert failed: {}", msg);
        } else {
            eprintln!("assert failed");
        }
        if stop_execution {
            let _ = print_assert_stats();
            std::process::exit(1);
        }
    }

    Ok(Value::Nil)
}

/// Separate positional and named arguments from a mixed list of Arg
pub fn split_args(
    raw_args: &[(Value, Option<String>)],
) -> (Vec<Value>, Vec<(String, Value)>) {
    let mut positional = Vec::new();
    let mut named = Vec::new();
    for (val, name) in raw_args {
        if let Some(n) = name {
            named.push((n.clone(), val.clone()));
        } else {
            positional.push(val.clone());
        }
    }
    (positional, named)
}
