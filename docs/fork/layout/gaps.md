# Workspace and overview gaps

Three gaps, each animating with the overview's open and close (gestures included). With none of
them set, the spacing is stock.

Status: implemented in `dab83f82`; the normal-view gap is measured outside the bars' exclusive
zones and may be negative since `34c91b6b`, and steps by the rounded working-area edges since
`e0139bac`.

## Usage

### `layout { workspace-gap }`

Set the space between workspaces in the normal view, in logical pixels, visible while switching workspaces.
By default it is 10% of the output height.

The space is measured between the parts of the workspaces outside bars and other layer-shell exclusive zones, so a bar's zone doesn't show as a band between workspaces while switching.
`0` makes one workspace's content meet the next one's, and negative values overlap them further: with `gaps 8`, `workspace-gap -8` leaves the same 8 pixels between the windows of two workspaces as between two columns.

The space grows or shrinks smoothly to the overview's own [`workspace-gap`](#overview--workspace-gap-) as the overview opens.

```kdl
layout {
    workspace-gap 0
}
```

The value may be negative (the config accepts −65535 to 65535). The resulting gap is clamped at
minus half the output height.

### `overview { workspace-gap }`

Set the space between workspaces in the overview, in logical pixels on screen.
By default it is the normal view's space between workspaces, zoomed out with them.

The value must be 0 or more: unlike the normal view's gap, the overview gaps can't be negative.

The space changes smoothly from the normal view's [`workspace-gap`](#layout--workspace-gap-) as the overview opens, following gestures too.

```kdl
overview {
    workspace-gap 24
}
```

### `overview { window-gap }`

Set the space between windows in the overview, in logical pixels on screen: between columns, and between windows stacked in a column.
By default it is the layout's `gaps`, zoomed out with the workspace.

The windows move apart as the overview opens and back as it closes, without resizing.
The gaps at the edges of the workspace don't change.
The value must be 0 or more.

```kdl
overview {
    window-gap 16
}
```

## Design / internals

- **Config.** `layout { workspace-gap }` is `FloatOrInt<-65535, 65535>` in
  `niri-config/src/layout.rs`. `overview { workspace-gap window-gap }` are
  `FloatOrInt<0, 65535>` in `niri-config/src/misc.rs`. All three are `Option`s; unset, niri
  behaves as stock.
- **Workspace gap.** `Monitor::workspace_gap(zoom)` (`src/layout/monitor.rs`) interpolates from
  the normal gap to the overview gap by how far the zoom has gone, `t = (1 − zoom) / (1 −
  overview_zoom)`, so it follows the overview's open and close, gestures included. Unset, the
  normal gap is the stock 10% of the view height and the overview gap is that times the
  overview zoom. A set normal gap is turned into a step between the working areas' edges,
  rounded to physical pixels like the mat's `behind-bars false` edges, so the bars' exclusive
  zones overlap and no row of wallpaper shows between the areas at fractional scales. The
  result is rounded to physical pixels, at least one when positive.
- **Window gap.** `overview_extra_gap` (`src/layout/scrolling.rs`) is the space the overview
  adds on top of `gaps`, in workspace pixels, so windows sit `window-gap` apart on screen once
  the overview is open: `(window-gap / overview_zoom − gaps) · progress`, at least `−gaps`.
  `ScrollingSpace` and each `Column` keep it as `extra_gap` and add it to column x steps and
  tile y steps only, so windows move apart without resizing and hit testing follows.
- **Files:** `niri-config/src/{layout,misc,lib}.rs`,
  `src/layout/{monitor,workspace,scrolling,tests}.rs`.
