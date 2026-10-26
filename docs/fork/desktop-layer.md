# Spec: desktop layers

Status: implemented on `ocbk` · Target: niri fork + amane-shell's desktop widgets · Scope: a
layer rule that puts a background layer on the backdrop as the desktop, and an IPC event that tells
it when it can't be seen.

## 1. Goal

amane-shell's desktop widgets are a clear Background-layer surface with frosted cards on it. Drawn
as a stock background layer, niri copies it into every workspace (`scale_relocate_crop`): the cards
slide with a workspace switch, shrink with the overview and show on every workspace in it. They
should behave like things on the wallpaper:

- still, as the wallpaper is, while workspaces slide over them;
- the same size in the overview, fading out as it opens (with the gesture) and in as it closes;
- taking input wherever no window is, as a background layer does;
- able to stop drawing while nothing under the workspaces shows.

The wallpaper itself already does the first part, from `place-within-backdrop` with a transparent
`layout { background-color }`.

## 2. The `desktop` layer rule

`layer-rule { desktop true }`, resolved into `ResolvedLayerRules::desktop`. It applies only where
`place-within-backdrop` would (a Background layer with `exclusive_zone == DontCare`):
`MappedLayer::is_desktop()`.

- **Placement.** A desktop layer is in the backdrop set (`place_within_backdrop()` is true for it
  too), so it's drawn once, at output coordinates, under the workspaces. `layers_in_render_order`
  yields desktop layers first, which puts them above the rest of the backdrop (the wallpaper)
  whatever order they mapped in.
- **Fade.** `Niri::layer_fade` gives `1 - overview_progress`, which `render_normal` and
  `render_popups` multiply into the rule's opacity. `background_effect::render_for_tile` takes the
  same alpha into each area's `RenderParams.alpha`, so the frost fades with the surface (xray and
  non-xray). At 0 the surface isn't drawn at all. The backdrop xray buffer is filled through the
  same path, so frosted windows see the faded desktop.
- **Popups.** A desktop layer's popups (its menus) are drawn by `render_desktop_popups`, with the
  other Background/Bottom popups above the windows, at output coordinates; the backdrop pass skips
  them. Their grabs and the layer's on-demand keyboard focus are kept (a backdrop layer's are
  dropped).
- **Input.** `contents_under` and `is_layout_obscured_under` hit-test desktop layers at output
  coordinates (no workspace offset): their popups with the Background popups, the surface after the
  workspace's Background layers, both only while the overview is closed, as for other background
  layers.

## 3. `DesktopObscured` IPC event

```rust
DesktopObscured { output: String, obscured: bool }
```

Sent per output when the value changes, from `ipc_refresh_desktop_obscured` (part of
`ipc_refresh_layout`, which runs every event loop pass), and replicated to new clients by
`DesktopObscuredState`. `Layout::is_desktop_obscured(output)` is true when:

- the overview is fully open (progress 1), where desktop layers have faded out; or
- the overview is closed, no workspace switch is in progress on the output
  (`Monitor::is_desktop_covered`), and the active workspace's visible tiles cover its working
  area (`Workspace::is_working_area_covered`).

The coverage test grows each tile by `layout.gaps` on every side, since the mat fills the gaps, and
subtracts the tiles from the working area in whole logical pixels (area rounded inward, tiles
outward). A fullscreen or maximized window covers it, as do columns that fill the view. Windows
are taken as opaque: a translucent or frosted window that covers the area counts as covering.

A switch in progress or an overview anywhere between closed and open counts as visible: either
shows what's under the workspaces.

## 4. Client

amane's `Desktop` service holds the event per output. amane-shell's widgets freeze their surface
(draw nothing, commit nothing) while their output's desktop is obscured, and catch up in one frame
when it shows again. See amane-shell's `docs/widgets.md`.

## 5. Files

- `niri-config/src/{layer_rule,lib}.rs`, `src/layer/{mod,mapped}.rs`
- `src/niri.rs` (render order, fade, desktop popups, input, focus), `src/handlers/xdg_shell.rs`
- `src/render_helpers/background_effect.rs`, `src/window/mapped.rs` (the effect alpha)
- `src/layout/{mod,monitor,workspace}.rs` (coverage)
- `niri-ipc/src/{lib,state}.rs`, `src/ipc/{server,client}.rs`
- `docs/wiki/Configuration:-Layer-Rules.md`
