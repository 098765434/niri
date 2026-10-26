### Overview

<sup>Since: 26.04</sup>

You can apply background effects to windows and layer-shell surfaces.
These include blur, xray, saturation, and noise.
They can be enabled in the `background-effect {}` section of [window](./Configuration:-Window-Rules.md#background-effect) or [layer](./Configuration:-Layer-Rules.md#background-effect) rules.

![Screenshot with blur](./img/blur.png)

The window needs to be semitransparent for you to see the background effect (otherwise it's fully covered by the opaque window).
Focus ring and border can also cover the background effect, see [this FAQ entry](./FAQ.md#why-are-transparent-windows-tinted-why-is-the-borderfocus-ring-showing-up-through-semitransparent-windows) for how to change this.

### Blur

Windows and layer surfaces can request their background to be blurred via the [`ext-background-effect` protocol](https://wayland.app/protocols/ext-background-effect-v1).
In this case, the application will usually offer some "background blur" setting that you'll need to enable in its configuration.

You can also enable blur on the niri side with the `blur true` background effect window rule:

```kdl
// Enable blur behind the Alacritty terminal.
window-rule {
    match app-id="^Alacritty$"

    background-effect {
        blur true
    }
}

// Enable blur behind the fuzzel launcher.
layer-rule {
    match namespace="^launcher$"

    background-effect {
        blur true
    }
}
```

Blur enabled via the window rule will follow the window corner radius set via [`geometry-corner-radius`](./Configuration:-Window-Rules.md#geometry-corner-radius).
On the other hand, blur enabled through `ext-background-effect` will exactly follow the shape requested by the window.
If the window or layer has clientside rounded corners or other complex shape, it should set a corresponding blur shape through `ext-background-effect`, then it will get correctly shaped background blur without any manual niri configuration.

This fork also offers `niri-blur-shapes-v1`: instead of a region of rectangles, a surface sends rounded rectangles, each with its own corner radii and opacity.
niri draws each with antialiased corners at its opacity, so a client with many rounded pieces of glass (such as a control center) gets blur exactly in their shape that fades with them.
A surface with blur shapes is treated like one with a blur region: blur is on unless a rule says `blur false`.

A shape can also name a *material*, a liquid-glass look niri draws over the blurred background: the edge bends what's behind it like a lens, red and blue part slightly there, the rim is lit from one side, and the client's tint is laid on, thicker over a backdrop close to it in brightness.
Materials are defined in the config, and niri tells clients which names exist; a name that isn't defined draws plain blur.

```kdl
materials {
    material "glass" {
        refraction 10     // how far the edge bends the background in, logical px
        bevel 16          // how wide the curved edge is, logical px
        dispersion 0.3    // red/blue parting at the edge, 0-1
        rim 0.6           // lit rim brightness, 0-1
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

`tone` below 1 squeezes the backdrop's brightness toward the tint's side before the tint is laid on, keeping its hue: under a dark tint the backdrop keeps 0 to `tone` of its brightness, under a light one it stays within `tone` of white. Like the darken and lighten layers in Apple's materials, this keeps a thin tint readable over any wallpaper. It only applies to shapes with a tint.
Each distinct blur costs a blur of its own: for xray, one more blur of the backdrop, redone only when it changes; for non-xray shapes, one more capture and blur of the area behind that blur's shapes, every frame they draw.
niri keeps up to four such blurs per backdrop.

Changing a material reloads live.

A layer or window rule can also name a material in its `background-effect`, or in the one in its `popups {}` block (fork).
niri then draws the whole surface's (or each pop-up's) effect as that material, in the shape of its geometry and `geometry-corner-radius`, with no tint: the client draws its own fill.
This is how a client that can't send blur shapes, such as a pop-up menu, gets the same glass as its parent.

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

Windows can also blur their pop-up menus using `ext-background-effect`.
On the niri side, you can do it with a `popups` block inside [`window-rule`](./Configuration:-Window-Rules.md#popups) and [`layer-rule`](./Configuration:-Layer-Rules.md#popups).
See those wiki pages for examples and limitations.

Global blur settings are configured in the [`blur {}` config section](./Configuration:-Miscellaneous.md#blur) and apply to all background blur.

### Xray

Xray makes the window background "see through" to your wallpaper, ignoring any other windows below.
You can enable it with `xray true` background effect [window](./Configuration:-Window-Rules.md#background-effect) or [layer](./Configuration:-Layer-Rules.md#background-effect) rule.

Xray is automatically enabled by default if any other background effect (like blur) is active.
This is because it's much more efficient: with xray active, niri only needs to blur the background once, and then can reuse this blurred version with no extra work (since the wallpaper changes very rarely).

If you have an animated wallpaper, xray will still have to recompute blur every frame, but that happens once and shared among all windows, rather than recomputed separately for each window.

#### Non-xray effects (experimental)

You can disable xray with `xray false` background effect window rule.
This gives you the normal kind of blur where everything below a window is blurred.
Keep in mind that non-xray blur and other non-xray effects are more expensive as niri has to recompute them any time you move the window, or the contents underneath change.

> [!WARNING]
> Non-xray effects are currently experimental because they have some known limitations.
>
> - They disappear during window open/close animations and while dragging a tiled window.
> Fixing this requires a refactor to the niri rendering code to defer offscreen rendering, and possibly other refactors.

### Implementation notes

The `ext-background-effect` protocol supports any wl_surface.
We currently implement it only for toplevels, layer surfaces, and pop-ups, which should cover the vast majority of what's actually used by applications.

For pop-ups, effects default to *non-xray* because pop-ups generally appear on top of windows.

In particular, the following surface types don't support `ext-background-effect`.
They can be implemented as the need arises.

- Subsurfaces. Would require implementing `clip-to-geometry` support for background effects.
- Lock surfaces. Not useful as it would just show our red locked session background.
- Cursor and drag-and-drop icon.
The main challenge here will be screencasts where the cursor is rendered separately.
This is problematic because non-xray effects require rendering the whole scene in one go rather than separately.
