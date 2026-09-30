// PULSE overlay bridge — the decisions, without GNOME Shell.
//
// Physically verified on Fedora 39 / GNOME 45 (Wayland): overlays stay above
// a focused application, and the shortcut reaches PULSE whichever window has
// focus. See docs/overlay/gnome-bridge.md.
//
// Everything that can go wrong inside GNOME Shell is decided here, in plain
// JavaScript with no `gi://` or `resource://` import, so it is unit-tested
// outside the Shell. `extension.js` only builds the adapter that connects it
// to Mutter, Main.wm and D-Bus.

export const BUS_NAME = 'dev.pulse.app';
export const OBJECT_PATH = '/dev/pulse/app/OverlayBridge';
export const INTERFACE = 'dev.pulse.app.OverlayBridge';
export const TOGGLE_METHOD = 'ToggleOverlayEditMode';
export const HELLO_METHOD = 'Hello';
/** This extension's version; keep equal to metadata.json → version. */
export const COMPANION_VERSION = 2;
export const KEYBINDING = 'toggle-overlays';

/**
 * Exactly the titles PULSE gives its overlay windows
 * (`src-tauri/src/overlay/spec.rs`, `window_title`): the marker, then a
 * validated overlay id. Main ("PULSE") and Mini ("PULSE Mini") never match.
 */
export const OVERLAY_TITLE = /^PULSE Overlay :: [a-z0-9][a-z0-9-]{0,39}$/;

/** How many times a cleared "above" state is re-applied to one window. */
export const MAX_REASSERTS = 3;

/**
 * Whether a window is one of PULSE's overlays. All must hold:
 * - PULSE is on the bus and its PID is known (from the bus daemon);
 * - the window belongs to that very process;
 * - it is an ordinary (normal-type), managed window;
 * - its title is exactly an overlay marker.
 * Any error reading a window means "no".
 */
export function isPulseOverlay(facts, pulsePid) {
  if (!facts || typeof pulsePid !== 'number' || pulsePid <= 0) return false;
  return (
    facts.pid === pulsePid &&
    facts.normal === true &&
    facts.overrideRedirect !== true &&
    typeof facts.title === 'string' &&
    OVERLAY_TITLE.test(facts.title)
  );
}

/** Reads the facts the predicate needs; never throws. */
export function readFacts(window, normalType) {
  try {
    return {
      pid: window.get_pid(),
      title: window.get_title(),
      normal: window.get_window_type() === normalType,
      overrideRedirect: window.is_override_redirect(),
    };
  } catch {
    return null;
  }
}

/**
 * The bridge's lifecycle and decisions. `shell` is the adapter:
 *
 *   addKeybinding(handler) / removeKeybinding()
 *   watchPulse(onAppeared(pid), onVanished()) -> handle / unwatchPulse(handle)
 *   onWindowCreated(cb) -> id / disconnectDisplay(id)
 *   listWindows() -> Meta.Window[]
 *   connectWindow(window, signal, cb) -> id / disconnectWindow(window, id)
 *   sendToggle(onError)
 *   sendHello(version, onError)   — announce ourselves to PULSE
 *   normalType
 *   log(message)
 */
export class PulseOverlayBridge {
  constructor(shell) {
    this._shell = shell;
    this._enabled = false;
    this._pulsePid = null;
    this._tracked = new Map(); // window -> { ids, reasserts }
    this._madeAbove = new Set();
    this._logged = new Set();
  }

  get enabled() {
    return this._enabled;
  }

  enable() {
    if (this._enabled) return; // never a second keybinding or handler
    this._enabled = true;
    this._shell.addKeybinding(() => this.toggle());
    this._createdId = this._shell.onWindowCreated((window) =>
      this._guard(() => this._consider(window)),
    );
    this._watch = this._shell.watchPulse(
      (pid) => this._guard(() => this._pulseAppeared(pid)),
      () => this._guard(() => this._pulseVanished()),
    );
  }

