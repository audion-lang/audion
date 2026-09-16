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

use std::net::UdpSocket;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, OnceLock};

use rosc::encoder;
use rosc::{OscMessage, OscPacket, OscType};

use crate::scheduler::Scheduler;
use crate::value::Value;

pub struct OscClient {
    socket: UdpSocket,
    target: String,
    next_node_id: AtomicI32,
    next_buffer_id: AtomicI32,
    allocated_nodes: std::sync::Mutex<Vec<i32>>,
    allocated_buffers: std::sync::Mutex<Vec<i32>>,
    recording_path: std::sync::Mutex<Option<String>>,
    recording_node_id: std::sync::Mutex<Option<i32>>,
    recording_buf_id: std::sync::Mutex<Option<i32>>,
    recording_synthdef_bytes: std::sync::Mutex<Vec<u8>>,
    /// When set, node-lifecycle messages (`/s_new`, `/n_set`, `/n_free`) are
    /// routed through the lookahead scheduler as timestamped bundles instead of
    /// being sent immediately.
    scheduler: OnceLock<Arc<Scheduler>>,
}

impl OscClient {
    pub fn new(target: &str) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("failed to bind UDP socket");

        // macOS's default SO_SNDBUF for a fresh UDP socket is small enough that
        // a moderately large OSC message (e.g. a many-breakpoint /b_setn from
        // the function-editor widget) fails outright with EMSGSIZE ("Message
        // too long") — silently, since the caller only sees the discarded
        // Result. Raise it well past what any single non-SynthDef message
        // should need. (SynthDefs themselves are loaded via /d_load — see
        // load_synthdef below — specifically because no SO_SNDBUF setting can
        // lift UDP's hard ~65KB per-datagram ceiling; this buffer bump is for
        // everything else.)
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = socket.as_raw_fd();
            let bufsize: libc::c_int = 1 << 20; // 1MB
            unsafe {
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_SNDBUF,
                    &bufsize as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
            }
        }

        let client = OscClient {
            socket,
            target: target.to_string(),
            next_node_id: AtomicI32::new(1000),
            next_buffer_id: AtomicI32::new(0),
            allocated_nodes: std::sync::Mutex::new(Vec::new()),
            allocated_buffers: std::sync::Mutex::new(Vec::new()),
            recording_path: std::sync::Mutex::new(None),
            recording_node_id: std::sync::Mutex::new(None),
            recording_buf_id: std::sync::Mutex::new(None),
            recording_synthdef_bytes: std::sync::Mutex::new(Vec::new()),
            scheduler: OnceLock::new(),
        };
        client
    }

    /// Attach the lookahead scheduler. After this, `/s_new`, `/n_set` and
    /// `/n_free` go out as timestamped bundles at their logical beat.
    pub fn set_scheduler(&self, sched: Arc<Scheduler>) {
        let _ = self.scheduler.set(sched);
    }

    /// Send one scsynth message: via the scheduler when attached (timestamped to
    /// the caller's logical beat), otherwise immediately.
    fn dispatch(&self, addr: &str, args: Vec<OscType>) {
        if let Some(sched) = self.scheduler.get() {
            sched.submit_sc(vec![OscMessage {
                addr: addr.to_string(),
                args,
            }]);
        } else {
            self.send(addr, args);
        }
    }

    pub fn synth_new(&self, def_name: &str, controls: &[(String, Value)]) -> i32 {
        let node_id = self.next_node_id.fetch_add(1, Ordering::Relaxed);

        let mut args: Vec<OscType> = vec![
            OscType::String(def_name.to_string()),
            OscType::Int(node_id),
            OscType::Int(0), // add action: addToHead
            OscType::Int(1), // target: default group
        ];

        for (name, val) in controls {
            args.push(OscType::String(name.clone()));
            match val {
                Value::Number(n) => args.push(OscType::Float(*n as f32)),
                _ => args.push(OscType::Float(0.0)),
            }
        }

        self.dispatch("/s_new", args);

        self.allocated_nodes.lock().unwrap().push(node_id);
        node_id
    }

    pub fn node_free(&self, node_id: i32) {
        self.dispatch("/n_free", vec![OscType::Int(node_id)]);
        let mut nodes = self.allocated_nodes.lock().unwrap();
        nodes.retain(|&id| id != node_id);
    }

    pub fn node_set(&self, node_id: i32, controls: &[(String, Value)]) {
        let mut args: Vec<OscType> = vec![OscType::Int(node_id)];
        for (name, val) in controls {
            args.push(OscType::String(name.clone()));
            match val {
                Value::Number(n) => args.push(OscType::Float(*n as f32)),
                _ => args.push(OscType::Float(0.0)),
            }
        }
        self.dispatch("/n_set", args);
    }

    /// Loads a compiled SynthDef onto the server. A UDP datagram has a hard
    /// ~65KB ceiling (the IP/UDP length field is 16-bit) that no amount of
    /// SO_SNDBUF tuning can lift — a `/d_recv` embedding the SynthDef's bytes
    /// inline fails outright with EMSGSIZE once a define grows large enough
    /// (many params/UGens, e.g. a wide modulation matrix — comfortably
    /// possible at 100KB+). So instead of `/d_recv`, write the bytes to a
    /// file scsynth can read directly and send `/d_load <path>` — only a
    /// short path string crosses the wire, so payload size stops mattering
    /// no matter how large a SynthDef gets. The file is written into the
    /// same persistent temp directory sclang already compiles into
    /// (`sclang::synthdef_output_dir()`), keyed by SynthDef name so re-
    /// defining just overwrites it.
    pub fn load_synthdef(&self, name: &str, synthdef_bytes: &[u8]) {
        let dir = crate::defcompile::synthdef_output_dir();
        let path = std::path::Path::new(&dir).join(format!("{}.scsyndef", name));
        if let Err(e) = std::fs::write(&path, synthdef_bytes) {
            eprintln!("warning: failed to write SynthDef '{}' to {}: {}", name, path.display(), e);
            return;
        }
        self.send("/d_load", vec![OscType::String(path.to_string_lossy().to_string())]);
    }

    /// Query scsynth for buffer info via /b_query → /b_info reply.
    /// Returns (num_frames, num_channels, sample_rate) or None on timeout.
    pub fn buffer_query(&self, buf_id: i32) -> Option<(i32, i32, f32)> {
        self.send("/b_query", vec![OscType::Int(buf_id)]);

        // Read replies until we get /b_info for our buffer, or timeout (500ms)
        let _ = self.socket.set_read_timeout(Some(std::time::Duration::from_millis(500)));
        let mut buf = [0u8; 1024];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, _)) => {
                    if let Ok((_, rosc::OscPacket::Message(msg))) = rosc::decoder::decode_udp(&buf[..len]) {
                        if msg.addr == "/b_info" && msg.args.len() >= 4 {
                            if let rosc::OscType::Int(id) = msg.args[0] {
                                if id == buf_id {
                                    let frames = match msg.args[1] { rosc::OscType::Int(n) => n, _ => 0 };
                                    let chans  = match msg.args[2] { rosc::OscType::Int(n) => n, _ => 0 };
                                    let sr     = match msg.args[3] { rosc::OscType::Float(f) => f, _ => 44100.0 };
                                    let _ = self.socket.set_read_timeout(None);
                                    return Some((frames, chans, sr));
                                }
                            }
                        }
                    }
                }
                Err(_) => break, // timeout or error
            }
        }
        let _ = self.socket.set_read_timeout(None);
        None
    }

    /// Allocate a buffer and read a sound file into it.
    /// Blocks until scsynth confirms with /done /b_allocRead (up to 10s for large files).
    /// Returns the buffer ID.
    pub fn buffer_alloc_read(&self, path: &str) -> i32 {
        let buf_id = self.next_buffer_id.fetch_add(1, Ordering::Relaxed);
        self.send(
            "/b_allocRead",
            vec![
                OscType::Int(buf_id),
                OscType::String(path.to_string()),
                OscType::Int(0), // start frame
                OscType::Int(0), // num frames (0 = entire file)
            ],
        );
        self.allocated_buffers.lock().unwrap().push(buf_id);
        // Wait for /done /b_allocRead confirmation so the buffer is ready to use
        let _ = self.socket.set_read_timeout(Some(std::time::Duration::from_secs(10)));
        let mut buf = [0u8; 1024];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, _)) => {
                    if let Ok((_, rosc::OscPacket::Message(msg))) = rosc::decoder::decode_udp(&buf[..len]) {
                        if msg.addr == "/done" {
                            if let Some(rosc::OscType::String(cmd)) = msg.args.first() {
                                if cmd == "/b_allocRead" {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(_) => break, // timeout — proceed anyway
            }
        }
        let _ = self.socket.set_read_timeout(None);
        buf_id
    }

    /// Allocate an empty buffer with the given number of frames and channels.
    /// Returns the buffer ID.
    pub fn buffer_alloc(&self, num_frames: i32, num_channels: i32) -> i32 {
        let buf_id = self.next_buffer_id.fetch_add(1, Ordering::Relaxed);
        self.send(
            "/b_alloc",
            vec![
                OscType::Int(buf_id),
                OscType::Int(num_frames),
                OscType::Int(num_channels),
            ],
        );
        self.allocated_buffers.lock().unwrap().push(buf_id);
        buf_id
    }

    /// Allocate a buffer and cue a sound file for DiskIn streaming.
    /// Uses scsynth's completion message to chain /b_alloc → /b_read atomically,
    /// ensuring the file is cued only after the buffer is allocated.
    /// Returns the buffer ID.
    pub fn buffer_cue_soundfile(&self, num_frames: i32, num_channels: i32, path: &str) -> i32 {
        let buf_id = self.next_buffer_id.fetch_add(1, Ordering::Relaxed);

        // Build the /b_read message to embed as completion message
        let read_msg = OscMessage {
            addr: "/b_read".to_string(),
            args: vec![
                OscType::Int(buf_id),
                OscType::String(path.to_string()),
                OscType::Int(0),  // file start frame
                OscType::Int(-1), // num frames (-1 = as many as fit)
                OscType::Int(0),  // buffer start frame
                OscType::Int(1),  // leave file open for DiskIn
            ],
        };
        let read_bytes = encoder::encode(&OscPacket::Message(read_msg)).unwrap_or_default();

        // Send /b_alloc with the /b_read as completion message
        self.send(
            "/b_alloc",
            vec![
                OscType::Int(buf_id),
                OscType::Int(num_frames),
                OscType::Int(num_channels),
                OscType::Blob(read_bytes),
            ],
        );
        self.allocated_buffers.lock().unwrap().push(buf_id);
        buf_id
    }

    /// Read a sound file into an existing buffer.
    pub fn buffer_read(&self, buf_id: i32, path: &str) {
        self.send(
            "/b_read",
            vec![
                OscType::Int(buf_id),
                OscType::String(path.to_string()),
                OscType::Int(0), // file start frame
                OscType::Int(-1), // num frames (-1 = as many as fit)
                OscType::Int(0), // buffer start frame
                OscType::Int(1), // leave file open (0 = close, 1 = leave open for DiskIn)
            ],
        );
    }

    /// Read a sound file into an existing buffer and close the file handle.
    pub fn buffer_read_close(&self, buf_id: i32, path: &str) {
        self.send(
            "/b_read",
            vec![
                OscType::Int(buf_id),
                OscType::String(path.to_string()),
                OscType::Int(0), // file start frame
                OscType::Int(-1), // num frames (-1 = as many as fit)
                OscType::Int(0), // buffer start frame
                OscType::Int(0), // leave file open = 0 (close after reading)
            ],
        );
    }

    /// Close an open sound file associated with a buffer (for DiskIn streaming).
    pub fn buffer_close(&self, buf_id: i32) {
        self.send("/b_close", vec![OscType::Int(buf_id)]);
    }

    /// Write a contiguous block of samples into an existing buffer via /b_setn
    /// (starting at frame 0, channel-interleaved if the buffer is multichannel).
    /// Used to push a hand-drawn curve (function editor) into a buffer scsynth
    /// can scan with Phasor/BufRd at audio rate.
    pub fn buffer_setn(&self, buf_id: i32, data: &[f32]) {
        let mut args: Vec<OscType> = vec![
            OscType::Int(buf_id),
            OscType::Int(0),
            OscType::Int(data.len() as i32),
        ];
        args.extend(data.iter().map(|&f| OscType::Float(f)));
        self.send("/b_setn", args);
    }

    /// Free a buffer.
    pub fn buffer_free(&self, buf_id: i32) {
        self.send("/b_free", vec![OscType::Int(buf_id)]);
        self.allocated_buffers
            .lock()
            .unwrap()
            .retain(|&id| id != buf_id);
    }

    /// Free all allocated buffers.
    pub fn free_all_buffers(&self) {
        let bufs = self.allocated_buffers.lock().unwrap().clone();
        for buf_id in bufs {
            self.send("/b_free", vec![OscType::Int(buf_id)]);
        }
        self.allocated_buffers.lock().unwrap().clear();
        // Reset buffer ID counter so buffer IDs are deterministic across reloads.
        // This allows cached SynthDef bytecode (which has buffer IDs baked in) to work.
        self.next_buffer_id.store(0, Ordering::Relaxed);
    }

    pub fn free_all_nodes(&self) {
        let nodes = self.allocated_nodes.lock().unwrap().clone();
        for node_id in nodes {
            self.send("/n_free", vec![OscType::Int(node_id)]);
        }
        self.allocated_nodes.lock().unwrap().clear();
    }

    /// Allocate a stereo buffer and open it for DiskOut recording.
    /// Uses a completion message to chain /b_alloc → /b_write atomically.
    /// Returns the buffer ID.
    pub fn buffer_alloc_for_recording(&self, path: &str) -> i32 {
        let buf_id = self.next_buffer_id.fetch_add(1, Ordering::Relaxed);

        let write_msg = OscMessage {
            addr: "/b_write".to_string(),
            args: vec![
                OscType::Int(buf_id),
                OscType::String(path.to_string()),
                OscType::String("wav".to_string()),
                OscType::String("int24".to_string()),
                OscType::Int(0), // start frame
                OscType::Int(0), // num frames (0 = all, i.e. open-ended)
                OscType::Int(1), // leave open for streaming writes
            ],
        };
        let write_bytes = encoder::encode(&OscPacket::Message(write_msg)).unwrap_or_default();

        self.send(
            "/b_alloc",
            vec![
                OscType::Int(buf_id),
                OscType::Int(65536), // buffer size in frames (~1.5s at 44.1kHz)
                OscType::Int(2),     // stereo
                OscType::Blob(write_bytes),
            ],
        );
        self.allocated_buffers.lock().unwrap().push(buf_id);
        buf_id
    }

    /// Create a synth added to the tail of the default group.
    /// Tail placement ensures it captures audio produced by all other synths.
    pub fn synth_new_tail(&self, def_name: &str, controls: &[(String, Value)]) -> i32 {
        let node_id = self.next_node_id.fetch_add(1, Ordering::Relaxed);

        let mut args: Vec<OscType> = vec![
            OscType::String(def_name.to_string()),
            OscType::Int(node_id),
            OscType::Int(1), // add action: addToTail
            OscType::Int(1), // target: default group
        ];
        for (name, val) in controls {
            args.push(OscType::String(name.clone()));
            match val {
                Value::Number(n) => args.push(OscType::Float(*n as f32)),
                _ => args.push(OscType::Float(0.0)),
            }
        }
        self.dispatch("/s_new", args);
        self.allocated_nodes.lock().unwrap().push(node_id);
        node_id
    }

    /// Returns true if the recording SynthDef has been compiled and cached.
    pub fn has_recording_synthdef(&self) -> bool {
        !self.recording_synthdef_bytes.lock().unwrap().is_empty()
    }

    /// Cache the compiled SynthDef bytes for reuse across record_start calls.
    pub fn set_recording_synthdef(&self, bytes: Vec<u8>) {
        *self.recording_synthdef_bytes.lock().unwrap() = bytes;
    }

    /// Load the recording SynthDef onto scsynth and, once loaded, create the
    /// DiskOut synth — embedded as a completion message so the synth is only
    /// spawned after the SynthDef is fully registered.
    pub fn load_recording_synthdef_then_synth(&self, def_name: &str, controls: &[(String, Value)]) -> i32 {
        let synthdef_bytes = self.recording_synthdef_bytes.lock().unwrap().clone();
        let node_id = self.next_node_id.fetch_add(1, Ordering::Relaxed);

        let mut synth_args: Vec<OscType> = vec![
            OscType::String(def_name.to_string()),
            OscType::Int(node_id),
            OscType::Int(1), // addToTail
            OscType::Int(1), // default group
        ];
        for (name, val) in controls {
            synth_args.push(OscType::String(name.clone()));
            match val {
                Value::Number(n) => synth_args.push(OscType::Float(*n as f32)),
                _ => synth_args.push(OscType::Float(0.0)),
            }
        }

        let synth_msg = OscMessage { addr: "/s_new".to_string(), args: synth_args };
        let synth_bytes = encoder::encode(&OscPacket::Message(synth_msg)).unwrap_or_default();

        self.send("/d_recv", vec![
            OscType::Blob(synthdef_bytes),
            OscType::Blob(synth_bytes),
        ]);
        self.allocated_nodes.lock().unwrap().push(node_id);
        node_id
    }

    /// Save recording state so record_stop / record_path can access it.
    pub fn set_recording_state(&self, path: String, node_id: i32, buf_id: i32) {
        *self.recording_path.lock().unwrap() = Some(path);
        *self.recording_node_id.lock().unwrap() = Some(node_id);
        *self.recording_buf_id.lock().unwrap() = Some(buf_id);
    }

    /// Consume recording state (node + buf), leaving path intact for record_path().
    /// Returns (node_id, buf_id, path) or None if not recording.
    pub fn take_recording_state(&self) -> Option<(i32, i32, String)> {
        let node_id = self.recording_node_id.lock().unwrap().take()?;
        let buf_id = self.recording_buf_id.lock().unwrap().take()?;
        let path = self.recording_path.lock().unwrap().clone().unwrap_or_default();
        Some((node_id, buf_id, path))
    }

    /// Return the path of the current or last recording, or None if never started.
    pub fn get_recording_path(&self) -> Option<String> {
        self.recording_path.lock().unwrap().clone()
    }

    fn send(&self, addr: &str, args: Vec<OscType>) {
        let msg = OscMessage {
            addr: addr.to_string(),
            args,
        };
        let packet = OscPacket::Message(msg);
        if let Ok(bytes) = encoder::encode(&packet) {
            if let Err(e) = self.socket.send_to(&bytes, &self.target) {
                eprintln!("warning: failed to send {} ({} bytes) to {}: {}", addr, bytes.len(), self.target, e);
            }
        }
    }
}
