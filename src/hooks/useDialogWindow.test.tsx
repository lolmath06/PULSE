import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { StrictMode } from 'react';
import * as nativeWindow from '@tauri-apps/api/window';
import * as runtime from '@/services/tauri';
import { DialogWindowPriority, useDialogWindow } from './useDialogWindow';

vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: vi.fn() }));

function native(previous = false) {
  return {
    isAlwaysOnTop: vi.fn().mockResolvedValue(previous),
    setAlwaysOnTop: vi.fn().mockResolvedValue(undefined),
    setFocus: vi.fn().mockResolvedValue(undefined),
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.mocked(nativeWindow.getCurrentWindow).mockReset();
});

describe('native dialog window priority', () => {
  it.each([false, true])(
    'raises/focuses main and restores its previous top state (%s)',
    async (top) => {
      const window = native(top);
      const priority = new DialogWindowPriority(window);
      const release = priority.retain();
      await priority.settled();
      expect(window.setAlwaysOnTop).toHaveBeenNthCalledWith(1, true);
      expect(window.setFocus).toHaveBeenCalledOnce();
      expect(window.setAlwaysOnTop.mock.invocationCallOrder[0]).toBeLessThan(
        window.setFocus.mock.invocationCallOrder[0]!,
      );
      release();
      release(); // Cleanup is idempotent.
      await priority.settled();
      expect(window.setAlwaysOnTop.mock.calls).toEqual([[true], [top]]);
    },
  );

  it('does not lower main until the last nested dialog closes', async () => {
    const window = native();
    const priority = new DialogWindowPriority(window);
    const first = priority.retain();
    const second = priority.retain();
    await priority.settled();
    first();
    await priority.settled();
    expect(window.setAlwaysOnTop.mock.calls).toEqual([[true]]);
    second();
    await priority.settled();
    expect(window.setAlwaysOnTop.mock.calls).toEqual([[true], [false]]);
  });

  it('does not flicker when a dialog is replaced in the same turn', async () => {
    const window = native();
    const priority = new DialogWindowPriority(window);
    const first = priority.retain();
    await priority.settled();
    first();
    const second = priority.retain();
    await priority.settled();
    expect(window.setAlwaysOnTop.mock.calls).toEqual([[true]]);
    second();
    await priority.settled();
    expect(window.setAlwaysOnTop.mock.calls).toEqual([[true], [false]]);
  });

  it('does not raise a dialog that closed during the native getter', async () => {
    const window = native();
    let resolve!: (value: boolean) => void;
    window.isAlwaysOnTop.mockReturnValue(new Promise<boolean>((done) => (resolve = done)));
    const priority = new DialogWindowPriority(window);
    const release = priority.retain();
    await Promise.resolve();
    release();
    resolve(false);
    await priority.settled();
    expect(window.setAlwaysOnTop).not.toHaveBeenCalled();
  });

  it('serializes restoration after an in-flight raise when cancelled', async () => {
    const window = native();
    let finish!: () => void;
    window.setAlwaysOnTop.mockImplementationOnce(
      () => new Promise<void>((done) => (finish = done)),
    );
    const priority = new DialogWindowPriority(window);
    const release = priority.retain();
    await waitFor(() => expect(window.setAlwaysOnTop).toHaveBeenCalledWith(true));
    release();
    expect(window.setAlwaysOnTop.mock.calls).toEqual([[true]]);
    finish();
    await priority.settled();
    expect(window.setAlwaysOnTop.mock.calls).toEqual([[true], [false]]);
    expect(window.setFocus).not.toHaveBeenCalled();
  });

  it('restores priority even if focusing fails', async () => {
    const window = native();
    window.setFocus.mockRejectedValue(new Error('focus denied'));
    const log = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const priority = new DialogWindowPriority(window);
    const release = priority.retain();
    await priority.settled();
    release();
    await priority.settled();
    expect(log).toHaveBeenCalledOnce();
    expect(window.setAlwaysOnTop.mock.calls).toEqual([[true], [false]]);
  });

  it('does not access native windows in browser mode', () => {
    vi.spyOn(runtime, 'isTauriRuntime').mockReturnValue(false);
    const getter = vi.mocked(nativeWindow.getCurrentWindow);
    const { unmount } = renderHook(useDialogWindow);
    unmount();
    expect(getter).not.toHaveBeenCalled();
  });

  it('never changes an overlay window', () => {
    const window = { ...native(), label: 'overlay-example' };
    vi.spyOn(runtime, 'isTauriRuntime').mockReturnValue(true);
    vi.mocked(nativeWindow.getCurrentWindow).mockReturnValue(
      window as unknown as nativeWindow.Window,
    );
    const { unmount } = renderHook(useDialogWindow);
    unmount();
    expect(window.isAlwaysOnTop).not.toHaveBeenCalled();
  });

  it('restores on main dialog unmount, including StrictMode effect replay', async () => {
    const window = { ...native(), label: 'main' };
    vi.spyOn(runtime, 'isTauriRuntime').mockReturnValue(true);
    vi.mocked(nativeWindow.getCurrentWindow).mockReturnValue(
      window as unknown as nativeWindow.Window,
    );
    const { unmount } = renderHook(useDialogWindow, { wrapper: StrictMode });
    await waitFor(() => expect(window.setFocus).toHaveBeenCalledOnce());
    unmount();
    await waitFor(() => expect(window.setAlwaysOnTop.mock.calls).toEqual([[true], [false]]));
  });
});