  disable() {
    if (!this._enabled) return;
    this._enabled = false;
    this._guard(() => this._shell.removeKeybinding());
    this._guard(() => this._shell.disconnectDisplay(this._createdId));
    this._guard(() => this._shell.unwatchPulse(this._watch));
    // Disconnect first, so undoing below cannot trigger a re-apply.
    const changed = [...this._madeAbove];
    this._untrackAll();
    // Undo only what this extension did, on windows it positively identified.
    for (const window of changed) {
      this._guard(() => {
        if (window.is_above()) window.unmake_above();
      });
    }
    this._madeAbove.clear();
    this._pulsePid = null;
    this._createdId = null;
    this._watch = null;
  }

  /** The keybinding: ask PULSE to toggle, never wait for it. */
  toggle() {
    if (!this._enabled) return;
    if (this._pulsePid === null) {
      this._logOnce('absent', 'PULSE is not running; Ctrl+Shift+F12 ignored');
      return;
    }
    this._guard(() =>
      this._shell.sendToggle((error) => this._logOnce('toggle', `toggle failed: ${error}`)),
    );
  }

  /** Number of windows with connected handlers (for tests and checks). */
  get trackedCount() {
    return this._tracked.size;
  }

  _pulseAppeared(pid) {
    if (!this._enabled) return;
    this._pulsePid = typeof pid === 'number' && pid > 0 ? pid : null;
    this._logged.delete('absent');
    this._logged.delete('toggle');
    this._logged.delete('hello');
    if (this._pulsePid !== null) {
      // Once per appearance of PULSE on the bus: lets PULSE show that the
      // running extension reaches it. Asynchronous; never waited for.
      this._guard(() =>
        this._shell.sendHello(COMPANION_VERSION, (error) =>
          this._logOnce('hello', `PULSE did not accept our hello: ${error}`),
        ),
      );
    }
    for (const window of this._shell.listWindows()) this._consider(window);
  }

  _pulseVanished() {
    this._pulsePid = null;
    this._untrackAll();
    this._madeAbove.clear();
  }

  /** Only PULSE's own windows are ever tracked; others are never touched. */
  _consider(window) {
    if (!this._enabled || this._pulsePid === null || !window || this._tracked.has(window)) return;
    const facts = readFacts(window, this._shell.normalType);
    if (!facts || facts.pid !== this._pulsePid) return;
    const entry = { ids: [], reasserts: 0 };
    this._tracked.set(window, entry);
    const evaluate = () => this._guard(() => this._evaluate(window));
    entry.ids.push(this._shell.connectWindow(window, 'notify::title', evaluate));
    entry.ids.push(this._shell.connectWindow(window, 'notify::above', evaluate));
    entry.ids.push(
      this._shell.connectWindow(window, 'unmanaging', () =>
        this._guard(() => this._untrack(window)),
      ),
    );
    this._evaluate(window);
  }

  _evaluate(window) {
    const entry = this._tracked.get(window);
    if (!this._enabled || !entry) return;
    const overlay = isPulseOverlay(readFacts(window, this._shell.normalType), this._pulsePid);
    if (overlay && !window.is_above()) {
      if (this._madeAbove.has(window)) {
        // Something cleared it: re-apply, a bounded number of times only.
        if (entry.reasserts >= MAX_REASSERTS) {
          this._logOnce('reassert', 'an overlay keeps losing "above"; no longer re-applying');
          return;
        }
        entry.reasserts += 1;
      }
      window.make_above();
      this._madeAbove.add(window);
    } else if (!overlay && this._madeAbove.has(window)) {
      // No longer an overlay (title changed): give back what we changed.
      if (window.is_above()) window.unmake_above();
      this._madeAbove.delete(window);
    }
  }

  _untrack(window) {
    const entry = this._tracked.get(window);
    if (!entry) return;
    for (const id of entry.ids) this._guard(() => this._shell.disconnectWindow(window, id));
    this._tracked.delete(window);
    this._madeAbove.delete(window);
  }

  _untrackAll() {
    for (const window of [...this._tracked.keys()]) this._untrack(window);
  }

  _logOnce(key, message) {
    if (this._logged.has(key)) return;
    this._logged.add(key);
    this._shell.log(`PULSE overlay bridge: ${message}`);
  }

  /** Nothing ever throws out of a Shell callback. */
  _guard(action) {
    try {
      return action();
    } catch (error) {
      this._logOnce(`error:${error}`, `unexpected error: ${error}`);
      return undefined;
    }
  }
}
