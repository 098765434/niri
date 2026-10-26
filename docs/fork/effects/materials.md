# Liquid-glass materials

Materials are named liquid-glass looks defined in a `materials {}` config block. niri draws
them over the blurred background for [blur shapes](blur-shapes.md) that name one, for layer and
window rules that name one, and for the [workspace mat](../layout/workspace-mat.md).

## Usage

A shape can name a *material*, a liquid-glass look niri draws over the blurred background: the edge bends what's behind it like a lens, red and blue part slightly there, the rim is lit from one side, and the client's tint is laid on, thicker over a backdrop close to it in brightness.
Materials are defined in the config, and niri tells clients which names exist; a name that isn't defined draws plain blur.

```kdl
materials {
    material "glass" {
        refraction 10     // how far the edge bends the background in, logical px
        bevel 16          // how wide the curved edge is, logical px
        dispersion 0.3    // red/blue parting at the edge, 0-1
        rim 0.6           // lit rim brightness, 0-1
        glow 0.15         // soft glow over the bevel's lit sides, 0-1; a quarter of rim unless set
        light-angle 225   // where the light comes from, degrees clockwise from the right
        far-rim 0.35      // light caught on the far edge, 0-1
        specular 0.5      // highlight where the light hits the edge, 0-1
        tint 1            // how strongly the client's tint shows, 0-2
        adaptive-tint 0   // thicken the tint over a backdrop of similar brightness, 0-1
        tone 1            // how much of the backdrop's brightness range to keep, toward the tint's side, 0-1
        materialize true  // build the lens and rim up with the shape's opacity as it fades in
        report-backdrop false // tell the client how bright the backdrop is behind each shape
        // saturation 1.5 // optional, in place of the blur's
        // noise 0.02     // optional, in place of the blur's
        // blur-passes 2  // optional, in place of `blur {}`'s
        // blur-offset 1.5 // optional, in place of `blur {}`'s
    }
}
```

A shape fading in with `materialize` on (the default) bends and lights the backdrop only as much as it shows, so the glass materializes rather than a finished lens fading in.

With `report-backdrop` on, niri tells the client how bright the blurred backdrop behind each of that material's shapes is (version 2 of `niri-blur-shapes-v1`), so it can turn a small piece of glass light or dark with what's behind it.
The reading is coarse: an 8×8 version of the shape's area, read back from the GPU without waiting, at most four times a second per shape, and sent only when it moves noticeably.
It works for non-xray shapes only, and it's off by default.

With `blur-passes` or `blur-offset` set, the backdrop behind that material's shapes is blurred with them instead of `blur {}`'s, so big glass can frost more than small glass, as Apple's does: Figma's macOS kit frosts small glass at 6 and medium and large glass at 16.

The lens bends the backdrop in from past the shape's edge, as Apple's glass does, so a rim continues what lies beside the shape; the blurred capture behind a surface's shapes reaches `refraction` further out for it. `rim` is a hairline at the edge, brightest facing `light-angle` and caught again on the far side by `far-rim`; `glow` is a soft haze over the bevel on those sides. Apple's glass lights the rim from the top with an opposite fill light (`light-angle 270`, `far-rim 0.5`) and has next to no glow.

`tone` below 1 squeezes the backdrop's brightness toward the tint's side before the tint is laid on, keeping its hue: under a dark tint the backdrop keeps 0 to `tone` of its brightness, under a light one it stays within `tone` of white. Like the darken and lighten layers in Apple's materials, this keeps a thin tint readable over any wallpaper. It only applies to shapes with a tint.
Each distinct blur costs a blur of its own: for xray, one more blur of the backdrop, redone only when it changes; for non-xray shapes, one more capture and blur of the area behind that blur's shapes, every frame they draw.
niri keeps up to four such blurs per backdrop.

Changing a material reloads live.

### On layer and window rules

A layer or window rule can also name a material in its `background-effect`, or in the one in its `popups {}` block.
niri then draws the whole surface's (or each pop-up's) effect as that material, in the shape of its geometry and `geometry-corner-radius`, with no tint: the client draws its own fill.
This is how a client that can't send blur shapes, such as a pop-up menu, gets the same glass as its parent.

- `material`: the name of a material to draw the effect with, as liquid glass in the shape of the surface's geometry and `geometry-corner-radius`.
  The surface draws its own tint over it, so the material adds the lens and the lit rim.
  It needs blur, and works in a `popups {}` block too. A surface that sets its own blur region keeps plain blur there, and a name that isn't defined draws plain blur.

```kdl
layer-rule {
    match namespace="^my-shell$"

    popups {
        geometry-corner-radius 10
        background-effect {
            blur true
            material "glass"
        }
    }
}
```

See upstream's [window effects page](../../wiki/Window-Effects.md) for background effects in
general, and [`background-effect`](../../wiki/Configuration:-Layer-Rules.md#background-effect)
in the rules.

## Design / internals

A `materials {}` config block (`niri-config/src/appearance.rs`) defines named
looks that shapes pick by id. They are drawn by
`src/render_helpers/shaders/glass.frag`:

- SDF-normal refraction at the edge, with red/blue parting (`dispersion`).
- The lip bends the backdrop in from past the edge, as the recovered macOS 26
  glass shaders do (sampling outward along the SDF normal). `74b623a7` had it
  magnify inwards because the capture stopped at the shape's edge; now
  `render_shapes` grows each blur group's capture by the material's
  `refraction`, so the rim shows real backdrop.
- A rim hairline lit from `light-angle`, with a far-edge catch (`far-rim`, Apple's
  opposite fill light), a specular highlight, and a separate `glow` over the
  bevel's lit sides (a quarter of `rim` unless set).
- With `tone` below 1, the backdrop's brightness is first squeezed toward the
  tint's side (dark tint: `c·tone`; light tint: `1 − (1 − c)·tone`), as Apple's
  darken/lighten layers do, so thin tints stay readable.
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
