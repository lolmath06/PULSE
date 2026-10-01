import { primaryMonitor } from '@tauri-apps/api/window';
import { isTauriRuntime } from '@/services/tauri';
import type { ScreenSize } from '@/presets/overlayPacks';
import { FALLBACK_SCREEN } from '@/presets/overlayPacks';

/**
 * The primary monitor's size in logical pixels — where full-width bars and
 * full-height rails are sized. Falls back to 1920×1080 outside Tauri or when
 * the platform does not say.
 */
export async function primaryScreen(): Promise<ScreenSize> {
  if (!isTauriRuntime()) return FALLBACK_SCREEN;
  try {
    const monitor = await primaryMonitor();
    if (!monitor || monitor.scaleFactor <= 0) return FALLBACK_SCREEN;
    const width = Math.round(monitor.size.width / monitor.scaleFactor);
    const height = Math.round(monitor.size.height / monitor.scaleFactor);
    return width >= 320 && height >= 240 ? { width, height } : FALLBACK_SCREEN;
  } catch {
    return FALLBACK_SCREEN;
  }
}
