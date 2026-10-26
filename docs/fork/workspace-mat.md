# Spec: Workspace mat for a niri fork

Status: draft · Scope: a sharp-cornered backing painted behind the scrolling layout's
windows, so the gaps between and around windows read as one clean surface in normal mode
while the wallpaper stays visible wherever there are no windows. The mat is a solid colour
by default, or optionally liquid glass. It fades out as the overview opens, which reveals
the same gaps over the wallpaper.

This document covers the design and the planned implementation. User-facing docs will go in
[`Configuration:-Layout.md`](../wiki/Configuration:-Layout.md) once the design settles.

## 1. Goal

Desktop noise between windows (bits of wallpaper in gaps and in the corners of rounded
windows) makes the layout look busy. The mat replaces that with a flat surface, usually
black, without giving up a wallpaper:

| Where | Normal mode | Overview |
|-------|-------------|----------|
| Gaps between columns and between tiles in a column | mat | wallpaper (mat faded out) |
| Outer gaps at the ends of the strip, top and bottom gaps | mat | wallpaper |
| Behind an exclusive-zone bar (struts / work-area padding) | mat | wallpaper |
| Outside a rounded window's clipped corners | mat | wallpaper |
| Under a column whose tiles don't fill the height | mat or wallpaper, by `column-fill` (§3.3) | wallpaper |
| Past the ends of the strip | wallpaper, with a hard or soft edge by `ends` (§4) | wallpaper |
| Empty workspace | wallpaper | wallpaper |
| Around floating windows | no mat | no mat |

Design choices made with the user:

- **The mat is the gap.** Gap geometry is the same in both modes. In normal mode the mat fills
  the gaps so they read like a sharp border. Opening the overview only changes the mat's
  alpha, so the "border" turns into a wallpaper gap with no layout change, no window resize and
  no reflow. This replaces the earlier approach of `gaps 0` plus a black `border`.
- **Covers the area behind the bar.** The mat spans the full view height, including the area
  reserved by exclusive zones, so a translucent bar sits on the mat, not on the wallpaper.
- **Floating windows get no mat.** They float above the strip and its mat.
- **The mat's sharp corners fill around rounded windows.** Windows keep
  `geometry-corner-radius` + `clip-to-geometry`. The mat draws beneath the clip, so the
  antialiased corners blend into the mat.
- **Configurable:** how short columns are filled (`column-fill`), how the strip ends look
  (`ends`: hard or soft), and whether the mat is solid or glass (`material`).

## 2. Architecture

The mat is purely visual: it only reads geometry the scrolling layout already computes each
frame, and stores no layout state. It has no input region and no IPC.

```
src/layout/monitor.rs                 src/layout/workspace.rs          src/layout/scrolling.rs
─────────────────────                 ───────────────────────          ───────────────────────
render_workspaces()
  mat_alpha = 1 - overview progress ─▶ render_scrolling(.., mat_alpha) ─▶ ScrollingSpace::render
                                                                         ├─ closing windows
                                                                         ├─ columns / tiles
                                                                         └─ Mat::render  (pushed last = bottom)
ScrollingSpace::update_render_elements ─▶ mat_geometry() ─▶ Mat (cells, slab, ends)

src/layout/mat.rs (new)
  Mat::render ── solid:  SolidColorRenderElement per cell, BorderRenderElement gradient per soft end
              └─ glass:  Xray::render with RenderParams { geometry: cell, clip: slab, material }
```

### 2.1 Geometry — `fn mat_geometry` in `src/layout/mat.rs`

A free function so it can be unit tested. Inputs: each column's render x position
(`col_x + render_offset.x - view_pos`), width and tile-stack bottom; the gap; `view_size`;
`column-fill`. Output, in view coordinates:

- **Slab:** the bounding rectangle. x runs from the leftmost column's left edge minus `gaps`
  to the rightmost column's right edge plus `gaps`. y runs `0 .. view_size.h`, the full view
  height including struts and exclusive zones.
