//! Three-finger touchpad swipes acting on the focused window while Mod is held.
//!
//! - Up: the window follows the fingers and fades; releasing far or fast enough closes it.
//! - Down: the window nudges down, then maximizes once the swipe passes a threshold.
//! - Left/right: the column steps one preset width narrower/wider per stretch of movement.
//!
//! Deltas are in raw touchpad units and follow the fingers regardless of natural scrolling.

use std::time::Duration;

use smithay::utils::{Logical, Point};

use super::tile::Tile;
use super::{Layout, LayoutElement, MonitorSet};
use crate::input::swipe_tracker::SwipeTracker;
use crate::rubber_band::RubberBand;

/// Touchpad movement before the gesture picks an axis. Copied from GNOME Shell, same as the normal
/// three-finger swipe.
const AXIS_DECISION_THRESHOLD: f64 = 16.;

/// Amount of touchpad movement to move the window by one view height.
const VERTICAL_MOVEMENT: f64 = 300.;

/// Fraction of the view height that a swipe up must reach, after deceleration, to close.
const CLOSE_THRESHOLD: f64 = 0.25;

/// Fraction of the view height over which the window fades while swiped up.
const CLOSE_FADE_DISTANCE: f64 = 0.5;

/// Window opacity once swiped up by `CLOSE_FADE_DISTANCE`.
const CLOSE_MIN_ALPHA: f64 = 0.4;

/// Amount of touchpad movement down that maximizes the window.
const MAXIMIZE_THRESHOLD: f64 = 60.;

/// How the window follows the fingers down before maximizing.
const MAXIMIZE_RUBBER_BAND: RubberBand = RubberBand {
    stiffness: 0.5,
    limit: 48.,
};

/// Amount of touchpad movement for each column width step.
const WIDTH_STEP_MOVEMENT: f64 = 60.;

/// Distance of the cue when the column width can't step any further.
const BUMP_DISTANCE: f64 = 16.;

#[derive(Debug)]
pub struct WindowSwipe<W: LayoutElement> {
    /// The window being swiped.
    window: W::Id,
    /// Touchpad movement until the axis is decided.
    cumulative: Point<f64, Logical>,
    /// Axis of the gesture, once decided.
    axis: Option<Axis>,
    /// Vertical touchpad movement.
    tracker: SwipeTracker,
    /// Horizontal touchpad movement since the last width step.
    width_accum: f64,
    /// Whether the end-of-range cue already played during this gesture.
    bumped: bool,
    /// Whether the swipe down already maximized the window.
    maximized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

impl<W: LayoutElement> WindowSwipe<W> {
    pub fn window(&self) -> &W::Id {
        &self.window
    }
}

impl<W: LayoutElement> Layout<W> {
    /// Starts a window swipe on the focused window.
    ///
    /// Returns `false` if there's no suitable window, in which case the swipe should be handled
    /// as a normal three-finger swipe.
    pub fn window_swipe_begin(&mut self) -> bool {
        self.window_swipe_cancel();

        if self.overview_open || self.interactive_move.is_some() {
            return false;
        }

        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &self.monitor_set
        else {
            return false;
        };

        let Some(window) = monitors[*active_monitor_idx].active_window() else {
            return false;
        };
        let window = window.id().clone();

        // The window may still be held up from a swipe that didn't close it yet.
        if let Some(tile) = self.window_swipe_tile(&window) {
            tile.swipe_spring_back();
        }

        self.window_swipe = Some(WindowSwipe {
            window,
            cumulative: Point::from((0., 0.)),
            axis: None,
            tracker: SwipeTracker::new(),
            width_accum: 0.,
            bumped: false,
            maximized: false,
        });

        true
    }

    pub fn window_swipe_is_active(&self) -> bool {
        self.window_swipe.is_some()
    }

    /// Feeds touchpad movement into the window swipe.
    ///
    /// Returns `false` if there's no ongoing window swipe.
    pub fn window_swipe_update(&mut self, delta_x: f64, delta_y: f64, timestamp: Duration) -> bool {
        let Some(swipe) = &mut self.window_swipe else {
            return false;
        };

        let axis = match swipe.axis {
            Some(axis) => axis,
            None => {
                swipe.cumulative += Point::from((delta_x, delta_y));

                let Point { x, y, .. } = swipe.cumulative;
                if x * x + y * y < AXIS_DECISION_THRESHOLD * AXIS_DECISION_THRESHOLD {
                    return true;
                }

                // Apply the movement so far right away.
                let axis = if x.abs() > y.abs() {
                    swipe.width_accum = x - delta_x;
                    Axis::Horizontal
                } else {
                    swipe.tracker.push(y - delta_y, timestamp);
                    Axis::Vertical
                };
                swipe.axis = Some(axis);
                axis
            }
        };

        match axis {
            Axis::Horizontal => {
                swipe.width_accum += delta_x;
                self.window_swipe_step_width();
            }
            Axis::Vertical => {
                swipe.tracker.push(delta_y, timestamp);
                self.window_swipe_follow_vertical();
            }
        }

        true
    }

