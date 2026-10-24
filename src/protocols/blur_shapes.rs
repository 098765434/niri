//! niri-blur-shapes-v1: rounded, translucent blur shapes in place of a blur region.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use niri_config::CornerRadius;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, Weak,
};
use smithay::utils::{Logical, Point, Rectangle, Size};
use smithay::wayland::compositor::{with_states, Cacheable};

use super::raw::blur_shapes::v1::server::niri_blur_shapes_manager_v1::{
    self, NiriBlurShapesManagerV1,
};
use super::raw::blur_shapes::v1::server::niri_blur_shapes_v1::{self, NiriBlurShapesV1};

const VERSION: u32 = 1;

/// One rounded rectangle of blur, in surface-local coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlurShape {
    pub rect: Rectangle<f64, Logical>,
    pub radius: CornerRadius,
    pub alpha: f32,
    pub material: u32,
    /// Premultiplied RGBA.
    pub tint: [f32; 4],
}

/// The blur shapes of a surface, double-buffered with its commits.
///
/// `None` means the surface has no shapes and its blur region applies.
#[derive(Debug, Clone, Default)]
pub struct BlurShapesCachedState {
    pub shapes: Option<Vec<BlurShape>>,
}

impl Cacheable for BlurShapesCachedState {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        self.clone()
    }

    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        *into = self;
    }
}

pub trait BlurShapesHandler {
    /// The pending shapes of a surface changed, and apply on its next commit.
    fn blur_shapes_changed(&mut self, surface: WlSurface);
}

pub struct BlurShapesManagerState {}

pub struct BlurShapesUserData(Mutex<Weak<WlSurface>>);

impl BlurShapesUserData {
    fn wl_surface(&self) -> Option<WlSurface> {
        self.0.lock().unwrap().upgrade().ok()
    }
}

/// Whether a surface has a blur shapes object.
#[derive(Default)]
struct BlurShapesSurfaceData(AtomicBool);

impl BlurShapesManagerState {
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<NiriBlurShapesManagerV1, ()>,
        D: Dispatch<NiriBlurShapesManagerV1, ()>,
        D: Dispatch<NiriBlurShapesV1, BlurShapesUserData>,
        D: BlurShapesHandler,
        D: 'static,
    {
        display.create_global::<D, NiriBlurShapesManagerV1, _>(VERSION, ());
        Self {}
    }
}

impl<D> GlobalDispatch<NiriBlurShapesManagerV1, (), D> for BlurShapesManagerState
where
    D: GlobalDispatch<NiriBlurShapesManagerV1, ()>,
    D: Dispatch<NiriBlurShapesManagerV1, ()>,
    D: Dispatch<NiriBlurShapesV1, BlurShapesUserData>,
    D: BlurShapesHandler,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        manager: New<NiriBlurShapesManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, D>,
    ) {
        let manager = data_init.init(manager, ());
        manager.done();
    }
}

impl<D> Dispatch<NiriBlurShapesManagerV1, (), D> for BlurShapesManagerState
where
    D: Dispatch<NiriBlurShapesManagerV1, ()>,
    D: Dispatch<NiriBlurShapesV1, BlurShapesUserData>,
    D: BlurShapesHandler,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        manager: &NiriBlurShapesManagerV1,
        request: niri_blur_shapes_manager_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            niri_blur_shapes_manager_v1::Request::GetBlurShapes { id, surface } => {
                let attached = with_states(&surface, |states| {
                    let data = states
                        .data_map
                        .get_or_insert_threadsafe(BlurShapesSurfaceData::default);
                    data.0.swap(true, Ordering::AcqRel)
                });

                if attached {
                    manager.post_error(
                        niri_blur_shapes_manager_v1::Error::AlreadyConstructed,
                        "wl_surface already has a blur shapes object",
                    );
                    return;
                }

                data_init.init(id, BlurShapesUserData(Mutex::new(surface.downgrade())));
            }
            niri_blur_shapes_manager_v1::Request::Destroy => (),
        }
    }
}

