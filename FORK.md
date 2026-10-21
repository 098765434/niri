# Fork notes — niri

A fork of [niri](https://github.com/YaLTeR/niri), the scrollable-tiling Wayland compositor. It adds
**live gesture-progress IPC events** and **pinned columns** on top of a tagged upstream release;
everything else is stock upstream. Each feature is intended to be proposed upstream — the fork stays
a thin, rebasable series.

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/YaLTeR/niri` (remote: `upstream`) |
| Base release | **`v26.04`** (matches the niri 26.04 that nixpkgs 26.05 ships) |
| Fork branch | `ocbk` |
| Custom commits | two feature series on top of `v26.04` (gesture-IPC, pinned columns — see below) |

```
git log --oneline v26.04..ocbk
  # Pinned columns
  Add pinned columns (compositor core)     # the feature
  # Gesture-progress IPC
  Rename work branch to ocbk
  Add fork banner; document rebase/range-diff workflow
  Add FORK.md; stop tracking .claude tooling dir
  Fix duplicate active_column_idx          # build fix (tests config)
  Add live gesture-progress IPC events     # the feature
```

## What this fork adds

### 1. Live gesture-progress IPC events

Three continuous, per-frame values streamed over the niri IPC event stream so an external bar
(the amane shell) can animate in lockstep with the compositor:

- `OverviewProgress { progress }` — overview open/close animation, `0.0`..`1.0` (global).
- `ViewOffsetChanged { output, view_pos, active_column_idx }` — horizontal column scroll.
- `WorkspaceSwitchProgress { output, workspace_idx, active_workspace_idx }` — vertical switch.

Design and implementation notes: [`niri-gesture-ipc-spec.md`](./niri-gesture-ipc-spec.md).
Touched files: `niri-ipc/src/lib.rs`, `niri-ipc/src/state.rs`, `src/ipc/server.rs`,
`src/ipc/client.rs`, `src/layout/{mod,scrolling,workspace}.rs`.

### 2. Pinned columns (compositor core)

A **pinned column** is a first-class fourth layout position (alongside scrollable, floating,
tabbed): it is anchored to a screen edge (a *pin rail*), reserves space so it never occludes the
scrollable row, and is excluded from scroll math — a narrow companion (chat, player, notes) that
stays put while the main row scrolls underneath. Up to two columns per edge, both edges allowed.

Driven by new IPC actions (see Usage); surfaced on `niri_ipc::WindowLayout` as
`is_pinned` / `pin_edge` so a bar can show a pin badge and drive drag-to-pin. Scrollable
`focus-column-left/right` skip pins; `focus-pinned` reaches them. The gesture/overview drop-target
integration, the amane client UI, and global cross-workspace pins are later passes.

Design and implementation notes: [`niri-pinned-columns-spec.md`](./niri-pinned-columns-spec.md)
(including an "Implementation status" section on where the code refines the spec).
Touched files: `niri-ipc/src/lib.rs`, `niri-config/src/binds.rs`, `src/input/mod.rs`,
`src/ipc/client.rs`, `src/layout/{mod,workspace,scrolling,tile}.rs`, `src/layout/tests.rs`.

## Usage (pinned columns)

New actions, usable as config binds or `niri msg action`:

```kdl
binds {
    Mod+P       { pin-column; }                  // pin the active column to the left rail
    Mod+Shift+P { pin-column edge="right"; }     // ...or the right rail
    Mod+U       { unpin-column; }                // return it to the scrollable row
    Mod+Ctrl+P  { toggle-column-pinned; }        // pin if unpinned, else unpin
    Mod+Alt+P   { focus-pinned; }                // focus the first pinned column
}
```

```sh
niri msg action pin-column --edge right
niri msg action unpin-column
niri msg action focus-pinned
```

`--edge` defaults to `left`. A pinned column is clamped to ≤30% of the viewport; the third pin on
an edge is rejected (no-op + log). Fullscreening a pinned window unpins it for the duration and
re-pins on unfullscreen; closing a pinned column's last window auto-unpins and collapses its rail.

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
