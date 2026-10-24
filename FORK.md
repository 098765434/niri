# Fork notes — niri

A fork of [niri](https://github.com/YaLTeR/niri), the scrollable-tiling Wayland compositor. It adds
**live gesture-progress IPC events** and **Mod-held three-finger window swipes** on top of a tagged
upstream release; everything else is stock upstream.

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/YaLTeR/niri` (remote: `upstream`) |
| Base release | **`v26.04`** (matches the niri 26.04 that nixpkgs 26.05 ships) |
| Fork branch | `ocbk` |
| Custom commits | 2, on top of `v26.04` (see below) |

```
git log --oneline v26.04..ocbk
  Add live gesture-progress IPC events     # the feature
  Fix duplicate active_column_idx          # build fix (tests config)
```

## What this fork adds

Three continuous, per-frame values streamed over the niri IPC event stream so an external bar
(the amane shell) can animate in lockstep with the compositor:

- `OverviewProgress { progress }` — overview open/close animation, `0.0`..`1.0` (global).
- `ViewOffsetChanged { output, view_pos, active_column_idx }` — horizontal column scroll.
- `WorkspaceSwitchProgress { output, workspace_idx, active_workspace_idx }` — vertical switch.

Design and implementation notes: [`niri-gesture-ipc-spec.md`](./niri-gesture-ipc-spec.md).
Touched files: `niri-ipc/src/lib.rs`, `niri-ipc/src/state.rs`, `src/ipc/server.rs`,
`src/ipc/client.rs`, `src/layout/{mod,scrolling,workspace}.rs`.

### Mod-held three-finger window swipes

While Mod is held, three-finger touchpad swipes act on the focused window (branch
`mod-swipe-gestures`):

| Direction | Action | Feel |
|---|---|---|
| Up | close window | window follows the fingers and fades; release past a distance/velocity threshold closes, otherwise springs back (also springs back if the client doesn't close within 1 s) |
| Down | maximize to edges | rubber-banded nudge, commits live once the swipe passes the threshold |
| Left / Right | column one preset narrower / wider | one step per stretch of travel, no wrap-around, small bump at the ends |

Releasing Mod mid-swipe cancels. Disable with `gestures { mod-swipe { off; } }`. Design, tuning
constants and limitations: [`niri-mod-swipe-spec.md`](./niri-mod-swipe-spec.md). Touched files: `niri-config/src/gestures.rs`,
`src/input/mod.rs`, `src/layout/{window_swipe,mod,tile,closing_window,scrolling,floating,workspace,tests}.rs`,
`docs/wiki/{Gestures,Configuration:-Gestures}.md`.

### Rounded, fading blur shapes (`niri-blur-shapes-v1`)

A private protocol (`resources/niri-blur-shapes-v1.xml`) that lets a client describe its
background blur as rounded rectangles, each with its own alpha, material id and tint, in place
of an `ext-background-effect-v1` region (integer rectangles, no rounding, no fade). Each shape
is drawn through the existing corner-rounding shader, half a physical pixel inside the shape so
it stays under the client's own antialiased edge, at its alpha (blur can fade). Xray draws one
quad per shape from the shared blurred background; non-xray captures and blurs the area behind
all of a surface's shapes once and draws each shape from that capture. Materials are named liquid-glass
presets in a `materials {}` config block (`niri-config/src/appearance.rs`), announced to clients
and drawn by `src/render_helpers/shaders/glass.frag`: SDF-normal refraction with red/blue
parting, a lit rim with far-edge catch and highlight, and an adaptive tint, after
kwin-effects-glass and Niri-glass (both GPL-3.0). Client: amane's
`LayerWindow::blur_shapes`. Touched files: `src/protocols/{blur_shapes,raw,mod}.rs`,
`src/handlers/background_effect.rs`, `src/render_helpers/{background_effect,xray,framebuffer_effect}.rs`,
`src/niri.rs`, `src/render_helpers/shaders/{glass.frag,mod.rs}`, `niri-config/src/{appearance,lib}.rs`,
`docs/wiki/Window-Effects.md`.

## Consumed by

Built as the compositor via a flake input (`programs.niri.package`) — e.g. in a NixOS config:
```nix
niri-fork = { url = "github:098765434/niri/ocbk"; inputs.nixpkgs.follows = "nixpkgs"; };
```
Commit on this branch, then `nix flake update niri-fork` and rebuild.

## Updating to a new upstream release

The fork keeps its commits on top of a release tag, so updating is a rebase onto the new tag:

```sh
git fetch --tags upstream
git rebase --onto <new-tag> v26.04 ocbk
# On conflicts, find commits upstream already has and skip them:
git range-diff v26.04 <new-tag> ocbk
git rebase --skip                            # for each commit range-diff shows as upstreamed
# then build + test in the dev shell:
nix develop -c cargo build && nix develop -c cargo test
```

Then update the base tag (`git tag -f`) and re-pin the consuming config (`nix flake update
niri-fork`).
