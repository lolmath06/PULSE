# Windows native overlay backend

> **Status: implemented, CI-verified and physically verified for the core
> Phase 13B Windows behaviours.** Physical testing confirmed a visible topmost
> locked overlay, click-through input, Ctrl+Shift+F12 Edit/Locked switching and
> the expected Win32 managed-style logs. DPI matrices, multi-monitor placement
> and borderless-game behaviour were not separately exercised.

## What it does

An overlay on Windows is PULSE's own top-level window (Tauri/WebView2),
transparent and undecorated, with these extended styles:

| State      | Styles                                                                                     | Effect                                                                       |
| ---------- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------- |
| always     | `WS_EX_TOOLWINDOW`, never `WS_EX_APPWINDOW`                                                | no taskbar button, not in Alt+Tab                                            |
| keep above | `WS_EX_TOPMOST` + `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE \| SWP_NOMOVE \| SWP_NOSIZE)` | in the topmost band, re-asserted on every Edit/Lock change, never activating |
| **Locked** | `WS_EX_LAYERED \| WS_EX_TRANSPARENT` + `WS_EX_NOACTIVATE`                                  | hit-testing skips the window: clicks reach what is below; never focused      |
| **Edit**   | none of the three                                                                          | drag, resize, _Lock_, _Open PULSE_ work; may take focus while editing        |

The same Edit/Locked contract as every backend (`overlay::backend::window_policy`):
Edit interactive and focusable; Locked click-through and never focused; both
ask to stay above.

## How — and why a subclass

Code: `src-tauri/src/platform/windows/overlay_window.rs` (Win32, tauri-free)
and the glue in `src-tauri/src/overlay_native.rs`.

1. PULSE drives tao's **own** window flags: `set_ignore_cursor_events`
   (tao adds `WS_EX_TRANSPARENT | WS_EX_LAYERED`), `set_focusable`
   (`WS_EX_NOACTIVATE`), `set_always_on_top` (`WS_EX_TOPMOST` +
   `HWND_TOPMOST`).
2. tao rewrites the **whole** `GWL_EXSTYLE` from its flags whenever one
   changes (`WindowFlags::apply_diff` → `SetWindowLongW`). A bit set directly —
   `WS_EX_TOOLWINDOW` — would be dropped on the next change: the Windows twin
   of the GTK input-shape bug fixed in Phase 11.5B. So the overlay window is
   **subclassed** (`SetWindowSubclass`): on every `WM_STYLECHANGING` for
   `GWL_EXSTYLE`, whoever sends it, the managed bits are forced to the overlay's
   state (`enforce`), leaving all other bits (`WS_EX_NOREDIRECTIONBITMAP`, …)
   to tao. The state lives in the subclass's reference data; the subclass
   removes itself on `WM_NCDESTROY`.
3. The styles are then applied immediately (`SetWindowLongPtrW` if needed),
   the topmost band re-asserted without activation, and `GWL_EXSTYLE` **read
   back**. A mismatch or failure is logged once per transition
   (`PULSE: overlay '…' Win32 styles …`); `PULSE_OVERLAY_INPUT_DEBUG=1` logs
   every applied state by name (`TOPMOST | LAYERED | TRANSPARENT | …`).
4. All of it runs on the UI thread (a subclass must be installed from the
   window's own thread), queued after tao's flag changes.

`WS_EX_LAYERED` is set without `SetLayeredWindowAttributes`, exactly as tao
does for its own click-through: tao's transparent windows carry
`WS_EX_NOREDIRECTIONBITMAP` and WebView2 draws through DirectComposition, so
there is no redirection bitmap for layered attributes to govern. **This is
the first thing to confirm on real hardware** (the overlay must stay visible
when locked).

## When it is applied

On creation, on every reconciliation (Edit/Lock change, any overlay
configuration change) — idempotent — and never on a timer. The input mode
follows the shared orchestration in `overlay::input` (applied on change,
creation and re-map; never on focus changes).

## Tests

`platform::windows::overlay_window::tests` (run on Fedora):

- Locked requires `TOPMOST | LAYERED | TRANSPARENT | NOACTIVATE | TOOLWINDOW`
  and never `APPWINDOW`;
- Edit drops `TRANSPARENT`, `LAYERED` and `NOACTIVATE`, keeps `TOPMOST` and
  `TOOLWINDOW`;
- enforcing keeps every bit that is not PULSE's;
- Lock → Edit → Lock stays exact whatever tao rewrites in between, and
  enforcing is idempotent;
- the state survives the subclass reference data.

## Limits

- **Exclusive-fullscreen** games own the display; a desktop window is not
  drawn over them. Borderless/windowed games are the target.
- Other topmost windows share the band; the last one raised wins until the
  next Edit/Lock change re-asserts PULSE's position. There is no polling to
  fight them.
- No injection of any kind: no DLL, no DirectX/Vulkan/OpenGL hook, nothing
  anti-cheat software could treat as tampering.
- Future, not started: an Xbox Game Bar widget as an optional gaming backend
  ([`native-bridge-roadmap.md`](native-bridge-roadmap.md)).

## Phase 13B physical Windows result

Physically verified:

1. a Tiny Stats native overlay rendered correctly;
2. Locked remained visible and topmost over an ordinary focused application;
3. Locked was click-through;
4. Ctrl+Shift+F12 switched Edit/Locked with another application focused;
5. debug logs showed
   `TOPMOST | LAYERED | TRANSPARENT | NOACTIVATE | TOOLWINDOW` when locked and
   `TOPMOST | TOOLWINDOW` in Edit.

Not separately exercised during the recorded validation:

- the complete taskbar / Alt+Tab matrix;
- a borderless-fullscreen game;
- the DPI scaling matrix;
- multi-monitor persistence;
- the former exhaustive manual protocol as a whole.

The authoritative closure record is
[`../release/windows-physical-validation.md`](../release/windows-physical-validation.md).
