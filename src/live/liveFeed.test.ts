import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MetricRef } from '@/types/metrics';
import {
  LIVE_CAPACITY,
  deliverLiveTickForTesting,
  liveSubscriptionSize,
  retainLive,
  setLiveBackendForTesting,
} from '@/live/liveFeed';
import type { LiveBackend } from '@/live/liveFeed';

const CPU: MetricRef = { key: 'cpu.usage.total', sourceId: 'cpu:system' };
const RAM: MetricRef = { key: 'memory.usage.percent', sourceId: 'memory:system' };

function fakeBackend() {
  const calls: MetricRef[][] = [];
  const backend: LiveBackend = {
    setSubscription: vi.fn((metrics: readonly MetricRef[]) => {
      calls.push([...metrics]);
      return Promise.resolve({ refused: [] });
    }),
    buffer: vi.fn(() => Promise.resolve([])),
    onTick: vi.fn(() => () => undefined),
  };
  return { backend, calls };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

let hidden = false;
beforeEach(() => {
  hidden = false;
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
});
afterEach(() => setLiveBackendForTesting(null));

describe('live feed (one window)', () => {
  it('a hundred widgets on CPU are one reference in one subscription call', async () => {
    const { backend, calls } = fakeBackend();
    setLiveBackendForTesting(backend);
    const releases = Array.from({ length: 100 }, () => retainLive([CPU]));
    await flush();

    expect(calls).toEqual([[CPU]]);
    expect(liveSubscriptionSize()).toBe(1);

    releases.slice(1).forEach((release) => release());
    await flush();
    expect(calls).toHaveLength(1);
    releases[0]!();
    await flush();
    expect(calls.at(-1)).toEqual([]);
  });

  it('the union follows what is retained', async () => {
    const { backend, calls } = fakeBackend();
    setLiveBackendForTesting(backend);
    const releaseCpu = retainLive([CPU]);
    retainLive([CPU, RAM]);
    await flush();
    expect(calls.at(-1)).toEqual([CPU, RAM]);
    releaseCpu();
    await flush();
    expect(calls).toHaveLength(1);
  });

  it('a hidden window unsubscribes, and resubscribes when shown', async () => {
    const { backend, calls } = fakeBackend();
    setLiveBackendForTesting(backend);
    retainLive([CPU]);
    await flush();
    hidden = true;
    document.dispatchEvent(new Event('visibilitychange'));
    await flush();
    expect(calls.at(-1)).toEqual([]);
    hidden = false;
    document.dispatchEvent(new Event('visibilitychange'));
    await flush();
    expect(calls.at(-1)).toEqual([CPU]);
  });

  it('rings are bounded and keep unavailable readings as gaps', async () => {
    const { backend } = fakeBackend();
    setLiveBackendForTesting(backend);
    retainLive([CPU]);
    await flush();
    const { useLiveSeries } = await import('@/live/liveFeed');
    expect(useLiveSeries).toBeTypeOf('function');
    for (let t = 0; t < LIVE_CAPACITY + 50; t += 1) {
      deliverLiveTickForTesting({
        t: t * 1000,
        values: [{ metric: CPU, v: t % 7 === 0 ? null : t }],
      });
    }
    // Not retained: ignored entirely.
    deliverLiveTickForTesting({ t: 1, values: [{ metric: RAM, v: 5 }] });
    const { renderHook } = await import('@testing-library/react');
    const { result } = renderHook(() => useLiveSeries([CPU, RAM]));
    const points = result.current.points.get('cpu.usage.total@cpu:system')!;
    expect(points).toHaveLength(LIVE_CAPACITY);
    expect(points.some((point) => point.v === null)).toBe(true);
    expect(result.current.points.get('memory.usage.percent@memory:system')).toEqual([]);
  });
});
