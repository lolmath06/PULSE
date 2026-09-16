import { describe, expect, it } from 'vitest';
import {
  binaryUnitFor,
  formatBytes,
  formatBytesScaledTo,
  formatHertz,
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

describe('formatHertz', () => {
  it('formats the examples the contract documents', () => {
    expect(formatHertz(800_000_000)).toBe('800 MHz');
    expect(formatHertz(3_200_000_000)).toBe('3.20 GHz');
    expect(formatHertz(5_400_000_000)).toBe('5.40 GHz');
  });

  it('switches to gigahertz exactly at one gigahertz', () => {
    expect(formatHertz(999_999_999)).toBe('1000 MHz');
    expect(formatHertz(1_000_000_000)).toBe('1.00 GHz');
    expect(formatHertz(1_000_000_001)).toBe('1.00 GHz');
  });

  it('renders real readings from a hybrid CPU', () => {
    // P-core boosting, E-core at its maximum, a core parked at idle.
    expect(formatHertz(5_600_000_000)).toBe('5.60 GHz');
    expect(formatHertz(4_100_000_000)).toBe('4.10 GHz');
    expect(formatHertz(987_199_000)).toBe('987 MHz');
  });

  it('keeps two decimals so close clock speeds stay distinguishable', () => {
    expect(formatHertz(4_620_000_000)).toBe('4.62 GHz');
    expect(formatHertz(4_580_000_000)).toBe('4.58 GHz');
    expect(formatHertz(3_950_000_000)).toBe('3.95 GHz');
  });

  it('never renders an unknown frequency as zero', () => {
    // "0 GHz" would read as a claim that the core has stopped.
    expect(formatHertz(0)).toBe('—');
    expect(formatHertz(-1)).toBe('—');
    expect(formatHertz(Number.NaN)).toBe('—');
    expect(formatHertz(Number.POSITIVE_INFINITY)).toBe('—');
  });

  it('is presentation only and leaves the hertz contract untouched', () => {
    // A sanity check on the unit boundary: the numbers here are the exact
    // values the backend sends, never pre-scaled.
    expect(formatHertz(2_400_000_000)).toBe('2.40 GHz');
    // A value already scaled to MHz by mistake would be nonsense as hertz;
    // showing "0 MHz" would hide the bug, so it gets the placeholder.
    expect(formatHertz(2_400)).toBe('—');
  });
});
