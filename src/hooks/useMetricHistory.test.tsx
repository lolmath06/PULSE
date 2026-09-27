import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import * as historyService from '@/services/history';
import { emitHistorySampleForTesting, historySubscriberCount } from '@/services/history';
import { useMetricHistory } from '@/hooks/useMetricHistory';
import { CPU_USAGE_TOTAL } from '@/types/wellknown';
import { T0, historyResponse } from '@/test/visualization';

afterEach(() => vi.restoreAllMocks());

const event = { batchId: 1, timestampMs: T0, rowCount: 10 };

describe('useMetricHistory', () => {
  it('coalesces a burst of batches into one follow-up load', async () => {
    let release: () => void = () => undefined;
    const spy = vi.spyOn(historyService, 'getMetricHistory').mockImplementation(
      () =>
        new Promise((resolve) => {
          release = () => resolve(historyResponse([{ metric: CPU_USAGE_TOTAL, points: [] }]));
        }),
    );
    const metrics = [CPU_USAGE_TOTAL];
    renderHook(() => useMetricHistory(metrics, '15m'));
    expect(spy).toHaveBeenCalledTimes(1);

    act(() => {
      for (let i = 0; i < 5; i += 1) emitHistorySampleForTesting(event);
    });
    expect(spy).toHaveBeenCalledTimes(1);

    await act(async () => release());
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(2));
    await act(async () => release());
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('does nothing — not even listening — while disabled', () => {
    const spy = vi.spyOn(historyService, 'getMetricHistory');
    renderHook(() => useMetricHistory([CPU_USAGE_TOTAL], '1h', false));
    expect(spy).not.toHaveBeenCalled();
    expect(historySubscriberCount()).toBe(0);
  });

  it('reports why history is unavailable', async () => {
    vi.spyOn(historyService, 'getMetricHistory').mockResolvedValue({
      status: 'unavailable',
      reason: 'schema 2 is newer than this PULSE',
    });
    const { result } = renderHook(() => useMetricHistory([CPU_USAGE_TOTAL], '15m'));
    await waitFor(() => expect(result.current.status).toBe('unavailable'));
    expect(result.current.reason).toMatch(/newer/);
  });

  it('a new array with the same metrics does not reload', async () => {
    const spy = vi
      .spyOn(historyService, 'getMetricHistory')
      .mockResolvedValue(historyResponse([{ metric: CPU_USAGE_TOTAL, points: [] }]));
    const { rerender } = renderHook(() => useMetricHistory([{ ...CPU_USAGE_TOTAL }], '15m'));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    rerender();
    rerender();
    expect(spy).toHaveBeenCalledTimes(1);
  });
});
