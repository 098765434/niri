# Fork design docs

Design and implementation notes for this fork's features. User-facing docs stay in the
[wiki](../wiki/) pages that each feature extends; the overview of the whole fork is
[FORK.md](../../FORK.md).

| Doc | Feature | Status |
|---|---|---|
| [design/gesture-ipc.md](design/gesture-ipc.md) | Original spec for the gesture-progress IPC events | history |
| [gestures/mod-swipe.md](gestures/mod-swipe.md) | Mod-held three-finger window swipes | implemented |
| [effects/blur-shapes.md](effects/blur-shapes.md) | `niri-blur-shapes-v1` and liquid-glass materials | implemented |
| [effects/materials.md](effects/materials.md) | Liquid-glass materials: `materials {}` and `material` in rules | implemented |
| [effects/vibrancy.md](effects/vibrancy.md) | `niri-vibrancy-v1`: vibrant subsurfaces blended with the backdrop | implemented |
| [layout/workspace-mat.md](layout/workspace-mat.md) | Backing behind the scrolling layout | implemented |
| [layout/gaps.md](layout/gaps.md) | Workspace and overview gaps | implemented |
| [layout/desktop-layer.md](layout/desktop-layer.md) | `desktop` layer rule and the `DesktopObscured` event | implemented |
| [ipc/events.md](ipc/events.md) | Reference for the IPC events: gesture progress and `DesktopObscured` | implemented |
| [ipc/actions.md](ipc/actions.md) | `MoveColumnToIndex` with a `window_id` | implemented |
