//! Types the fork adds to the IPC.

use serde::{Deserialize, Serialize};

use crate::state::EventStreamStatePart;
use crate::Event;

/// A feature this compositor has beyond upstream niri, with the version of its contract.
///
/// Returned by [`Request::Features`](crate::Request::Features). Upstream niri can't parse that
/// request and answers with an error, so an error means none of these exist.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Feature {
    /// The feature's name, such as `blur-shapes` or `desktop-obscured`.
    pub name: String,
    /// The version of the feature's contract. It goes up when the feature changes in a way a
    /// client can tell.
    pub version: u32,
}

/// Caps Lock and Num Lock, communicated over the event stream.
#[derive(Debug, Default)]
pub struct LockKeysState {
    /// Whether Caps Lock is on.
    pub caps_lock: bool,
    /// Whether Num Lock is on.
    pub num_lock: bool,
    /// Whether a value has been received yet.
    pub seen: bool,
}

impl EventStreamStatePart for LockKeysState {
    fn replicate(&self) -> Vec<Event> {
        if !self.seen {
            return Vec::new();
        }
        vec![Event::KeyboardLockKeysChanged {
            caps_lock: self.caps_lock,
            num_lock: self.num_lock,
        }]
    }

    fn apply(&mut self, event: Event) -> Option<Event> {
        match event {
            Event::KeyboardLockKeysChanged {
                caps_lock,
                num_lock,
            } => {
                self.caps_lock = caps_lock;
                self.num_lock = num_lock;
                self.seen = true;
            }
            event => return Some(event),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::state::{EventStreamState, EventStreamStatePart as _};
    use crate::Event;

    /// The fork's parts of the state, as a new event-stream client receives them.
    fn replay(state: &EventStreamState) -> Vec<String> {
        let mut events = state.overview_progress.replicate();
        events.extend(state.view_offset.replicate());
        events.extend(state.workspace_switch.replicate());
        events.extend(state.desktop_obscured.replicate());
        events.extend(state.lock_keys.replicate());
        events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect()
    }

    #[test]
    fn progress_events_replay_their_last_value() {
        let mut state = EventStreamState::default();
        assert!(
            replay(&state).is_empty(),
            "nothing is replayed before it is seen"
        );

        for progress in [0.25, 0.5] {
            state.apply(Event::OverviewProgress { progress });
        }
        state.apply(Event::ViewOffsetChanged {
            output: "DP-1".into(),
            view_pos: 120.0,
            active_column_idx: 2,
        });
        state.apply(Event::WorkspaceSwitchProgress {
            output: "DP-1".into(),
            workspace_idx: 1.5,
            active_workspace_idx: 1,
        });

        assert_eq!(
            replay(&state),
            [
                r#"{"OverviewProgress":{"progress":0.5}}"#,
                r#"{"ViewOffsetChanged":{"output":"DP-1","view_pos":120.0,"active_column_idx":2}}"#,
                r#"{"WorkspaceSwitchProgress":{"output":"DP-1","workspace_idx":1.5,"active_workspace_idx":1}}"#,
            ]
        );
    }

    #[test]
    fn desktop_obscured_replays_every_output_in_name_order() {
        let mut state = EventStreamState::default();
        for (output, obscured) in [("HDMI-A-1", false), ("DP-1", true), ("HDMI-A-1", true)] {
            state.apply(Event::DesktopObscured {
                output: output.into(),
                obscured,
            });
        }
        assert_eq!(
            replay(&state),
            [
                r#"{"DesktopObscured":{"output":"DP-1","obscured":true}}"#,
                r#"{"DesktopObscured":{"output":"HDMI-A-1","obscured":true}}"#,
            ]
        );
    }

    #[test]
    fn lock_keys_replay_once_seen() {
        let mut state = EventStreamState::default();
        state.apply(Event::KeyboardLockKeysChanged {
            caps_lock: true,
            num_lock: false,
        });
        state.apply(Event::KeyboardLockKeysChanged {
            caps_lock: true,
            num_lock: true,
        });
        // stateless: nothing to replay
        state.apply(Event::OutputsChanged {});
        assert_eq!(
            replay(&state),
            [r#"{"KeyboardLockKeysChanged":{"caps_lock":true,"num_lock":true}}"#]
        );
    }
}
