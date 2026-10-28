//! What a render is for, colour-wise: an output in HDR mode, which shows light past SDR white,
//! or anything else (an SDR output, a screenshot, a screencast), which doesn't.
//!
//! Elements are drawn without knowing where to, so the target rides on the renderer's EGL
//! context: `Tty::render` sets [`ColorTarget::hdr`] around an HDR output's composition, and every
//! other render sees the SDR default. A surface converted from its own colour description rolls
//! its highlights off to the target's peak, which is how an HDR video in a screenshot comes out
//! in SDR.

use std::cell::Cell;

use glam::Mat3;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::gles::{GlesFrame, GlesRenderer, Uniform};

use super::shaders::mat3_uniform;
use crate::protocols::color_management::ColorParams;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorTarget {
    /// The most the target shows, SDR white being 1.0.
    pub peak: f32,
}

impl ColorTarget {
    pub const SDR: Self = Self { peak: 1. };

    /// An output in HDR mode showing up to `peak_nits`, its SDR white at `sdr_white` nits.
    pub fn hdr(sdr_white: f32, peak_nits: f32) -> Self {
        Self {
            peak: (peak_nits / sdr_white).max(1.),
        }
    }
}

struct TargetCell(Cell<ColorTarget>);

/// Sets what the renderer's draws are for, until set again.
pub fn set_target(renderer: &mut GlesRenderer, target: ColorTarget) {
    let data = renderer.egl_context().user_data();
    data.insert_if_missing(|| TargetCell(Cell::new(ColorTarget::SDR)));
    if let Some(cell) = data.get::<TargetCell>() {
        cell.0.set(target);
    }
}

fn renderer_target(renderer: &mut GlesRenderer) -> ColorTarget {
    renderer
        .egl_context()
        .user_data()
        .get::<TargetCell>()
        .map_or(ColorTarget::SDR, |cell| cell.0.get())
}

/// The format for an intermediate buffer drawn now: half-float for an HDR target, so a window
/// fading or moving keeps its highlights, else 8-bit.
pub fn buffer_format(renderer: &mut GlesRenderer) -> Fourcc {
    if renderer_target(renderer).peak > 1. {
        Fourcc::Abgr16161616f
    } else {
        Fourcc::Abgr8888
    }
}

pub fn target(frame: &mut GlesFrame<'_, '_>) -> ColorTarget {
    frame
        .egl_context()
        .user_data()
        .get::<TargetCell>()
        .map_or(ColorTarget::SDR, |cell| cell.0.get())
}

/// The `color_convert.frag` uniforms for a surface drawn for `target`.
pub fn uniforms(params: &ColorParams, target: ColorTarget) -> Vec<Uniform<'static>> {
    // the params are row-major, glam is column-major
    let matrix = Mat3::from_cols_array(&params.matrix).transpose();
    vec![
        Uniform::new("cm_tf", params.tf),
        Uniform::new("cm_power", params.power),
        mat3_uniform("cm_matrix", matrix),
        Uniform::new("cm_scale", params.scale),
        Uniform::new("cm_content_peak", params.content_peak),
        Uniform::new("cm_target_peak", target.peak),
    ]
}
