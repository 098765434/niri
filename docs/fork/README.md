# niri fork docs

A fork of [niri](https://github.com/YaLTeR/niri), the scrollable-tiling Wayland compositor, run
under the [amane-shell](https://github.com/098765434/amane-shell) bar. It adds liquid-glass
effects, layout extras, a gesture and IPC for the shell; everything else is stock upstream.

Branch `ocbk`, based on the upstream release `v26.04`. Rebasing, branches and the full commit
log: [maintaining.md](maintaining.md).

## Features

| Feature | Summary | Config / IPC surface | Doc | Status |
|---|---|---|---|---|
| Materials | named liquid-glass looks drawn over the blur | `materials { material "…" { … } }`; `background-effect { material "…" }` in rules | [effects/materials.md](effects/materials.md) | implemented, `454d9751`, `3cde7327`, `a1a78bd0`, `78620a8c`, `7bef7001` |
| Blur shapes | rounded, fading blur shapes, each with a material and tint | protocol `niri-blur-shapes-v1` | [effects/blur-shapes.md](effects/blur-shapes.md) | implemented, `13d945b5`, `74b623a7`, `3cde7327` |
| Vibrancy | a subsurface blended with the backdrop (plus-lighter / plus-darker) | protocol `niri-vibrancy-v1` | [effects/vibrancy.md](effects/vibrancy.md) | implemented, `4c68634c`, `ada326f0` |
| Effects memory | what xray buffers, blur chains and shape captures hold, built and freed when used | — | [effects/memory.md](effects/memory.md) | implemented, `0b2ab4e5` |
| Workspace mat | a backing behind the scrolling layout that fills the gaps | `layout { mat { … } }` | [layout/workspace-mat.md](layout/workspace-mat.md) | implemented, `58528c07`, `f297cd5e`, `74d6c36d` |
| Gaps | gaps between workspaces, and between windows in the overview | `layout { workspace-gap }`, `overview { workspace-gap window-gap }` | [layout/gaps.md](layout/gaps.md) | implemented, `dab83f82`, `34c91b6b`, `e0139bac` |
| Desktop layer | a background layer that stays still on the wallpaper as the desktop | `layer-rule { desktop true }` | [layout/desktop-layer.md](layout/desktop-layer.md) | implemented, `871b5ee5` |
| HDR | HDR10 output with the SDR desktop unchanged, and colour-managed HDR surfaces | `output { hdr; sdr-brightness }`, `niri msg output … hdr`, protocol `wp-color-management-v1` | [display/hdr.md](display/hdr.md) | implemented, `b322b674`, `cf559b95` |
| Mod swipe | Mod-held three-finger swipes close, maximize and resize the focused window | `gestures { mod-swipe { off } }` | [gestures/mod-swipe.md](gestures/mod-swipe.md) | implemented, `f4367f88`, `4177c74f` |
| IPC events | per-frame gesture progress, and when the desktop is obscured | `OverviewProgress`, `ViewOffsetChanged`, `WorkspaceSwitchProgress`, `DesktopObscured` | [ipc/events.md](ipc/events.md) | implemented, `59fe93e4`, `9d5bf9fc`, `871b5ee5` |
| IPC actions | move a window's column without moving the focus | `MoveColumnToIndex { window_id }`, `niri msg action move-column-to-index --window-id` | [ipc/actions.md](ipc/actions.md) | implemented, `36f2cce3` |

Each feature doc starts with **Usage** (config, protocol or IPC), then **Design / internals**.

## Groups

- **Effects:** liquid glass. [Materials](effects/materials.md) are the looks,
  [blur shapes](effects/blur-shapes.md) are how a client asks for them in its own shapes, and
  [vibrancy](effects/vibrancy.md) blends a client's labels with the glass under them.
  [Memory](effects/memory.md) is what all of it holds on the GPU and when it lets go.
- **Layout:** the scrolling layout's surroundings. The [workspace mat](layout/workspace-mat.md)
  fills the gaps, [gaps](layout/gaps.md) sets the space between workspaces and overview windows,
  and the [desktop layer](layout/desktop-layer.md) keeps widgets on the wallpaper.
- **Display:** [HDR](display/hdr.md), the output in HDR10 and clients' colour described through
  `wp-color-management-v1`.
- **Gestures:** [Mod swipe](gestures/mod-swipe.md), touchpad swipes on the focused window.
- **IPC:** [events](ipc/events.md) a bar animates from, and [actions](ipc/actions.md) a dock
  sends.

The protocol specs are authoritative for the wire format:
[`niri-blur-shapes-v1.xml`](../../resources/niri-blur-shapes-v1.xml) and
[`niri-vibrancy-v1.xml`](../../resources/niri-vibrancy-v1.xml).

## Used by

- **amane** (fork, which amane-shell is built with): the client side of blur shapes, vibrancy, the desktop layer and
  the IPC events and actions. See amane's
  [fork docs](https://github.com/098765434/amane/blob/ocbk/docs/fork/README.md).
- **amane-shell** (the bar, dock and widgets): see its
  [docs](https://github.com/098765434/amane-shell/blob/main/docs/README.md).

The workspace mat, gaps and mod swipe are used only by niri. HDR is set from amane-shell's
Shell Settings, over IPC and the config.

## Design history

Specs kept as written, with a status header:

- [design/gesture-ipc.md](design/gesture-ipc.md): the original spec for the gesture-progress
  IPC events.
