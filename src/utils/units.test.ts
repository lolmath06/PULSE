import { describe, expect, it } from 'vitest';
import {
  binaryUnitFor,
  formatBytes,
  formatBytesScaledTo,
  formatBitsPerSecond,
  formatCelsius,
  formatCount,
  formatDbm,
  formatHertz,
  formatHours,
  formatIops,
  formatLatency,
  formatPacketRate,
  formatPercent,
  formatRpm,
  formatSampleTime,
  formatThroughput,
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

describe('formatCelsius', () => {
  it('shows whole degrees, which is the precision sensors have', () => {
    expect(formatCelsius(47)).toBe('47 °C');
    expect(formatCelsius(47.4)).toBe('47 °C');
    expect(formatCelsius(93.5)).toBe('94 °C');
  });

  it('shows a decimal only when one is asked for', () => {
    expect(formatCelsius(47.5, 1)).toBe('47.5 °C');
  });

  it('treats zero and negative readings as real temperatures', () => {
    // Unlike a frequency, 0 °C is a measurement: a cold room, a cold sensor.
    expect(formatCelsius(0)).toBe('0 °C');
    expect(formatCelsius(-5)).toBe('-5 °C');
  });

  it('refuses what is not a temperature', () => {
    expect(formatCelsius(Number.NaN)).toBe('—');
    expect(formatCelsius(Number.POSITIVE_INFINITY)).toBe('—');
    // Below absolute zero is a failed read, not a cold machine.
    expect(formatCelsius(-300)).toBe('—');
  });

  it('never leaks the unit the backend reads', () => {
    // hwmon reports 42000 millidegrees; the conversion happens in Rust, and a
    // raw millidegree value arriving here would be visibly absurd.
    expect(formatCelsius(42)).toBe('42 °C');
    expect(formatCelsius(42)).not.toContain('42000');
  });
});

describe('formatRpm', () => {
  it('formats a fan speed with its unit', () => {
    expect(formatRpm(2187)).toBe(`${(2187).toLocaleString()} RPM`);
  });

  it('shows a stopped fan as zero rather than as an absence', () => {
    // A GPU below its zero-RPM threshold really is turning at 0. The absence
    // of a sensor is shown as "—" by the component, never by this function.
    expect(formatRpm(0)).toBe('0 RPM');
  });

  it('rounds to whole revolutions', () => {
    expect(formatRpm(1200.6)).toBe(`${(1201).toLocaleString()} RPM`);
  });

  it('refuses what is not a fan speed', () => {
    expect(formatRpm(-1)).toBe('—');
    expect(formatRpm(Number.NaN)).toBe('—');
  });
});

describe('formatThroughput', () => {
  it('uses the same binary prefixes as capacities', () => {
    // A card that showed a 2 TiB disk transferring at 125 MB/s would be mixing
    // two conventions in one place.
    expect(formatThroughput(0)).toBe('0 B/s');
    expect(formatThroughput(512)).toBe('512 B/s');
    expect(formatThroughput(1024)).toBe('1.0 KiB/s');
    expect(formatThroughput(125 * 1024 * 1024)).toBe('125 MiB/s');
    expect(formatThroughput(1.5 * 1024 ** 3)).toBe('1.5 GiB/s');
  });

  it('treats zero as a measurement, not an absence', () => {
    // A disk with no I/O during the interval genuinely transferred nothing,
    // and that is different from a disk nothing measured.
    expect(formatThroughput(0)).toBe('0 B/s');
    expect(formatThroughput(0)).not.toBe('—');
  });

  it('drops the decimal above a hundred', () => {
    // `847.3 MiB/s` is false precision on a figure that moves every refresh.
    expect(formatThroughput(847.3 * 1024 * 1024)).toBe('847 MiB/s');
    expect(formatThroughput(99.4 * 1024 * 1024)).toBe('99.4 MiB/s');
  });

  it('refuses a negative or non-finite rate', () => {
    expect(formatThroughput(-1)).toBe('—');
    expect(formatThroughput(Number.NaN)).toBe('—');
    expect(formatThroughput(Number.POSITIVE_INFINITY)).toBe('—');
  });

  it('matches a real measurement from the development machine', () => {
    // 17 840 761 B/s, reading PULSE's own build output.
    expect(formatThroughput(17_840_761.9)).toBe('17.0 MiB/s');
  });
});

describe('formatIops', () => {
  it('shows whole operations above ten', () => {
    // A disk does not complete 940.7 operations; the fraction is an artefact
    // of dividing by an interval that is not exactly one second.
    expect(formatIops(940)).toBe('940 IOPS');
    expect(formatIops(407.738)).toBe('408 IOPS');
  });

  it('keeps a decimal for a trickle of activity', () => {
    // Rounding 0.4 and 2 both to whole numbers would hide the difference
    // between an almost-idle disk and a genuinely idle one.
    expect(formatIops(0.4)).toBe('0.4 IOPS');
    expect(formatIops(2)).toBe('2.0 IOPS');
  });

  it('treats zero as a measurement', () => {
    expect(formatIops(0)).toBe('0 IOPS');
  });

  it('groups large figures', () => {
    expect(formatIops(125_000)).toBe((125_000).toLocaleString() + ' IOPS');
  });

  it('refuses a negative or non-finite rate', () => {
    expect(formatIops(-1)).toBe('—');
    expect(formatIops(Number.NaN)).toBe('—');
  });
});

describe('formatLatency', () => {
  it('scales its precision to the magnitude', () => {
    // The hardware's precision scales the same way: two decimals distinguish
    // 0.73 ms from 0.71 ms on an NVMe drive, and `18.24 ms` on a spinning disk
    // claims a resolution the OS's millisecond accounting does not have.
    expect(formatLatency(0.731_827_192)).toBe('0.73 ms');
    expect(formatLatency(3.24)).toBe('3.2 ms');
    expect(formatLatency(18.4)).toBe('18 ms');
  });

  it('never shows false precision', () => {
    expect(formatLatency(0.731_827_192)).not.toContain('731827');
  });

  it('treats zero as a measurement', () => {
    // Operations completed, and the OS accounted less than its rounding unit
    // of time to them. Normal on a fast SSD.
    expect(formatLatency(0)).toBe('0.00 ms');
  });

  it('refuses a negative or non-finite latency', () => {
    expect(formatLatency(-1)).toBe('—');
    expect(formatLatency(Number.NaN)).toBe('—');
  });

  it('matches a real measurement from the development machine', () => {
    expect(formatLatency(0.259_124_087)).toBe('0.26 ms');
  });
});

describe('formatHours', () => {
  it('keeps hours as hours', () => {
    // A controller counts power-on time in whole hours, and "1 year" would
    // round away the figure a user compares against a warranty.
    expect(formatHours(421)).toBe('421 h');
    expect(formatHours(0)).toBe('0 h');
  });

  it('groups above a thousand', () => {
    expect(formatHours(12_847)).toBe((12_847).toLocaleString() + ' h');
  });

  it('refuses a negative or non-finite duration', () => {
    expect(formatHours(-1)).toBe('—');
    expect(formatHours(Number.NaN)).toBe('—');
  });
});

describe('formatCount', () => {
  it('shows zero as the measurement a user hopes for', () => {
    expect(formatCount(0)).toBe('0');
    expect(formatCount(0)).not.toBe('—');
  });

  it('groups large counts', () => {
    expect(formatCount(1284)).toBe((1284).toLocaleString());
  });

  it('refuses a negative or non-finite count', () => {
    expect(formatCount(-1)).toBe('—');
    expect(formatCount(Number.NaN)).toBe('—');
  });
});

describe('formatBitsPerSecond', () => {
  it('uses decimal prefixes, as networking always has', () => {
    // A "gigabit" link is 1 000 000 000 bits per second, not 1 073 741 824,
    // and every switch, driver and datasheet agrees. Binary prefixes would
    // render an ordinary gigabit link as `0.93 Gibit/s`.
    expect(formatBitsPerSecond(1_000_000_000)).toBe('1.0 Gbit/s');
    expect(formatBitsPerSecond(2_500_000_000)).toBe('2.5 Gbit/s');
    expect(formatBitsPerSecond(10_000_000_000)).toBe('10.0 Gbit/s');
    expect(formatBitsPerSecond(100_000_000)).toBe('100.0 Mbit/s');
  });

  it('keeps a decimal where Wi-Fi rates need one', () => {
    // 866.7 Mbit/s and 867 Mbit/s are different negotiated rates.
    expect(formatBitsPerSecond(866_700_000)).toBe('866.7 Mbit/s');
    expect(formatBitsPerSecond(175_500_000)).toBe('175.5 Mbit/s');
    expect(formatBitsPerSecond(390_000_000)).toBe('390.0 Mbit/s');
  });

  it('shows whole bits and kilobits', () => {
    expect(formatBitsPerSecond(512)).toBe('512 bit/s');
    expect(formatBitsPerSecond(64_000)).toBe('64 Kbit/s');
  });

  it('refuses zero, because zero is not a link speed', () => {
    // Both platforms use it to mean "nothing negotiated", and the backend
    // already reports that as unavailable — so a zero arriving here is a
    // fault, and `0 bit/s` would claim a connection with no capacity.
    expect(formatBitsPerSecond(0)).toBe('—');
    expect(formatBitsPerSecond(-1)).toBe('—');
    expect(formatBitsPerSecond(Number.NaN)).toBe('—');
    expect(formatBitsPerSecond(Number.POSITIVE_INFINITY)).toBe('—');
  });

  it('is a different unit from observed traffic', () => {
    // The factor-of-eight trap: one gigabit per second is 125 MiB/s-ish, and
    // rendering both in one unit looks entirely plausible on screen.
    expect(formatBitsPerSecond(1_000_000_000)).toContain('bit/s');
    expect(formatThroughput(1_000_000_000)).toContain('iB/s');
    expect(formatBitsPerSecond(1_000_000_000)).not.toBe(formatThroughput(1_000_000_000));
  });
});

describe('formatPacketRate', () => {
  it('shows whole packets below a thousand', () => {
    expect(formatPacketRate(142)).toBe('142/s');
    expect(formatPacketRate(10.956)).toBe('11/s');
    expect(formatPacketRate(999)).toBe('999/s');
  });

  it('abbreviates above a thousand', () => {
    // `142,857/s` is harder to read at a glance than `142.9k/s`, and the extra
    // digits are not information on a figure that moves every refresh.
    expect(formatPacketRate(8400)).toBe('8.4k/s');
    expect(formatPacketRate(142_857)).toBe('142.9k/s');
    expect(formatPacketRate(2_500_000)).toBe('2.5M/s');
  });

  it('treats zero as a measurement', () => {
    expect(formatPacketRate(0)).toBe('0/s');
    expect(formatPacketRate(0)).not.toBe('—');
  });

  it('refuses a negative or non-finite rate', () => {
    expect(formatPacketRate(-1)).toBe('—');
    expect(formatPacketRate(Number.NaN)).toBe('—');
  });
});

describe('formatDbm', () => {
  it('shows whole decibels', () => {
    // A radio's reading moves by a decibel or two between samples anyway, so a
    // decimal shows precision the measurement does not have.
    expect(formatDbm(-68)).toBe('-68 dBm');
    expect(formatDbm(-68.4)).toBe('-68 dBm');
    expect(formatDbm(-45)).toBe('-45 dBm');
    expect(formatDbm(-90)).toBe('-90 dBm');
  });

  it('accepts the whole range a Wi-Fi radio reports', () => {
    expect(formatDbm(-30)).toBe('-30 dBm');
    expect(formatDbm(-100)).toBe('-100 dBm');
    expect(formatDbm(0)).toBe('0 dBm');
  });

  it('refuses a positive reading', () => {
    // 0 dBm is a milliwatt arriving at the antenna, which does not happen, so
    // anything above it is a misread rather than an extremely strong link.
    expect(formatDbm(10)).toBe('—');
    expect(formatDbm(Number.NaN)).toBe('—');
    expect(formatDbm(Number.NEGATIVE_INFINITY)).toBe('—');
  });

  it('matches a real measurement from the development machine', () => {
    expect(formatDbm(-70)).toBe('-70 dBm');
  });
});
