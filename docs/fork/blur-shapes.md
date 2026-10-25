# Blur shapes and liquid-glass materials

Status: implemented on `ocbk` (`13d945b5`, `454d9751`, `74b623a7`). User-facing
docs: [Window Effects](../wiki/Window-Effects.md).

## Why

`ext-background-effect-v1` describes blur as a region of integer rectangles: no
rounded corners and no fade. A shell with many rounded pieces of glass (a
control centre, cards that swell open) gets square, all-or-nothing blur. This
fork adds a private protocol in which the client describes its blur as rounded
rectangles, each with its own opacity and look.

## Protocol — `niri-blur-shapes-v1`

`resources/niri-blur-shapes-v1.xml`, handled in `src/protocols/blur_shapes.rs`.

- `niri_blur_shapes_manager_v1`
  - `get_blur_shapes(id, surface)` creates the per-surface object.
  - Event `material(id, name)` is sent for each configured material, then `done`.
    These events are sent again on config reload.
- `niri_blur_shapes_v1`
  - `reset` clears the pending shapes.
  - `add_rounded_rect(x, y, width, height, radius_tl, radius_tr, radius_br, radius_bl, alpha, material, tint)`
    takes `fixed` values in surface-local logical px. The material is an id from
    the manager (0 = plain blur), and the tint is packed RGBA.
  - The state is double-buffered and applies on `wl_surface.commit`.

A surface with blur shapes is treated like one with a blur region: blur is on
unless a rule says `blur false`.

## Rendering

- Each shape is drawn through the existing corner-rounding shader, half a
  physical pixel inside the shape, so it stays under the client's own
  antialiased edge, at its alpha.
- **Xray:** one quad per shape from the shared blurred background
  (`src/render_helpers/xray.rs`).
- **Non-xray:** the area behind all of a surface's shapes is captured and
  blurred once, and each shape is drawn from that capture
  (`src/render_helpers/framebuffer_effect.rs`). This is needed because smithay
  re-captures duplicated framebuffer effects every frame.
- Each shape drawn from one capture gets its own element id. A shared id made
  smithay pair instances by geometry, and a frame could keep a stale rectangle
  of frost (`74b623a7`).

## Materials

A `materials {}` config block (`niri-config/src/appearance.rs`) defines named
looks that shapes pick by id. They are drawn by
`src/render_helpers/shaders/glass.frag`:

- SDF-normal refraction at the edge, with red/blue parting (`dispersion`).
- The lip magnifies inwards. It draws the backdrop from further inside, like a
  thick lens, instead of sampling past the edge (which showed a band of
  whatever lay outside, plus a fringe) (`74b623a7`).
- A rim lit from `light-angle`, with a far-edge catch and a specular highlight.
- The client's tint is laid on. With `adaptive-tint`, it is thickened over a
  backdrop of similar brightness.
- Optional `saturation` / `noise` override the blur's own values.

Changing a material reloads live. A name that isn't defined draws plain blur.
The shader follows kwin-effects-glass and Niri-glass (both GPL-3.0).

## Touched files

`resources/niri-blur-shapes-v1.xml`, `src/protocols/{blur_shapes,raw,mod}.rs`,
`src/handlers/background_effect.rs`,
`src/render_helpers/{background_effect,xray,framebuffer_effect}.rs`,
`src/render_helpers/shaders/{glass.frag,mod.rs}`, `src/niri.rs`,
`niri-config/src/{appearance,lib}.rs`, `docs/wiki/Window-Effects.md`.

## Client

amane's `LayerWindow::blur_shapes` (fork,
[098765434/amane](https://github.com/098765434/amane), `docs/fork/blur.md`). It
falls back to an `ext-background-effect` region on other compositors.
