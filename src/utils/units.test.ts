import { describe, expect, it } from 'vitest';
import {
  binaryUnitFor,
  formatBytes,
  formatBytesScaledTo,
  formatPercent,
  formatSampleTime,
} from '@/utils/units';

describe('formatBytes', () => {
  it('uses binary prefixes, matching what system tools show', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(1024)).toBe('1.0 KiB');
    expect(formatBytes(1024 * 1024)).toBe('1.0 MiB');
    expect(formatBytes(1024 ** 3)).toBe('1.0 GiB');
    expect(formatBytes(1024 ** 4)).toBe('1.0 TiB');
  });

  it('renders a realistic memory total the way free(1) does', () => {
    // 32 GB advertised == 32768 MiB reported by the kernel.
    expect(formatBytes(32_784_204 * 1024)).toBe('31.3 GiB');
  });

  it('honours the requested precision', () => {
    expect(formatBytes(1024 ** 3, 0)).toBe('1 GiB');
    expect(formatBytes(1024 ** 3 * 1.5, 2)).toBe('1.50 GiB');
  });

  it('returns a placeholder rather than NaN for impossible input', () => {
    expect(formatBytes(Number.NaN)).toBe('—');
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe('—');
    expect(formatBytes(-1)).toBe('—');
  });
});

describe('formatBytesScaledTo', () => {
  it('scales both halves of a pair to the same unit', () => {
    const total = 34_359_738_368; // 32 GiB
    const used = 12_884_901_888; // 12 GiB

    expect(formatBytesScaledTo(used, total)).toBe('12.0');
    expect(binaryUnitFor(total)).toBe('GiB');
  });

  it('does not promote a small value to its own larger unit', () => {
    // 900 MiB against a 32 GiB total must read as 0.9, not "900".
    expect(formatBytesScaledTo(900 * 1024 ** 2, 32 * 1024 ** 3)).toBe('0.9');
  });

  it('returns a placeholder for impossible input', () => {
    expect(formatBytesScaledTo(Number.NaN, 1024)).toBe('—');
    expect(formatBytesScaledTo(1024, -1)).toBe('—');
  });
});

describe('formatPercent', () => {
  it('formats a percentage with a unit', () => {
    expect(formatPercent(0)).toBe('0.0 %');
    expect(formatPercent(37.8)).toBe('37.8 %');
    expect(formatPercent(100)).toBe('100.0 %');
    expect(formatPercent(18.44, 1)).toBe('18.4 %');
  });

  it('refuses values outside 0–100 instead of displaying them', () => {
    // The backend clamps, so anything here means something went wrong.
    expect(formatPercent(-0.1)).toBe('—');
    expect(formatPercent(100.1)).toBe('—');
    expect(formatPercent(Number.NaN)).toBe('—');
    expect(formatPercent(Number.POSITIVE_INFINITY)).toBe('—');
  });
});

describe('formatSampleTime', () => {
  it('renders an epoch-milliseconds timestamp as a time of day', () => {
    const formatted = formatSampleTime(Date.UTC(2026, 0, 15, 12, 34, 56));

    // Locale and timezone vary; assert the shape, not the literal string.
    expect(formatted).toMatch(/\d{1,2}[:.]\d{2}[:.]\d{2}/);
  });

  it('returns a placeholder for a missing timestamp', () => {
    expect(formatSampleTime(0)).toBe('—');
    expect(formatSampleTime(Number.NaN)).toBe('—');
  });
});
