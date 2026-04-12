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

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

/// DMX512 client that sends Art-Net (UDP) packets.
/// Default destination port is 6454 (standard Art-Net port).
///
/// Art-Net is the standard way to send DMX over a network — works with
/// QLC+, MadMapper, MA, and most professional lighting software/nodes.
pub struct DmxClient {
    socket: Mutex<Option<UdpSocket>>,
    target: Mutex<Option<SocketAddr>>,
    universe: Mutex<u16>,
    channels: Mutex<[u8; 512]>,
    sequence: AtomicU8,
}

impl DmxClient {
    pub fn new() -> Self {
        DmxClient {
            socket: Mutex::new(None),
            target: Mutex::new(None),
            universe: Mutex::new(0),
            channels: Mutex::new([0u8; 512]),
            sequence: AtomicU8::new(1),
        }
    }

    /// Connect to an Art-Net node at host:port.
    /// Returns true on success, false if the address is invalid or socket cannot be bound.
    pub fn connect(&self, host: &str, port: u16) -> bool {
        let addr: SocketAddr = match format!("{}:{}", host, port).parse() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let sock = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => s,
            Err(_) => return false,
        };
        *self.socket.lock().unwrap() = Some(sock);
        *self.target.lock().unwrap() = Some(addr);
        true
    }

    /// Set the Art-Net universe to target (0-based).
    pub fn set_universe(&self, universe: u16) {
        *self.universe.lock().unwrap() = universe;
    }

    /// Set a single channel value. channel is 0-indexed (0–511) internally.
    pub fn set_channel(&self, channel: usize, value: u8) {
        if channel < 512 {
            self.channels.lock().unwrap()[channel] = value;
        }
    }

    /// Set a range of channels. start is 0-indexed.
    pub fn set_range(&self, start: usize, values: &[u8]) {
        let mut ch = self.channels.lock().unwrap();
        for (i, &v) in values.iter().enumerate() {
            let idx = start + i;
            if idx >= 512 {
                break;
            }
            ch[idx] = v;
        }
    }

    /// Transmit the current channel buffer as an ArtDmx packet.
    /// Returns true if the packet was sent successfully.
    pub fn send(&self) -> bool {
        let universe = *self.universe.lock().unwrap();
        let channels: [u8; 512] = *self.channels.lock().unwrap();
        let seq = self.sequence.fetch_add(1, Ordering::Relaxed);
        let seq = if seq == 0 { 1 } else { seq };
        let packet = build_artdmx(universe, seq, &channels);

        let sock_guard = self.socket.lock().unwrap();
        let target_guard = self.target.lock().unwrap();
        match (sock_guard.as_ref(), target_guard.as_ref()) {
            (Some(s), Some(t)) => s.send_to(&packet, t).is_ok(),
            _ => false,
        }
    }

    /// Zero all channels and transmit.
    pub fn blackout(&self) -> bool {
        {
            let mut ch = self.channels.lock().unwrap();
            for b in ch.iter_mut() {
                *b = 0;
            }
        }
        self.send()
    }

    /// Disconnect (drop socket and target address).
    pub fn disconnect(&self) {
        *self.socket.lock().unwrap() = None;
        *self.target.lock().unwrap() = None;
    }
}

/// Build a raw ArtDmx (OpOutput 0x5000) UDP packet.
/// Spec: ANSI E1.17 / Art-Net 4 — 18-byte header + 512 bytes of DMX data.
fn build_artdmx(universe: u16, sequence: u8, data: &[u8; 512]) -> Vec<u8> {
    let mut p = Vec::with_capacity(530);
    // ID field: "Art-Net\0"
    p.extend_from_slice(b"Art-Net\0");
    // OpCode: OpOutput = 0x5000, little-endian
    p.push(0x00);
    p.push(0x50);
    // Protocol version: 14 (big-endian)
    p.push(0x00);
    p.push(14);
    // Sequence (1–255; 0 = disabled)
    p.push(sequence);
    // Physical port (informational, set to 0)
    p.push(0x00);
    // Universe: 15-bit, little-endian (SubUni = low byte, Net = high 7 bits)
    p.push((universe & 0xFF) as u8);
    p.push(((universe >> 8) & 0x7F) as u8);
    // Length: 512 big-endian = 0x02, 0x00
    p.push(0x02);
    p.push(0x00);
    // DMX data
    p.extend_from_slice(data);
    p
}
