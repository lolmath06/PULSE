import { describe, expect, it } from 'vitest';
import { formatDisplayServer, formatPlatformLabel } from '@/utils/format';

describe('formatPlatformLabel', () => {
  it('prefers the reported distribution name', () => {
    expect(formatPlatformLabel('linux', 'Fedora Linux 39 (Workstation Edition)')).toBe(
      'Linux · Fedora Linux 39 (Workstation Edition)',
    );
  });

  it('falls back to the OS family when no version is reported', () => {
    expect(formatPlatformLabel('windows', null)).toBe('Windows');
    expect(formatPlatformLabel('linux', '   ')).toBe('Linux');
  });

  it('passes through unknown identifiers rather than hiding them', () => {
    expect(formatPlatformLabel('freebsd', null)).toBe('freebsd');
  });
});

describe('formatDisplayServer', () => {
  it('uses the conventional casing of each display server', () => {
    expect(formatDisplayServer('wayland')).toBe('Wayland');
    expect(formatDisplayServer('x11')).toBe('X11');
  });

  it('returns null when the backend reports none', () => {
    expect(formatDisplayServer(null)).toBeNull();
  });
});
