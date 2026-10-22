# Spec: Mod-held three-finger window swipes for a niri fork

Status: implemented (branch `ocbk`) · Scope: while Mod is held, three-finger touchpad swipes
act on the focused window instead of scrolling the view or switching workspaces.

User-facing docs: [Gestures → Window Swipes](./docs/wiki/Gestures.md#window-swipes) and
[`gestures { mod-swipe }`](./docs/wiki/Configuration:-Gestures.md#mod-swipe). This document
covers the design and the implementation, for maintaining and tuning the feature.

## 1. Goal

One modal rule: *hold Mod, then swipe*. Releasing Mod disarms the layer. The vertical axis
is continuous and follows the fingers; the horizontal axis is discrete and steps.

| Direction | Action | Feel |
|-----------|--------|------|
| Up | Close the window | Window follows the fingers and fades. On release, a distance/velocity threshold closes it or springs it back. |
| Down | Maximize to edges | Rubber-banded nudge down, then commits **live** once past the threshold. Release before that springs back. |
| Left | Column one preset narrower | One step per stretch of travel; stops at the narrowest preset with a bump. |
| Right | Column one preset wider | Mirror of left; stops at the widest preset (or full width) with a bump. |

Design choices made with the user:

- **Down maximizes to edges** (`maximize-window-to-edges`), not fullscreen, so the bar stays
  visible and the client isn't told it's fullscreen.
- **Down commits mid-swipe**, the moment the threshold is crossed, rather than on release.
- **Width steps repeat with distance**, so one long swipe can step several presets.
- **Deltas follow the fingers** regardless of the natural-scroll setting, like the
  four-finger overview gesture.

## 2. Architecture

The gesture state lives in the layout, next to `interactive_move`; input only routes events
and performs side effects the layout can't (sending `close` to a client, timers).

```
src/input/mod.rs                     src/layout/window_swipe.rs          src/layout/tile.rs
────────────────                     ──────────────────────────          ──────────────────
on_gesture_swipe_begin  ── Mod held? ─▶ Layout::window_swipe_begin()
on_gesture_swipe_update ─────────────▶ Layout::window_swipe_update()  ──▶ Tile::swipe_offset
                                        ├─ vertical:   follow / maximize    Tile::swipe_alpha
                                        └─ horizontal: step_window_width
on_gesture_swipe_end    ─────────────▶ Layout::window_swipe_end() ─▶ Some(window) = close
  close_swiped_window(): send_close + 1 s timer ─▶ window_swipe_close_timed_out()
on_keyboard (Mod released) ──────────▶ Layout::window_swipe_cancel()
```

### 2.1 Activation — `src/input/mod.rs`

- `mod_swipe_armed()` is true when `gestures.mod-swipe` isn't `off` and the keyboard's
  modifier state contains the configured Mod key (`backend.mod_key(...)`, so it follows
  `input { mod-key }` and the nested-winit Alt fallback).
- In `on_gesture_swipe_begin`, a three-finger swipe with Mod armed calls
  `window_swipe_begin()`. If that returns `false` (no focused window, overview open,
  interactive move in progress), the swipe falls through to the normal three-finger
  handling. The MRU switcher check runs before this, as before.
- `on_gesture_swipe_update` hands **raw** deltas (before natural-scroll inversion) to
  `window_swipe_update()` first and returns if it handled them.
- `on_gesture_swipe_end` passes `event.cancelled()` to `window_swipe_end()`.
- `on_keyboard`: inside the XKB filter, after the modifier state updates, a window swipe
  in progress is cancelled as soon as the Mod key is no longer held.

### 2.2 Gesture state — `src/layout/window_swipe.rs`

`WindowSwipe<W>` holds the window id, the movement before an axis is chosen, the chosen
`Axis`, a `SwipeTracker` for vertical movement (reused from `src/input/swipe_tracker.rs`
for velocity and projected end position), the horizontal movement since the last width
step, and two flags: `bumped` (end-of-range cue played) and `maximized` (down committed).

- **Axis decision**: after 16 units of travel, the larger component picks the axis — the
  same GNOME-derived threshold as the normal three-finger swipe. The travel so far is
  applied at once so nothing is lost.
- **Up / down** are one vertical axis: the sign of the tracker position chooses the mode,
  so the user can change their mind mid-swipe before anything commits.
- The swiped tile is looked up by window id on every update, so the gesture survives layout
  changes. If the tile is gone, the update is a no-op; `Layout::remove_window` also drops
  the state for that window.

### 2.3 Tile visuals — `src/layout/tile.rs`

- `swipe_offset` is added in `Tile::render_offset()`, alongside `interactive_move_offset`.
  Every caller of the render offset — including the close-animation start position — sees
  the swiped position.
- `swipe_alpha` multiplies the tile alpha. When below 1, the tile renders through its own
  `swipe_offscreen` buffer, the same way alpha animations do. A separate field is needed
  because the layout invariants only allow visible tiles to *animate* alpha towards 1.
- `swipe_spring_back()` zeroes the offset and alpha and hands them to the existing
  `animate_move_{x,y}_from` and `animate_alpha` springs (window-movement config), so no new
  animation code was needed.
- `swipe_closing` marks a tile that was swiped away and is waiting for its client to close.

### 2.4 Close flight — `src/layout/closing_window.rs`

`ClosingWindow` gained a `fly_y` distance, applied over the close animation's progress. The
scrolling and floating `start_close_animation_for_window` set it from
`Tile::swipe_close_fly_y()` — the distance needed to carry the tile off the top of the
view — so a swiped-away window keeps travelling up while it fades, instead of stopping
where the fingers let go.

### 2.5 Width stepping — `src/layout/scrolling.rs`

`Column::step_width(forwards) -> bool` is a clamped variant of `Column::toggle_width`:

- From a preset, it moves to the neighbouring preset, or returns `false` at either end.
- From a custom width, it picks the nearest larger or smaller preset by size (the same
  matching `toggle_width` uses), or returns `false` if there is none.
- Full width and maximized count as wider than every preset: stepping wider returns
  `false`, stepping narrower goes to the widest preset.

It's plumbed as `ScrollingSpace::step_window_width` → `Workspace::step_window_width` →
`Layout::step_window_width`. Floating windows have no column, so they return `false` and
get the bump cue.

## 3. Behaviour details

### 3.1 Up — close

- Offset is `pos × view_height / 300`, so one view height of window travel takes as much
  finger movement as a full workspace switch.
- Alpha fades linearly to 0.4 over half the view height.
- On release, the projected end position (current position plus velocity decay, from
  `SwipeTracker`) must reach a quarter of the view height upwards. A fast flick closes even
  from a short distance; a slow drag that stops before release springs back.
- On commit, the tile is held where it is with `swipe_closing` set, and input sends
  `xdg_toplevel.close`. After **1 s**, `window_swipe_close_timed_out()` springs the tile
  back if it still exists and is still marked closing — this covers "save changes?"
  dialogs and clients that ignore close. A new swipe on the same window first springs the
  held tile back.

### 3.2 Down — maximize

- Up to the threshold, the offset is rubber-banded (stiffness 0.5, limit 48 px), so the
  window acknowledges the gesture without travelling far.
- At 60 units of travel, the tile springs back from its nudged position *while*
  `Layout::set_maximized(window, true)` starts the resize, so the two animations blend.
  Further movement in the same swipe is ignored.
- A window that's already maximized or fullscreen only springs back.

### 3.3 Left / right — width

- Every 60 units of horizontal travel steps one preset; the leftover carries over, so
  wobbling back and forth near a boundary doesn't fire repeatedly.
- At the end of the range, the tile jumps 16 px in the swipe direction and springs back —
  once per gesture.

### 3.4 Cancellation

The tile springs back from wherever it is when any of these happen:

- Mod is released mid-swipe.
- libinput reports the gesture as cancelled.
- A new window swipe begins (it cancels the previous one first).
- A horizontal swipe ends — it never moves the tile vertically, but this clears any
  leftover offset.

## 4. Tuning

All thresholds are constants at the top of `src/layout/window_swipe.rs`, in raw touchpad
units (libinput unaccelerated deltas) unless noted:

| Constant | Value | Meaning |
|----------|-------|---------|
| `AXIS_DECISION_THRESHOLD` | `16` | Travel before the axis is chosen |
| `VERTICAL_MOVEMENT` | `300` | Travel that moves the window one view height |
| `CLOSE_THRESHOLD` | `0.25` | Fraction of view height the projected release must reach to close |
| `CLOSE_FADE_DISTANCE` | `0.5` | Fraction of view height over which the window fades |
| `CLOSE_MIN_ALPHA` | `0.4` | Opacity at full fade |
| `MAXIMIZE_THRESHOLD` | `60` | Downward travel that commits maximize |
| `MAXIMIZE_RUBBER_BAND` | `0.5`, `48` px | Resistance and limit of the downward nudge |
| `WIDTH_STEP_MOVEMENT` | `60` | Horizontal travel per width step |
| `BUMP_DISTANCE` | `16` px | Size of the end-of-range cue |

The close timeout is `SWIPED_WINDOW_CLOSE_TIMEOUT` (1 s) in `src/input/mod.rs`.

## 5. Configuration

```kdl
gestures {
    mod-swipe {
        // off
    }
}
```

Enabled by default. `off` restores normal three-finger swipes even while Mod is held.
Defined as `ModSwipe` in `niri-config/src/gestures.rs`; the config test in
`niri-config/src/lib.rs` covers parsing and merging, and `niri-config/tests/wiki-parses.rs`
checks the wiki snippets.

## 6. Verification

- `cargo test` — targeted layout tests in `src/layout/tests.rs`:
  `step_window_width_clamps_at_ends`, `step_window_width_from_full_width`,
  `window_swipe_up_closes_and_short_swipe_springs_back`, `window_swipe_down_maximizes`,
  `window_swipe_horizontal_steps_width`, `window_swipe_cancelled_by_window_removal`.
- `RUN_SLOW_TESTS=1 cargo test --release` — the randomized layout tests include the
  `WindowSwipe*` and `StepWindowWidth` ops, checking layout invariants after every step.
- Manual: touchpad gestures don't reach the nested winit backend, so test in a real
  session. Check each direction, an early release, a fast flick, a window with a close
  prompt, releasing Mod mid-swipe, and that swipes without Mod and four-finger overview
  are unchanged.

## 7. Known limitations

- The window lifts and fades while swiped up but doesn't shrink; a scale factor would
  have to be threaded through `Tile::render_inner`.
- A custom `window-close` shader doesn't get the upward flight; only the built-in close
  animation applies `fly_y`.
- Floating windows don't step width; they could use the floating preset widths later.
- The gesture acts on the focused window on the active monitor, not the window under the
  cursor.
