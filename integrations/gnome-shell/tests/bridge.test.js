import { describe, expect, it } from 'vitest';
import {
  MAX_REASSERTS,
  OVERLAY_TITLE,
  PulseOverlayBridge,
  isPulseOverlay,
} from '../pulse-overlay@jamby/lib.js';

// The decisions of the GNOME Shell bridge, driven through a fake Shell. No
// GNOME Shell, no Mutter, no D-Bus: this proves the logic, not the physical
// behaviour (see docs/overlay/gnome-bridge-poc.md).

const PULSE_PID = 4242;
const NORMAL = 0;
const DIALOG = 3;

class FakeWindow {
  constructor({ pid = PULSE_PID, title = 'PULSE Overlay :: o-abc', type = NORMAL } = {}) {
    this.pid = pid;
    this.title = title;
    this.type = type;
    this.above = false;
    this.calls = [];
    this.handlers = new Map();
    this.nextId = 1;
  }
  get_pid() {
    return this.pid;
  }
  get_title() {
    return this.title;
  }
  get_window_type() {
    return this.type;
  }
  is_override_redirect() {
    return false;
  }
  is_above() {
    return this.above;
  }
  make_above() {
    this.calls.push('make_above');
    this.above = true;
    this.emit('notify::above');
  }
  unmake_above() {
    this.calls.push('unmake_above');
    this.above = false;
    this.emit('notify::above');
  }
  connect(signal, callback) {
    const id = this.nextId++;
    this.handlers.set(id, { signal, callback });
    return id;
  }
  disconnect(id) {
    this.handlers.delete(id);
  }
  emit(signal) {
    for (const { signal: s, callback } of [...this.handlers.values()]) if (s === signal) callback();
  }
  setTitle(title) {
    this.title = title;
    this.emit('notify::title');
  }
}

function fakeShell(windows = []) {
  const shell = {
    normalType: NORMAL,
    windows,
    keybindings: 0,
    displayHandlers: new Map(),
    watches: new Map(),
    toggles: 0,
    toggleError: null,
    logs: [],
    nextId: 1,
    addKeybinding(handler) {
      shell.keybindings += 1;
      shell.keyHandler = handler;
    },
    removeKeybinding() {
      shell.keybindings -= 1;
      shell.keyHandler = null;
    },
    watchPulse(onAppeared, onVanished) {
      const id = shell.nextId++;
      shell.watches.set(id, { onAppeared, onVanished });
      return id;
    },
    unwatchPulse(id) {
      shell.watches.delete(id);
    },
    onWindowCreated(callback) {
      const id = shell.nextId++;
      shell.displayHandlers.set(id, callback);
      return id;
    },
    disconnectDisplay(id) {
      shell.displayHandlers.delete(id);
    },
    listWindows: () => shell.windows,
    connectWindow: (window, signal, callback) => window.connect(signal, callback),
    disconnectWindow: (window, id) => window.disconnect(id),
    sendToggle(onError) {
      shell.toggles += 1;
      if (shell.toggleError) onError(shell.toggleError);
    },
    log: (message) => shell.logs.push(message),
    // Test helpers.
    pulseAppears(pid = PULSE_PID) {
      for (const { onAppeared } of shell.watches.values()) onAppeared(pid);
    },
    pulseVanishes() {
      for (const { onVanished } of shell.watches.values()) onVanished();
    },
    create(window) {
      shell.windows.push(window);
      for (const callback of shell.displayHandlers.values()) callback(window);
    },
  };
  return shell;
}

describe('overlay window identification', () => {
  const overlay = { pid: PULSE_PID, title: 'PULSE Overlay :: o-abc', normal: true };

  it('matches a PULSE overlay window', () => {
    expect(isPulseOverlay(overlay, PULSE_PID)).toBe(true);
  });

  it('never matches the main window or Mini', () => {
    expect(isPulseOverlay({ ...overlay, title: 'PULSE' }, PULSE_PID)).toBe(false);
    expect(isPulseOverlay({ ...overlay, title: 'PULSE Mini' }, PULSE_PID)).toBe(false);
  });

  it('never matches another application, even with the exact title', () => {
    expect(isPulseOverlay({ ...overlay, pid: 7 }, PULSE_PID)).toBe(false);
    for (const title of [
      'PULSE — dashboard',
      'Firefox — PULSE Overlay :: o-abc',
      'PULSE Overlay :: o-abc — Mozilla Firefox',
      'PULSE Overlay :: Bad Id',
      'PULSE Overlay :: ',
    ]) {
      expect(OVERLAY_TITLE.test(title), title).toBe(false);
    }
  });

  it('needs PULSE on the bus, a normal window, and sane facts', () => {
    expect(isPulseOverlay(overlay, null)).toBe(false);
    expect(isPulseOverlay(overlay, 0)).toBe(false);
    expect(isPulseOverlay({ ...overlay, normal: false }, PULSE_PID)).toBe(false);
    expect(isPulseOverlay({ ...overlay, overrideRedirect: true }, PULSE_PID)).toBe(false);
    expect(isPulseOverlay(null, PULSE_PID)).toBe(false);
    expect(isPulseOverlay({ ...overlay, title: undefined }, PULSE_PID)).toBe(false);
  });
});

