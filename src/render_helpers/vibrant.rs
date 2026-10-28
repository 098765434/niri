//! A surface's texture drawn as niri-vibrancy-v1 asks: blended with what's already drawn behind
//! it rather than painted over it. Plus-lighter adds the surface to what's behind, clamped;
//! plus-darker takes the surface away from what's behind, clamped at 0, the client having drawn
//! how far each pixel darkens. Both are only blend state, so they work under any texture
//! program (rounded clipping too), and the surface is never opaque or scanned out, which would
//! skip the blend.
//!
//! It also brings a surface with its own colour description (wp-color-management-v1: HDR video,
//! a game's scRGB) into the composition, through `cm_surface`; a clipped surface does that in
//! its own program instead (`clipped_surface_cm`), so the two never stack.

use std::ops::Deref;

use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{ffi, GlesError, GlesFrame, GlesRenderer};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Transform};

use super::color;
use super::renderer::{AsGlesFrame as _, NiriRenderer};
use super::shaders::Shaders;
use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};
use crate::protocols::color_management::ColorParams;
use crate::protocols::vibrancy::Vibrancy;

#[derive(Debug)]
pub struct VibrantSurfaceRenderElement<R: NiriRenderer> {
    inner: WaylandSurfaceRenderElement<R>,
    mode: Vibrancy,
    /// How to convert it, when it has a colour description of its own.
    color: Option<ColorParams>,
}

impl<R: NiriRenderer> VibrantSurfaceRenderElement<R> {
    pub fn new(
        inner: WaylandSurfaceRenderElement<R>,
        mode: Vibrancy,
        color: Option<ColorParams>,
    ) -> Self {
        Self { inner, mode, color }
    }

    pub fn mode(&self) -> Vibrancy {
        self.mode
    }

    pub fn color(&self) -> Option<ColorParams> {
        self.color
    }

    /// Whether it may go straight to a plane: not when it blends, nor when it's converted.
    fn scanout(&self) -> bool {
        self.mode == Vibrancy::None && self.color.is_none()
    }
}

/// Draws `inner`, converted from its colour description unless `convert` is off (a clipped
/// surface converts in its own program).
fn draw_converted(
    frame: &mut GlesFrame<'_, '_>,
    color: Option<ColorParams>,
    convert: bool,
    draw: impl FnOnce(&mut GlesFrame<'_, '_>) -> Result<(), GlesError>,
) -> Result<(), GlesError> {
    let program = match color {
        Some(_) if convert => Shaders::get_from_frame(frame).cm_surface.clone(),
        _ => None,
    };
    let (Some(params), Some(program)) = (color, program) else {
        return draw(frame);
    };
    let target = color::target(frame);
    frame.override_default_tex_program(program, color::uniforms(&params, target));
    let drawn = draw(frame);
    frame.clear_tex_program_override();
    drawn
}

impl<R: NiriRenderer> Deref for VibrantSurfaceRenderElement<R> {
    type Target = WaylandSurfaceRenderElement<R>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<R: NiriRenderer> Element for VibrantSurfaceRenderElement<R> {
    fn id(&self) -> &Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.inner.location(scale)
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        // an opaque region is drawn without blending, so a vibrant surface has none
        match self.mode {
            Vibrancy::None => self.inner.opaque_regions(scale),
            Vibrancy::PlusLighter | Vibrancy::PlusDarker => OpaqueRegions::default(),
        }
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        match self.mode {
            Vibrancy::None => self.inner.kind(),
            Vibrancy::PlusLighter | Vibrancy::PlusDarker => Kind::Unspecified,
        }
    }
}

/// The blend a mode draws with: the colour equation and its source and destination factors.
/// Alpha is always kept as it was (`ZERO, ONE` added).
fn blend(mode: Vibrancy) -> Option<(ffi::types::GLenum, ffi::types::GLenum, ffi::types::GLenum)> {
    match mode {
        Vibrancy::None => None,
        // D + S, clamped
        Vibrancy::PlusLighter => Some((ffi::FUNC_ADD, ffi::ONE, ffi::ONE)),
        // D - S, clamped at 0: S is how far the client darkens, a(1 - c) for a grey c
        Vibrancy::PlusDarker => Some((ffi::FUNC_REVERSE_SUBTRACT, ffi::ONE, ffi::ONE)),
    }
}

