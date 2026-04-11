# Audion System Information — Internal Reference

## File Map

```
src/main.rs          CLI entry. clap derive. `audion run file.au` or bare `audion` → REPL.
src/lib.rs           Module re-exports only.
src/token.rs         TokenKind enum + Token struct + Span{start,end,line}. Keywords: Fn, Let, If, Else, While, Loop, For, Return, Thread, Define, Break, Continue, This, Include, As, Using.
src/lexer.rs         Lexer struct: source as Vec<char>, pos, line. Single-pass hand-written.
src/ast.rs           Expr/Stmt/Arg/BinOp/UnaryOp enums. All derive Clone+Debug.
src/parser.rs        Parser struct: tokens Vec<Token>, pos. Recursive descent.
src/value.rs         Value enum (runtime). AudionArray struct (ordered hash map with cursor). Display, PartialEq impls.
src/environment.rs   Environment struct: HashMap<String,Value> + parent chain via Arc<Mutex>.
src/interpreter.rs   Interpreter struct + ControlFlow enum. Tree-walker.
src/builtins.rs      BUILTIN_NAMES constant + call_builtin() dispatch + inline builtin implementations. Thread-local PRNG (xorshift64). Global NetHandle store for TCP.
src/math.rs          math_* builtin implementations (separate module, pub fn pattern).
src/strings.rs       str_* builtin implementations (separate module, same pattern as math.rs).
src/midi.rs          MidiClient struct. midir crate. Mutex<Option<MidiOutputConnection>>. Thread-safe MIDI output.
src/osc.rs           OscClient struct (scsynth-specific). UDP socket + rosc. Node + buffer ID tracking.
src/osc_protocol.rs  OscProtocolClient struct (generic OSC). UDP send/recv via osc_config/osc_send/osc_listen/osc_recv.
src/sclang.rs        SuperCollider language helpers (scala scale parsing, etc).
src/sampler.rs       detect_channels() for WAV header reading. Channel count detection.
src/clock.rs         Clock struct. AtomicU64 for BPM, Instant for elapsed.
src/repl.rs          run_repl(). rustyline. Multi-line brace detection.
src/error.rs         AudionError enum: LexError, ParseError, RuntimeError. type Result<T>.
```

## Pipeline

```
source &str → Lexer::tokenize() → Vec<Token>
           → Parser::parse()    → Vec<Stmt>
           → Interpreter::run() or ::run_line() → Value
```

## Two Execution Paths

**File mode** (`run_file` in main.rs): lex → parse → `interp.run(&stmts)`.
- `run()` executes all top-level stmts, then checks env for `"main"` Function and calls it via `call_function("main", &[], &[])`.
- After main() returns, calls `join_threads()` which does `std::mem::take` on thread_handles HashMap and joins all.

**REPL mode** (`run_repl` in repl.rs): each line goes through `run_source()` → lex → parse → `interp.run_line(&stmts)`.
- `run_line()` does NOT auto-call main(). Does NOT join threads. Threads run in background.
- Returns last ExprStmt value (re-evaluates the expr — **known quirk**: expr is evaluated twice in run_line, once in exec_stmt and once to capture return value).

## Interpreter Internals

**Struct fields** (all pub except thread_handles, included_files, included_envs):
- `env: Arc<Mutex<Environment>>` — current scope
- `osc: Arc<OscClient>`
- `midi: Arc<MidiClient>`
- `osc_protocol: Arc<OscProtocolClient>`
- `clock: Arc<Clock>`
- `shutdown: Arc<AtomicBool>`
- `thread_handles: HashMap<String, JoinHandle<()>>` — only on parent interpreter
- `included_files: Arc<Mutex<HashSet<PathBuf>>>` — include-once tracking
- `included_envs: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<Environment>>>>>` — cached environments for included files (allows re-aliasing same file)
- `synthdef_cache: SynthDefCache` — in-memory cache of compiled SynthDefs (watch mode only)

