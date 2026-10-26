# Vibrancy

Status: implemented on `ocbk`. The client side is amane's vibrant layer (`amane/docs/fork/blur.md`).

## Why

Apple's labels on a material are *vibrant*: they're blended with the frosted backdrop under them
(plus-lighter in dark, plus-darker in light), so they take on its colour. A client can't do this
itself, because it never sees what's behind its surface. Translucent label colours come close,
since the backdrop shows through them, but they only fade towards it. They don't brighten or
darken it.

## Protocol: `niri-vibrancy-v1`

`resources/niri-vibrancy-v1.xml`, handled in `src/protocols/vibrancy.rs`.

- `niri_vibrancy_manager_v1.get_vibrancy(id, surface)` makes the per-surface object. A second
  one while the first lives is the `already_constructed` error.
- `niri_vibrancy_v1.set_mode(none | plus_lighter | plus_darker)` is double-buffered and applies
  on `wl_surface.commit`. Destroying the object sets `none` on the next commit.

It's meant for a subsurface that holds only the greyscale vibrant content (labels, glyphs,
fills), above a surface whose background is a material.

## Rendering

`push_elements_from_surface_tree` (`src/render_helpers/surface.rs`) wraps every surface's
element in a `VibrantSurfaceRenderElement` (`src/render_helpers/vibrant.rs`), with the mode the
surface's state has now. With `none` it is the plain element.

- **plus_lighter** draws with `glBlendFunc(ONE, ONE)`: the surface is added to what's behind,
  clamped.
- **plus_darker** draws as a multiply, `glBlendFunc(DST_COLOR, ONE_MINUS_SRC_ALPHA)`. Over a
  premultiplied source this gives `D · (1 − a + s)`, which darkens what's behind while keeping
  its colour. True plus-darker would need a texture program of its own, which would clash with
  the rounded-corner one. Multiply needs only a blend function, so it works under any program.
- Alpha is kept as it was (`ZERO, ONE`).
- A vibrant element reports no opaque region, because smithay draws an opaque region with
  blending off and then puts its own blend function back. It also has no underlying storage,
  so it can't be scanned out past the blend.

The elements are drawn back to front, so when a vibrant subsurface is drawn the framebuffer
already holds the frosted backdrop, the material's tint and its parent surface.

## Not done

- Screencasts and screenshots render the same elements, so they show the blend too. Nothing
  special is done for them.
