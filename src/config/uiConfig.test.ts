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
import type { UiConfigBackend, UiConfigSnapshot } from '@/config/uiConfig';

beforeEach(() => resetUiConfigForTesting());

const identity = (raw: unknown) => raw;

describe('shared UI configuration', () => {
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
