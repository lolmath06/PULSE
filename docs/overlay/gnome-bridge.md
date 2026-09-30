# GNOME bridge — keeping overlays above on GNOME Wayland

**Status (Phase 12):** production. Physically verified on Fedora 39,
GNOME Shell 45.10, Mutter 45.7, native Wayland:

| What                                                                    | Status                                                         |
| ----------------------------------------------------------------------- | -------------------------------------------------------------- |
| Overlays stay above a focused application (Firefox)                     | **verified** (Phase 11.5A)                                     |
| Ctrl+Shift+F12 reaches PULSE while Firefox has focus                    | **verified** (Phase 11.5A)                                     |
| Locked overlays pass every click through (20/20 clicks)                 | **verified** (Phase 11.5B, no bridge needed)                   |
| Extension v2 `Hello`, status panel, Enable/Disable, shortcut from PULSE | implemented, tested with fakes; to confirm by hand (see below) |

The design history (proof of concept, the click-through root cause) is in
[`gnome-bridge-poc.md`](gnome-bridge-poc.md).

## Why a companion extension

Wayland lets no client keep itself above other windows, and GNOME 45 has no
global-shortcut portal. GNOME Shell _is_ the compositor: an extension running
inside it uses Mutter's own `Meta.Window.make_above()` (what the window menu's
_Always on Top_ does) and registers a key binding Mutter handles before any
application sees the key. PULSE still draws every overlay itself; the
extension only changes the stacking layer and forwards one action.

## Pieces

| Piece               | Where                                           | Role                                                                                                   |
| ------------------- | ----------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Extension (v2)      | `integrations/gnome-shell/pulse-overlay@jamby/` | `make_above` on PULSE overlays; Mutter binding `toggle-overlays`; `Hello` to PULSE                     |
| Installer           | `integrations/gnome-shell/install.sh`           | `check`, `status`, `install`/`update`, `enable`, `disable`, `uninstall` — user directory only, no sudo |
| PULSE D-Bus service | `src-tauri/src/bridge.rs`                       | `dev.pulse.app` at `/dev/pulse/app/OverlayBridge`                                                      |
| Status facts        | `src-tauri/src/gnome_bridge.rs`                 | GNOME's Extensions API, the installed `metadata.json`, the bridge's shortcut setting                   |
| Status decision     | `src-tauri/src/overlay/gnome_bridge.rs`         | pure: facts → state, summary, guidance, allowed actions                                                |
| Backend selection   | `src-tauri/src/overlay/backend.rs`              | Wayland + active bridge → _GNOME native bridge_                                                        |
| UI                  | PULSE → Overlays → _Overlay backend_            | state, versions, setup steps, Enable / Disable / Refresh, copyable commands                            |

## D-Bus contract (bridge v2)

| Name      | `dev.pulse.app`                |
| --------- | ------------------------------ |
| Object    | `/dev/pulse/app/OverlayBridge` |
| Interface | `dev.pulse.app.OverlayBridge`  |

| Method                          | Effect                                                                                                                   |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `Ping() → s`                    | `"pong"`                                                                                                                 |
| `GetOverlayBridgeVersion() → u` | `2`                                                                                                                      |
| `ToggleOverlayEditMode()`       | the shared toggle, on **visible** overlays: if any is in Edit, lock them; otherwise edit them. Hidden overlays untouched |
| `Hello(u version) → u`          | the running extension announces its version and learns the bridge's. **Refused unless the caller is `gnome-shell`**      |

`Hello`'s version is the only argument in the interface; PULSE clamps it and
uses it for display. The caller check asks the bus daemon for the sender's PID
(`GetConnectionUnixProcessID`) and reads `/proc/<pid>/comm`. No method reads a
file, runs a command or reaches another PULSE function; there are no
properties and no signals.

An extension v1 (the prototype) never says `Hello`; a PULSE older than bridge
v2 answers `UnknownMethod`, which extension v2 logs once and ignores.

## How PULSE knows the bridge's state

Gathered at launch, when PULSE → Overlays opens, on **Refresh**, and after
Enable/Disable. **Never on a timer.** A configuration change (a dragged
slider) re-reads only PULSE's cache and never reaches GNOME Shell.

| Source                                                       | Tells                                                                          |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| `org.gnome.Shell.Extensions.GetExtensionInfo(uuid)`          | known to GNOME? state (enabled, disabled, error, out of date…), loaded version |
| `ShellVersion`, `UserExtensionsEnabled`                      | supported GNOME version? user extensions switched off?                         |
| `~/.local/share/gnome-shell/extensions/<uuid>/metadata.json` | installed version (what the next login loads), installed by `install.sh`?      |
| `Hello`                                                      | the running copy reaches **this** PULSE                                        |

States shown to the user:

