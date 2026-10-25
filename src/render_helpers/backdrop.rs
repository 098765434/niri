//! How bright the backdrop is behind each blur shape whose material reports it, for
//! niri-blur-shapes-v1's `backdrop` event: a client can then turn a small piece of glass light
//! or dark with what's behind it.
//!
//! When a shape is drawn, its area of the blurred capture is scaled down to `SIZE`×`SIZE` on the
//! GPU and read into a pixel-pack buffer behind a fence, so the frame never waits for it. A later
//! frame maps the buffer once the fence has passed. Each shape is read at most every `EVERY`,
//! and a reading is reported only when it moves by more than `STEP`.
//!
//! The buffers live until they're read. A surface that goes away with a reading in flight leaves
//! its small buffer and fence behind, since there's no GL context to delete them from then.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use smithay::backend::renderer::gles::ffi;
use smithay::utils::{Buffer, Rectangle};

/// The side of the scaled-down area that is read back, in pixels.
const SIZE: i32 = 8;
/// How long a shape waits between readings.
const EVERY: Duration = Duration::from_millis(250);
/// How far a reading must move from the last one reported to be reported.
const STEP: f32 = 0.08;

const BYTES: usize = (SIZE * SIZE * 4) as usize;

/// One surface's readings, shared by the elements drawing its shapes.
pub type SharedProbe = Rc<RefCell<Probe>>;

#[derive(Debug, Default)]
pub struct Probe {
    /// By the shape's place in the surface's committed list.
    shapes: HashMap<u32, Shape>,
    /// Readings to report, by shape.
    changed: Vec<(u32, f32)>,
}

#[derive(Debug, Default)]
struct Shape {
    pending: Option<Pending>,
    issued: Option<Instant>,
    reported: Option<f32>,
}

#[derive(Debug)]
struct Pending {
    buffer: ffi::types::GLuint,
    fence: ffi::types::GLsync,
}

impl Probe {
    /// Whether shape `index` is due a reading: none in flight, and the last one long enough ago.
    pub fn due(&self, index: u32) -> bool {
        self.shapes.get(&index).is_none_or(|shape| {
            shape.pending.is_none() && shape.issued.is_none_or(|at| at.elapsed() >= EVERY)
        })
    }

