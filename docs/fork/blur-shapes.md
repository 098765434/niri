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
  - Event `backdrop(index, luminance)` (version 2) reports how bright the
    blurred backdrop behind a shape is, 0 to 1, for materials with
    `report-backdrop`. The surface's resource is kept in its data map
    (`BlurShapesSurfaceData`) so it can be sent from the renderer.

A surface with blur shapes is treated like one with a blur region: blur is on
unless a rule says `blur false`.

## Rendering

- Each shape is drawn through the existing corner-rounding shader, half a
  physical pixel inside the shape, so it stays under the client's own
  antialiased edge, at its alpha.
- **Xray:** one quad per shape from the shared blurred background
  (`src/render_helpers/xray.rs`). A material with a blur of its own
  (`blur-passes`, `blur-offset`) draws from a second blur of the same
  background, which the `EffectBuffer` keeps beside its own (up to four, redone
  only when the background changes).
- **Non-xray:** the area behind all of a surface's shapes is captured and
  blurred once, and each shape is drawn from that capture
  (`src/render_helpers/framebuffer_effect.rs`). This is needed because smithay
  re-captures duplicated framebuffer effects every frame. Shapes whose
  materials blur differently are grouped, one capture per blur, of the area
  behind that group's shapes. Every capture element comes after every shape in
  the element list, so all are taken before any shape draws and none sees
  another's glass.
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
- With `materialize` (on by default), `glass_uniforms` scales the refraction,
  dispersion, rim and specular by the element's alpha, so a shape fading in
  builds its lens up instead of showing a finished lens at low opacity.

Changing a material reloads live. A name that isn't defined draws plain blur.
The shader follows kwin-effects-glass and Niri-glass (both GPL-3.0).

### Backdrop readings (`report-backdrop`)

`src/render_helpers/backdrop.rs`. For a non-xray shape whose material reports
it, `render_params_for_shapes` keeps the shape's index in its list
(`RenderParams.report`).
- **Measure:** the `FramebufferSampleElement` drawing it blits its area of the
  blurred capture down to 8×8 and reads it into a pixel-pack buffer behind a
  fence. Nothing waits for the GPU. Each shape is measured at most every
  250 ms.
- **Report:** `render_for_tile`, on a later frame, maps the buffers whose
  fences have passed, takes the mean luminance, and sends `backdrop` when it
  moved by more than 0.08 since the last reading sent.
- **Limits:** a reading lands with the next frame drawn after it, so a change
  that redraws a single frame and then rests reports late, at the next redraw.
  A surface destroyed with a reading in flight leaves its 256-byte buffer
  behind, since no GL context is current to delete it then.

### Materials on rules

`background-effect { material "name" }` in a layer or window rule, or in its
`popups {}` block, draws the rule's effect as that material. This is for
surfaces that can't send shapes: amane's xdg-popups (menus, tooltips) have no
blur-shapes object of their own.

- The name is resolved in `BackgroundEffect::update_render_elements`, into
  `Options.material`, on every update. A reloaded material therefore damages
  through the options comparison.
- `render_for_tile` turns the geometry-shaped params into a single blur shape
  (`material_params`): the clip is the geometry inset by half a pixel, with the
  rule's corner radius, and `exact_clip` is set. From there the shape path draws
  it, both non-xray (`render_shapes`) and xray.
- The tint is clear: the client fills its card itself, so it can follow its
  theme. Adaptive tint then has nothing to thicken.
- A surface with its own blur region (ext-background-effect, clip `None`) keeps
  plain blur.
- `BackgroundEffect`, `BackgroundEffectRule` and `ResolvedPopupsRules` are no
  longer `Copy`, because of the name. The render paths clone them.

## Touched files

`resources/niri-blur-shapes-v1.xml`, `src/protocols/{blur_shapes,raw,mod}.rs`,
`src/render_helpers/backdrop.rs`,
`src/handlers/background_effect.rs`,
`src/render_helpers/{background_effect,xray,framebuffer_effect}.rs`,
`src/render_helpers/shaders/{glass.frag,mod.rs}`, `src/niri.rs`,
`niri-config/src/{appearance,lib,window_rule}.rs`, `src/{layer,window}/mapped.rs`,
`docs/wiki/Window-Effects.md`, `docs/wiki/Configuration:-{Layer,Window}-Rules.md`.

## Client

amane's `LayerWindow::blur_shapes` (fork,
[098765434/amane](https://github.com/098765434/amane), `docs/fork/blur.md`). It
falls back to an `ext-background-effect` region on other compositors.