impl<D> Dispatch<NiriBlurShapesV1, BlurShapesUserData, D> for BlurShapesManagerState
where
    D: Dispatch<NiriBlurShapesV1, BlurShapesUserData>,
    D: BlurShapesHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &NiriBlurShapesV1,
        request: niri_blur_shapes_v1::Request,
        data: &BlurShapesUserData,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        // requests on a destroyed surface's object are ignored, the object is inert
        let Some(surface) = data.wl_surface() else {
            return;
        };

        match request {
            niri_blur_shapes_v1::Request::Reset => {
                with_states(&surface, |states| {
                    let mut cached = states.cached_state.get::<BlurShapesCachedState>();
                    cached.pending().shapes = Some(Vec::new());
                });
            }
            niri_blur_shapes_v1::Request::AddRoundedRect {
                x,
                y,
                width,
                height,
                radius_top_left,
                radius_top_right,
                radius_bottom_right,
                radius_bottom_left,
                alpha,
                material,
                tint,
            } => {
                let shape = shape(
                    [x, y, width, height],
                    [
                        radius_top_left,
                        radius_top_right,
                        radius_bottom_right,
                        radius_bottom_left,
                    ],
                    alpha,
                    material,
                    tint,
                );
                with_states(&surface, |states| {
                    let mut cached = states.cached_state.get::<BlurShapesCachedState>();
                    let pending = cached.pending();
                    pending.shapes.get_or_insert_with(Vec::new).extend(shape);
                });
            }
            niri_blur_shapes_v1::Request::Destroy => {
                with_states(&surface, |states| {
                    if let Some(data) = states.data_map.get::<BlurShapesSurfaceData>() {
                        data.0.store(false, Ordering::Release);
                    }
                    let mut cached = states.cached_state.get::<BlurShapesCachedState>();
                    cached.pending().shapes = None;
                });
            }
        }

        state.blur_shapes_changed(surface);
    }
}

/// A shape from the wire, none when it covers nothing.
fn shape(
    rect: [f64; 4],
    radius: [f64; 4],
    alpha: f64,
    material: u32,
    tint: u32,
) -> Option<BlurShape> {
    let [x, y, w, h] = rect;
    if !(w > 0. && h > 0.) || !x.is_finite() || !y.is_finite() {
        return None;
    }

    let [tl, tr, br, bl] = radius.map(|r| r.max(0.) as f32);
    let radius = CornerRadius {
        top_left: tl,
        top_right: tr,
        bottom_right: br,
        bottom_left: bl,
    }
    .fit_to(w as f32, h as f32);

    let [r, g, b, a] = tint.to_be_bytes().map(|c| f32::from(c) / 255.);

    Some(BlurShape {
        rect: Rectangle::new(Point::new(x, y), Size::new(w, h)),
        radius,
        alpha: alpha.clamp(0., 1.) as f32,
        material,
        tint: [r, g, b, a],
    })
}

#[macro_export]
macro_rules! delegate_blur_shapes {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::blur_shapes::v1::server::niri_blur_shapes_manager_v1::NiriBlurShapesManagerV1: ()
        ] => $crate::protocols::blur_shapes::BlurShapesManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::blur_shapes::v1::server::niri_blur_shapes_manager_v1::NiriBlurShapesManagerV1: ()
        ] => $crate::protocols::blur_shapes::BlurShapesManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::blur_shapes::v1::server::niri_blur_shapes_v1::NiriBlurShapesV1: $crate::protocols::blur_shapes::BlurShapesUserData
        ] => $crate::protocols::blur_shapes::BlurShapesManagerState);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_fits_its_radii_and_clamps_its_alpha() {
        let s = shape([1., 2., 40., 20.], [30., 30., 30., 30.], 1.5, 0, 0).unwrap();
        assert_eq!(s.radius.top_left, 10.);
        assert_eq!(s.alpha, 1.);
        assert_eq!(s.rect.loc, Point::new(1., 2.));
    }

    #[test]
    fn an_empty_shape_is_dropped() {
        assert!(shape([0., 0., 0., 20.], [0.; 4], 1., 0, 0).is_none());
        assert!(shape([f64::NAN, 0., 10., 20.], [0.; 4], 1., 0, 0).is_none());
    }

    #[test]
    fn the_tint_is_read_as_rgba() {
        let s = shape([0., 0., 10., 10.], [0.; 4], 1., 0, 0x20406080).unwrap();
        assert_eq!(s.tint[0], 0x20 as f32 / 255.);
        assert_eq!(s.tint[3], 0x80 as f32 / 255.);
    }
}
