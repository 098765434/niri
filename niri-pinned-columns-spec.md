# Spec: Pinned columns for a niri fork

Status: compositor core implemented · Target: niri fork + Amane shell client · Depends on:
[`niri-gesture-ipc-spec.md`](./niri-gesture-ipc-spec.md) (shares its event/accessor/refresh
plumbing and the three gesture signals).

## Implementation status

The **compositor core** (spec §3–§6, §8, plus §9's compositor half) is implemented on the `ocbk`
branch — see [`FORK.md`](./FORK.md) for the touched files and usage. Notes on where the
implementation refines this spec:

- **IPC field (§9).** `is_pinned` / `pin_edge` are exposed on `niri_ipc::WindowLayout` (not on
  `Window`), so they ride the existing `WindowLayoutsChanged` change-detection path for free — no
  new field in the `Window` `changed` comparison. Semantically they are layout properties, so this
  fits. `pin_edge` serializes as `"left"` / `"right"` / `null`.
- **Reserved rails (§3).** Derived from the pinned columns on demand (`reserved_rails()` /
  `scroll_area()`) rather than cached, so a pinned column's width changing (resize, preset toggle,
  reconfigure) can never leave a stale reserve. `scroll_area()` clamps the total reserve so the
  scroll width stays non-negative even with the maximum four pins.
- **Clipping (§4).** Unpinned columns are cropped to `scroll_area()` with `CropRenderElement`
  (horizontal only); pinned columns render uncropped on their rail. `window_under` mirrors this so
  the rails don't catch input meant for the pins.
- **Focus nav.** `focus-column-left/right` skip pinned columns; a dedicated `focus-pinned` action
  reaches them (resolves the §11 open question in the affirmative). One §6 nicety is simplified: if
  a pinned column is active and the user swipes the row, focus lands on the swiped-to unpinned
  column rather than staying on the pin — the swipe end-point retargets the active column.

**Not yet implemented** (later passes): the gesture/overview integration beyond snap/prediction
exclusion (§5.3 overview drop target, §5.4 shape-indicator notch), the Amane shell client UI (§5.1
pin glyph/zero-parallax, §9 client half), and global cross-workspace pins (§7, the `--global` flag).

## 1. Concept

A **pinned column** is a first-class fourth layout position, alongside scrollable, floating,
and tabbed. It is defined by three rules and nothing else:

1. **Edge-anchored** — its on-screen x is clamped to a screen edge (a *pin rail*) instead of
   following `view_pos`.