**ControlFlow enum** (private to interpreter.rs):
- `None` — continue normally
- `Break` / `Continue` — loop control, propagated up through exec_stmt
- `Return(Value)` — propagated up until caught by function call

**Scope management pattern** — used in Block, For, and function calls:
```rust
let old_env = std::mem::replace(&mut self.env, new_child_env);
// ... execute body ...
self.env = old_env;  // restore
```
Critical: For loop restores env even on Return or shutdown. Block does too.

**Include & Namespace system** (`exec_include`, `install_namespace`, `exec_using`):
- `include "path" [as alias];` → `Stmt::Include { path: String, alias: Option<Vec<String>> }`
- `using ns::path;` → `Stmt::Using { path: Vec<String> }`
- `path_to_namespace_segments(path)`: strips `.au` extension, splits on `/` via `std::path::Path::components()`, keeps only `Component::Normal` (skips `/`, `.`, `..`). e.g. `"lib/math/utils.au"` → `["lib", "math", "utils"]`.
- `install_namespace(segments, leaf_env)`: walks segments creating intermediate `Value::Namespace(Environment::new())` at each level. Uses get-or-create pattern — if intermediate already exists as Namespace, reuses it. Leaf gets the included file's environment.
- `exec_using(path)`: resolves namespace path via `resolve_namespace_path()`, then copies all local `values()` (HashMap entries, not parent-inherited) into current scope via `define()`. Later `using` overwrites same-named bindings.
- Include-once: `included_files` HashSet prevents re-execution, but `included_envs` HashMap caches the environment so re-aliasing (`include "same.au" as different_name;`) still installs the namespace.

**Function calls** (in eval_expr Expr::Call match arm):
1. Evaluate callee to get Value
2. Evaluate all args into `Vec<(Value, Option<String>)>` — positional have None name, named have Some
3. `builtins::split_args()` separates into `(Vec<Value>, Vec<(String, Value)>)`
4. If BuiltinFn → `builtins::call_builtin()`
5. If Function → check param count against positional.len(), create child env from **closure** (not current env), bind params, swap env, exec body, restore env
6. Non-callable → RuntimeError

