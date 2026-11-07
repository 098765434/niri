//! niri-secure-fill-v1: a password manager fills the focused text field only if it's still
//! the one it checked, with who owns it as the compositor knows it.
//!
//! An input method's commit_string goes to whichever text input is focused when the request
//! arrives, so a secret meant for one app can reach another that took the focus in between.
//! Here the client takes a target (a snapshot of the focused text input and its owner's pid
//! and executable, read from the socket and /proc, never from the app), and a fill of that
//! target is checked against the focus and committed in the same dispatch.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Weak,
};

use super::raw::secure_fill::v1::server::niri_secure_fill_manager_v1::{
    self, NiriSecureFillManagerV1,
};
use super::raw::secure_fill::v1::server::niri_secure_fill_target_v1::{
    self, Flags, NiriSecureFillTargetV1, Refusal,
};

const VERSION: u32 = 1;

/// How long a target can be filled after it was taken.
const LIFETIME: Duration = Duration::from_secs(120);

/// The least time between two fills, from any client.
const SPACING: Duration = Duration::from_millis(300);

/// The most text one fill puts in, in bytes.
const MAX_TEXT: usize = 4096;

pub const FLAG_WINDOW: u32 = 1;
pub const FLAG_LAYER: u32 = 2;
pub const FLAG_SANDBOXED: u32 = 4;
pub const FLAG_TEXT_INPUT: u32 = 8;
pub const FLAG_UNKNOWN_CREDENTIALS: u32 = 16;

/// What a fill is checked against: the surface whose text input had the keyboard, and which of
/// its client's text inputs was the enabled one.
#[derive(Debug, Clone)]
pub struct TargetKey {
    pub surface: Weak<WlSurface>,
    pub text_input: Option<ObjectId>,
}

impl TargetKey {
    /// Whether `now` is the same field: the same live surface and the same enabled text input.
    pub fn matches(&self, now: &TargetKey) -> bool {
        let same_surface = match (self.surface.upgrade(), now.surface.upgrade()) {
            (Ok(was), Ok(is)) => was == is,
            _ => false,
        };

        same_surface && self.text_input.is_some() && self.text_input == now.text_input
    }
}

/// Who owns the field, as the compositor knows it.
#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub pid: i32,
    pub exe: String,
    pub app_id: String,
    pub window_id: u64,
    pub flags: u32,
}

/// Why a fill was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Changed,
    NoTextInput,
    Locked,
    Expired,
    Used,
    TooSoon,
    InvalidText,
}

impl Refused {
    fn to_wire(self) -> Refusal {
        match self {
            Refused::Changed => Refusal::Changed,
            Refused::NoTextInput => Refusal::NoTextInput,
            Refused::Locked => Refusal::Locked,
            Refused::Expired => Refusal::Expired,
            Refused::Used => Refusal::Used,
            Refused::TooSoon => Refusal::TooSoon,
            Refused::InvalidText => Refusal::InvalidText,
        }
    }
}

pub trait SecureFillHandler {
    fn secure_fill_state(&mut self) -> &mut SecureFillManagerState;

    /// The focused text field now and who owns it; no key without a focused surface.
    fn secure_fill_snapshot(&mut self) -> (Option<TargetKey>, Identity);

    /// Commits the text to the enabled text input, which the caller checked is `key`'s.
    fn secure_fill_commit(&mut self, text: &str);

    fn secure_fill_locked(&self) -> bool;
}

pub struct SecureFillManagerState {
    last_fill: Option<Instant>,
}

pub struct TargetData(Mutex<Target>);

struct Target {
    key: Option<TargetKey>,
    taken: Instant,
    used: bool,
}

/// Whether a process shares niri's mount and user namespaces and root, so the path its exe link
/// shows is the one niri sees.
pub fn same_namespaces(pid: i32) -> bool {
    let link = |pid: &str, what: &str| std::fs::read_link(format!("/proc/{pid}/{what}")).ok();

    let pid = pid.to_string();

    ["ns/mnt", "ns/user"].into_iter().all(|what| {
        let theirs = link(&pid, what);
        theirs.is_some() && theirs == link("self", what)
    }) && link(&pid, "root").is_some_and(|root| root == std::path::Path::new("/"))
}

/// Text a fill may hold: something, not too long, no control characters (a line break could
/// submit a form, a tab move to another field).
pub fn valid_text(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_TEXT && !text.chars().any(char::is_control)
}

impl SecureFillManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<NiriSecureFillManagerV1, SecureFillGlobalData>,
        D: Dispatch<NiriSecureFillManagerV1, ()>,
        D: Dispatch<NiriSecureFillTargetV1, TargetData>,
        D: SecureFillHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = SecureFillGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, NiriSecureFillManagerV1, _>(VERSION, global_data);

        Self { last_fill: None }
    }
}

pub struct SecureFillGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

