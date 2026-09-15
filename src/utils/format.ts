/** Small display helpers shared by the shell. Kept free of platform logic. */

/** Human label for a raw platform identifier coming from the backend. */
export function formatPlatformLabel(os: string, osVersion: string | null): string {
  const base =
    {
      linux: 'Linux',
      windows: 'Windows',
      macos: 'macOS',
    }[os] ?? os;

  return osVersion && osVersion.trim().length > 0 ? `${base} · ${osVersion}` : base;
}

/** `wayland` -> `Wayland`, `x11` -> `X11`, null -> null. */
export function formatDisplayServer(displayServer: string | null): string | null {
  if (!displayServer) return null;
  const normalized = displayServer.toLowerCase();
  if (normalized === 'x11') return 'X11';
  return normalized.charAt(0).toUpperCase() + normalized.slice(1);
}