- **Cells:** a partition of the slab's x range, one cell per column. Columns are sorted by
  render x. The boundary between two neighbours is the middle of the gap between them
  (the middle of their overlap while moving), **rounded to physical pixels once** and used by
  both cells. This way the cells tile the slab exactly:
  - no 1-physical-pixel seams between columns at fractional scales, and
  - no holes while columns swap with move animations (`Column::render_offset`), since
    neighbouring cells always share a boundary.

  Each cell's height comes from `column-fill` (§3.3). With `"full"`, every cell is the full
  height and the cells merge into one rectangle: the slab.
- **Soft ends** (`ends "soft"`): two extra rectangles of width `soft-end-length` just outside
  the slab's left and right edges, each with its column's cell height.
- `None` when there are no columns, or when the mat is off.

`ColumnData::width` is refreshed from `Column::width()`, which reads tile sizes. **Verify**
that during resize animations these are the animated sizes, so the mat edge follows the
window edge instead of jumping to the target size.

### 2.2 State — `Mat` on `ScrollingSpace`

A `Mat` struct, owned by `ScrollingSpace`, holds the config and one `SolidColorBuffer` per
cell, kept across frames so damage tracking only sees real changes. It also holds the slab
and the soft-end rectangles. `update_render_elements` calls `mat_geometry()` and resizes the
buffers. With `column-fill "full"` there is one buffer.

### 2.3 Rendering

- `ScrollingSpace::render` takes a new `mat_alpha: f32` and pushes the mat **after** the
  tile loop. niri renders front-to-back, so the mat ends up below every tile, its shadow,
  border, focus ring and tab indicator, but above the background layer and the workspace
  `background-color`. If `mat_alpha == 0.`, it is skipped entirely.
- `Workspace::render_scrolling` passes it through. `render_floating` is unchanged, so
  floating windows stay above the mat and get none of their own.
- `Monitor::render_workspaces` computes `mat_alpha` from
  `overview_progress.clamped_value()`: `1 - progress` with `overview "fade"`, or `1` with
  `overview "keep"`. The overview zoom is applied above the workspace, so in the overview
  each workspace's mat zooms with it.

**Solid mat (default).** One `SolidColorRenderElement` per cell. Each soft end is a
`BorderRenderElement` with `border_width 0.` (a filled rectangle, as `tile.rs` already uses
during the fullscreen animation). It runs a horizontal gradient from the mat colour to the same
colour at alpha 0, pointing away from the slab.

**Glass mat (`material`).** Behind the mat there is only the wallpaper and the workspace
background, which is exactly what this fork's xray path already holds blurred once per
frame (`src/render_helpers/xray.rs`). So the glass mat needs no capture of its own. It
pushes `Xray::render` areas, one per cell, from `ctx.xray`, each with:

- `geometry`: the cell,
- `clip`: the **slab**, radius 0. The glass shader's refraction, rim and highlight are
  computed against the clip geometry, so the rims sit on the slab's outer edges only. No rim
  lines appear between cells.
- `material`: the named material looked up in `materials {}`, the same table
  `niri-blur-shapes-v1` uses,
- `tint`: the mat `color`, premultiplied (its alpha sets the tint strength),
- `alpha`: `mat_alpha`.

The `XrayPos` for the workspace is already passed down to `ScrollingSpace::render`, so the
overview zoom and workspace offsets come for free. If the glass shader failed to compile,
`Xray::render` already falls back to plain blur. If there is no xray context, the mat
falls back to solid colour.

### 2.4 Config — `niri-config/src/layout.rs`

A `mat` block next to `border`, with a matching `MatPart` for merging, following the
`FocusRing`/`Border` pattern:

