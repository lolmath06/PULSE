import type { PlatformInfo } from '@/types/platform';
import { invokeCommand } from '@/services/tauri';

/**
 * Single entry point for platform information.
 *
 * Every system-facing read goes through a Tauri command; the UI must never
 * touch /proc, /sys, WMI, PDH or NVML directly.
 */
export function getPlatformInfo(): Promise<PlatformInfo> {
  return invokeCommand<PlatformInfo>('get_platform_info');
}
