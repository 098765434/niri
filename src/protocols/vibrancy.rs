//! niri-vibrancy-v1: a surface's content blended with what's behind it (plus-lighter or
//! plus-darker) instead of painted over it, for vibrant labels on a material.

use std::sync::Mutex;

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum, Weak,
};
use smithay::wayland::compositor::{with_states, Cacheable, SurfaceData};

use super::raw::vibrancy::v1::server::niri_vibrancy_manager_v1::{self, NiriVibrancyManagerV1};
use super::raw::vibrancy::v1::server::niri_vibrancy_v1::{self, NiriVibrancyV1};

const VERSION: u32 = 1;

/// How a surface blends with what's drawn behind it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Vibrancy {
    /// Painted over, as usual.
    #[default]
    None,
    /// Added to what's behind, clamped.
    PlusLighter,
    /// Taken away from what's behind, clamped at 0.
    PlusDarker,
}

impl Vibrancy {
    fn from_wire(mode: u32) -> Self {
        match mode {
            1 => Self::PlusLighter,
            2 => Self::PlusDarker,
            _ => Self::None,
        }
    }
}

/// A surface's mode, double-buffered with its commits.
#[derive(Debug, Clone, Copy, Default)]
pub struct VibrancyCachedState {
    pub mode: Vibrancy,
}

impl Cacheable for VibrancyCachedState {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        *self
    }

    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        *into = self;
    }
}

/// The mode a surface has now.
pub fn vibrancy(states: &SurfaceData) -> Vibrancy {
    states
        .cached_state
        .get::<VibrancyCachedState>()
        .current()
        .mode
}

pub struct VibrancyManagerState;

pub struct VibrancyUserData(Mutex<Weak<WlSurface>>);

impl VibrancyUserData {
    fn wl_surface(&self) -> Option<WlSurface> {
        self.0.lock().unwrap().upgrade().ok()
    }
}

/// A surface's vibrancy object, if it has one.
#[derive(Default)]
struct VibrancySurfaceData(Mutex<Option<NiriVibrancyV1>>);

impl VibrancyManagerState {
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<NiriVibrancyManagerV1, ()>,
        D: Dispatch<NiriVibrancyManagerV1, ()>,
        D: Dispatch<NiriVibrancyV1, VibrancyUserData>,
        D: 'static,
    {
        display.create_global::<D, NiriVibrancyManagerV1, _>(VERSION, ());
        Self
    }
}

impl<D> GlobalDispatch<NiriVibrancyManagerV1, (), D> for VibrancyManagerState
where
    D: GlobalDispatch<NiriVibrancyManagerV1, ()>,
    D: Dispatch<NiriVibrancyManagerV1, ()>,
    D: Dispatch<NiriVibrancyV1, VibrancyUserData>,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        manager: New<NiriVibrancyManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(manager, ());
    }
}

impl<D> Dispatch<NiriVibrancyManagerV1, (), D> for VibrancyManagerState
where
    D: Dispatch<NiriVibrancyManagerV1, ()>,
    D: Dispatch<NiriVibrancyV1, VibrancyUserData>,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        manager: &NiriVibrancyManagerV1,
        request: niri_vibrancy_manager_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            niri_vibrancy_manager_v1::Request::GetVibrancy { id, surface } => {
                let attached = with_states(&surface, |states| {
                    let data = states
                        .data_map
                        .get_or_insert_threadsafe(VibrancySurfaceData::default);
                    data.0
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|resource| resource.is_alive())
                });

                if attached {
                    manager.post_error(
                        niri_vibrancy_manager_v1::Error::AlreadyConstructed,
                        "wl_surface already has a vibrancy object",
                    );
                    return;
                }

                let resource =
                    data_init.init(id, VibrancyUserData(Mutex::new(surface.downgrade())));
                with_states(&surface, |states| {
                    if let Some(data) = states.data_map.get::<VibrancySurfaceData>() {
                        *data.0.lock().unwrap() = Some(resource);
                    }
                });
            }
            niri_vibrancy_manager_v1::Request::Destroy => (),
        }
    }
}

impl<D> Dispatch<NiriVibrancyV1, VibrancyUserData, D> for VibrancyManagerState
where
    D: Dispatch<NiriVibrancyV1, VibrancyUserData>,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        _resource: &NiriVibrancyV1,
        request: niri_vibrancy_v1::Request,
        data: &VibrancyUserData,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        // requests on a destroyed surface's object are ignored, the object is inert
        let Some(surface) = data.wl_surface() else {
            return;
        };

        let mode = match request {
            niri_vibrancy_v1::Request::SetMode { mode } => {
                let mode = match mode {
                    WEnum::Value(mode) => mode as u32,
                    WEnum::Unknown(mode) => mode,
                };
                Vibrancy::from_wire(mode)
            }
            niri_vibrancy_v1::Request::Destroy => {
                with_states(&surface, |states| {
                    if let Some(data) = states.data_map.get::<VibrancySurfaceData>() {
                        *data.0.lock().unwrap() = None;
                    }
                });
                Vibrancy::None
            }
        };

        with_states(&surface, |states| {
            states
                .cached_state
                .get::<VibrancyCachedState>()
                .pending()
                .mode = mode;
        });
    }
}

#[macro_export]
macro_rules! delegate_vibrancy {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::vibrancy::v1::server::niri_vibrancy_manager_v1::NiriVibrancyManagerV1: ()
        ] => $crate::protocols::vibrancy::VibrancyManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::vibrancy::v1::server::niri_vibrancy_manager_v1::NiriVibrancyManagerV1: ()
        ] => $crate::protocols::vibrancy::VibrancyManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::vibrancy::v1::server::niri_vibrancy_v1::NiriVibrancyV1: $crate::protocols::vibrancy::VibrancyUserData
        ] => $crate::protocols::vibrancy::VibrancyManagerState);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_from_the_wire() {
        assert_eq!(Vibrancy::from_wire(0), Vibrancy::None);
        assert_eq!(Vibrancy::from_wire(1), Vibrancy::PlusLighter);
        assert_eq!(Vibrancy::from_wire(2), Vibrancy::PlusDarker);
        assert_eq!(Vibrancy::from_wire(9), Vibrancy::None);
    }
}
