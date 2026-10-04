import { beforeEach, describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import {
  flushUiConfig,
  initUiConfig,
  memoryBackend,
  readSection,
  resetUiConfigForTesting,
  useUiConfigStatus,
  useUiSection,
  writeSection,
} from '@/config/uiConfig';
import type { UiConfigBackend, UiConfigChange, UiConfigSnapshot } from '@/config/uiConfig';

beforeEach(() => resetUiConfigForTesting());

const identity = (raw: unknown) => raw;

describe('shared UI configuration', () => {
  it('waits for the event listener before requesting the initial snapshot', async () => {
    const backend = memoryBackend();
    const order: string[] = [];
    let installed!: () => void;
    const ready = initUiConfig({
      ...backend,
      subscribe: () =>
        new Promise((resolve) => {
          installed = () => {
            order.push('subscribed');
            resolve(() => undefined);
          };
        }),
      load: () => {
        order.push('loaded');
        return backend.load();
      },
    });
    expect(order).toEqual([]);
    installed();
    await ready;
    expect(order).toEqual(['subscribed', 'loaded']);
  });

  it('keeps a lock received while an older initial snapshot is in flight', async () => {
    const backend = memoryBackend({ overlays: { locked: false } });
    const old = await backend.load();
    let finish!: (snapshot: UiConfigSnapshot) => void;
    const ready = initUiConfig({
      ...backend,
      load: () => new Promise((resolve) => (finish = resolve)),
    });
    await Promise.resolve();
    backend.peer('overlays', { locked: true });
    finish(old);
    await ready;
    expect(readSection('overlays')).toEqual({ locked: true });
  });

  it('rejects stale events per section, including events older than our own echo', async () => {
    const backend = memoryBackend({ overlays: { locked: false } });
    let deliver!: (change: UiConfigChange) => void;
    await initUiConfig({
      ...backend,
      subscribe: (listener) => {
        deliver = listener;
        return () => undefined;
      },
    });
    const event = (revision: number, locked: boolean, origin = 'backend'): UiConfigChange => ({
      revision,
      section: 'overlays',
      value: { locked },
      origin,
    });
    deliver({ ...event(5, true), section: 'settings', value: { language: 'fr' } });
    deliver(event(3, true));
    deliver(event(2, false));
    expect(readSection('overlays')).toEqual({ locked: true });
    writeSection('overlays', { locked: false });
    deliver(event(6, false, 'self'));
    deliver(event(4, true));
    expect(readSection('overlays')).toEqual({ locked: false });
    expect(readSection('settings')).toEqual({ language: 'fr' });
  });

  it('does not replay a buffered event already covered by the snapshot', async () => {
    const backend = memoryBackend({ overlays: { locked: false } });
    await initUiConfig({
      ...backend,
      load: () => {
        backend.peer('overlays', { locked: true });
        backend.peer('overlays', { locked: false });
        return backend.load();
      },
    });
    expect(readSection('overlays')).toEqual({ locked: false });
  });

  it('writes a section to the backend once per burst', async () => {
    const backend = memoryBackend();
    await initUiConfig(backend);

    for (let i = 0; i < 20; i += 1) writeSection('settings', { i });
    expect(readSection('settings')).toEqual({ i: 19 });
    await flushUiConfig();

    expect(backend.saves).toBe(1);
    expect(backend.saved().settings).toEqual({ i: 19 });
  });

  it('a section written by another window reaches this one; its own echo is ignored', async () => {
    const backend = memoryBackend();
    await initUiConfig(backend);
    const { result } = renderHook(() => useUiSection('dashboards', identity));

    act(() => backend.peer('dashboards', { from: 'overlay' }));
    expect(result.current).toEqual({ from: 'overlay' });

    writeSection('dashboards', { from: 'me' });
    await act(() => flushUiConfig());
    expect(result.current).toEqual({ from: 'me' });
  });

  it('a relaunch reads what was saved', async () => {
    const backend = memoryBackend();
    await initUiConfig(backend);
    writeSection('templates', { items: [{ id: 't1' }] });
    await flushUiConfig();

    resetUiConfigForTesting();
    await initUiConfig(backend);
    expect(readSection('templates')).toEqual({ items: [{ id: 't1' }] });
  });

  it('normalises once per raw value', async () => {
    await initUiConfig(memoryBackend({ settings: { a: 1 } }));
    let calls = 0;
    const normalize = (raw: unknown) => {
      calls += 1;
      return raw;
    };
    const { rerender } = renderHook(() => useUiSection('settings', normalize));
    rerender();
    rerender();
    expect(calls).toBe(1);
  });

  it('a backend that cannot load still gives a working, in-memory configuration', async () => {
    const broken: UiConfigBackend = {
      load: () => Promise.reject(new Error('backend down')),
      save: () => Promise.reject(new Error('backend down')),
      subscribe: () => () => undefined,
      origin: () => 'main',
    };
    await initUiConfig(broken);
    writeSection('settings', { ok: true });
    expect(readSection('settings')).toEqual({ ok: true });
  });

  it('reports a read-only configuration and why', async () => {
    const snapshot: UiConfigSnapshot = {
      revision: 1,
      document: { version: 1 },
      load: { kind: 'newerVersion', found: 3 },
      readOnly: 'written by a newer PULSE',
      path: '/tmp/x',
    };
    await initUiConfig({
      load: () => Promise.resolve(snapshot),
      save: () => Promise.resolve(2),
      subscribe: () => () => undefined,
      origin: () => 'main',
    });
    const { result } = renderHook(() => useUiConfigStatus());
    expect(result.current.readOnly).toMatch(/newer/);
    expect(result.current.load).toEqual({ kind: 'newerVersion', found: 3 });
  });
});
