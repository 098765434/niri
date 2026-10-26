use std::sync::{Arc, Mutex};

use glam::Mat3;
use niri_config::{CornerRadius, MaterialParams};
use smithay::backend::renderer::gles::{GlesRenderer, Uniform};
use smithay::utils::{Logical, Point, Rectangle, Scale};
use smithay::wayland::compositor::{with_states, SurfaceData};
use wayland_server::protocol::wl_surface::WlSurface;

use crate::handlers::background_effect::{get_cached_blur_area, BlurArea};
use crate::niri_render_elements;
use crate::protocols::blur_shapes::{self, BlurShape};
use crate::render_helpers::blur::BlurOptions;
use crate::render_helpers::damage::ExtraDamage;
use crate::render_helpers::framebuffer_effect::{
    FramebufferEffect, FramebufferEffectElement, FramebufferShapeElement,
};
use crate::render_helpers::shaders::mat3_uniform;
use crate::render_helpers::xray::{XrayElement, XrayPos};
use crate::render_helpers::RenderCtx;
use crate::utils::region::TransformedRegion;
use crate::utils::surface_geo;

#[derive(Debug)]
pub struct BackgroundEffect {
    nonxray: FramebufferEffect,
    /// Damage when options change.
    damage: ExtraDamage,
    /// Corner radius for clipping.
    ///
    /// Stored here in addition to `RenderParams` to damage when it changes.
    // FIXME: would be good to remove this duplication of radius.
    corner_radius: CornerRadius,
    blur_config: niri_config::Blur,
    options: Options,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Options {
    pub blur: bool,
    pub xray: bool,
    pub noise: Option<f64>,
    pub saturation: Option<f64>,
    /// The rule's material, drawn in the shape of the geometry.
    pub material: Option<MaterialParams>,
}

impl Options {
    fn is_visible(&self) -> bool {
        self.xray
            || self.blur
            || self.noise.is_some_and(|x| x > 0.)
            || self.saturation.is_some_and(|x| x != 1.)
    }
}

/// Render-time parameters.
#[derive(Debug)]
pub struct RenderParams {
    /// Geometry of the background effect.
    pub geometry: Rectangle<f64, Logical>,
    /// Effect subregion, will be clipped to `geometry`.
    ///
    /// `subregion.iter()` should return `geometry`-relative rectangles.
    pub subregion: Option<TransformedRegion>,
    /// Geometry and radius for clipping in the same coordinate space as `geometry`.
    pub clip: Option<(Rectangle<f64, Logical>, CornerRadius)>,
    /// Scale to use for rounding to physical pixels.
    pub scale: f64,
    /// Opacity of the whole effect.
    pub alpha: f32,
    /// Whether `clip` is a shape the surface drew itself, to follow exactly rather than with
    /// the rule's corner radius.
    pub exact_clip: bool,
    /// The material to draw with instead of plain blur.
    pub material: Option<MaterialParams>,
    /// Premultiplied tint for the material.
    pub tint: [f32; 4],
    /// The shape's place in the surface's list, when its material reports the backdrop.
    pub report: Option<u32>,
}

/// The configured materials and their names, by id less one, as `niri-blur-shapes-v1`
/// announces them.
static MATERIALS: Mutex<Vec<(String, MaterialParams)>> = Mutex::new(Vec::new());

pub fn set_materials(materials: &niri_config::Materials) {
    *MATERIALS.lock().unwrap() = materials
        .0
        .iter()
        .map(|m| (m.name.clone(), m.params()))
        .collect();
}

/// The material a shape names, none for plain blur or an id that isn't configured.
fn material(id: u32) -> Option<MaterialParams> {
    let index = usize::try_from(id).ok()?.checked_sub(1)?;
    MATERIALS
        .lock()
        .unwrap()
        .get(index)
        .map(|(_, params)| *params)
}

/// The material a rule names, none if it isn't configured.
fn material_named(name: &str) -> Option<MaterialParams> {
    MATERIALS
        .lock()
        .unwrap()
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, params)| *params)
}

/// The glass shader's own uniforms, for an element whose shader input maps to its clip
/// geometry through `input_to_geo`, drawn at `alpha`. A material that materialises bends
/// and lights the backdrop by as much as it shows, so the lens builds up as it fades in
/// rather than a finished lens fading.
pub fn glass_uniforms(
    material: &MaterialParams,
    tint: [f32; 4],
    input_to_geo: Mat3,
    alpha: f32,
) -> [Uniform<'static>; 5] {
    let (sin, cos) = material.light_angle.to_radians().sin_cos();
    let grown = if material.materialize {
        alpha.clamp(0., 1.)
    } else {
        1.
    };
    [
        mat3_uniform("geo_to_input", input_to_geo.inverse()),
        Uniform::new("glass_tint", tint),
        Uniform::new(
            "glass_shape",
            [
                material.refraction * grown,
                material.bevel,
                material.dispersion * grown,
                material.rim * grown,
            ],
        ),
        Uniform::new(
            "glass_light",
            [cos, sin, material.far_rim, material.specular * grown],
        ),
        Uniform::new("glass_tint_params", [material.tint, material.adaptive_tint]),
    ]
}

