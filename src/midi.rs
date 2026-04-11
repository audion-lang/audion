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

use std::sync::{atomic::AtomicBool, Mutex};
use std::thread::JoinHandle;

use midir::{MidiOutput, MidiOutputConnection};

pub struct MidiClient {
    connection: Mutex<Option<MidiOutputConnection>>,
    // For midi_bpm_sync
    pub sync_thread: Mutex<Option<JoinHandle<()>>>,
    pub sync_enabled: AtomicBool,
    pub previous_bpm: Mutex<Option<f64>>,
}

impl MidiClient {
    pub fn new() -> Self {
        MidiClient {
            connection: Mutex::new(None),
            sync_thread: Mutex::new(None),
            sync_enabled: AtomicBool::new(false),
            previous_bpm: Mutex::new(None),
        }
    }

    /// List available MIDI output port names.
    pub fn list_ports() -> Vec<String> {
        let out = match MidiOutput::new("audion") {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };
        let ports = out.ports();
        ports
            .iter()
            .filter_map(|p| out.port_name(p).ok())
            .collect()
    }

    /// Connect to a MIDI output port by name substring match.
    /// Returns true on success, false if no matching port found.
    pub fn connect(&self, name: &str) -> bool {
        let out = match MidiOutput::new("audion") {
            Ok(o) => o,
            Err(_) => return false,
        };
        let ports = out.ports();
        let needle = name.to_lowercase();
        let port = ports.iter().find(|p| {
            out.port_name(p)
                .map(|n| n.to_lowercase().contains(&needle))
                .unwrap_or(false)
        });
        match port {
            Some(p) => {
                let port_clone = p.clone();
                match out.connect(&port_clone, "audion-out") {
                    Ok(conn) => {
                        *self.connection.lock().unwrap() = Some(conn);
                        true
                    }
                    Err(_) => false,
                }
            }
            None => false,
        }
    }

    /// Connect to a MIDI output port by index.
    /// Returns true on success, false if index out of range.
    pub fn connect_by_index(&self, idx: usize) -> bool {
        let out = match MidiOutput::new("audion") {
            Ok(o) => o,
            Err(_) => return false,
        };
        let ports = out.ports();
        if idx >= ports.len() {
            return false;
        }
        let port = &ports[idx];
        let port_clone = port.clone();
        match out.connect(&port_clone, "audion-out") {
            Ok(conn) => {
                *self.connection.lock().unwrap() = Some(conn);
                true
            }
            Err(_) => false,
        }
    }

    /// Send raw MIDI bytes. Silently ignored if not connected.
    pub fn send(&self, msg: &[u8]) {
        if let Some(conn) = self.connection.lock().unwrap().as_mut() {
            let _ = conn.send(msg);
        }
    }

    /// Send Note On. Channel is 0-15 internally.
    pub fn note_on(&self, channel: u8, note: u8, velocity: u8) {
        self.send(&[0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F]);
    }

    /// Send Note Off.
    pub fn note_off(&self, channel: u8, note: u8) {
        self.send(&[0x80 | (channel & 0x0F), note & 0x7F, 0]);
    }

    /// Send Control Change.
    pub fn cc(&self, channel: u8, controller: u8, value: u8) {
        self.send(&[0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F]);
    }

    /// Send Program Change.
    pub fn program_change(&self, channel: u8, program: u8) {
        self.send(&[0xC0 | (channel & 0x0F), program & 0x7F]);
    }

    /// Send MIDI Clock tick (0xF8).
    pub fn clock_tick(&self) {
        self.send(&[0xF8]);
    }

    /// Send MIDI Start (0xFA).
    pub fn start(&self) {
        self.send(&[0xFA]);
    }

    /// Send MIDI Stop (0xFC).
    pub fn stop(&self) {
        self.send(&[0xFC]);
    }

    /// Send All Notes Off (CC 123) on all 16 channels.
    pub fn panic(&self) {
        for ch in 0..16u8 {
            self.cc(ch, 123, 0);
        }
    }

    /// Disconnect (drop the connection).
    pub fn disconnect(&self) {
        *self.connection.lock().unwrap() = None;
    }
}
