import { invoke } from '@tauri-apps/api/core';

/**
 * True when the frontend runs inside the Tauri webview.
 *
 * PULSE's UI is also runnable in a plain browser (`pnpm dev`) for fast styling
 * work; in that case no backend is reachable and callers must degrade
 * gracefully rather than crash.
 */
export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export class BackendUnavailableError extends Error {
  constructor() {
    super('PULSE backend is not available (running outside the Tauri runtime).');
    this.name = 'BackendUnavailableError';
  }
}

/** Thin typed wrapper around `invoke` so commands are called in one place. */
export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauriRuntime()) {
    throw new BackendUnavailableError();
  }
  return invoke<T>(command, args);
}