impl RenderParams {
    fn fit_clip_radius(&mut self) {
        if let Some((geo, radius)) = &mut self.clip {
            // HACK: increase radius to avoid slight bleed on rounded corners.
            *radius = radius.expanded_by(1.);

            *radius = radius.fit_to(geo.size.w as f32, geo.size.h as f32);
        }
    }
}

niri_render_elements! {
    BackgroundEffectElement => {
        FramebufferEffect = FramebufferEffectElement,
        FramebufferShape = FramebufferShapeElement,
        Xray = XrayElement,
        ExtraDamage = ExtraDamage,
    }
}

impl BackgroundEffect {
    pub fn new() -> Self {
        Self {
            nonxray: FramebufferEffect::new(),
            damage: ExtraDamage::new(),
            corner_radius: CornerRadius::default(),
            blur_config: niri_config::Blur::default(),
            options: Options::default(),
        }
    }

    /// Damage the background effect, for example when a blur subregion changes.
    pub fn damage(&mut self) {
        self.damage.damage_all();
        self.nonxray.damage();
    }

    pub fn update_config(&mut self, config: niri_config::Blur) {
        if self.blur_config == config {
            return;
        }

        self.blur_config = config;
        self.damage.damage_all();
        self.nonxray.damage();
    }

    pub fn update_render_elements(
        &mut self,
        corner_radius: CornerRadius,
        effect: niri_config::BackgroundEffect,
        has_blur_region: bool,
    ) {
        // If the surface explicitly requests a blur region, default blur to true.
        let blur = if has_blur_region {
            effect.blur != Some(false)
        } else {
            effect.blur == Some(true)
        };

        let mut options = Options {
            blur,
            xray: effect.xray == Some(true),
            noise: effect.noise,
            saturation: effect.saturation,
            // looked up each time, so a reloaded material damages through the comparison below
            material: effect.material.as_deref().and_then(material_named),
        };

        // If we have some background effect but xray wasn't explicitly set, default it to true
        // since it's cheaper.
        if options.is_visible() && effect.xray.is_none() {
            options.xray = true;
        }

        if self.options == options && self.corner_radius == corner_radius {
            return;
        }

        self.options = options;
        self.corner_radius = corner_radius;
        self.damage.damage_all();
        self.nonxray.damage();
    }

    pub fn is_visible(&self) -> bool {
        self.options.is_visible()
    }

    /// Renders the effect for each of `areas`, with the position of each against the xray
    /// backdrop. `bounds` covers them all, for damage.
    pub fn render(
        &self,
        ctx: RenderCtx<GlesRenderer>,
        ns: Option<usize>,
        bounds: Rectangle<f64, Logical>,
        mut areas: Vec<(RenderParams, XrayPos)>,
        push: &mut dyn FnMut(BackgroundEffectElement),
    ) {
        if !self.is_visible() {
            return;
        }

        for (params, _) in &mut areas {
            if params.exact_clip {
                continue;
            }
            if let Some(clip) = &mut params.clip {
                clip.1 = self.corner_radius;
            }
            params.fit_clip_radius();
        }

        let damage = self.damage.render(bounds);

        // Use noise/saturation from options, falling back to blur defaults if blurred, and
        // to no effect if not blurred.
        let blur = self.options.blur && !self.blur_config.off;
        let blur_options = blur.then_some(BlurOptions::from(self.blur_config));
        let noise = if blur { self.blur_config.noise } else { 0. };
        let noise = self.options.noise.unwrap_or(noise) as f32;
        let saturation = if blur {
            self.blur_config.saturation
        } else {
            1.
        };
        let saturation = self.options.saturation.unwrap_or(saturation) as f32;

        if self.options.xray {
            let Some(xray) = ctx.xray else {
                return;
            };

            push(damage.into());
            xray.render(ctx, areas, blur_options, noise, saturation, &mut |elem| {
                push(elem.into())
            });
        } else {
            // Render non-xray effect: shapes draw from one capture of the area behind them all.
            let shapes = areas.first().is_some_and(|(params, _)| params.exact_clip);
            if shapes {
                let shapes = areas.into_iter().map(|(params, _)| params).collect();
                self.nonxray.render_shapes(
                    ns,
                    shapes,
                    blur_options,
                    noise,
                    saturation,
                    &mut |elem| push(elem.into()),
                );
            } else {
                for (params, _) in areas {
                    let elem = self
                        .nonxray
                        .render(ns, params, blur_options, noise, saturation);
                    push(elem.into());
                }
            }
        }
    }
}