impl<D> GlobalDispatch<NiriSecureFillManagerV1, SecureFillGlobalData, D> for SecureFillManagerState
where
    D: GlobalDispatch<NiriSecureFillManagerV1, SecureFillGlobalData>,
    D: Dispatch<NiriSecureFillManagerV1, ()>,
    D: Dispatch<NiriSecureFillTargetV1, TargetData>,
    D: SecureFillHandler,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        manager: New<NiriSecureFillManagerV1>,
        _global_data: &SecureFillGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(manager, ());
    }

    fn can_view(client: Client, global_data: &SecureFillGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<NiriSecureFillManagerV1, (), D> for SecureFillManagerState
where
    D: Dispatch<NiriSecureFillManagerV1, ()>,
    D: Dispatch<NiriSecureFillTargetV1, TargetData>,
    D: SecureFillHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _manager: &NiriSecureFillManagerV1,
        request: niri_secure_fill_manager_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            niri_secure_fill_manager_v1::Request::GetTarget { id } => {
                let (key, identity) = state.secure_fill_snapshot();

                let target = data_init.init(
                    id,
                    TargetData(Mutex::new(Target {
                        key,
                        taken: Instant::now(),
                        used: false,
                    })),
                );

                target.identity(
                    identity.pid,
                    identity.exe,
                    identity.app_id,
                    (identity.window_id >> 32) as u32,
                    identity.window_id as u32,
                    Flags::from_bits_truncate(identity.flags),
                );
            }
            niri_secure_fill_manager_v1::Request::Destroy => (),
        }
    }
}

impl<D> Dispatch<NiriSecureFillTargetV1, TargetData, D> for SecureFillManagerState
where
    D: Dispatch<NiriSecureFillTargetV1, TargetData>,
    D: SecureFillHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &NiriSecureFillTargetV1,
        request: niri_secure_fill_target_v1::Request,
        data: &TargetData,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            niri_secure_fill_target_v1::Request::Fill { text } => {
                let result = fill(state, data, &text);

                // the text is dropped here either way; wipe the bytes we were handed
                let mut bytes = text.into_bytes();
                bytes.fill(0);
                std::hint::black_box(&bytes);

                match result {
                    Ok(()) => resource.filled(),
                    Err(refused) => resource.refused(refused.to_wire()),
                }
            }
            niri_secure_fill_target_v1::Request::Destroy => (),
        }
    }
}

fn fill<D: SecureFillHandler>(state: &mut D, data: &TargetData, text: &str) -> Result<(), Refused> {
    let mut target = data.0.lock().unwrap();

    if target.used {
        return Err(Refused::Used);
    }

    // one try: a refused target can't be filled later either
    target.used = true;

    if !valid_text(text) {
        return Err(Refused::InvalidText);
    }

    if target.taken.elapsed() > LIFETIME {
        return Err(Refused::Expired);
    }

    if state.secure_fill_locked() {
        return Err(Refused::Locked);
    }

    let now = Instant::now();
    if let Some(last) = state.secure_fill_state().last_fill {
        if now.duration_since(last) < SPACING {
            return Err(Refused::TooSoon);
        }
    }

    let Some(key) = &target.key else {
        return Err(Refused::Changed);
    };

    if key.text_input.is_none() {
        return Err(Refused::NoTextInput);
    }

    let (current, _) = state.secure_fill_snapshot();
    if !current.is_some_and(|current| key.matches(&current)) {
        return Err(Refused::Changed);
    }

    state.secure_fill_commit(text);
    state.secure_fill_state().last_fill = Some(now);

    Ok(())
}

#[macro_export]
macro_rules! delegate_secure_fill {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::secure_fill::v1::server::niri_secure_fill_manager_v1::NiriSecureFillManagerV1: $crate::protocols::secure_fill::SecureFillGlobalData
        ] => $crate::protocols::secure_fill::SecureFillManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::secure_fill::v1::server::niri_secure_fill_manager_v1::NiriSecureFillManagerV1: ()
        ] => $crate::protocols::secure_fill::SecureFillManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::secure_fill::v1::server::niri_secure_fill_target_v1::NiriSecureFillTargetV1: $crate::protocols::secure_fill::TargetData
        ] => $crate::protocols::secure_fill::SecureFillManagerState);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_shares_its_own_namespaces() {
        assert!(same_namespaces(std::process::id() as i32));
        assert!(!same_namespaces(i32::MAX));
    }

    #[test]
    fn fill_text_is_checked() {
        assert!(valid_text("hunter2"));
        assert!(valid_text("pässwörd with spaces"));
        assert!(!valid_text(""));
        assert!(!valid_text("line\nbreak"));
        assert!(!valid_text("tab\there"));
        assert!(!valid_text("escape\u{1b}[A"));
        assert!(!valid_text(&"a".repeat(MAX_TEXT + 1)));
        assert!(valid_text(&"a".repeat(MAX_TEXT)));
    }
}
