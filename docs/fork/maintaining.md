# Maintaining the fork

How the fork is kept on top of upstream niri: provenance, branches, the doc rule, the rebase
workflow and every fork commit. The features themselves are in the [index](README.md).

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/YaLTeR/niri` (remote: `upstream`) |
| Fork | `https://github.com/098765434/niri` (remote: `origin`) |
| Base release | **`v26.04`** (matches the niri 26.04 that nixpkgs 26.05 ships) |
| Fork branch | `ocbk` |
| The delta | `git log --oneline v26.04..ocbk` |

## Branches

| Branch | State |
|---|---|
| `ocbk` | the fork; everything below is on it |
| `pinned-columns` | unmerged work, pushed (`99562604`, based before the docs moved); see [below](#pinned-columns) |
| `mod-swipe-gestures` | local only, merged into `ocbk` (ends at `6d9306a7`) |
| `origin/gesture-ipc-events` | the branch's old name, before `8fd4e23e` renamed it to `ocbk`; stale (ends at `c5b8c69d`) |

The other `origin/*` branches are upstream's, mirrored by the GitHub fork.

### Pinned columns

A column docked to a screen edge, with its own rail, outside the scroll math; actions
`pin-column`, `unpin-column`, `toggle-column-pinned`, `focus-pinned`. The compositor core is done
with tests; the overview drop target, shell UI and cross-workspace pins are deferred.

The branch predates `docs/fork/`. When it is rebased onto `ocbk`:

- its root `niri-pinned-columns-spec.md` becomes `docs/fork/layout/pinned-columns.md`, with
  Usage first like the other feature docs, and a row in the [index](README.md);
- its `FORK.md` has the old layout (a long feature list) and will conflict: keep `ocbk`'s stub
  and move the pinned-columns notes into the new doc.

## Docs rule

Upstream files stay byte-identical to `v26.04`. All fork documentation lives in `docs/fork/`;
the only upstream-adjacent changes are the one-line banner at the top of `README.md` and the
`FORK.md` stub. So this prints nothing:

```sh
git diff v26.04 HEAD -- docs/wiki
```

and a rebase only ever conflicts on `FORK.md` and the README banner line, never on a wiki page.
New user-facing docs for a fork feature go in that feature's doc under `docs/fork/`, as its
Usage section, not into `docs/wiki/`.

Specs that are implemented go in `docs/fork/design/` as history, with a
`Status: implemented in <hash>` line.

## Reviewing the fork

```sh
git log --oneline --stat v26.04..ocbk     # every fork commit and the files it touched
git diff --stat v26.04..ocbk              # the whole delta, by file
git diff v26.04..ocbk -- <paths>          # one feature (paths in each feature doc)
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

Then replace `v26.04` with the new tag here, in [the index](README.md) and in `FORK.md`, and
re-pin the consuming config (`nix flake update niri-fork`).

## Upstreaming

| Feature | Upstreamable? |
|---|---|
| IPC events | maybe: generic IPC, but per-frame events are chatty |
| IPC actions | likely: small, optional, and matches `MoveWindowToWorkspace`'s `window_id` |
| Mod swipe | maybe: behind a config switch |
| Blur shapes, materials | no as-is: private protocol; GPL-3.0 shader lineage (kwin-effects-glass, Niri-glass) |
| Vibrancy | no as-is: private protocol |
| Gaps | maybe: plain config, stock behaviour unchanged when unset |
| Workspace mat | maybe: plain config, no protocol |
| Desktop layer | maybe: plain config plus one IPC event |

## Commit log

Every commit in `v26.04..ocbk`, oldest first, by feature. Later docs-only commits show in
`git log --oneline v26.04..ocbk -- docs FORK.md README.md`.

| Commit | Feature | What |
|---|---|---|
| `59fe93e4` | [IPC events](ipc/events.md) | live gesture-progress IPC events |
| `9d5bf9fc` | [IPC events](ipc/events.md) | fix the duplicate `ScrollingSpace::active_column_idx` the IPC commit added, and un-gate the `#[cfg(test)]` one |
| `9366e512` | housekeeping | add `FORK.md`; stop tracking `.claude/` |
| `c5b8c69d` | housekeeping | README fork banner; rebase/range-diff workflow |
| `8fd4e23e` | housekeeping | rename the work branch to `ocbk` |
| `b414445c` | housekeeping | `FORK.md` update |
| `f4367f88` | [mod swipe](gestures/mod-swipe.md) | Mod-held three-finger window swipes |
| `6d9306a7` | [mod swipe](gestures/mod-swipe.md) | `FORK.md`: document it |
| `6baa66e5` | [mod swipe](gestures/mod-swipe.md) | design doc |
| `4177c74f` | [mod swipe](gestures/mod-swipe.md) | step into full width past the widest preset |
| `13d945b5` | [blur shapes](effects/blur-shapes.md) | `niri-blur-shapes-v1`: rounded, fading blur shapes |
| `454d9751` | [materials](effects/materials.md) | liquid-glass materials for blur shapes |
| `74b623a7` | [blur shapes](effects/blur-shapes.md) | magnify inwards at the lip; one element id per blur shape |
| `76262f47` | housekeeping | feature table in `FORK.md`, design docs in `docs/fork` |
| `36f2cce3` | [IPC actions](ipc/actions.md) | `MoveColumnToIndex` takes an optional `window_id` |
| `3cde7327` | [materials](effects/materials.md) | materials on rules, materializing lenses, backdrop readings |
| `c821108e` | housekeeping | `FORK.md`: rule materials, materialize and backdrop readings |
| `4c68634c` | [vibrancy](effects/vibrancy.md) | `niri-vibrancy-v1`: blend a surface with what's behind it |
| `ada326f0` | [vibrancy](effects/vibrancy.md) | plus-darker subtracts, as Apple's does |
| `a1a78bd0` | [materials](effects/materials.md) | a blur of their own (`blur-passes`, `blur-offset`) |
| `e15dec22` | housekeeping | docs: blur shapes with a material's own blur |
| `148ff076` | [workspace mat](layout/workspace-mat.md) | draft spec |
| `78620a8c` | [materials](effects/materials.md) | `tone`, Apple's luminosity adjustment |
| `7bef7001` | [materials](effects/materials.md) | outward lens with a capture reach, and `glow` apart from the rim |
| `58528c07` | [workspace mat](layout/workspace-mat.md) | a workspace mat behind the scrolling layout |
| `cc098153` | [workspace mat](layout/workspace-mat.md) | docs: the mat in the wiki and `FORK.md` |
| `f297cd5e` | [workspace mat](layout/workspace-mat.md) | soft ends fade out inside the end columns, and for glass too |
| `74d6c36d` | [workspace mat](layout/workspace-mat.md) | `behind-bars` |
| `dab83f82` | [gaps](layout/gaps.md) | `workspace-gap`, overview `workspace-gap` and `window-gap` |
| `34c91b6b` | [gaps](layout/gaps.md) | measure outside the bars' exclusive zones, allow negatives |
| `e0139bac` | [gaps](layout/gaps.md) | step by the rounded working-area edges |
| `871b5ee5` | [desktop layer](layout/desktop-layer.md) | `desktop` layer rule and the `DesktopObscured` event |
| `cff5af59` and the six after it | housekeeping | docs reorganization: `docs/fork/` feature folders, wiki pages reset to `v26.04`, `FORK.md` stub |
