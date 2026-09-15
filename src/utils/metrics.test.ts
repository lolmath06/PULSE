import { describe, expect, it } from 'vitest';
import type { Availability } from '@/types/metrics';
import { describeAvailability, formatEngineState, isTransient } from '@/utils/metrics';

describe('formatEngineState', () => {
  it('labels both engine states', () => {
    expect(formatEngineState('ready')).toBe('Ready');
    expect(formatEngineState('empty')).toBe('Empty');
  });
});

describe('describeAvailability', () => {
  it('keeps the reasons for absence distinguishable', () => {
    // The whole point of the availability contract: these must not collapse
    // into one generic "unavailable".
    const states: Availability[] = [
      { status: 'unsupported', reason: 'no Windows API' },
      { status: 'notDetected', reason: 'no discrete GPU' },
      { status: 'permissionDenied', reason: 'needs root' },
      { status: 'temporarilyUnavailable', reason: 'driver restarting' },
      { status: 'notRegistered', reason: 'unknown reference' },
    ];

    const described = states.map(describeAvailability);

    expect(new Set(described).size).toBe(states.length);
    expect(described[0]).toContain('Not supported');
    expect(described[1]).toContain('Not detected');
    expect(described[2]).toContain('Permission required');
  });

  it('surfaces the provider error message', () => {
    expect(
      describeAvailability({
        status: 'providerError',
        error: { code: 'io', message: 'hwmon read failed', recoverable: true },
      }),
    ).toBe('Provider error: hwmon read failed');
  });

  it('labels an available metric', () => {
    expect(describeAvailability({ status: 'available' })).toBe('Available');
  });
});

describe('isTransient', () => {
  it('separates "wait" from "act"', () => {
    expect(isTransient({ status: 'temporarilyUnavailable', reason: 'retry' })).toBe(true);
    expect(
      isTransient({
        status: 'providerError',
        error: { code: 'timeout', message: 'slow', recoverable: true },
      }),
    ).toBe(true);

    expect(isTransient({ status: 'permissionDenied', reason: 'needs root' })).toBe(false);
    expect(isTransient({ status: 'notDetected', reason: 'absent' })).toBe(false);
    expect(isTransient({ status: 'available' })).toBe(false);
    expect(
      isTransient({
        status: 'providerError',
        error: { code: 'unsupported', message: 'never', recoverable: false },
      }),
    ).toBe(false);
  });
});