    /// Starts reading `src` of `texture` for shape `index`.
    ///
    /// # Safety
    ///
    /// `gl` is the current context, inside a frame; the bindings it changes are put back.
    pub unsafe fn issue(
        &mut self,
        gl: &ffi::Gles2,
        texture: ffi::types::GLuint,
        src: Rectangle<i32, Buffer>,
        index: u32,
    ) {
        if src.size.w <= 0 || src.size.h <= 0 {
            return;
        }

        while gl.GetError() != ffi::NO_ERROR {}

        let mut read_fbo = 0i32;
        let mut draw_fbo = 0i32;
        let mut pack = 0i32;
        gl.GetIntegerv(ffi::READ_FRAMEBUFFER_BINDING, &mut read_fbo);
        gl.GetIntegerv(ffi::DRAW_FRAMEBUFFER_BINDING, &mut draw_fbo);
        gl.GetIntegerv(ffi::PIXEL_PACK_BUFFER_BINDING, &mut pack);
        gl.Disable(ffi::SCISSOR_TEST);

        // the small target the area is scaled down into
        let mut small = 0;
        gl.GenTextures(1, &mut small);
        gl.BindTexture(ffi::TEXTURE_2D, small);
        gl.TexImage2D(
            ffi::TEXTURE_2D,
            0,
            ffi::RGBA as i32,
            SIZE,
            SIZE,
            0,
            ffi::RGBA,
            ffi::UNSIGNED_BYTE,
            std::ptr::null(),
        );
        gl.BindTexture(ffi::TEXTURE_2D, 0);

        let mut fbos = [0; 2];
        gl.GenFramebuffers(2, fbos.as_mut_ptr());
        gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, fbos[0]);
        gl.FramebufferTexture2D(
            ffi::READ_FRAMEBUFFER,
            ffi::COLOR_ATTACHMENT0,
            ffi::TEXTURE_2D,
            texture,
            0,
        );
        gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, fbos[1]);
        gl.FramebufferTexture2D(
            ffi::DRAW_FRAMEBUFFER,
            ffi::COLOR_ATTACHMENT0,
            ffi::TEXTURE_2D,
            small,
            0,
        );
        gl.BlitFramebuffer(
            src.loc.x,
            src.loc.y,
            src.loc.x + src.size.w,
            src.loc.y + src.size.h,
            0,
            0,
            SIZE,
            SIZE,
            ffi::COLOR_BUFFER_BIT,
            ffi::LINEAR,
        );

        // read it into a buffer, without waiting
        gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, fbos[1]);
        let mut buffer = 0;
        gl.GenBuffers(1, &mut buffer);
        gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, buffer);
        gl.BufferData(
            ffi::PIXEL_PACK_BUFFER,
            BYTES as isize,
            std::ptr::null(),
            ffi::STREAM_READ,
        );
        gl.ReadPixels(
            0,
            0,
            SIZE,
            SIZE,
            ffi::RGBA,
            ffi::UNSIGNED_BYTE,
            std::ptr::null_mut(),
        );
        let fence = gl.FenceSync(ffi::SYNC_GPU_COMMANDS_COMPLETE, 0);

        gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, pack as u32);
        gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, read_fbo as u32);
        gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, draw_fbo as u32);
        gl.Enable(ffi::SCISSOR_TEST);
        gl.DeleteFramebuffers(2, fbos.as_ptr());
        gl.DeleteTextures(1, &small);

        let shape = self.shapes.entry(index).or_default();
        shape.issued = Some(Instant::now());

        // a texture that can't be blitted from (another format) gives no reading
        if gl.GetError() != ffi::NO_ERROR || fence.is_null() {
            gl.DeleteBuffers(1, &buffer);
            if !fence.is_null() {
                gl.DeleteSync(fence);
            }
            return;
        }

        shape.pending = Some(Pending { buffer, fence });
    }

    /// Whether a reading is in flight.
    pub fn waiting(&self) -> bool {
        self.shapes.values().any(|shape| shape.pending.is_some())
    }

    /// Takes in the readings whose fences have passed.
    ///
    /// # Safety
    ///
    /// `gl` is the current context; the bindings it changes are put back.
    pub unsafe fn poll(&mut self, gl: &ffi::Gles2) {
        if !self.waiting() {
            return;
        }

        let mut pack = 0i32;
        gl.GetIntegerv(ffi::PIXEL_PACK_BUFFER_BINDING, &mut pack);

        for (index, shape) in &mut self.shapes {
            let Some(pending) = &shape.pending else {
                continue;
            };

            let luminance = match gl.ClientWaitSync(pending.fence, 0, 0) {
                ffi::ALREADY_SIGNALED | ffi::CONDITION_SATISFIED => {
                    gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, pending.buffer);
                    let data = gl.MapBufferRange(
                        ffi::PIXEL_PACK_BUFFER,
                        0,
                        BYTES as isize,
                        ffi::MAP_READ_BIT,
                    ) as *const u8;
                    let luminance = (!data.is_null()).then(|| {
                        let read = mean_luminance(std::slice::from_raw_parts(data, BYTES));
                        gl.UnmapBuffer(ffi::PIXEL_PACK_BUFFER);
                        read
                    });
                    Some(luminance)
                }
                ffi::WAIT_FAILED => Some(None),
                _ => None,
            };

            // still in flight
            let Some(luminance) = luminance else {
                continue;
            };

            gl.DeleteBuffers(1, &pending.buffer);
            gl.DeleteSync(pending.fence);
            shape.pending = None;

            if let Some(luminance) = luminance {
                if shape
                    .reported
                    .is_none_or(|reported| (luminance - reported).abs() > STEP)
                {
                    shape.reported = Some(luminance);
                    self.changed.push((*index, luminance));
                }
            }
        }

        gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, pack as u32);
    }

    /// The readings to report since the last call.
    pub fn take_changed(&mut self) -> Vec<(u32, f32)> {
        std::mem::take(&mut self.changed)
    }

    /// Forgets the shapes at `count` and after, once none is in flight: the list got shorter.
    pub fn keep(&mut self, count: u32) {
        self.shapes
            .retain(|index, shape| *index < count || shape.pending.is_some());
    }
}

/// The mean luminance of premultiplied RGBA pixels, 0 to 1.
fn mean_luminance(pixels: &[u8]) -> f32 {
    let count = pixels.len() / 4;
    if count == 0 {
        return 0.;
    }

    let sum: f32 = pixels
        .chunks_exact(4)
        .map(|px| {
            let [r, g, b] = [px[0], px[1], px[2]].map(|c| f32::from(c) / 255.);
            0.2126 * r + 0.7152 * g + 0.0722 * b
        })
        .sum();
    sum / count as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luminance_of_black_white_and_grey() {
        assert_eq!(mean_luminance(&[0, 0, 0, 255].repeat(4)), 0.);
        assert!((mean_luminance(&[255, 255, 255, 255].repeat(4)) - 1.).abs() < 1e-5);
        let half = [[0, 0, 0, 255], [255, 255, 255, 255]].concat();
        assert!((mean_luminance(&half) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn a_shape_is_due_until_read_then_waits() {
        let mut probe = Probe::default();
        assert!(probe.due(3));
        probe.shapes.entry(3).or_default().issued = Some(Instant::now());
        assert!(!probe.due(3));
        probe.shapes.get_mut(&3).unwrap().issued = Some(Instant::now() - EVERY);
        assert!(probe.due(3));
    }

    #[test]
    fn shapes_past_the_list_are_forgotten() {
        let mut probe = Probe::default();
        probe.shapes.insert(0, Shape::default());
        probe.shapes.insert(4, Shape::default());
        probe.keep(2);
        assert!(probe.shapes.contains_key(&0));
        assert!(!probe.shapes.contains_key(&4));
    }
}
