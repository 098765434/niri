//! The fork's IPC events, as the server keeps them for new event-stream clients.

use niri_ipc::state::EventStreamStatePart as _;

use super::*;

/// The `DesktopObscured` events a new client would receive, as (output, obscured).
fn replayed_desktop_obscured(f: &mut Fixture) -> Vec<(String, bool)> {
    f.niri_state().ipc_refresh_layout();
    let server = f.niri().ipc_server.as_ref().unwrap();
    let state = server.event_stream_state().borrow();
    state
        .desktop_obscured
        .replicate()
        .into_iter()
        .map(|event| match event {
            niri_ipc::Event::DesktopObscured { output, obscured } => (output, obscured),
            other => panic!("unexpected event {other:?}"),
        })
        .collect()
}

#[test]
fn desktop_obscured_is_sent_for_each_output() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    f.add_output(2, (1280, 720));

    assert_eq!(
        replayed_desktop_obscured(&mut f),
        [("headless-1".into(), false), ("headless-2".into(), false)]
    );
}

#[test]
fn unplugged_output_is_not_replayed() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    f.add_output(2, (1280, 720));
    replayed_desktop_obscured(&mut f);

    let output = f.niri_output(2);
    f.niri().remove_output(&output);

    assert_eq!(
        replayed_desktop_obscured(&mut f),
        [("headless-1".into(), false)]
    );
}
