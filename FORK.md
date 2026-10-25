# Fork notes — niri

A fork of [niri](https://github.com/YaLTeR/niri), the scrollable-tiling Wayland compositor, run
under the [amane-shell](https://github.com/098765434/amane-shell) bar. It adds live
gesture-progress IPC so the bar can animate in step with the compositor, Mod-held touchpad
swipes on windows, and rounded, fading liquid-glass blur for layer surfaces. Everything else is
stock upstream; the fork is a thin series of commits on top of a tagged release.

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/YaLTeR/niri` (remote: `upstream`) |
| Base release | **`v26.04`** (matches the niri 26.04 that nixpkgs 26.05 ships) |
| Fork branch | `ocbk` |
| The delta | `git log --oneline v26.04..ocbk` |

## At a glance

| Feature | Kind | Commits | Docs | Upstreamable? |
|---|---|---|---|---|
| [Gesture-progress IPC events](#gesture-progress-ipc-events) | feature | `59fe93e4` | [design](docs/fork/gesture-ipc.md) | maybe — generic IPC, but per-frame events are chatty |
| [Mod-held window swipes](#mod-held-three-finger-window-swipes) | feature | `f4367f88`, `4177c74f` | [design](docs/fork/mod-swipe.md), [wiki](docs/wiki/Gestures.md#window-swipes) | maybe — behind a config switch |
| [Blur shapes + glass materials](#rounded-fading-blur-shapes-and-liquid-glass) | feature | `13d945b5`, `454d9751`, `74b623a7` | [design](docs/fork/blur-shapes.md), [wiki](docs/wiki/Window-Effects.md) | no as-is — private protocol; GPL-3.0 shader lineage |
| [`active_column_idx` build fix](#fixes-and-housekeeping) | fix | `9d5bf9fc` | — | n/a (fixes the fork's own IPC commit) |
| [Fork docs and housekeeping](#fixes-and-housekeeping) | internal | `9366e512`, `c5b8c69d`, `8fd4e23e`, `b414445c`, `6d9306a7`, `6baa66e5` | this file | no |

Work not on `ocbk` yet: see [In progress](#in-progress). All fork-only design docs are in
[`docs/fork/`](docs/fork/README.md).

## Features

### Gesture-progress IPC events

Three continuous, per-frame values on the niri IPC event stream, so an external bar can follow
compositor gestures as they happen:

- `OverviewProgress { progress }`: the overview open/close animation, `0.0`..`1.0` (global).
- `ViewOffsetChanged { output, view_pos, active_column_idx }`: horizontal column scroll.
- `WorkspaceSwitchProgress { output, workspace_idx, active_workspace_idx }`: vertical switch.

- **Surface:** new `niri_ipc::Event` variants, tracked in `niri-ipc/src/state.rs`, printed by
  `niri msg event-stream`.
- **Files:** `niri-ipc/src/{lib,state}.rs`, `src/ipc/{server,client}.rs`,
  `src/layout/{mod,scrolling,workspace}.rs`.
- **Client:** amane's `Gestures` / `Overview` services.
- **Review it:** `git show 59fe93e4 9d5bf9fc`

### Mod-held three-finger window swipes

While Mod is held, three-finger touchpad swipes act on the focused window:

| Direction | Action | Feel |
|---|---|---|
| Up | close window | the window follows the fingers and fades; releasing past a distance/velocity threshold closes it, otherwise it springs back (it also springs back if the client doesn't close within 1 s) |
| Down | maximize to edges | a rubber-banded nudge that commits live once the swipe passes the threshold |
| Left / Right | column one preset narrower / wider | one step per stretch of travel, no wrap-around, a small bump at the ends; wider than the widest preset goes to full width (`4177c74f`) |

Releasing Mod mid-swipe cancels.

- **Surface:** config `gestures { mod-swipe { off; } }`.
- **Files:** `niri-config/src/gestures.rs`, `src/input/mod.rs`,
  `src/layout/{window_swipe,mod,tile,closing_window,scrolling,floating,workspace,tests}.rs`,
  `docs/wiki/{Gestures,Configuration:-Gestures}.md`.
- **Review it:** `git show f4367f88 4177c74f`

### Rounded, fading blur shapes and liquid glass

`niri-blur-shapes-v1` is a private protocol in which a client describes its background blur as
rounded rectangles, each with its own alpha, material and tint. It replaces an
`ext-background-effect-v1` region, which is integer rectangles with no rounding and no fade.
Materials are named liquid-glass looks defined in a `materials {}` config block and drawn by
`glass.frag`: refraction with red/blue parting, a lit rim and an adaptive tint.

- **Surface:** protocol `resources/niri-blur-shapes-v1.xml`; config `materials { material "…" { … } }`.
- **Files:** `src/protocols/{blur_shapes,raw,mod}.rs`, `src/handlers/background_effect.rs`,
  `src/render_helpers/{background_effect,xray,framebuffer_effect}.rs`,
  `src/render_helpers/shaders/{glass.frag,mod.rs}`, `src/niri.rs`,
  `niri-config/src/{appearance,lib}.rs`, `docs/wiki/Window-Effects.md`.
- **Client:** amane's `LayerWindow::blur_shapes`.
- **Credits:** the shader follows kwin-effects-glass and Niri-glass (both GPL-3.0).
- **Review it:** `git show 13d945b5 454d9751 74b623a7`

## Fixes and housekeeping

- `9d5bf9fc` **Fix duplicate `active_column_idx`**: the IPC commit added a second
  `ScrollingSpace::active_column_idx`; this removes it and un-gates the `#[cfg(test)]` one.
- `9366e512` adds FORK.md and stops tracking `.claude/`.
- `c5b8c69d` adds the README fork banner and the rebase workflow.
- `8fd4e23e` renames the branch to `ocbk`.
- `b414445c`, `6d9306a7`, `6baa66e5` are FORK.md updates and the mod-swipe design doc.

## In progress

| Work | Where | Status |
|---|---|---|
| **Pinned columns**: a column docked to a screen edge, with its own rail, outside the scroll math; actions `pin-column`, `unpin-column`, `toggle-column-pinned`, `focus-pinned` | branch `pinned-columns` (`99562604`, pushed), spec `niri-pinned-columns-spec.md` on that branch | compositor core done with tests; overview drop target, shell UI and cross-workspace pins deferred; not merged into `ocbk` |
| **Workspace mat**: a solid or glass backing behind the scrolling layout that fades out in the overview | [`docs/fork/workspace-mat.md`](docs/fork/workspace-mat.md) (untracked) | draft spec, no code |
| **`MoveColumnToIndex` by window**: an optional `window_id` that moves that window's column (on whichever workspace it's on) without changing the active column or the focus, for amane-shell's dock drag (`Layout::move_column_with_window_to_index`, config action `MoveColumnToIndexById`) | [`docs/fork/column-by-window.md`](docs/fork/column-by-window.md); `ocbk` | done with a layout test and a proptest op; stock niri ignores the field and moves the focused column |

The `pinned-columns` branch predates the move of the fork docs into `docs/fork/`. Expect a
FORK.md conflict when it's rebased onto `ocbk`; move its spec to `docs/fork/pinned-columns.md`
then.

## Reviewing the fork

```sh
git log --oneline --stat v26.04..ocbk     # every fork commit and the files it touched
git diff --stat v26.04..ocbk              # the whole delta, by file
git diff v26.04..ocbk -- <paths>          # one feature (paths listed above)
git range-diff <old-base> <new-base> ocbk # after a rebase: what changed in each commit
```

## Consumed by

Built as the compositor via a flake input (`programs.niri.package`), e.g. in a NixOS config:
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

Then replace `v26.04` in this file with the new tag, and re-pin the consuming config
(`nix flake update niri-fork`).