/// Sets the blend `mode` draws with, for the draw that follows.
///
/// smithay sets `ONE, ONE_MINUS_SRC_ALPHA` once and only puts it back after drawing an opaque
/// region, which a vibrant surface never has.
fn set_blend(frame: &mut GlesFrame<'_, '_>, mode: Vibrancy) -> Result<(), GlesError> {
    let Some((equation, src, dst)) = blend(mode) else {
        return Ok(());
    };
    frame.with_context(|gl| unsafe {
        gl.BlendEquationSeparate(equation, ffi::FUNC_ADD);
        gl.BlendFuncSeparate(src, dst, ffi::ZERO, ffi::ONE);
    })
}

/// Puts smithay's blend back after a vibrant draw.
fn reset_blend(frame: &mut GlesFrame<'_, '_>, mode: Vibrancy) -> Result<(), GlesError> {
    if mode == Vibrancy::None {
        return Ok(());
    }
    frame.with_context(|gl| unsafe {
        gl.BlendEquation(ffi::FUNC_ADD);
        gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
    })
}

impl RenderElement<GlesRenderer> for VibrantSurfaceRenderElement<GlesRenderer> {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        self.draw_gles(frame, src, dst, damage, opaque_regions, cache, true)
    }

    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        // scanned out, the surface would skip the blend and the conversion
        if self.scanout() {
            self.inner.underlying_storage(renderer)
        } else {
            None
        }
    }
}

impl VibrantSurfaceRenderElement<GlesRenderer> {
    #[allow(clippy::too_many_arguments)]
    pub fn draw_gles(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
        convert: bool,
    ) -> Result<(), GlesError> {
        set_blend(frame, self.mode)?;
        let drawn = draw_converted(frame, self.color, convert, |frame| {
            RenderElement::<GlesRenderer>::draw(
                &self.inner,
                frame,
                src,
                dst,
                damage,
                opaque_regions,
                cache,
            )
        });
        reset_blend(frame, self.mode)?;
        drawn
    }
}

impl<'render> VibrantSurfaceRenderElement<TtyRenderer<'render>> {
    #[allow(clippy::too_many_arguments)]
    pub fn draw_tty(
        &self,
        frame: &mut TtyFrame<'render, '_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
        convert: bool,
    ) -> Result<(), TtyRendererError<'render>> {
        set_blend(frame.as_gles_frame(), self.mode)?;
        let program = match self.color {
            Some(_) if convert => Shaders::get_from_frame(frame.as_gles_frame())
                .cm_surface
                .clone(),
            _ => None,
        };
        let overridden = if let (Some(params), Some(program)) = (self.color, program) {
            let gles = frame.as_gles_frame();
            let target = color::target(gles);
            gles.override_default_tex_program(program, color::uniforms(&params, target));
            true
        } else {
            false
        };
        let drawn =
            RenderElement::draw(&self.inner, frame, src, dst, damage, opaque_regions, cache);
        if overridden {
            frame.as_gles_frame().clear_tex_program_override();
        }
        reset_blend(frame.as_gles_frame(), self.mode)?;
        drawn
    }
}

impl<'render> RenderElement<TtyRenderer<'render>>
    for VibrantSurfaceRenderElement<TtyRenderer<'render>>
{
    fn draw(
        &self,
        frame: &mut TtyFrame<'render, '_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), TtyRendererError<'render>> {
        self.draw_tty(frame, src, dst, damage, opaque_regions, cache, true)
    }

    fn underlying_storage(
        &self,
        renderer: &mut TtyRenderer<'render>,
    ) -> Option<UnderlyingStorage<'_>> {
        if self.scanout() {
            self.inner.underlying_storage(renderer)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends_per_mode() {
        assert_eq!(blend(Vibrancy::None), None);
        assert_eq!(
            blend(Vibrancy::PlusLighter),
            Some((ffi::FUNC_ADD, ffi::ONE, ffi::ONE))
        );
        assert_eq!(
            blend(Vibrancy::PlusDarker),
            Some((ffi::FUNC_REVERSE_SUBTRACT, ffi::ONE, ffi::ONE))
        );
    }
}
