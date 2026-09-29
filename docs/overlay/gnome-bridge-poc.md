# GNOME Wayland overlay bridge — proof of concept (Phase 11.5A)

> **Status: UNPROVEN.** The code exists and its logic is unit-tested, but
> nobody has yet seen it keep an overlay above Firefox or toggle overlays with
> Ctrl+Shift+F12 on a real GNOME session. It stays unproven until Matheo runs
> the two physical tests below.

Branch `phase11.5-gnome-bridge-poc`. Target: Fedora 39, GNOME Shell 45.10,
Mutter 45.7, GJS 1.78.5, native Wayland.

## Why a normal Wayland client cannot guarantee Z-order

Wayland gives an application no way to put itself above other windows: the
`xdg_toplevel` protocol has no "above" state, and the stacking order belongs
to the compositor. Tauri's `set_always_on_top` is therefore a no-op on native
Wayland, and Phase 11 physically confirmed that a focused Firefox is drawn
over a PULSE overlay. The same session's XDG Desktop Portal (GNOME 45) has no
`GlobalShortcuts` interface, so PULSE cannot receive Ctrl+Shift+F12 while a
Wayland application has focus either.

## Why a GNOME Shell extension can

GNOME Shell _is_ the compositor (Mutter runs inside it). An extension runs in
that process and uses Mutter's own window objects: it can set the "above"
state on any window (`Meta.Window.make_above()` — the same thing the window
menu's _Always on Top_ does) and register a key binding that Mutter handles
before any application sees the key (`Main.wm.addKeybinding`).

## APIs verified on this machine

Read from the installed typelib (`/usr/lib64/mutter-13/Meta-13.typelib`) and
the Shell's own JavaScript (resources in `libshell-13.so`), not from examples:

| API                                                                                              | Present          |
| ------------------------------------------------------------------------------------------------ | ---------------- |
| `Meta.Window.make_above()`, `unmake_above()`, `is_above()`, `raise()`, `stick()`                 | yes, 0 args each |
| `Meta.Window.get_pid()`, `get_title()`, `get_window_type()`, `is_override_redirect()`            | yes              |
| `Meta.Window.get_wm_class()`, `get_gtk_application_id()`, `get_role()`                           | yes (not used)   |
| `Meta.Window` signals `unmanaging`; property `above` (so `notify::above`), `title`               | yes              |
| `Meta.Display` signal `window-created`                                                           | yes              |
| `Meta.KeyBindingFlags.IGNORE_AUTOREPEAT`                                                         | yes (16)         |
| `Main.wm.addKeybinding(name, settings, flags, modes, handler)` → `global.display.add_keybinding` | yes              |
| `Main.wm.removeKeybinding(name)`                                                                 | yes              |
| `Extension.getSettings()` reading `metadata['settings-schema']` from `schemas/`                  | yes              |
| ES-module extensions (`export default class … extends Extension`)                                | yes (45)         |

`raise()` and `stick()` are **not** used: `make_above` is the whole Z-order
mechanism, and workspace behaviour is left unchanged.

## How PULSE's overlay windows are identified

What Mutter can see of a PULSE window: title, PID, window type,
override-redirect, and a WM class / app id derived by GTK (PULSE's
`GApplication` is anonymous, so there is no stable application id to rely
on). Two facts are therefore combined, and **both** must hold:

1. **PID** — the window belongs to the process that owns the D-Bus name
   `dev.pulse.app`. The extension asks the **bus daemon**
   (`org.freedesktop.DBus.GetConnectionUnixProcessID`), not PULSE, and only
   while that name has an owner. Another application's windows have another
   PID, whatever their title.
2. **Title** — exactly `PULSE Overlay :: <overlay-id>`
   (`^PULSE Overlay :: [a-z0-9][a-z0-9-]{0,39}$`). PULSE now titles its
   overlay windows this way (`overlay::spec::window_title`), with the
   validated id, never the user-typed name. The main window is `PULSE` and
   Mini is `PULSE Mini`: neither can match.

Also required: a normal-type window, not override-redirect. Windows of other
processes are never even connected to: the extension only attaches handlers
to windows whose PID is PULSE's.

## How "above" is applied — event flow, no polling

1. `enable()` registers the key binding, connects `window-created`, and
   watches the bus name `dev.pulse.app`.
2. When PULSE appears on the bus: its PID is fetched once, and every existing
   window of that PID is considered once.
3. For each PULSE window: connect `notify::title`, `notify::above`,
   `unmanaging`; if it is an overlay and not above → `make_above()`.
4. A new window (`window-created`) of that PID gets the same treatment — its
   title may arrive later, hence `notify::title`.
5. If something clears "above" (`notify::above`), it is re-applied **at most
   three times** per window, then left alone with one log line: no fight with
   the compositor.
6. A window whose title stops matching is given back (`unmake_above`).
7. When PULSE leaves the bus, all handlers are disconnected.
8. `disable()` removes the key binding, disconnects every handler first, then
   undoes `make_above` only on windows the extension itself changed.

There is no timer, no loop, no `raise()`, no polling of any kind.

## Click-through

Unchanged. Click-through is PULSE's own empty input region on the locked
overlay; the extension changes only the stacking layer, adds no Shell actor,
and never intercepts input. Expected: a click on a locked overlay reaches
Firefox, Firefox gets focus, PULSE does not — and the overlay stays drawn above.

## The global shortcut

- Registered by the extension with `Main.wm.addKeybinding('toggle-overlays',
settings, IGNORE_AUTOREPEAT, NORMAL | OVERVIEW, handler)`.
- The accelerator lives in the extension's **private** schema
  `org.gnome.shell.extensions.pulse-overlay`, key `toggle-overlays`, type
  `as`, default `['<Control><Shift>F12']` (the format of GNOME's own schemas;
  no installed system schema uses that combination). It is compiled only
  inside the extension's own directory.
- On press, the extension calls PULSE on the session bus with
  `NO_AUTO_START` and a 2-second timeout, asynchronously; it never waits.
- If PULSE is not running: nothing is called, nothing is retried, one log
  line at most. The extension never starts PULSE.
- The Tauri X11 shortcut plugin is not involved on native Wayland.

## D-Bus interface (PULSE side)

`src-tauri/src/bridge.rs`, started at launch on Linux, owned only while PULSE
runs (the name is released on quit).

| Name      | `dev.pulse.app`                |
| --------- | ------------------------------ |
| Object    | `/dev/pulse/app/OverlayBridge` |
| Interface | `dev.pulse.app.OverlayBridge`  |

| Method                          | Effect                                                                                |
| ------------------------------- | ------------------------------------------------------------------------------------- |
| `Ping() → s`                    | `"pong"`                                                                              |
| `GetOverlayBridgeVersion() → u` | `1`                                                                                   |
| `ToggleOverlayEditMode()`       | the existing Phase 11 toggle: if any overlay is in Edit, lock all; otherwise edit all |

No method takes an argument; there are no properties and no signals (a test
asserts it from the introspection data). The overlay state lives in PULSE;
the extension keeps none.

## Security boundaries

- Anything on the user's session bus can call the three methods — the same
  trust level as the user's own processes. The worst it can do is toggle
  Edit/Locked.
- The extension manipulates only windows whose PID is the name owner's and
  whose title is the exact marker. If some other program of the same user
  took the name `dev.pulse.app` while PULSE is not running, only **its own**
  windows with that exact title could be made above — nothing else.
- No file access, no subprocess, no network in the extension; everything in a
  Shell callback is wrapped so nothing throws into GNOME Shell.

## Install, enable, disable, uninstall

GNOME Shell 45 discovers new extensions **only at login** (its extension
manager scans the directories at startup; `ReloadExtension` is deprecated).
So installing requires **one log out / log in**. Nothing was installed by the
automation.

```sh
cd ~/Documents/PULSE/integrations/gnome-shell
./install.sh check      # validates, writes nothing
./install.sh install    # → ~/.local/share/gnome-shell/extensions/pulse-overlay@jamby/
# log out and back in once
./install.sh enable
./install.sh disable
./install.sh uninstall  # removes the directory only if install.sh created it
```

The script refuses to overwrite or delete a directory it did not create
(marker file `.installed-by-pulse`). No sudo.

**Rollback:** `./install.sh uninstall`, then `git switch phase11-dashboard-overlay`.
If GNOME Shell ever misbehaved at login, extensions can be turned off from a
TTY or another session with `gsettings set org.gnome.shell disable-user-extensions true`.

Logs: `journalctl --user -b /usr/bin/gnome-shell | grep "PULSE overlay bridge"`.

## What was verified automatically (not the proof)

- Local API surface (table above), the schema (`glib-compile-schemas
--strict --dry-run`), `metadata.json`, and both JavaScript files parse as ES
  modules.
- The extension's decisions, with a fake Shell (`integrations/gnome-shell/tests/`):
  matching, duplicate enable, disable disconnecting everything, bounded
  re-apply, PULSE absent, errors never thrown. Writing that test found and
  fixed a real bug: `disable()` used to re-apply "above" through its own
  `notify::above` handler.
- PULSE side, one release instance on native Wayland with one overlay:
  `dev.pulse.app` owned at launch; `Ping` → `"pong"`; version `1`;
  introspection shows exactly the three methods; the bus daemon's PID for the
  name owner equals the PULSE process; `ToggleOverlayEditMode` twice →
  Locked → Edit → Locked in the stored configuration; quitting from the tray
  released the name.
- **Not verified:** the extension inside GNOME Shell. It was not installed or
  enabled.

## Physical proof (by Matheo)

**Test 1 — always on top**

1. Start PULSE normally (native Wayland) with the extension enabled.
2. Show **one** overlay and lock it.
3. Put Firefox underneath part of the overlay.
4. Click Firefox through the locked overlay.

PASS only if **all**: Firefox receives the click; Firefox becomes focused;
PULSE does not take focus; **the overlay stays visually above Firefox**.
Then focus another native Wayland application: the overlay must stay above
it too.

**Test 2 — global shortcut**

1. One visible overlay, locked. Focus Firefox; PULSE is not focused.
2. Press Ctrl+Shift+F12 → the overlay goes Locked → Edit.
3. Press again → Edit → Locked.
4. Hide the PULSE main window (Keep running mode) and repeat with Firefox
   focused: it must still toggle.

If Test 1 fails, stop: no `raise()` loops and no further experiments. The
proposed next step would be a separate, approved experiment — the HUD drawn
as GNOME Shell `St` actors instead of an application window
([`native-bridge-roadmap.md`](native-bridge-roadmap.md)).

## Known prototype limits

- PULSE → Overlays still reports always-on-top _limited_ and the global
  shortcut _unsupported_ on this session: PULSE does not yet know whether the
  extension is installed. Wiring that in is production work, after the proof.
- The overlay window title changed from `PULSE — <name>` to
  `PULSE Overlay :: <id>`; it is visible only in window lists.
