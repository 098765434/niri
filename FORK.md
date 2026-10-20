# Fork notes — niri

This is a personal fork of [niri](https://github.com/YaLTeR/niri), the scrollable-tiling
Wayland compositor. It adds **live gesture-progress IPC events** on top of a tagged upstream
release; everything else is stock upstream.

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/YaLTeR/niri` (remote: `upstream`) |
| Base release | **`v26.04`** (matches the niri 26.04 that nixpkgs 26.05 ships) |
| Fork branch | `gesture-ipc-events` |
| Custom commits | 2, on top of `v26.04` (see below) |

```
git log --oneline v26.04..gesture-ipc-events
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

## Consumed by

Built as the system compositor via the `niri-fork` input in `~/nixos-config/flake.nix`
(`programs.niri.package`). Commit on this branch, then `nix flake update niri-fork` and rebuild.

## Updating to a new upstream release

This is a shallow clone; the full procedure is in the workspace docs:
**`../docs/runbooks/update-niri.md`**. In short:

```
git fetch --unshallow --tags upstream        # first time only; then: git fetch --tags upstream
git rebase --onto <new-tag> v26.04 gesture-ipc-events
# resolve, then build + test in the dev shell:
nix develop -c cargo build && nix develop -c cargo test
# then re-pin + rebuild the system (see the runbook)
```
