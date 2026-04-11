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

use rosc::encoder;
use rosc::{OscMessage, OscPacket, OscType};

use crate::value::Value;

pub struct OscClient {
    socket: UdpSocket,
    target: String,
    next_node_id: AtomicI32,
    next_buffer_id: AtomicI32,
    allocated_nodes: std::sync::Mutex<Vec<i32>>,
    allocated_buffers: std::sync::Mutex<Vec<i32>>,
}

impl OscClient {
    pub fn new(target: &str) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("failed to bind UDP socket");
        let client = OscClient {
            socket,
            target: target.to_string(),
            next_node_id: AtomicI32::new(1000),
            next_buffer_id: AtomicI32::new(0),
            allocated_nodes: std::sync::Mutex::new(Vec::new()),
            allocated_buffers: std::sync::Mutex::new(Vec::new()),
        };
        // Free all nodes in the default group (group 1) to clear orphans
        // from previous runs, crashed sessions, or test suites.
        // This is equivalent to SuperCollider's Cmd+Period (CmdPeriod).
        client.send("/g_freeAll", vec![OscType::Int(1)]);
        client
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

        self.send("/s_new", args);

        self.allocated_nodes.lock().unwrap().push(node_id);
        node_id
    }

    pub fn node_free(&self, node_id: i32) {
        self.send("/n_free", vec![OscType::Int(node_id)]);
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
        self.send("/n_set", args);
    }

    pub fn load_synthdef(&self, synthdef_bytes: &[u8]) {
        self.send("/d_recv", vec![OscType::Blob(synthdef_bytes.to_vec())]);
    }

    /// Allocate a buffer and read a sound file into it.
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

    fn send(&self, addr: &str, args: Vec<OscType>) {
        let msg = OscMessage {
            addr: addr.to_string(),
            args,
        };
        let packet = OscPacket::Message(msg);
        if let Ok(bytes) = encoder::encode(&packet) {
            let _ = self.socket.send_to(&bytes, &self.target);
        }
    }
}
