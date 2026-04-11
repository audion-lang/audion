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
use std::sync::Mutex;

use rosc::encoder;
use rosc::{OscMessage, OscPacket, OscType};

use crate::value::Value;

/// User-facing OSC client for sending/receiving arbitrary OSC messages.
/// Separate from OscClient (which is scsynth-specific).
pub struct OscProtocolClient {
    target: Mutex<Option<String>>,
    socket: Mutex<Option<UdpSocket>>,
    listener: Mutex<Option<UdpSocket>>,
}

impl OscProtocolClient {
    pub fn new() -> Self {
        OscProtocolClient {
            target: Mutex::new(None),
            socket: Mutex::new(None),
            listener: Mutex::new(None),
        }
    }

    /// Set target address for sending (e.g. "127.0.0.1:9000").
    /// Creates a UDP socket bound to an ephemeral port.
    pub fn connect(&self, addr: &str) -> bool {
        match UdpSocket::bind("0.0.0.0:0") {
            Ok(sock) => {
                *self.socket.lock().unwrap() = Some(sock);
                *self.target.lock().unwrap() = Some(addr.to_string());
                true
            }
            Err(_) => false,
        }
    }

    /// Get the current target address, if configured.
    pub fn get_target(&self) -> Option<String> {
        self.target.lock().unwrap().clone()
    }

    /// Send an OSC message to the configured target.
    pub fn send(&self, address: &str, args: Vec<OscType>) -> bool {
        let target = self.target.lock().unwrap().clone();
        let sock = self.socket.lock().unwrap();
        if let (Some(target), Some(sock)) = (target, sock.as_ref()) {
            let msg = OscMessage {
                addr: address.to_string(),
                args,
            };
            let packet = OscPacket::Message(msg);
            if let Ok(bytes) = encoder::encode(&packet) {
                return sock.send_to(&bytes, &target).is_ok();
            }
        }
        false
    }

    /// Start listening for incoming OSC messages on a given port.
    pub fn listen(&self, port: u16) -> bool {
        let addr = format!("0.0.0.0:{}", port);
        match UdpSocket::bind(&addr) {
            Ok(sock) => {
                // Non-blocking so osc_recv() doesn't hang
                let _ = sock.set_nonblocking(true);
                *self.listener.lock().unwrap() = Some(sock);
                true
            }
            Err(_) => false,
        }
    }

    /// Non-blocking receive. Returns None if no message available.
    pub fn recv(&self) -> Option<(String, Vec<Value>)> {
        let listener = self.listener.lock().unwrap();
        if let Some(sock) = listener.as_ref() {
            let mut buf = [0u8; 65535];
            match sock.recv_from(&mut buf) {
                Ok((size, _addr)) => {
                    if let Ok(packet) = rosc::decoder::decode_udp(&buf[..size]) {
                        return Self::packet_to_values(packet.1);
                    }
                    None
                }
                Err(_) => None,
            }
        } else {
            None
        }
    }

    /// Send /notify 1 to scsynth from the listener socket so scsynth registers
    /// this port as a reply target for SendReply/SendTrig UGens.
    /// Sends /notify 0 first to deregister any stale entry from a previous run.
    pub fn notify_scsynth(&self, scsynth_addr: &str) -> bool {
        let listener = self.listener.lock().unwrap();
        if let Some(sock) = listener.as_ref() {
            // Deregister first to avoid "too many users" across restarts
            let unreg = OscPacket::Message(OscMessage {
                addr: "/notify".to_string(),
                args: vec![OscType::Int(0)],
            });
            if let Ok(bytes) = encoder::encode(&unreg) {
                let _ = sock.send_to(&bytes, scsynth_addr);
            }
            let reg = OscPacket::Message(OscMessage {
                addr: "/notify".to_string(),
                args: vec![OscType::Int(1)],
            });
            if let Ok(bytes) = encoder::encode(&reg) {
                return sock.send_to(&bytes, scsynth_addr).is_ok();
            }
        }
        false
    }

    /// Close the listener socket.
    pub fn close_listener(&self) {
        *self.listener.lock().unwrap() = None;
    }

    /// Close the sender socket and clear the target.
    pub fn close_sender(&self) {
        *self.socket.lock().unwrap() = None;
        *self.target.lock().unwrap() = None;
    }

    fn packet_to_values(packet: OscPacket) -> Option<(String, Vec<Value>)> {
        match packet {
            OscPacket::Message(msg) => {
                let args: Vec<Value> = msg
                    .args
                    .into_iter()
                    .map(|a| match a {
                        OscType::Int(i) => Value::Number(i as f64),
                        OscType::Float(f) => Value::Number(f as f64),
                        OscType::Double(d) => Value::Number(d),
                        OscType::Long(l) => Value::Number(l as f64),
                        OscType::String(s) => Value::String(s),
                        OscType::Bool(b) => Value::Bool(b),
                        OscType::Nil => Value::Nil,
                        _ => Value::Nil,
                    })
                    .collect();
                Some((msg.addr, args))
            }
            OscPacket::Bundle(bundle) => {
                // Return the first message in the bundle
                for p in bundle.content {
                    if let Some(result) = Self::packet_to_values(p) {
                        return Some(result);
                    }
                }
                None
            }
        }
    }

    /// Convert an Audion Value to an OscType for sending.
    pub fn value_to_osc(val: &Value) -> OscType {
        match val {
            Value::Number(n) => {
                // If it looks like an integer, send as Int
                if *n == (*n as i32) as f64 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64 {
                    OscType::Int(*n as i32)
                } else {
                    OscType::Float(*n as f32)
                }
            }
            Value::String(s) => OscType::String(s.clone()),
            Value::Bool(b) => OscType::Bool(*b),
            Value::Nil => OscType::Nil,
            _ => OscType::String(format!("{}", val)),
        }
    }
}
