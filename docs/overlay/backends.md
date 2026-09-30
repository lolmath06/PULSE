# Overlay backends

An overlay is an ordinary PULSE window (Tauri/WebKitGTK or WebView2) drawn by
the same widget engine as the dashboard. What differs per platform is **how
it is kept above, made click-through and kept from taking focus**. Each
session uses exactly one backend (`src-tauri/src/overlay/backend.rs`):

| Backend                     | Chosen when                                         | Above other windows         | Click-through (Locked)               | Shortcut                         | Status                                  |
| --------------------------- | --------------------------------------------------- | --------------------------- | ------------------------------------ | -------------------------------- | --------------------------------------- |
| **Windows native**          | Windows                                             | `HWND_TOPMOST`, re-asserted | `WS_EX_LAYERED \| WS_EX_TRANSPARENT` | `RegisterHotKey`                 | implemented, compiled, **not verified** |
| **GNOME native bridge**     | Wayland + the PULSE extension active in GNOME Shell | Mutter `make_above`         | empty GTK input shape                | Mutter binding via the extension | **physically verified** (GNOME 45)      |
| **Standard Wayland window** | any other Wayland session                           | the compositor decides      | empty GTK input shape                | `GlobalShortcuts` portal, if any | best effort                             |
| **X11 window**              | X11, XWayland                                       | `_NET_WM_STATE_ABOVE`       | empty X Shape input region           | `XGrabKey` (XWayland: limited)   | best effort (measured on XWayland)      |
| **Unavailable**             | other platforms                                     | —                           | —                                    | —                                | —                                       |

PULSE → Overlays → _Overlay backend_ names the backend in force, its
verification level and its headline capabilities; each capability's full
reason is one hover away and listed under _Every capability, in detail_.

## One contract

`window_policy(locked)` is the only thing the rest of PULSE asks for:

|             | Edit                | Locked             |
| ----------- | ------------------- | ------------------ |
| Pointer     | interactive         | click-through      |
| Focus       | may take focus      | never              |
| Stacking    | asks to stay above  | asks to stay above |
| Chrome (UI) | bar, grip, controls | only the widgets   |

The mechanisms live in `src-tauri/src/overlay_native.rs` (window side: GTK
input shape on Linux, Win32 styles on Windows) and in the GNOME Shell
extension (compositor side). Nothing else in PULSE knows which one runs.

The input mode is orchestrated by `overlay::input` (applied on creation, on
a lock change and on re-map; never on focus changes or keep-above
re-assertions); focus and stacking are re-applied on every reconciliation,
which is idempotent. No backend polls.

## Capability matrix

| Capability              | Windows native         | GNOME bridge                  | Standard Wayland           | X11 / XWayland                             |
| ----------------------- | ---------------------- | ----------------------------- | -------------------------- | ------------------------------------------ |
| Above other windows     | supported (unverified) | **supported (verified)**      | limited                    | supported                                  |
| Global shortcut         | supported (unverified) | **supported (verified)**      | portal, if offered         | X11 supported; XWayland limited            |
| Click-through           | supported (unverified) | **supported (verified)**      | limited (GNOME: supported) | supported                                  |
| Transparent             | supported (unverified) | supported                     | supported                  | XWayland supported; X11 needs a compositor |
| Absolute positioning    | supported (unverified) | unsupported (GNOME places it) | unsupported                | supported                                  |
| Multi-monitor placement | supported (unverified) | unsupported                   | unsupported                | supported                                  |

Presets that depend on placement (top/bottom bars, rails) carry a hint where
the session cannot place windows — they are never blocked: drag the overlay
into place once in Edit mode.

## Not in scope

Exclusive-fullscreen games, DirectX/Vulkan/OpenGL hooks, DLL injection. See
[`windows-native.md`](windows-native.md) for the optional Xbox Game Bar idea.

Details: [`gnome-bridge.md`](gnome-bridge.md) ·
[`windows-native.md`](windows-native.md) ·
[`platform-capabilities.md`](platform-capabilities.md) (Phase 11 measurements).