```kdl
layout {
    gaps 8

    mat {
        // off
        color "#000000"
        overview "fade"          // "fade" (default) or "keep"
        column-fill "full"       // "full" (default) or "tiles"
        ends "hard"              // "hard" (default) or "soft"
        soft-end-length 96       // logical px; only used with ends "soft"
        // material "smoke"      // draw as liquid glass with this named material
    }

    border { off; }              // the mat replaces the black border
}

// Existing block, shared with niri-blur-shapes-v1.
materials {
    material "smoke" {
        refraction 6
        bevel 24
        rim 0.3
        tint 1.2
    }
}
```

- **Off by default**, so stock configs render exactly as before.
- With `material` set, `color` is the glass tint, e.g. `"#000000c0"` for dark smoked glass.
  An unknown material name logs a warning and draws the solid mat.
- Live-reloads like the rest of `layout`. Material changes already reach the shader through
  `set_materials`.
- No window-rule form: floating windows never get a mat, and per-window opt-outs would punch
  holes the cell partition is designed to avoid.

## 3. Behaviour details

### 3.1 Overview transition

Alpha follows the overview's own progress, including gesture-driven opens, so it tracks the
fingers the same way the zoom does. The spring's overshoot is clamped. During a plain
workspace switch (no overview) the mat stays at full alpha. A glass mat fades the same way:
its alpha goes to the xray element.

### 3.2 Animations

| Event | Mat behaviour |
|-------|---------------|
| Horizontal scroll (`view_offset`) | follows exactly, same frame as the tiles |
| Column move / swap | cells keep sharing boundaries. No holes, no seams |
| Column / window resize | edge follows the animated width (verify, §2.1) |
| Window open at a strip end | mat extends **at once** to the new column's width, before the window's open animation. With `ends "hard"` that's a mat block appearing over wallpaper. `"soft"` makes it gentler |
| Last window of a strip-end column closes | mat shrinks at once. The closing window animates over wallpaper |
| Fullscreen / maximized column | covered by the tile, so the mat doesn't matter |
| Interactive move (dragged tile) | dragged tile has no mat, like a floating window |
| Tile height change with `column-fill "tiles"` | cell bottom follows the animated tile heights |

The two strip-end pops may be worth smoothing by easing the slab's x extent toward its
target with the `window-movement` animation config. Decide after trying it live.

### 3.3 Short columns — `column-fill`

A column's tiles may not fill the height (fixed window heights, or a single tile set
shorter than the working area).

| `column-fill` | Cell height | Under a short column |
|---------------|-------------|----------------------|
| `"full"` (default) | `0 .. view_size.h` | mat |
| `"tiles"` | `0 ..` bottom of the column's last tile `+ gaps`, clamped to `view_size.h`. A column whose tiles fill the working area reaches the view bottom, covering the bottom strut | wallpaper |

Both keep the top of the view covered, so the area behind the bar is mat either way. Tabbed
columns use the height of the tab being shown. With `"tiles"`, the mat's outline is a
skyline, not a rectangle. The cells still share boundaries, so there are no holes between
columns. Each soft end matches the height of the column next to it.

With glass and `"tiles"`, the rim follows the slab (§2.3). The bottom of a short cell is a
cut with no rim. If that looks wrong live, the fallback is clipping each cell to its own rect,
which puts rims on each cell's bottom edge and also on its sides between columns.

### 3.4 Interaction with blur / xray

In this fork, layer surfaces such as the amane bar blur with **xray** by default. Xray samples
the background layer and backdrop, not the workspace contents, so an xray-blurred
translucent bar shows blurred **wallpaper**, not the mat, even though the mat covers the area
behind it.

- **Solid mat:** to make the bar sit on the mat, either set `xray false` in that layer rule,
  so it blurs what's really behind it (costs a capture and blur per frame), or draw the mat
  into the xray backdrop too (more code; has to track the scroll position).
- **Glass mat:** the mat itself is drawn from the same blurred wallpaper. An xray bar shows the
  same blur without the mat's tint and rims, which is close enough that it may not need
  fixing. Check live.

