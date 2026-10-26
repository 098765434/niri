# Workspace and overview gaps

Three gaps, each animating with the overview's open and close (gestures included). With none of
them set, the spacing is stock.

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

### `overview { workspace-gap }`

Set the space between workspaces in the overview, in logical pixels on screen.
By default it is the normal view's space between workspaces, zoomed out with them.

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

```kdl
overview {
    window-gap 16
}
```