| State                      | Meaning                                                            | Offered                         |
| -------------------------- | ------------------------------------------------------------------ | ------------------------------- |
| _Not needed here_          | not a GNOME Wayland session (X11, Windows, KDE…)                   | —                               |
| _Not installed_            | GNOME does not know it, no files                                   | copyable install command        |
| _Installed — log in again_ | files present, not loaded yet (GNOME 45 loads extensions at login) | —                               |
| _Disabled_                 | loaded, not enabled                                                | **Enable**                      |
| _Active_ (+ _Connected_)   | enabled and running (and greeted this PULSE)                       | **Disable**                     |
| _Error_                    | GNOME Shell reported an error, shown verbatim                      | **Disable**, reinstall guidance |
| _Incompatible_             | GNOME Shell other than 45, or GNOME says out of date               | —                               |
| _Extensions off_           | `disable-user-extensions` is on for the session                    | how to turn them back on        |
| _Unavailable_              | GNOME's Extensions service did not answer                          | Refresh                         |

Also shown: GNOME Shell version, the running, installed and bundled versions.
_Update available_ (installed older than bundled) and _restart pending_ (a
newer copy on disk than the running one) each get their step.

## Capabilities it unlocks

With the bridge **active** on a Wayland session, PULSE reports:

- **Always on top — supported** (was _limited_): Mutter `make_above`.
- **Global shortcut — supported** (was _unsupported_ on GNOME 45): the
  bridge's Mutter binding; the shortcut shown is read from the extension's
  own setting.
- **Click-through — supported** on any GNOME Wayland session, bridge or not
  (Phase 11.5B: an empty input region kept as GTK's own input shape).
- Positioning and multi-monitor placement stay **unsupported**: GNOME still
  places Wayland windows. Drag an overlay into place once in Edit mode.

## The shortcut

The bridge's binding lives in the extension's GSettings key
`org.gnome.shell.extensions.pulse-overlay` → `toggle-overlays` (schema
compiled in the extension's own `schemas/`). With the bridge active,
PULSE → Overlays → _Global shortcut_ → **Apply** writes that key
(`Ctrl+Shift+F12` → `<Control><Shift>F12`); Mutter re-binds at once.
**Disable** there clears it (the extension stays enabled; stacking is
unaffected). Without the bridge, the shortcut goes through the portal or the
X11 plugin as before.

## Install, update, enable, disable, uninstall

Everything is the user's explicit action; PULSE never installs or copies
extension files itself.

```sh
cd <PULSE source>/integrations/gnome-shell
./install.sh status      # bundled / installed / running versions (writes nothing)
./install.sh install     # or: update — copies + compiles the private schema
# log out and back in once (GNOME 45 loads extension code only at login)
./install.sh enable      # or PULSE → Overlays → Enable
./install.sh disable     # or PULSE → Overlays → Disable
./install.sh uninstall   # removes the directory only if install.sh created it
```

PULSE → Overlays shows the same path as numbered steps, with the current one
highlighted and its command ready to copy. **Enable** and **Disable** call
GNOME Shell's own `EnableExtension` / `DisableExtension`.

`install.sh` touches exactly one directory,
`~/.local/share/gnome-shell/extensions/pulse-overlay@jamby/`, refuses to
overwrite or delete one it did not create (marker `.installed-by-pulse`), and
never needs sudo.

**Updating from the prototype (v1):** `./install.sh update`, then log out and
back in. Until then PULSE shows _Active_, running v1, installed v2,
_restart pending_.

## Safety

- Only windows owned by the process holding `dev.pulse.app` (PID from the bus
  daemon) **and** titled exactly `PULSE Overlay :: <id>` are made above; the
  main window, Mini and every other application are never touched.
- Event-driven only: `window-created`, `notify::title`, `notify::above`,
  `unmanaging`, the bus-name watch. A cleared "above" is re-applied at most
  three times per window. No timer, no polling, no input interception.
- `disable()` removes the keybinding, disconnects every handler first, then
  undoes `make_above` only on windows it changed.
- Every Shell callback is guarded; nothing throws into GNOME Shell.
- If GNOME Shell ever misbehaved at login, from another session or a TTY:
  `gsettings set org.gnome.shell disable-user-extensions true`.

Logs: `journalctl --user -b /usr/bin/gnome-shell | grep "PULSE overlay bridge"`
and, from PULSE, `PULSE: GNOME bridge: …` at launch and
`PULSE: GNOME bridge extension v2 connected` on `Hello`.

## Limits

- Verified on GNOME 45 only; the extension declares `shell-version: ["45"]`,
  and PULSE reports other versions _Incompatible_ rather than guessing.
- GNOME places Wayland windows: no absolute position, no monitor choice.
- Exclusive-fullscreen games are out of scope, as everywhere
  ([`backends.md`](backends.md)).