    /// Ends the window swipe.
    ///
    /// Returns the window to close if the swipe up committed.
    pub fn window_swipe_end(&mut self, cancelled: bool) -> Option<W::Id> {
        let swipe = self.window_swipe.take()?;
        let tile = self.window_swipe_tile(&swipe.window)?;

        if cancelled || swipe.axis != Some(Axis::Vertical) || swipe.maximized {
            tile.swipe_spring_back();
            return None;
        }

        let view_height = tile.view_size().h;
        let projected = swipe.tracker.projected_end_pos() * view_height / VERTICAL_MOVEMENT;
        if tile.swipe_offset.y < 0. && projected <= -CLOSE_THRESHOLD * view_height {
            // Hold the tile up until the window closes.
            tile.swipe_closing = true;
            Some(swipe.window)
        } else {
            tile.swipe_spring_back();
            None
        }
    }

    /// Cancels the window swipe, returning the window to its place.
    pub fn window_swipe_cancel(&mut self) {
        let Some(swipe) = self.window_swipe.take() else {
            return;
        };

        if let Some(tile) = self.window_swipe_tile(&swipe.window) {
            tile.swipe_spring_back();
        }
    }

    /// Returns a swiped-away window to its place if it didn't close after all.
    ///
    /// For example, the client may have shown an "unsaved changes" dialog instead of closing.
    /// Returns `true` if the window was returned.
    pub fn window_swipe_close_timed_out(&mut self, window: &W::Id) -> bool {
        if self
            .window_swipe
            .as_ref()
            .is_some_and(|s| &s.window == window)
        {
            // A new swipe took over this window.
            return false;
        }

        let Some(tile) = self.window_swipe_tile(window) else {
            return false;
        };
        if !tile.swipe_closing {
            return false;
        }

        tile.swipe_spring_back();
        true
    }

    /// Steps the window's column one preset width narrower or wider, without wrapping around.
    ///
    /// Returns `false` if the column is already at the end of the preset range, or if the window
    /// isn't in a column.
    pub fn step_window_width(&mut self, window: &W::Id, forwards: bool) -> bool {
        for ws in self.workspaces_mut() {
            if ws.has_window(window) {
                return ws.step_window_width(window, forwards);
            }
        }

        false
    }

    fn window_swipe_tile(&mut self, window: &W::Id) -> Option<&mut Tile<W>> {
        self.workspaces_mut()
            .flat_map(|ws| ws.tiles_mut())
            .find(|tile| tile.window().id() == window)
    }

    fn window_swipe_step_width(&mut self) {
        let window = self.window_swipe.as_ref().unwrap().window.clone();

        loop {
            let swipe = self.window_swipe.as_mut().unwrap();
            let forwards = if swipe.width_accum >= WIDTH_STEP_MOVEMENT {
                true
            } else if swipe.width_accum <= -WIDTH_STEP_MOVEMENT {
                false
            } else {
                break;
            };
            swipe.width_accum -= WIDTH_STEP_MOVEMENT.copysign(swipe.width_accum);

            if self.step_window_width(&window, forwards) {
                continue;
            }

            let swipe = self.window_swipe.as_mut().unwrap();
            if !swipe.bumped {
                swipe.bumped = true;
                if let Some(tile) = self.window_swipe_tile(&window) {
                    let from = if forwards {
                        BUMP_DISTANCE
                    } else {
                        -BUMP_DISTANCE
                    };
                    tile.animate_move_x_from(from);
                }
            }
        }
    }

    fn window_swipe_follow_vertical(&mut self) {
        let swipe = self.window_swipe.as_mut().unwrap();
        if swipe.maximized {
            return;
        }

        let window = swipe.window.clone();
        let pos = swipe.tracker.pos();

        if pos >= MAXIMIZE_THRESHOLD {
            swipe.maximized = true;

            let Some(tile) = self.window_swipe_tile(&window) else {
                return;
            };
            // Let the tile flow from its nudged position into the maximize animation. If the
            // window is already maximized or fullscreen, springing back is the only feedback.
            tile.swipe_spring_back();
            if tile.window().pending_sizing_mode().is_normal() {
                self.set_maximized(&window, true);
            }
            return;
        }

        let Some(tile) = self.window_swipe_tile(&window) else {
            return;
        };

        let view_height = tile.view_size().h;
        let offset = pos * view_height / VERTICAL_MOVEMENT;
        if offset <= 0. {
            let progress = (-offset / (view_height * CLOSE_FADE_DISTANCE)).clamp(0., 1.);
            tile.swipe_offset = Point::from((0., offset));
            tile.swipe_alpha = 1. - (1. - CLOSE_MIN_ALPHA) * progress;
        } else {
            tile.swipe_offset = Point::from((0., MAXIMIZE_RUBBER_BAND.band(offset)));
            tile.swipe_alpha = 1.;
        }
    }
}
