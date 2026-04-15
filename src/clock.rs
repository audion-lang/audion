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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusty_link::{AblLink, SessionState};

thread_local! {
    static BEAT_DEADLINE: RefCell<Option<Instant>> = RefCell::new(None);
    static LINK_BEAT_TARGET: RefCell<Option<f64>> = RefCell::new(None);
}

pub struct Clock {
    bpm_bits: AtomicU64,
    start_time: Instant,
    link: Mutex<Option<AblLink>>,
    quantum_bits: AtomicU64,
}

impl Clock {
    pub fn new(bpm: f64) -> Self {
        Clock {
            bpm_bits: AtomicU64::new(bpm.to_bits()),
            start_time: Instant::now(),
            link: Mutex::new(None),
            quantum_bits: AtomicU64::new(4.0_f64.to_bits()),
        }
    }

    // -----------------------------------------------------------------------
    // Link session control
    // -----------------------------------------------------------------------

    pub fn link_enable(&self) {
        let bpm = f64::from_bits(self.bpm_bits.load(Ordering::Relaxed));
        let abl = AblLink::new(bpm);
        abl.enable(true);
        abl.enable_start_stop_sync(true);
        *self.link.lock().unwrap() = Some(abl);
    }

    pub fn link_disable(&self) {
        *self.link.lock().unwrap() = None;
    }

    pub fn link_is_enabled(&self) -> bool {
        self.link.lock().unwrap().is_some()
    }

    pub fn link_num_peers(&self) -> u64 {
        match self.link.lock().unwrap().as_ref() {
            Some(abl) => abl.num_peers(),
            None => 0,
        }
    }

    // -----------------------------------------------------------------------
    // Quantum (beats per bar, default 4.0 for 4/4 time)
    // -----------------------------------------------------------------------

    pub fn get_quantum(&self) -> f64 {
        f64::from_bits(self.quantum_bits.load(Ordering::Relaxed))
    }

    pub fn set_quantum(&self, q: f64) {
        self.quantum_bits.store(q.to_bits(), Ordering::Relaxed);
    }

    // -----------------------------------------------------------------------
    // BPM — link-aware: when Link is enabled, reads/writes go through Link
    // -----------------------------------------------------------------------

    pub fn set_bpm(&self, bpm: f64) {
        self.bpm_bits.store(bpm.to_bits(), Ordering::Relaxed);
        if let Some(abl) = self.link.lock().unwrap().as_ref() {
            let mut state = SessionState::new();
            abl.capture_app_session_state(&mut state);
            state.set_tempo(bpm, abl.clock_micros());
            abl.commit_app_session_state(&state);
        }
    }

    pub fn get_bpm(&self) -> f64 {
        match self.link.lock().unwrap().as_ref() {
            Some(abl) => {
                let mut state = SessionState::new();
                abl.capture_app_session_state(&mut state);
                state.tempo()
            }
            None => f64::from_bits(self.bpm_bits.load(Ordering::Relaxed)),
        }
    }

    // -----------------------------------------------------------------------
    // Link beat/phase queries
    // -----------------------------------------------------------------------

    pub fn link_beat(&self) -> f64 {
        match self.link.lock().unwrap().as_ref() {
            Some(abl) => {
                let mut state = SessionState::new();
                abl.capture_app_session_state(&mut state);
                state.beat_at_time(abl.clock_micros(), self.get_quantum())
            }
            None => 0.0,
        }
    }

    pub fn link_phase(&self) -> f64 {
        match self.link.lock().unwrap().as_ref() {
            Some(abl) => {
                let mut state = SessionState::new();
                abl.capture_app_session_state(&mut state);
                state.phase_at_time(abl.clock_micros(), self.get_quantum())
            }
            None => 0.0,
        }
    }

    pub fn link_request_beat(&self, beat: f64) {
        if let Some(abl) = self.link.lock().unwrap().as_ref() {
            let mut state = SessionState::new();
            abl.capture_app_session_state(&mut state);
            state.request_beat_at_time(beat, abl.clock_micros(), self.get_quantum());
            abl.commit_app_session_state(&state);
        }
    }

    // -----------------------------------------------------------------------
    // Link transport (start/stop sync)
    // -----------------------------------------------------------------------

    pub fn link_play(&self) {
        if let Some(abl) = self.link.lock().unwrap().as_ref() {
            let mut state = SessionState::new();
            abl.capture_app_session_state(&mut state);
            state.set_is_playing(true, abl.clock_micros());
            abl.commit_app_session_state(&state);
        }
    }

    pub fn link_stop(&self) {
        if let Some(abl) = self.link.lock().unwrap().as_ref() {
            let mut state = SessionState::new();
            abl.capture_app_session_state(&mut state);
            state.set_is_playing(false, abl.clock_micros());
            abl.commit_app_session_state(&state);
        }
    }

    pub fn link_is_playing(&self) -> bool {
        match self.link.lock().unwrap().as_ref() {
            Some(abl) => {
                let mut state = SessionState::new();
                abl.capture_app_session_state(&mut state);
                state.is_playing()
            }
            None => false,
        }
    }

    // -----------------------------------------------------------------------
    // Timing — wait_beats is link-aware
    // -----------------------------------------------------------------------

    pub fn beats_to_duration(&self, beats: f64) -> Duration {
        let bpm = self.get_bpm();
        let seconds_per_beat = 60.0 / bpm;
        Duration::from_secs_f64(beats * seconds_per_beat)
    }

    pub fn wait_beats(&self, beats: f64) {
        if self.link_is_enabled() {
            self.wait_beats_link(beats);
        } else {
            self.wait_beats_local(beats);
        }
    }

    fn wait_beats_local(&self, beats: f64) {
        let duration = self.beats_to_duration(beats);
        BEAT_DEADLINE.with(|cell| {
            let mut deadline = cell.borrow_mut();
            let target = match *deadline {
                Some(prev) => prev + duration,
                None => Instant::now() + duration,
            };
            *deadline = Some(target);
            let now = Instant::now();
            if target > now {
                std::thread::sleep(target - now);
            }
        });
    }

    fn wait_beats_link(&self, beats: f64) {
        let quantum = self.get_quantum();
        let guard = self.link.lock().unwrap();
        let Some(abl) = guard.as_ref() else { return };
        LINK_BEAT_TARGET.with(|cell| {
            let mut target = cell.borrow_mut();

            let mut state = SessionState::new();
            abl.capture_app_session_state(&mut state);
            let now = abl.clock_micros();

            let target_beat = match *target {
                Some(prev) => prev + beats,
                None => {
                    let current = state.beat_at_time(now, quantum);
                    current + beats
                }
            };
            *target = Some(target_beat);

            let target_time = state.time_at_beat(target_beat, quantum);
            let delta_us = target_time - now;
            if delta_us > 0 {
                std::thread::sleep(Duration::from_micros(delta_us as u64));
            }
        });
    }

    pub fn wait_ms(&self, ms: f64) {
        std::thread::sleep(Duration::from_secs_f64(ms / 1000.0));
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }
}
