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
//! Lookahead event scheduler.
//!
//! Sequencer threads run `latency` seconds ahead of the audio clock and, instead
//! of doing socket I/O inline, hand every outbound event to this scheduler with
//! the logical beat it belongs on.
//!
//!  * scsynth OSC is emitted straight away as an OSC **bundle** stamped with the
//!    wall-clock time the event should sound. scsynth then places it on its own
//!    sample clock, so our thread jitter never reaches the audio.
//!  * MIDI and user OSC can't be pre-scheduled by the receiver, so they sit in a
//!    time-ordered queue that a single dispatch thread drains, sleeping (then
//!    briefly spinning) until each event's exact instant.
//!
//! Everything is expressed in beats and converted to wall-clock at the moment of
//! sending, so a Link tempo change (ours or a peer's) re-flows pending events.

use std::collections::BTreeMap;
use std::net::UdpSocket;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use rosc::{encoder, OscBundle, OscMessage, OscPacket, OscTime};

use crate::clock::Clock;
use crate::midi::MidiClient;
use crate::osc_protocol::OscProtocolClient;

/// Within this window of a timed event, busy-spin instead of sleeping.
const SPIN_WINDOW: Duration = Duration::from_millis(2);
/// Longest the dispatch thread parks when it has nothing imminent.
const IDLE_PARK: Duration = Duration::from_millis(100);

fn beat_key(beat: f64) -> i64 {
    (beat * 1_000_000.0) as i64
}

/// An event that must be sent at a precise wall-clock instant (the receiver
/// can't schedule it itself).
enum Timed {
    Midi(Vec<u8>),
    /// Pre-encoded OSC packet bytes for the user OSC target.
    Raw(Vec<u8>),
}

struct Inner {
    timed: BTreeMap<(i64, u64), Timed>,
    seq: u64,
    stop: bool,
}

pub struct Scheduler {
    inner: Mutex<Inner>,
    cv: Condvar,
    clock: Arc<Clock>,
    midi: Arc<MidiClient>,
    raw: Arc<OscProtocolClient>,
    sc_sock: UdpSocket,
    sc_target: String,
}

impl Scheduler {
    /// Build a scheduler, mark the clock as scheduled, and spawn the dispatch
    /// thread. `server` is the scsynth OSC address (host:port).
    pub fn new_and_start(
        server: &str,
        clock: Arc<Clock>,
        midi: Arc<MidiClient>,
        raw: Arc<OscProtocolClient>,
    ) -> Arc<Self> {
        let sc_sock = UdpSocket::bind("0.0.0.0:0").expect("failed to bind scheduler UDP socket");
        clock.enable_scheduling();
        let sched = Arc::new(Scheduler {
            inner: Mutex::new(Inner {
                timed: BTreeMap::new(),
                seq: 0,
                stop: false,
            }),
            cv: Condvar::new(),
            clock,
            midi,
            raw,
            sc_sock,
            sc_target: server.to_string(),
        });
        let me = Arc::clone(&sched);
        std::thread::Builder::new()
            .name("audion-sched".to_string())
            .spawn(move || {
                let _ = thread_priority::ThreadPriority::Max.set_for_current();
                me.run();
            })
            .expect("failed to spawn scheduler thread");
        sched
    }

    // -- producer side (called from sequencer threads) ----------------------

    /// Emit scsynth messages as one bundle timed to the caller's logical beat.
    pub fn submit_sc(&self, msgs: Vec<OscMessage>) {
        if msgs.is_empty() {
            return;
        }
        let lead = self.lead_secs(self.clock.logical_beat());
        let sys = SystemTime::now() + Duration::from_secs_f64(lead);
        let timetag = OscTime::try_from(sys).unwrap_or_else(|_| OscTime::from((0u32, 0u32)));
        let bundle = OscBundle {
            timetag,
            content: msgs.into_iter().map(OscPacket::Message).collect(),
        };
        if let Ok(bytes) = encoder::encode(&OscPacket::Bundle(bundle)) {
            let _ = self.sc_sock.send_to(&bytes, &self.sc_target);
        }
    }

    /// Queue raw MIDI bytes for the caller's logical beat.
    pub fn submit_midi(&self, bytes: Vec<u8>) {
        self.enqueue(Timed::Midi(bytes));
    }

    /// Queue a pre-encoded user-OSC packet for the caller's logical beat.
    pub fn submit_raw(&self, bytes: Vec<u8>) {
        self.enqueue(Timed::Raw(bytes));
    }

    fn enqueue(&self, item: Timed) {
        let beat = self.clock.logical_beat();
        {
            let mut g = self.inner.lock().unwrap();
            let seq = g.seq;
            g.seq += 1;
            g.timed.insert((beat_key(beat), seq), item);
        }
        self.cv.notify_one();
    }

    /// Drop every pending timed event (used on watch-reload / shutdown).
    pub fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.timed.clear();
        drop(g);
        self.cv.notify_one();
    }

    /// Block until the timed queue drains or `timeout` elapses. scsynth bundles
    /// are already on the server, so only MIDI/OSC tails matter here.
    pub fn wait_until_empty(&self, timeout: Duration) {
        let start = Instant::now();
        loop {
            if self.inner.lock().unwrap().timed.is_empty() || start.elapsed() > timeout {
                return;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    // -- consumer side (dispatch thread) -----------------------------------

    fn run(&self) {
        loop {
            let earliest = {
                let g = self.inner.lock().unwrap();
                if g.stop {
                    return;
                }
                g.timed.keys().next().copied()
            };

            let Some(key) = earliest else {
                let g = self.inner.lock().unwrap();
                let _ = self.cv.wait_timeout(g, IDLE_PARK);
                continue;
            };

            let target = self.instant_at_beat(key.0 as f64 / 1_000_000.0);
            let remaining = target.saturating_duration_since(Instant::now());

            if remaining > SPIN_WINDOW {
                let nap = (remaining - SPIN_WINDOW).min(IDLE_PARK);
                let g = self.inner.lock().unwrap();
                let _ = self.cv.wait_timeout(g, nap);
                continue;
            }

            spin_until(target);
            let item = self.inner.lock().unwrap().timed.remove(&key);
            match item {
                Some(Timed::Midi(b)) => self.midi.send_direct(&b),
                Some(Timed::Raw(b)) => {
                    self.raw.send_encoded(&b);
                }
                None => {}
            }
        }
    }

    // -- beat <-> wall-clock ----------------------------------------------

    /// Seconds from now until `beat` at the current tempo (never negative).
    fn lead_secs(&self, beat: f64) -> f64 {
        let now_b = self.clock.now_beats();
        let spb = 60.0 / self.clock.get_bpm();
        ((beat - now_b) * spb).max(0.0)
    }

    fn instant_at_beat(&self, beat: f64) -> Instant {
        Instant::now() + Duration::from_secs_f64(self.lead_secs(beat))
    }
}

/// Sleep until ~1ms out, then busy-wait to the target for sub-ms accuracy.
fn spin_until(target: Instant) {
    loop {
        let now = Instant::now();
        if now >= target {
            return;
        }
        let rem = target - now;
        if rem > Duration::from_micros(1200) {
            std::thread::sleep(rem - Duration::from_micros(1000));
        } else {
            std::hint::spin_loop();
        }
    }
}
