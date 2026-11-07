//! Event-stream events the fork adds: lock keys and output changes.
//!
//! Each is one call from upstream code (`State::refresh` and `State::refresh_ipc_outputs`), so a
//! rebase has one line to carry per event. docs/fork/ipc/events.md describes them.

use niri_ipc::state::EventStreamStatePart as _;
use niri_ipc::Event;

use crate::niri::State;

impl State {
    /// Sends `KeyboardLockKeysChanged` when Caps Lock or Num Lock turned on or off.
    pub fn ipc_refresh_lock_keys(&mut self) {
        let Some(server) = &self.niri.ipc_server else {
            return;
        };
        let Some(keyboard) = self.niri.seat.get_keyboard() else {
            return;
        };

        let leds = keyboard.led_state();
        let caps_lock = leds.caps.unwrap_or(false);
        let num_lock = leds.num.unwrap_or(false);

        let mut state = server.event_stream_state.borrow_mut();
        let state = &mut state.lock_keys;
        if state.seen && state.caps_lock == caps_lock && state.num_lock == num_lock {
            return;
        }

        let event = Event::KeyboardLockKeysChanged {
            caps_lock,
            num_lock,
        };
        state.apply(event.clone());
        server.send_event(event);
    }

    /// Sends `OutputsChanged` after the outputs `Request::Outputs` answers were rebuilt.
    pub fn ipc_outputs_changed(&self) {
        if let Some(server) = &self.niri.ipc_server {
            server.send_event(Event::OutputsChanged {});
        }
    }
}
