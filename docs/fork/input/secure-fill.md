# Secure fill

A password manager puts a secret into the focused text field only if that field is still the one
it checked, and learns who owns the field from the compositor, not from the app.

## Usage

Protocol `niri-secure-fill-v1` (`resources/niri-secure-fill-v1.xml`). Only unrestricted clients
(not under a security context) see the global. A sandboxed app can't bind it.

1. `niri_secure_fill_manager_v1.get_target(id)` takes a snapshot of the text input that has the
   keyboard. The new `niri_secure_fill_target_v1` gets one `identity` event at once, with:
   - `pid`: the owner's process, from the client socket's credentials.
   - `exe`: what `/proc/<pid>/exe` points at, read by niri.
   - `app_id`: the window's app id. The app chose it, so it's only a hint.
   - `window_id`: the IPC window id, as two 32-bit halves.
   - `flags`: `window`, `layer`, `sandboxed`, `text_input` (an enabled text input) and
     `unknown_credentials`.
2. `fill(text)` puts the text in, if:
   - the same text input of the same surface still has the keyboard;
   - the session isn't locked;
   - the target is under 2 minutes old and hasn't been used;
   - the last fill from anyone was at least 300 ms ago;
   - the text is not empty, at most 4096 bytes, and has no control characters (a line break
     could submit a form).

   `filled` or `refused(reason)` answers, and a target answers only once.

The client should hold the enabled `zwp_input_method_v2` too. Text-input state reaches the
compositor only while an input method is bound, so without it no text input is ever active, and
every fill is refused with `no_text_input`.

## Design / internals

**Why commit_string isn't enough.** An input method's `commit_string` goes to whichever text input
is focused when the request arrives. smithay's serial check only decides whether `done` tells the
client to drop the state, and by then the text is already on that client's socket. So a secret
committed just after a focus change reaches the new app.

**The check.** `fill` runs in one dispatch, with nothing else in between. It takes a fresh
snapshot (`SecureFillHandler::secure_fill_snapshot` in `src/handlers/mod.rs`) and compares it
with the target's `TargetKey`: the text input's surface (weak) and the enabled text input's
object id.

The snapshot needs all of these:
- the keyboard focus is a window or a layer surface (never the lock screen or niri's own UI);
- smithay's text-input focus is that surface, or one of its popups or subsurfaces
  (`find_root_shell_surface`).

Only then does it commit the text to the enabled text input and send `done`.

**Identity.** The pid comes from `get_credentials_for_surface` (SO_PEERCRED, taken when the
client connected). niri reads `exe` at snapshot time, while the client is connected and its
process is alive. The result is `unknown_credentials` if xwayland-satellite or a proxy hides
the real process.

A process in another mount or user namespace than niri's, or with a root other than `/`, is
reported `sandboxed`, with no `exe`: inside a namespace, a path can be made to point anywhere.
The pid is the one that connected. If it handed its socket to a child and exited, a recycled pid
could stand for another program, which a pidfd from SO_PEERPIDFD at connect time would close.

**Limits.**
- Two fields in the same text input of the same surface look the same: GTK and Chromium reuse
  one text input for every field of a window, and smithay keeps its object id across
  re-enables. If the app moves the keyboard to another of its own fields between the client's
  check and the fill, that field gets the text. Closing this needs a per-enable serial from
  smithay in `TargetKey`.
- Any unrestricted client can bind the global, not only the seat's input method. It learns
  nothing a niri IPC client can't, and can only put in text it already has.
- The text passes through niri's memory. It's wiped after the request, but other copies may
  stay in smithay's and the client's buffers.

Code: `src/protocols/secure_fill.rs`, with the handler in `src/handlers/mod.rs`, registered in
`src/niri.rs`.