2. **Reserves space** — the scrollable region shrinks by the rail's width, so a pinned column
   never occludes scrollable ones (preserves niri's no-occlusion invariant).
3. **Excluded from scroll math** — it is skipped in `column_xs` extent, snap points,
   `projected_end_pos`, and the gesture-prediction target set.

A pinned column still occupies its slot in the column order, so focus, add/remove, and IPC
indices stay consistent. Pinning is a render-time exception, not a separate container.

Non-goals: pinning is for **narrow companions** (chat, player, notes, monitors), not for
parking a full-width window. A max-width clamp enforces this (§6).

## 2. Decisions (resolved)

| Question | Decision | Rationale |
|----------|----------|-----------|
| Reserve space vs overlay | **Reserve** | Keeps "nothing is ever occluded"; same mental model as a strut |
| Scope | **Per-output pin rail**, left and/or right | Falls out of column→workspace→output ownership; no special case |
| Cross-workspace | **Per-workspace by default**, optional global pin (§7) | Per-workspace is simpler and composes with the shape indicator; global is the "sticky window" people ask for |
| Capacity | **1–2 columns per edge** | More defeats the purpose and eats the viewport |
| Max width | **Clamp to a preset (≤30% viewport)** | Reserve model breaks if a pin can be arbitrarily wide |

## 3. State — `src/layout/scrolling.rs`

Add to `Column<W>`:

```rust
/// Whether this column is pinned to a screen edge.
is_pinned: bool,
/// Which edge, when pinned.
pin_edge: PinEdge, // ignored unless is_pinned
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinEdge { Left, Right }
```

Add to `ScrollingSpace<W>` a cached rail summary, recomputed when pins change or on
`update_config`:

```rust
/// Total reserved width on each edge (sum of pinned column widths + gaps), clamped.
reserved_left: f64,
reserved_right: f64,
```

### 3.1 Reserved-width computation

```rust
fn recompute_reserved(&mut self) {
    let max_pin_w = self.working_area.size.w * 0.30; // §6 clamp
    let gap = self.options.layout.gaps;
    let (mut l, mut r) = (0.0, 0.0);
    for (col, data) in zip(&self.columns, &self.data) {
        if !col.is_pinned { continue; }
        let w = data.width.min(max_pin_w) + gap;
        match col.pin_edge { PinEdge::Left => l += w, PinEdge::Right => r += w }
    }
    self.reserved_left = l;
    self.reserved_right = r;
}
```

### 3.2 The reduced scroll viewport

Everything that currently reads `self.working_area` for **scroll** purposes must use the
reduced area instead. Introduce one accessor and route the scroll/snap code through it:

```rust
fn scroll_area(&self) -> Rectangle<f64, Logical> {
    let mut a = self.working_area;
    a.loc.x  += self.reserved_left;
    a.size.w -= self.reserved_left + self.reserved_right;
    a
}
```

Use `scroll_area()` in: `compute_new_view_offset_*`, the snap-point generation in
`view_offset_gesture_end`, `view_offset_gesture_update` normalization, and
`center_visible_columns` / `expand_column_to_available_width`. **Do not** use it for pinned
columns' own placement (they use the rail, §4).

## 4. Render — `src/layout/scrolling.rs`

The single behavioral branch. Today every column is placed at `col_x - view_pos`. Split it:

```rust
// Pinned columns: fixed rail x, independent of view_pos.
// Unpinned columns: normal scroll placement, but packed into scroll_area().
fn column_render_x(&self, idx: usize, left_cursor: &mut f64, right_cursor: &mut f64) -> f64 {
    let col = &self.columns[idx];
    let gap = self.options.layout.gaps;
    if col.is_pinned {
        match col.pin_edge {
            PinEdge::Left => {
                let x = *left_cursor;
                *left_cursor += self.data[idx].width.min(self.scroll_area().size.w) + gap;
                self.working_area.loc.x + x
            }
            PinEdge::Right => {
                *right_cursor -= self.data[idx].width + gap;
                self.working_area.loc.x + self.working_area.size.w - (*right_cursor * -1.0)
                // (compute right-anchored x; keep rail ordering stable)
            }
        }
    } else {
        // Normal path, but the scrollable extent and view_pos are relative to scroll_area().
        self.column_x_unpinned(idx) - self.view_pos()
    }
}
```

Key points:
- `column_xs` / `column_x` must iterate **unpinned columns only** when computing the
  scrollable extent, so pins don't contribute to scroll distance. Keep a parallel
  `column_x_unpinned(idx)` that walks the unpinned subsequence.
- Pinned columns render in the pin rail in column order; two left pins stack from the left
  edge inward, two right pins from the right edge inward.
- Pinned columns render **after** (on top of, z-order) the scrollable row so a scrolling
  column passing the edge is clipped at the rail boundary, not drawn over the pin.
- Clip the scrollable row to `scroll_area()` horizontally so columns don't bleed into the
  rail during a swipe.

## 5. Interaction with the gesture signals

This is what makes pinning coherent with `niri-gesture-ipc-spec.md` rather than a bolt-on.

### 5.1 Horizontal view offset (signal 2)
- `view_pos` now means scroll position **within the reduced viewport**. The parallax-title
  math uses `scroll_area().size.w`; nothing else changes client-side.
- Pinned columns are **excluded** from `active_column_idx` scroll targeting and from the
  snap points in `view_offset_gesture_end`. A swipe scrolls only the unpinned row.
- In the IPC `ViewOffsetChanged` event, the pinned column's title is still present in the
  bar (from window events) but the client renders it with a **pin glyph and zero parallax**
  — it must not move while others do.

### 5.2 Gesture prediction (the predictive feature)
- The predicted landing target skips pinned indices. Side effect: the pin becomes a "safe
  zone" the prediction never lands on, making flick-nav more forgiving. No extra work — just
  filter pinned columns out of the candidate set before projecting.

### 5.3 Overview (signal 1)
- Render the pin as **present on every workspace** (ghosted) in the overview, matching the
  "always visible" mental model (applies to global pins; per-workspace pins show only on
  their workspace).
- When overview progress crosses a threshold (e.g. `p > 0.6`), expose a **pin rail drop
  target**. Dragging a column onto it issues `pin-column`; dragging a pin off issues
  `unpin-column`. Reuses the existing drag-to-reorder → `move-column-to-index` machinery.

### 5.4 Vertical workspace switch (signal 3)
- Per-workspace pins: the shape indicator gains a **pin notch** when the current workspace
  has a pin, so the morphing shape also encodes "this workspace has something docked."
- Global pins: unaffected by vertical swipe (they're on every workspace by definition).

## 6. Edge cases (each resolved)

| Case | Resolution |
|------|------------|
| Pin is focused, user swipes the row away | **Focus stays on the pin** (it's always visible, so focus-follows-visibility holds). |
| Pinned column's last window closes | **Auto-unpin** and collapse the reserved slot; don't leave an empty rail. |
| Pin resized / inherently wider than its strip | **Clamp to ≤30% viewport** (preset). Enforced in `recompute_reserved` and render; the window is sized to the clamp. |
| Two pins same edge | Allowed, **max 2 per edge**, stacked in column order; reserve = sum of widths. Reject a 3rd with a no-op + log. |
| Pins on both edges | Allowed; `reserved_left` and `reserved_right` both apply. |
| Overview zoom | Pin participates in the zoom-out but stays **edge-anchored in the zoomed frame** → reads as "docked", not floating. |
| Multi-monitor | **Per-output** by construction; no special case. |
| Pinned column moved via `move-column-to-index` | Moving *into* a pinned slot is a no-op; a pinned column ignores row-reorder until unpinned. |
| Fullscreen a window in a pinned column | Unpin for the duration of fullscreen (fullscreen wants the whole output), restore pin on unfullscreen. |
| `empty_workspace_above_first` / workspace cleanup | A pinned column counts as "has windows" for cleanup so its workspace isn't reaped. |

## 7. Global (cross-workspace) pins — optional phase 2

Per-workspace pinning (above) lives entirely in `ScrollingSpace`. A **global pin** — visible
on every workspace of an output — is the true "sticky window" feature and is more involved:

- Promote the pinned column to a per-`Monitor` slot rendered on top of whichever workspace is
  active, rather than owned by one workspace's `ScrollingSpace`.
- During a vertical workspace switch, the global pin stays put while workspaces scroll under
  it (render it after `render_workspaces`, edge-anchored, unaffected by `workspace_render_idx`).
- Ship per-workspace first; add global as a `--global` flag on `pin-column` once the
  per-workspace path is solid.

## 8. IPC — actions (new `niri msg action`s)

Same action plumbing as existing actions (validated in `src/ipc/server.rs`, dispatched via
`do_action`):

```
pin-column    [--edge left|right] [--global]   # pin active (or targeted) column
unpin-column                                     # unpin active (or targeted) column
toggle-column-pinned [--edge left|right]
```

- Add corresponding `niri_config::Action` variants and `niri_ipc::Action` variants.
- `--edge` defaults to `left`. `--global` is phase 2 (§7); reject with a clear error until
  implemented.
- Targeting follows existing convention: act on the active column unless a window/id is given.

## 9. IPC — event (bar integration)

One field, same `EventStreamState` treatment as everything else. Extend the IPC window/column
data so the bar can show a pin toggle + badge and so drag-to-pin knows current state:

```rust
// niri_ipc::Window (or the WindowLayout payload)
pub is_pinned: bool,
pub pin_edge: Option<PinEdge>, // serialize as "left" | "right" | null
```

Emit it through the existing `ipc_refresh_windows` change-detection path — add `is_pinned` to
the `changed` comparison so a pin/unpin produces a `WindowOpenedOrChanged`. No new event type
needed; the field rides the existing one.

Client (Amane) uses it to:
- show a pin glyph on pinned columns' bar items and suppress their parallax (§5.1),
- render the pin rail drop target in the expanded/overview bar (§5.3),
- offer a right-click "pin to left/right" menu item that issues the action in §8.

## 10. Implementation order

1. `Column.is_pinned` + `PinEdge`, `recompute_reserved`, `scroll_area()` — no behavior change
   yet (nothing is pinned).
2. Render branch (§4) + route scroll/snap code through `scroll_area()`.
3. Actions (§8); verify by pinning via `niri msg action pin-column` and swiping — the pin
   must hold while the row scrolls under the reduced viewport.
4. `is_pinned` IPC field (§9); verify in `niri msg --json event-stream`.
5. Edge cases (§6) one by one, each with a manual repro.
6. Gesture-signal integration (§5): exclude pins from snap/prediction, client pin-glyph + rail
   drop target.
7. Global pins (§7) — only after per-workspace is solid.

## 11. Open questions / upstream

- Attach to niri's existing **sticky/always-visible window** tracking issue (the FAQ links it)
  — pinned *columns* are adjacent and may be mergeable or at least coordinated upstream.
- Confirm the exact `column_xs` / `column_x` call sites so the unpinned-only extent change is
  complete; grep for every `view_pos()` / `column_x(` use.
- Right-anchored rail x math (§4) needs care to keep two right pins ordered stably — write it
  against a 2-pin test before trusting it.
- Decide whether pinned columns are skipped by `focus-column-left/right` (recommended: yes —
  they're reached by a dedicated `focus-pinned` bind, not by scrolling into them).
