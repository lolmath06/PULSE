// PULSE overlay bridge — GNOME Shell 45 glue.
//
// Physically verified on Fedora 39 / GNOME 45. See
// docs/overlay/gnome-bridge.md.
//
// Two jobs, both compositor-side:
//  1. keep PULSE's overlay windows above other windows (Meta.Window.make_above),
//     event-driven, no polling;
//  2. register Ctrl+Shift+F12 with Mutter (Main.wm.addKeybinding) and forward
//     it to PULSE over the session bus (dev.pulse.app, ToggleOverlayEditMode).
// Every decision lives in lib.js; this file only connects it to the Shell.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

import {
  BUS_NAME,
  HELLO_METHOD,
  INTERFACE,
  KEYBINDING,
  OBJECT_PATH,
  PulseOverlayBridge,
  TOGGLE_METHOD,
} from './lib.js';

const DBUS_TIMEOUT_MS = 2000;

export default class PulseOverlayExtension extends Extension {
  enable() {
    if (this._bridge?.enabled) return;
    const settings = this.getSettings();
    // Invalidates D-Bus callbacks that complete after a disable.
    const generation = (this._generation = (this._generation ?? 0) + 1);
    const current = () => this._generation === generation;

    const shell = {
      normalType: Meta.WindowType.NORMAL,

      addKeybinding: (handler) =>
        Main.wm.addKeybinding(
          KEYBINDING,
          settings,
          Meta.KeyBindingFlags.IGNORE_AUTOREPEAT,
          Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
          handler,
        ),
      removeKeybinding: () => Main.wm.removeKeybinding(KEYBINDING),

      watchPulse: (onAppeared, onVanished) =>
        Gio.bus_watch_name(
          Gio.BusType.SESSION,
          BUS_NAME,
          Gio.BusNameWatcherFlags.NONE,
          (connection, _name, owner) => {
            // The PID comes from the bus daemon, not from PULSE.
            connection.call(
              'org.freedesktop.DBus',
              '/org/freedesktop/DBus',
              'org.freedesktop.DBus',
              'GetConnectionUnixProcessID',
              new GLib.Variant('(s)', [owner]),
              new GLib.VariantType('(u)'),
              Gio.DBusCallFlags.NONE,
              DBUS_TIMEOUT_MS,
              null,
              (conn, result) => {
                if (!current()) return;
                try {
                  const [pid] = conn.call_finish(result).deepUnpack();
                  onAppeared(pid);
                } catch (error) {
                  console.warn(`PULSE overlay bridge: cannot identify PULSE: ${error}`);
                }
              },
            );
          },
          () => {
            if (current()) onVanished();
          },
        ),
      unwatchPulse: (id) => {
        if (id) Gio.bus_unwatch_name(id);
      },

      onWindowCreated: (callback) =>
        global.display.connect('window-created', (_display, window) => callback(window)),
      disconnectDisplay: (id) => {
        if (id) global.display.disconnect(id);
      },
      listWindows: () => global.get_window_actors().map((actor) => actor.meta_window),

      connectWindow: (window, signal, callback) => window.connect(signal, callback),
      disconnectWindow: (window, id) => window.disconnect(id),

      sendToggle: (onError) =>
        Gio.DBus.session.call(
          BUS_NAME,
          OBJECT_PATH,
          INTERFACE,
          TOGGLE_METHOD,
          null,
          null,
          // Never start PULSE from the extension.
          Gio.DBusCallFlags.NO_AUTO_START,
          DBUS_TIMEOUT_MS,
          null,
          (connection, result) => {
            try {
              connection.call_finish(result);
            } catch (error) {
              onError(error);
            }
          },
        ),

      sendHello: (version, onError) =>
        Gio.DBus.session.call(
          BUS_NAME,
          OBJECT_PATH,
          INTERFACE,
          HELLO_METHOD,
          new GLib.Variant('(u)', [version]),
          new GLib.VariantType('(u)'),
          Gio.DBusCallFlags.NO_AUTO_START,
          DBUS_TIMEOUT_MS,
          null,
          (connection, result) => {
            try {
              connection.call_finish(result);
            } catch (error) {
              // A PULSE older than bridge v2 has no Hello: not an error worth more than one line.
              onError(error);
            }
          },
        ),

      log: (message) => console.log(message),
    };

    this._bridge = new PulseOverlayBridge(shell);
    this._bridge.enable();
  }

  disable() {
    this._generation = (this._generation ?? 0) + 1;
    this._bridge?.disable();
    this._bridge = null;
  }
}