fn render_params_for_tile(
    geometry: Rectangle<f64, Logical>,
    scale: f64,
    clip_to_geometry: bool,
    block_out: bool,
    blur_region: Option<Arc<Vec<Rectangle<i32, Logical>>>>,
    surface_geo: Rectangle<f64, Logical>,
    surface_anim_scale: Scale<f64>,
) -> Option<RenderParams> {
    // Effects not requested by the surface itself are drawn to match the geometry.
    let mut clip = true;

    let mut effect_geometry = geometry;
    let mut subregion = None;
    if let Some(rects) = blur_region {
        if rects.is_empty() {
            // Surface has a set, but empty blur region.
            return None;
        } else {
            // If the surface itself requests the effects, apply different defaults.
            clip = clip_to_geometry;

            // Use geometry-shaped blur for blocked-out windows to avoid unintentionally
            // leaking any surface shapes. We render those windows as geometry-shaped solid
            // rectangles anyway.
            if block_out {
                clip = true;
            } else {
                let mut surface_geo = surface_geo.upscale(surface_anim_scale);
                surface_geo.loc += geometry.loc;

                subregion = Some(TransformedRegion {
                    rects,
                    scale: surface_anim_scale,
                    offset: surface_geo.loc,
                });

                surface_geo = surface_geo
                    .to_physical_precise_round(scale)
                    .to_logical(scale);
                effect_geometry = surface_geo;
            }
        }
    }

    // This corner radius is reset to self.corner_radius in render().
    let clip = clip.then_some((geometry, CornerRadius::default()));

    Some(RenderParams {
        geometry: effect_geometry,
        subregion,
        clip,
        scale,
        alpha: 1.,
        exact_clip: false,
        material: None,
        tint: [0.; 4],
        report: None,
    })
}

/// The render parameters for each of a surface's blur shapes that shows, drawn at
/// `surface_geo` (its position and size as drawn) and scaled by `surface_anim_scale`.
fn render_params_for_shapes(
    shapes: &[BlurShape],
    scale: f64,
    surface_geo: Rectangle<f64, Logical>,
    surface_anim_scale: Scale<f64>,
) -> Vec<RenderParams> {
    // half a physical pixel inside the shape, so the effect stays under the antialiased edge
    // the client draws over it
    let inset = 0.5 / scale;
    // the quad reaches a pixel past the clip, so rounding it to pixels never cuts the clip's
    // antialiased corners
    let margin = 1. / scale;

    shapes
        .iter()
        .zip(0u32..)
        .filter(|(shape, _)| shape.alpha > 0.)
        .filter_map(|(shape, index)| {
            let mut rect = shape.rect.upscale(surface_anim_scale);
            rect.loc += surface_geo.loc;

            let clip = Rectangle::new(
                rect.loc + Point::new(inset, inset),
                (rect.size.to_point() - Point::new(inset * 2., inset * 2.)).to_size(),
            );
            if clip.size.w <= 0. || clip.size.h <= 0. {
                return None;
            }

            let anim_scale = surface_anim_scale.x.min(surface_anim_scale.y) as f32;
            let radius = shape
                .radius
                .scaled_by(anim_scale)
                .expanded_by(-inset as f32)
                .fit_to(clip.size.w as f32, clip.size.h as f32);

            let geometry = Rectangle::new(
                clip.loc - Point::new(margin, margin),
                (clip.size.to_point() + Point::new(margin * 2., margin * 2.)).to_size(),
            );

            Some(RenderParams {
                geometry,
                subregion: None,
                clip: Some((clip, radius)),
                scale,
                alpha: shape.alpha,
                exact_clip: true,
                material: material(shape.material),
                tint: shape.tint,
                report: material(shape.material)
                    .is_some_and(|m| m.report_backdrop)
                    .then_some(index),
            })
        })
        .collect()
}

/// The render parameters for `material` filling `geometry` with `radius` corners, drawn the
/// way a blur shape is (see `render_params_for_shapes`). The tint is clear: the client draws
/// its own fill over it.
fn material_params(
    geometry: Rectangle<f64, Logical>,
    radius: CornerRadius,
    scale: f64,
    material: MaterialParams,
) -> RenderParams {
    let inset = 0.5 / scale;
    let margin = 1. / scale;

    let clip = Rectangle::new(
        geometry.loc + Point::new(inset, inset),
        (geometry.size.to_point() - Point::new(inset * 2., inset * 2.)).to_size(),
    );
    let radius = radius
        .expanded_by(-inset as f32)
        .fit_to(clip.size.w.max(0.) as f32, clip.size.h.max(0.) as f32);
    let geometry = Rectangle::new(
        clip.loc - Point::new(margin, margin),
        (clip.size.to_point() + Point::new(margin * 2., margin * 2.)).to_size(),
    );

    RenderParams {
        geometry,
        subregion: None,
        clip: Some((clip, radius)),
        scale,
        alpha: 1.,
        exact_clip: true,
        material: Some(material),
        tint: [0.; 4],
        report: None,
    }
}