**Thread spawning** (`exec_thread`):
- Creates child env from `self.env.clone()` (current scope, not closure)
- Clones body, osc, clock, shutdown
- Spawns via `std::thread::Builder::new().name(name).spawn()`
- Thread gets its own Interpreter via `new_for_thread()` (does NOT register builtins — they're inherited from parent env via scope chain)
- Thread errors → eprintln, don't propagate
- Handle stored in `thread_handles` map by name (overwrites if same name used twice!)

**Assignment semantics** (Expr::Assign):
- Tries `env.set()` which walks parent chain looking for existing binding
- If set() returns false (not found anywhere), defines in current scope
- This means bare `x = 5;` without `let` creates a variable. **Not an error.**

**Shutdown check**: every loop iteration (while, loop, for) checks `shutdown.load(Relaxed)`. Returns `ControlFlow::Return(Nil)` to unwind. Also checked at top of `exec_stmt`.

## Environment

- `get()`: check local HashMap, then recursively lock parent. Returns cloned Value.
- `set()`: if key exists locally → update. Else recursively try parent. Returns bool.
- `define()`: always inserts into local HashMap (shadows parent).
- **Lock granularity**: each get/set/define acquires lock, does work, releases. No held locks across expressions. Deadlock-safe because lock is never held while calling another lock on the same env (parent is a different Arc).
- **Potential issue**: lock-unlock-lock pattern in Assign means another thread could interleave between the set() and define() calls.

## Value Details

- Number: always f64. Display: shows as int if `n == (n as i64) as f64 && n.is_finite()`.
- Function: stores `body: Stmt` (cloned AST), `closure: Arc<Mutex<Environment>>`.
- BuiltinFn: just a String name. Resolved at call time via `call_builtin()`.
- Array: `Arc<Mutex<Vec<(Value, Value)>>>`. Ordered key-value pairs. Keys are Number or String. Deep-cloned on variable assignment (`deep_clone()`). Display: `[key => value, ...]`.
- PartialEq: Number==Number, String==String, Bool==Bool, Nil==Nil, Array element-by-element. Cross-type always false. Functions always false.

## Builtins Architecture

**Signature**: `call_builtin(name, positional: &[Value], named: &[(String, Value)], osc, midi, osc_protocol, clock)`

**BUILTIN_NAMES** (`src/builtins.rs`): a `&[&str]` constant that is the single source of truth for all builtin function names. `Interpreter::new()` iterates this to register every builtin in the environment. No manual registration needed.

**Builtin families** (grouped by prefix):
- Core: `print`, `bpm`, `wait`, `wait_ms`, `time`, `rand`, `seed`, `eval`
- Type casts: `int`, `float`, `bool`, `str`
- Date/time: `date`, `timestamp`, `timestamp_ms`
- Arrays: `count`, `push`, `pop`, `keys`, `has_key`, `remove`, `array_rand`, `array_push`, `array_pop`, `array_beginning`, `array_end`, `array_next`, `array_prev`, `array_current`, `array_key`
- Strings (`src/strings.rs`): `str_explode`, `str_join`, `str_replace`, `str_contains`, `str_upper`, `str_lower`, `str_trim`, `str_length`, `str_substr`, `str_starts_with`, `str_ends_with`
- Math (`src/math.rs`): `math_abs`, `math_ceil`, `math_floor`, `math_round`, `math_sin`, `math_cos`, `math_tan`, `math_pow`, `math_sqrt`, `math_log`, `math_pi`, `math_e`, `math_clamp`, `math_lerp`, `math_map`, etc.
- File I/O: `file_read`, `file_write`, `file_append`, `file_exists`, `file_delete`
- Directory I/O: `dir_scan`, `dir_exists`, `dir_create`, `dir_delete`
- JSON: `json_encode`, `json_decode`
- Networking TCP: `net_connect`, `net_listen`, `net_accept`, `net_read`, `net_write`, `net_close`, `net_http`
- Networking UDP: `net_udp_bind`, `net_udp_send`, `net_udp_recv`
- OSC protocol: `osc_config`, `osc_send`, `osc_listen`, `osc_recv`, `osc_close`
- SuperCollider: `synth`, `free`, `set`, `mtof`, `ftom`, `buffer_load`, `buffer_free`, `buffer_alloc`, `buffer_read`, `buffer_stream_open`, `buffer_stream_close`
- MIDI: `midi_config`, `midi_note`, `midi_cc`, `midi_program`, `midi_out`, `midi_clock`, `midi_start`, `midi_stop`, `midi_panic`
- Console I/O: `console_read`, `console_read_password`, `console_read_key`, `console_error`
- OS Environment: `os_env_get`, `os_env_set`, `os_env_list`
- OS Process: `os_process_id`/`os_pid`, `os_process_parent_id`/`os_ppid`, `os_exit`
- OS Directory: `os_current_working_directory`/`os_cwd`, `os_current_working_directory_change`/`os_chdir`, `os_arguments`/`os_args`
- OS System Info: `os_name`, `os_hostname`, `os_username`, `os_home`
- System: `exec`, `hash`

**PRNG** (`random_f64()`): xorshift64 is always used. When `seed()` is active, uses the user-provided seed (thread-local `RefCell<Option<u64>>`). When unseeded, auto-seeds from `SystemTime::as_nanos()` on first call per thread, then advances the xorshift state on each subsequent call (no syscalls, fast). `seed(String)` hashes via FNV-1a. `seed(Number)` uses value directly (OR'd with 1 to avoid zero state). `seed(false)` clears. `array_rand()` uses the same `random_f64()` path.

**Regex support**: `str_explode`, `str_replace`, and `str_contains` support regex patterns via `/pattern/`, `{pattern}`, or `%%pattern%%` delimiters. Parsing handled by `extract_regex_pattern()` (pub(crate) in builtins.rs, shared with strings.rs).

**split_args()**: takes `&[(Value, Option<String>)]`, returns `(Vec<Value>, Vec<(String, Value)>)`. Called in interpreter before every function call. Named args to user functions are currently **silently ignored** — only positional args are bound to params.

## Console I/O Builtins

- `console_read()` — read a line from stdin (blocking), returns string with newline stripped
- `console_read_password(prompt?)` — read password without echo, optional prompt string
- `console_read_key()` — read single keypress (blocking), returns string (char, "UP", "DOWN", "F1", etc.)
- `console_error(msg...)` — write to stderr (like `print()` but to stderr)

Dependencies: `rpassword`, `crossterm`

## OS Builtins

**Environment:**
- `os_env_get(name)` — get environment variable, returns string or nil if not set
- `os_env_set(name, value)` — set environment variable
- `os_env_list()` — return all environment variables as key=>value array

**Process:**
- `os_process_id()` / `os_pid()` — return current process ID as number
- `os_process_parent_id()` / `os_ppid()` — return parent process ID as number (or nil if unavailable)
- `os_exit(code?)` — exit process with optional exit code (default 0)

**Directory:**
- `os_current_working_directory()` / `os_cwd()` — return current working directory as string
- `os_current_working_directory_change(path)` / `os_chdir(path)` — change current working directory
- `os_arguments()` / `os_args()` — return command-line arguments as array (stored in `__ARGS__` env var)

**System Info:**
- `os_name()` — return OS name: "linux", "macos", or "windows"
- `os_hostname()` — return hostname as string
- `os_username()` — return current username as string
- `os_home()` — return home directory path as string (or nil if unavailable)

Dependencies: `sysinfo`, `hostname`, `whoami`, `dirs`

Command-line arguments are stored in the environment as `__ARGS__` array during interpreter initialization via `Interpreter::set_args()`.

## eval() Builtin

`eval(code_string)` — execute Audion code from a string and return the last expression value.

**Special handling:** Unlike other builtins, `eval()` is intercepted in the interpreter's function call handling (before `call_builtin()`) because it needs access to the interpreter's execution methods (`exec_stmt`, `eval_expr`). Two call sites handle it:
1. `Expr::Call` match arm in `eval_expr` (line ~710)
2. `Some(Value::BuiltinFn)` match arm in `call_function` (line ~1070)

**Implementation:** Creates a Lexer and Parser to tokenize and parse the code string, then executes statements in the current environment scope (full access to variables). Returns the last expression value like `run_line()`.

**Scope:** Executes in current scope, can read and modify variables.

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

**Precedence** (low to high): `||` → `&&` → `|` → `^` → `&` → equality → comparison → bit shifts → addition → multiplication → unary

**Implementation:**
- Tokens: `Ampersand`, `Pipe`, `Caret`, `Tilde`, `LtLt`, `GtGt` (src/token.rs)
- AST: `BinOp::{BitAnd, BitOr, BitXor, LeftShift, RightShift}`, `UnaryOp::BitNot` (src/ast.rs)
- Parser: precedence climbing via `parse_bit_or`, `parse_bit_xor`, `parse_bit_and`, `parse_bit_shift` (src/parser.rs)
- Interpreter: cast f64 to i64, operate, cast back to f64 (src/interpreter.rs)

## How to Add a New Builtin

Adding a builtin requires **no parser or lexer changes**. Three files minimum, four if using a separate module.

### Step 1: Add name to BUILTIN_NAMES (`src/builtins.rs`)

Find the `BUILTIN_NAMES` constant and add your function name string. Group it logically with related builtins:

```rust
pub const BUILTIN_NAMES: &[&str] = &[
    // ... existing names ...
    "my_func",        // ← add here
];
```

This is the only registration needed — `Interpreter::new()` automatically picks it up.

### Step 2: Add match arm in `call_builtin()` (`src/builtins.rs`)

Add a dispatch arm before the `_ => Err(...)` fallthrough:

```rust
match name {
    // ... existing arms ...
    "my_func" => builtin_my_func(args),           // simple — inline in builtins.rs
    "my_func" => crate::my_module::builtin_my_func(args),  // or delegate to a module
    _ => Err(AudionError::RuntimeError { ... }),
}
```

Available parameters to forward: `args`, `named_args`, `osc`, `midi`, `osc_protocol`, `clock`.

### Step 3: Implement the function

**Option A — Inline in `builtins.rs`** (for small/one-off builtins):

```rust
fn builtin_my_func(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "my_func() requires an argument".to_string(),
        });
    }
    let s = require_string("my_func", &args[0])?;
    Ok(Value::String(s.to_uppercase()))
}
```

**Option B — Separate module** (for families of related builtins, like `math.rs` or `strings.rs`):

1. Create `src/my_module.rs` with `pub fn` functions
2. Add `pub mod my_module;` to `src/lib.rs`
3. Add `mod my_module;` to `src/main.rs`
4. Dispatch via `crate::my_module::builtin_my_func(args)` in the match

### Step 4: Write tests (`tests/test_builtins.rs` or a new test file)

```rust
#[test]
fn test_my_func() {
    assert_eq!(eval(r#"my_func("hello");"#), Value::String("HELLO".to_string()));
}
```

Note: all `eval()` strings must end with `;` (Audion requires semicolons).

### Available helpers in `builtins.rs`

- `require_string(fn_name, &val)` → `Result<String>`
- `require_number(fn_name, &val)` → `Result<f64>` (in math.rs/strings.rs: local copies)
- `require_at_least(fn_name, args, n)` → `Result<()>` (in math.rs/strings.rs: local copies)
- `extract_regex_pattern(&str)` → `Option<&str>` (pub(crate), detects `/pattern/`, `{pattern}`, `%%pattern%%`)

### Return conventions

- Success: `Ok(Value::Number(...))`, `Ok(Value::String(...))`, `Ok(Value::Bool(...))`, `Ok(Value::Nil)`
- Failure (recoverable, like file not found): return `Ok(Value::Bool(false))` — lets user code check with `if`
- Error (programmer mistake, wrong args): return `Err(AudionError::RuntimeError { msg: ... })`
- Arrays: `Ok(Value::Array(Arc::new(Mutex::new(arr))))` where `arr` is `AudionArray::new()`

## Networking Internals

**Handle store**: global `OnceLock<Mutex<HashMap<u64, Arc<Mutex<NetHandle>>>>>` in builtins.rs. `NetHandle` enum: `Stream(TcpStream)` | `Listener(TcpListener)` | `UdpSocket(UdpSocket)`. Per-handle `Arc<Mutex>` so concurrent threads can do I/O on different handles without blocking the map. `NEXT_NET_HANDLE: AtomicU64` starts at 1.

**TCP builtins** (`net_connect`, `net_listen`, `net_accept`, `net_read`, `net_write`, `net_close`): all use `std::net` blocking I/O. `net_read` default buffer 8192 bytes, returns `String::from_utf8_lossy`. `net_close` removes from map, drop closes socket. `net_accept` locks listener handle, calls `accept()`, drops lock before `store_handle`.

**UDP builtins** (`net_udp_bind`, `net_udp_send`, `net_udp_recv`): use `std::net::UdpSocket` non-blocking I/O. `net_udp_bind` accepts either 1 arg (port only, binds to `0.0.0.0`) or 2 args (host + port). Socket is set to non-blocking mode immediately after binding. `net_udp_recv` returns `nil` when no data available (non-blocking), or an array `["data" => string, "host" => ip, "port" => port_num]` on success. `net_udp_send` sends to specified host:port and returns bytes sent or `false` on error. All UDP sockets share the same handle store and use `net_close` for cleanup.

**HTTP builtin** (`net_http`): uses `ureq 2.x` (sync HTTP client, rustls TLS). Supports GET/POST/PUT/DELETE/PATCH/HEAD. Optional body (arg[2]) and headers (arg[3] as key=>value array). Returns `["status" => u16, "body" => string, "headers" => [...]]`. `Error::Status` (4xx/5xx) still returns response array. `Error::Transport` returns `Bool(false)`.

## MIDI Client

**Structure** (`src/midi.rs`): `MidiClient { connection: Mutex<Option<MidiOutputConnection>> }`. Created eagerly at startup (empty), connection established lazily via `midi_config()`. Thread-safe via `Mutex` — `MidiOutputConnection` is `Send` but not `Sync`, so wrapping in `Mutex` allows safe sharing across threads via `Arc<MidiClient>`.

**Channel convention**: Audion uses 1-16 (human-friendly), converted to 0-15 internally via `saturating_sub(1)`. Default channel is 1 (internal 0).

**Cleanup**: `midi_panic()` sends CC 123 (All Notes Off) on all 16 channels. Called automatically in the Ctrl+C handler before `process::exit`.

## OSC Protocol Client

**Structure** (`src/osc_protocol.rs`): `OscProtocolClient { target: Mutex<Option<String>>, socket: Mutex<Option<UdpSocket>>, listener: Mutex<Option<UdpSocket>> }`. Separate from the internal `OscClient` (scsynth-specific). Created eagerly at startup (empty), target set lazily via `osc_config()`. Thread-safe via `Mutex` wrappers + `Arc<OscProtocolClient>`.

**Sending**: `osc_config("host:port")` binds a UDP socket to an ephemeral port and stores the target address. `osc_send("/addr", args...)` encodes via `rosc` and sends. Audion values are auto-converted: integer-valued numbers become `OscType::Int`, floats become `OscType::Float`, strings become `OscType::String`, bools and nil mapped directly.

**Receiving**: `osc_listen(port)` binds a non-blocking UDP socket on the given port. `osc_recv()` polls non-blocking — returns `nil` if no message, or an array `["/address", arg1, arg2, ...]` with OSC args converted back to Audion values.

## OSC Client (scsynth)

- `socket: UdpSocket` bound to `0.0.0.0:0` (OS-assigned ephemeral port)
- `target: String` — default `"127.0.0.1:57110"`
- `next_node_id: AtomicI32` starts at 1000, fetch_add(1, Relaxed)
- `allocated_nodes: Mutex<Vec<i32>>` — tracks all living node IDs for cleanup
- **synth_new** sends `/s_new [name, nodeId, 0(addToHead), 1(defaultGroup), ...controls]`
- **node_free** sends `/n_free [nodeId]`, removes from allocated_nodes
- **node_set** sends `/n_set [nodeId, ...controls]`
- **free_all_nodes** — clones the vec, sends /n_free for each, clears. Used by Ctrl+C handler.
- **buffer_alloc_read** sends `/b_allocRead [bufId, path, 0, 0]`. Auto-assigns buffer IDs via `next_buffer_id: AtomicI32` (starts at 0).
- Control values: Number→Float(n as f32), anything else→Float(0.0). **Truncates f64 to f32.**
- Send errors silently ignored (`let _ = socket.send_to`).

## Clock

- BPM stored as `AtomicU64` holding `f64::to_bits()`. Lock-free cross-thread reads.
- `beats_to_duration(beats)` = `beats * 60.0 / bpm` seconds.
- `wait_beats()` uses thread-local absolute deadline tracking (`BEAT_DEADLINE: RefCell<Option<Instant>>`). Each call advances the deadline by the beat duration and sleeps until it, absorbing execution overhead. First call sets deadline to `now + duration`. `wait_ms()` uses plain `thread::sleep` (no drift compensation — intended for non-musical delays).
- `elapsed_secs()` uses `Instant::now() - start_time`.

## Parser Specifics

**Token matching**: uses `std::mem::discriminant()` to compare token kinds ignoring inner values.

**Named arg detection**: peeks current token for Ident, peeks +1 for Colon. If both match → named arg. Otherwise → positional.

**FnDecl vs FnExpr**: `fn name(...){}` at statement level → FnDecl (Stmt). `fn(...){}` in expression position → FnExpr (Expr).

**parse_block vs parse_block_or_stmt**: `if/while` use `parse_block_or_stmt` (allowing single statement body). `loop/thread/fn` use `parse_block` (requiring braces).

## Lexer Specifics

- Source stored as `Vec<char>` (not bytes). Indexing is O(1) but memory is 4x.
- Number parsing: integer then optional `.` followed by digit (requires digit after dot: `1.` alone won't parse as float, will parse as `1` then `.`).
- Identifiers: `[a-zA-Z_][a-zA-Z0-9_]*`. Keyword check via match on string. Reserved keywords: fn, let, if, else, while, loop, for, return, thread, define, break, continue, this, include, as, using, true, false, nil.
- Bitwise operators supported: `&` (AND), `|` (OR), `^` (XOR), `~` (NOT), `<<` (left shift), `>>` (right shift). Logical operators: `&&` (AND), `||` (OR).
- Block comments don't nest. `/* /* */ */` → the second `*/` will cause a lex error.

## Error Handling

- LexError and ParseError carry line number. RuntimeError does not (no span tracking in interpreter).
- Errors in REPL → printed, execution continues. Errors in file mode → printed, exit(1).
- Thread errors → eprintln only, no propagation to parent.
- All `.unwrap()` calls on mutex locks — panics on poisoned mutex (from thread panic).

## SynthDef Compilation Cache (Watch Mode)

**Purpose**: Avoid re-compiling unchanged SynthDefs on file reload during `--watch` mode. Compiling a SynthDef requires spawning `sclang` which takes ~100-500ms. This breaks flow during live coding.

**Implementation**:
- Cache is `Arc<Mutex<HashMap<String, (u64, Vec<u8>)>>>` — maps synthdef name → (ast_hash, compiled_bytes)
- Cache lives at the watch loop level (in `run_file_watch`), persists across interpreter reloads
- Each `Stmt::SynthDef` execution computes a hash of `(name, params, body)` using `hash_synthdef()`
- Cache hit → skip `sclang` compilation, reuse bytes, print `"reusing cached synth 'name'"`
- Cache miss → compile via `sclang`, store bytes in cache, print `"defined synth 'name'"`
- **Always** loads sample buffers (even on cache hit) because `free_all_buffers()` clears them on reload
- Cached bytes are sent to scsynth via `/d_recv` (fast UDP, no compilation overhead)

**Buffer ID determinism**:
- Compiled SynthDef bytecode has buffer IDs hardcoded (generated by `sclang`)
- `free_all_buffers()` resets `next_buffer_id` counter to 0, ensuring deterministic IDs across reloads
- This allows cached bytecode with embedded buffer IDs to work correctly after reload

**Scope**: In-memory only. Cache clears when you stop audion. Separate file runs don't share cache.

**Hash stability**: Uses `format!("{:?}", body)` of the AST. Any whitespace/comment changes in source won't affect hash (parser normalizes). Changing UGen params, adding/removing UGens, or renaming params will invalidate cache.

## Known Limitations / Technical Debt

1. **run_line() evaluates ExprStmt twice** — once in exec_stmt, once to capture last value
2. **RuntimeError has no line info** — only LexError/ParseError track lines
3. **Named args to user functions silently ignored** — only builtins use them
4. **Thread name collision** — second `thread foo {}` overwrites first handle, first becomes un-joinable
5. **No ++ or -- operators** — must use `x += 1`
6. **Assignment creates variables** — `x = 5;` without `let` silently creates, could mask typos
7. **f64→f32 truncation** in OSC control values
8. **No string interpolation**
9. **REPL hardcodes server/bpm** — doesn't pass CLI args when no subcommand

## Test Infrastructure

All tests are integration tests in `tests/` directory (no unit tests in source files).
- `tests/common/mod.rs` — shared helpers: `eval()`, `parse_source()`
- `eval(src) -> Value` creates a fresh Interpreter per test call, uses `run_line()` (not `run()`). OscClient points to 57110 but tests don't need scsynth running (UDP send silently fails).
- Test strings must end with `;` (Audion requires semicolons).