## 4. Strip ends — `ends`

Two options are kept, both configurable:

**`"hard"` (default).** The slab stops one outer gap past the outermost columns with a
sharp edge. Wallpaper shows past the ends of the strip: to the right when the strip is
narrower than the screen, and on either side while scrolling to the ends. The most literal
reading of "wallpaper where the workspace isn't covered".

**`"soft"`.** The same slab, so the outer gap itself stays fully covered, plus a fade over
`soft-end-length` beyond it. The mat dissolves into the wallpaper instead of ending as a slab,
and it softens the open/close pops (§3.2).

- **Solid:** a gradient `BorderRenderElement` per end (§2.3). No new shader.
- **Glass:** the glass shader takes one alpha per shape, so a glass soft end needs a small
  `glass.frag` change: an alpha ramp along x over the fade length, passed as a uniform. Until
  that lands, glass with `ends "soft"` draws hard ends and logs it once.

Considered and dropped for now: covering the full view width whenever the workspace has a
column, and snapping a strip end to the view edge when it's close. Either can come back as
another `ends` value if live use shows a need.

## 5. Verification

- **Unit tests** for `mat_geometry`:
  - no columns → `None`;
  - one column → slab is the column ± gap;
  - cells partition the slab's x range exactly: sorted, no overlap, no gap, shared boundaries
    identical after physical-pixel rounding at scales 1, 1.25 and 1.5;
  - columns with move offsets that overlap or separate still partition with no holes;
  - full view height regardless of working area with `"full"`; skyline heights with `"tiles"`,
    and a full-height column reaching `view_size.h`;
  - soft ends sit just outside the slab with the neighbouring cell's height.
- **Config:** parse test for the `mat` block (every key and value) and merge with `MatPart`;
  unknown material falls back to solid; default config unchanged.
- **Live:** run as nested niri (`cargo run` in a winit window), then on the real session via the
  `--override-input niri-fork=path:...` nixos-config build. Check at scale 1 and at a
  fractional scale for seams, with the amane bar running for the bar/xray question.
  `niri msg action load-config-file` reloads the mat settings live, so `ends`,
  `column-fill` and `material` can be compared side by side.
- **Performance:** solid is one or a few quads per workspace, mostly hidden under windows.
  Glass is one xray quad per cell from the already-blurred background, but the glass shader
  runs over the full slab area every frame it's damaged. Measure GPU frame time with the glass
  mat on, while scrolling and during the overview transition. The amane glass measurements
  showed per-shape glass cost adds up quickly on the Radeon 860M.

## 6. Known limitations (draft)

- Strip-end pops on open/close, softened but not removed by `ends "soft"` (§3.2).
- An xray-blurred bar shows wallpaper rather than a solid mat (§3.4).
- Glass with `ends "soft"` needs a `glass.frag` change before it works (§4).
- Glass with `column-fill "tiles"` has no rim on the cut bottom of short cells (§3.3).
- One mat style for the whole layout. No per-output or per-workspace setting yet.
- Tiles in tabbed columns are fine (one column, one width). Tab indicators drawn outside
  the column sit on the mat.

## 7. Fit with the fork's values

Purely visual behaviour in the compositor. It only reads existing geometry, adds no layout
state, no IPC, and no change to tiling or resizing, and it's per-workspace and so
per-monitor. The glass option reuses the existing material table, xray buffer and glass
shader rather than adding another effect path. It can't live in the shell: in the overview,
the background layer is rendered as an identical copy behind every workspace, so it can't
hold different mats per workspace, and IPC-driven drawing would trail the compositor's
scrolling by at least a frame.

Touched files: `niri-config/src/layout.rs`, `src/layout/{mat,mod,scrolling,workspace,monitor}.rs`,
`src/render_helpers/shaders/glass.frag` (soft glass ends only),
`docs/wiki/Configuration:-Layout.md`, `FORK.md`.