/// Per-surface background effect stored in its data map.
struct SurfaceBackgroundEffect(Mutex<BackgroundEffect>);

impl SurfaceBackgroundEffect {
    fn get(states: &SurfaceData) -> &Self {
        states
            .data_map
            .get_or_insert(|| SurfaceBackgroundEffect(Mutex::new(BackgroundEffect::new())))
    }
}

pub fn damage_surface(states: &SurfaceData) {
    if let Some(effect) = states.data_map.get::<SurfaceBackgroundEffect>() {
        effect.0.lock().unwrap().damage();
    }
}

// Silence, Clippy
// A Smithay user is talking
#[allow(clippy::too_many_arguments)]
pub fn render_for_tile(
    ctx: RenderCtx<GlesRenderer>,
    ns: Option<usize>,
    geometry: Rectangle<f64, Logical>,
    scale: f64,
    clip_to_geometry: bool,
    surface: &WlSurface,
    surface_off: Point<f64, Logical>,
    surface_anim_scale: Scale<f64>,
    blur_config: niri_config::Blur,
    radius: CornerRadius,
    effect: niri_config::BackgroundEffect,
    should_block_out: bool,
    xray_pos: XrayPos,
    push: &mut dyn FnMut(BackgroundEffectElement),
) {
    with_states(surface, |states| {
        let background_effect = SurfaceBackgroundEffect::get(states);
        let mut background_effect = background_effect.0.lock().unwrap();

        let blur_area = get_cached_blur_area(states);
        let has_blur_region = blur_area.as_ref().is_some_and(|r| !r.is_empty());

        background_effect.update_config(blur_config);
        background_effect.update_render_elements(radius, effect, has_blur_region);

        if !background_effect.is_visible() {
            return;
        }

        let mut surface_geo = surface_geo(states).unwrap_or_default().to_f64();
        surface_geo.loc += surface_off;

        // Blocked-out surfaces get geometry-shaped blur, like with a region.
        if let (Some(BlurArea::Shapes(shapes)), false) = (&blur_area, should_block_out) {
            // tell the client what the readings of the backdrop taken while drawing earlier
            // frames came to
            {
                let mut probe = background_effect.nonxray.probe().borrow_mut();
                probe.keep(shapes.len() as u32);
                if probe.waiting() {
                    let _ = ctx.renderer.with_context(|gl| unsafe { probe.poll(gl) });
                }
                for (index, luminance) in probe.take_changed() {
                    blur_shapes::send_backdrop(states, index, luminance);
                }
            }

            let mut shape_geo = surface_geo.upscale(surface_anim_scale);
            shape_geo.loc += geometry.loc;

            let areas: Vec<_> =
                render_params_for_shapes(shapes, scale, shape_geo, surface_anim_scale)
                    .into_iter()
                    .map(|params| {
                        let pos = xray_pos.offset(params.geometry.loc - geometry.loc);
                        (params, pos)
                    })
                    .collect();
            let Some(bounds) = areas
                .iter()
                .map(|(params, _)| params.geometry)
                .reduce(|a, b| a.merge(b))
            else {
                return;
            };

            background_effect.render(ctx, ns, bounds, areas, push);
            return;
        }

        let blur_region = match blur_area {
            Some(BlurArea::Rects(rects)) => Some(rects),
            // a blocked-out surface with shapes: any non-empty region gives geometry blur
            Some(BlurArea::Shapes(_)) => Some(Arc::new(vec![Rectangle::default()])),
            None => None,
        };

        let Some(mut params) = render_params_for_tile(
            geometry,
            scale,
            clip_to_geometry,
            should_block_out,
            blur_region,
            surface_geo,
            surface_anim_scale,
        ) else {
            return;
        };

        // A rule's material is drawn like a blur shape the size of the geometry, with the
        // rule's corners. It needs the geometry as its clip: a surface's own blur region is
        // left to plain blur.
        if let Some(material) = background_effect.options.material {
            if let Some((clip, _)) = params.clip {
                params = material_params(clip, radius, scale, material);
            }
        }

        let bounds = params.geometry;
        let xray_pos = xray_pos.offset(params.geometry.loc - geometry.loc);
        background_effect.render(ctx, ns, bounds, vec![(params, xray_pos)], push);
    });
}