describe('bridge lifecycle', () => {
  it('a second enable adds no second keybinding or handler', () => {
    const shell = fakeShell();
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    bridge.enable();
    expect(shell.keybindings).toBe(1);
    expect(shell.displayHandlers.size).toBe(1);
    expect(shell.watches.size).toBe(1);
  });

  it('disable removes the keybinding and disconnects every handler', () => {
    const overlay = new FakeWindow();
    const shell = fakeShell([overlay]);
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    shell.pulseAppears();
    expect(overlay.handlers.size).toBe(3);
    bridge.disable();
    expect(shell.keybindings).toBe(0);
    expect(shell.displayHandlers.size).toBe(0);
    expect(shell.watches.size).toBe(0);
    expect(overlay.handlers.size).toBe(0);
    expect(bridge.trackedCount).toBe(0);
    // It gives back what it changed.
    expect(overlay.above).toBe(false);
    // Enable → disable → enable leaves exactly one of each.
    bridge.enable();
    bridge.disable();
    bridge.enable();
    expect(shell.keybindings).toBe(1);
    expect(shell.displayHandlers.size).toBe(1);
  });
});

describe('always on top', () => {
  it('makes only PULSE overlays above; main, Mini and other apps are never touched', () => {
    const overlay = new FakeWindow();
    const main = new FakeWindow({ title: 'PULSE' });
    const mini = new FakeWindow({ title: 'PULSE Mini' });
    const firefox = new FakeWindow({ pid: 99, title: 'PULSE Overlay :: o-abc' });
    const dialog = new FakeWindow({ title: 'PULSE Overlay :: o-abc', type: DIALOG });
    const shell = fakeShell([overlay, main, mini, firefox, dialog]);
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    expect(overlay.above).toBe(false); // PULSE's PID not known yet
    shell.pulseAppears();
    expect(overlay.calls).toEqual(['make_above']);
    expect(main.calls).toEqual([]);
    expect(mini.calls).toEqual([]);
    expect(firefox.calls).toEqual([]);
    expect(dialog.calls).toEqual([]);
    expect(firefox.handlers.size).toBe(0);
    expect(bridge.trackedCount).toBe(4);
  });

  it('a new overlay is made above when it appears or gets its title', () => {
    const shell = fakeShell();
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    shell.pulseAppears();
    const late = new FakeWindow({ title: '' });
    shell.create(late);
    expect(late.above).toBe(false);
    late.setTitle('PULSE Overlay :: o-late');
    expect(late.calls).toEqual(['make_above']);
  });

  it('re-applies a cleared state a bounded number of times, then stops', () => {
    const overlay = new FakeWindow();
    const shell = fakeShell([overlay]);
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    shell.pulseAppears();
    for (let i = 0; i < 10; i += 1) overlay.unmake_above();
    const makes = overlay.calls.filter((call) => call === 'make_above').length;
    expect(makes).toBe(1 + MAX_REASSERTS);
    expect(shell.logs.filter((log) => log.includes('no longer re-applying'))).toHaveLength(1);
  });

  it('a window that stops being an overlay is given back', () => {
    const overlay = new FakeWindow();
    const shell = fakeShell([overlay]);
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    shell.pulseAppears();
    overlay.setTitle('PULSE');
    expect(overlay.above).toBe(false);
  });

  it('forgets a window when it is unmanaged, and everything when PULSE quits', () => {
    const a = new FakeWindow();
    const b = new FakeWindow({ title: 'PULSE Overlay :: o-b' });
    const shell = fakeShell([a, b]);
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    shell.pulseAppears();
    a.emit('unmanaging');
    expect(a.handlers.size).toBe(0);
    expect(bridge.trackedCount).toBe(1);
    shell.pulseVanishes();
    expect(b.handlers.size).toBe(0);
    expect(bridge.trackedCount).toBe(0);
  });

  it('a window that throws never throws out of the bridge', () => {
    const broken = new FakeWindow();
    broken.get_title = () => {
      throw new Error('gone');
    };
    const shell = fakeShell([broken]);
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    expect(() => shell.pulseAppears()).not.toThrow();
    expect(broken.calls).toEqual([]);
  });
});

describe('Ctrl+Shift+F12', () => {
  it('forwards one toggle per press while PULSE is running', () => {
    const shell = fakeShell();
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    shell.pulseAppears();
    shell.keyHandler();
    shell.keyHandler();
    expect(shell.toggles).toBe(2);
  });

  it('without PULSE: no call, no retry, one log line at most', () => {
    const shell = fakeShell();
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    for (let i = 0; i < 5; i += 1) shell.keyHandler();
    expect(shell.toggles).toBe(0);
    expect(shell.logs).toHaveLength(1);
    expect(shell.logs[0]).toMatch(/not running/);
  });

  it('a failing call is logged once and never thrown', () => {
    const shell = fakeShell();
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    shell.pulseAppears();
    shell.toggleError = new Error('NoReply');
    expect(() => {
      shell.keyHandler();
      shell.keyHandler();
    }).not.toThrow();
    expect(shell.logs.filter((log) => log.includes('toggle failed'))).toHaveLength(1);
  });

  it('after disable the shortcut does nothing', () => {
    const shell = fakeShell();
    const bridge = new PulseOverlayBridge(shell);
    bridge.enable();
    shell.pulseAppears();
    const handler = shell.keyHandler;
    bridge.disable();
    handler();
    expect(shell.toggles).toBe(0);
  });
});
