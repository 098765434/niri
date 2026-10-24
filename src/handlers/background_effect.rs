use std::sync::{Arc, Mutex};

use smithay::delegate_background_effect;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::background_effect::{
    self, BackgroundEffectSurfaceCachedState, ExtBackgroundEffectHandler,
};
use smithay::wayland::compositor::{
    add_post_commit_hook, with_states, RegionAttributes, SurfaceData,
};

use crate::delegate_blur_shapes;
use crate::niri::State;
use crate::protocols::blur_shapes::{BlurShape, BlurShapesCachedState, BlurShapesHandler};
use crate::utils::region::region_to_non_overlapping_rects;

/// Per-surface cache for processed blur region (non-overlapping rects).
#[derive(Default)]
struct CachedBlurRegionUserData(Mutex<CachedBlurRegionInner>);

#[derive(Default)]
struct CachedBlurRegionInner {
    /// Whether a region change is pending to be committed.
    pending_dirty: bool,
    /// Whether the region must be recomputed.
    dirty: bool,
    /// Whether the post-commit hook has been registered for this surface.
    hook_registered: bool,
    /// Cached non-overlapping rects in surface-local coordinates.
    ///
    /// `None` means there's no blur region.
    rects: Option<Arc<Vec<Rectangle<i32, Logical>>>>,
    /// Committed blur shapes, which take the place of the region when set.
    shapes: Option<Arc<Vec<BlurShape>>>,
}

/// Where a surface asked for its background effect.
#[derive(Debug, Clone)]
pub enum BlurArea {
    /// Non-overlapping rects of its blur region, in surface-local coordinates.
    Rects(Arc<Vec<Rectangle<i32, Logical>>>),
    /// Its blur shapes, in surface-local coordinates.
    Shapes(Arc<Vec<BlurShape>>),
}

impl BlurArea {
    pub fn is_empty(&self) -> bool {
        match self {
            BlurArea::Rects(rects) => rects.is_empty(),
            BlurArea::Shapes(shapes) => shapes.is_empty(),
        }
    }
}

/// Gets the cached blur area for a surface, lazily recomputing if dirty.
///
/// `None` means the surface asked for no particular area.
pub fn get_cached_blur_area(states: &SurfaceData) -> Option<BlurArea> {
    let cache = states
        .data_map
        .get_or_insert_threadsafe(CachedBlurRegionUserData::default);
    let mut guard = cache.0.lock().unwrap();

    if guard.dirty {
        guard.dirty = false;
        recompute_blur_region(states, &mut guard);
        recompute_blur_shapes(states, &mut guard);
    }

    if let Some(shapes) = &guard.shapes {
        return Some(BlurArea::Shapes(shapes.clone()));
    }
    guard.rects.clone().map(BlurArea::Rects)
}

fn recompute_blur_shapes(states: &SurfaceData, inner: &mut CachedBlurRegionInner) {
    let cached = &states.cached_state;
    inner.shapes = if cached.has::<BlurShapesCachedState>() {
        let mut guard = cached.get::<BlurShapesCachedState>();
        guard.current().shapes.clone().map(Arc::new)
    } else {
        None
    };
}

fn recompute_blur_region(states: &SurfaceData, inner: &mut CachedBlurRegionInner) {
    let cached = &states.cached_state;

    let rects = if let Some(arc) = &mut inner.rects {
        if Arc::strong_count(arc) > 1 {
            debug!("cloning rects due to non-unique reference");
        }
        arc
    } else {
        inner.rects.insert(Arc::new(Vec::new()))
    };
    let rects = Arc::make_mut(rects);

    if cached.has::<BackgroundEffectSurfaceCachedState>() {
        let mut guard = cached.get::<BackgroundEffectSurfaceCachedState>();
        if let Some(region) = &guard.current().blur_region {
            region_to_non_overlapping_rects(region, rects);
        } else {
            inner.rects = None;
        }
        return;
    }

    inner.rects = None;
}

fn mark_blur_region_pending_dirty(wl_surface: &WlSurface) {
    let register_hook = with_states(wl_surface, |states| {
        let cache = states
            .data_map
            .get_or_insert_threadsafe(CachedBlurRegionUserData::default);
        let mut guard = cache.0.lock().unwrap();
        guard.pending_dirty = true;

        if guard.hook_registered {
            false
        } else {
            guard.hook_registered = true;
            true
        }
    });

    if register_hook {
        add_post_commit_hook::<State, _>(wl_surface, |_state, _dh, surface| {
            with_states(surface, |states| {
                if let Some(cache) = states.data_map.get::<CachedBlurRegionUserData>() {
                    let mut guard = cache.0.lock().unwrap();
                    if guard.pending_dirty {
                        guard.pending_dirty = false;
                        guard.dirty = true;

                        crate::render_helpers::background_effect::damage_surface(states);
                    }
                } else {
                    error!("unexpected missing CachedBlurRegionUserData");
                }
            });
        });
    }
}

impl ExtBackgroundEffectHandler for State {
    fn capabilities(&self) -> background_effect::Capability {
        background_effect::Capability::Blur
    }

    fn set_blur_region(&mut self, wl_surface: WlSurface, _region: RegionAttributes) {
        mark_blur_region_pending_dirty(&wl_surface);
    }

    fn unset_blur_region(&mut self, wl_surface: WlSurface) {
        mark_blur_region_pending_dirty(&wl_surface);
    }
}
delegate_background_effect!(State);

impl BlurShapesHandler for State {
    fn blur_shapes_state(&mut self) -> &mut crate::protocols::blur_shapes::BlurShapesManagerState {
        &mut self.niri.blur_shapes_state
    }

    fn blur_shapes_changed(&mut self, surface: WlSurface) {
        mark_blur_region_pending_dirty(&surface);
    }
}
delegate_blur_shapes!(State);
