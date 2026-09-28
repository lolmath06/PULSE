import { useSyncExternalStore } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { invokeCommand, isTauriRuntime } from '@/services/tauri';

/**
 * The shared UI configuration, as every window sees it.
 *
 * One document lives in the backend (`ui-config.json` in the app's config
 * directory). Each window holds a copy, writes whole **sections** back, and
 * receives every other window's writes as `ui-config-changed` events — so the
 * main window, the Mini window and every overlay always agree.
 *
 * Writes are debounced per section (a dragged slider is one write, not sixty)
 * and the backend coalesces again before touching the disk.
 *
 * The contents of a section are this layer's business only as opaque JSON:
 * each feature normalises its own section field by field when reading it.
 */

export type UiSection = 'visualization' | 'dashboards' | 'overlays' | 'templates' | 'settings';

export const UI_SECTIONS: readonly UiSection[] = [
  'visualization',
  'dashboards',
  'overlays',
  'templates',
  'settings',
];

export const UI_CONFIG_EVENT = 'ui-config-changed';

export type LoadOutcome =
  | { kind: 'fresh' }
  | { kind: 'loaded' }
  | { kind: 'recoveredFromBackup'; reason: string; quarantined: string | null }
  | { kind: 'reset'; reason: string; quarantined: string | null }
  | { kind: 'newerVersion'; found: number };

export interface UiConfigSnapshot {
  readonly revision: number;
  readonly document: Record<string, unknown>;
  readonly load: LoadOutcome;
  readonly readOnly: string | null;
  readonly path: string;
}

export interface UiConfigChange {
  readonly revision: number;
  readonly section: string;
  readonly origin: string;
  readonly value: unknown;
}

/** Where the document lives. The Tauri backend in the app; memory in tests. */
export interface UiConfigBackend {
  load(): Promise<UiConfigSnapshot>;
  save(section: UiSection, value: unknown): Promise<number>;
  subscribe(listener: (change: UiConfigChange) => void): () => void;
  /** This window's label, to ignore our own echoes. */
  origin(): string;
}

export function tauriBackend(): UiConfigBackend {
  return {
    load: () => invokeCommand<UiConfigSnapshot>('get_ui_config'),
    save: (section, value) => invokeCommand<number>('set_ui_config_section', { section, value }),
    subscribe(listener) {
      const pending = listen<UiConfigChange>(UI_CONFIG_EVENT, (event) => listener(event.payload));
      return () => void pending.then((unlisten) => unlisten()).catch(() => undefined);
    },
    origin: () => getCurrentWebviewWindow().label,
  };
}

/**
 * An in-memory backend. `saved` survives across `initUiConfig` calls, which
 * is how tests simulate closing and relaunching PULSE; `peer` simulates a
 * second window writing.
 */
export function memoryBackend(initial: Record<string, unknown> = {}) {
  let document: Record<string, unknown> = { version: 1, ...initial };
  let revision = 1;
  const listeners = new Set<(change: UiConfigChange) => void>();
  const backend: UiConfigBackend & {
    readonly saved: () => Record<string, unknown>;
    readonly peer: (section: UiSection, value: unknown) => void;
    saves: number;
  } = {
    saves: 0,
    load: () =>
      Promise.resolve({
        revision,
        document: structuredClone(document),
        load: { kind: 'loaded' } as LoadOutcome,
        readOnly: null,
        path: 'memory',
      }),
    save(section, value) {
      backend.saves += 1;
      document = { ...document, [section]: structuredClone(value) };
      revision += 1;
      for (const listener of [...listeners]) {
        listener({ revision, section, origin: 'self', value });
      }
      return Promise.resolve(revision);
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    origin: () => 'self',
    saved: () => structuredClone(document),
    peer(section, value) {
      document = { ...document, [section]: structuredClone(value) };
      revision += 1;
      for (const listener of [...listeners]) {
        listener({ revision, section, origin: 'other-window', value });
      }
    },
  };
  return backend;
}

/** How long a section waits for more changes before it is sent. */
export const SAVE_DEBOUNCE_MS = 150;

interface State {
  status: 'loading' | 'ready';
  document: Record<string, unknown>;
  load: LoadOutcome | null;
  readOnly: string | null;
  path: string | null;
  saveError: string | null;
}

let backend: UiConfigBackend | null = null;
let state: State = {
  status: 'loading',
  document: {},
  load: null,
  readOnly: null,
  path: null,
  saveError: null,
};
let unsubscribe: (() => void) | null = null;
const timers = new Map<UiSection, ReturnType<typeof setTimeout>>();
const listeners = new Set<() => void>();

function emit() {
  for (const listener of [...listeners]) listener();
}

function setState(next: Partial<State>) {
  state = { ...state, ...next };
  emit();
}

/** A hook that runs once after load, before the first render. */
type Migration = (read: typeof readSection, write: typeof writeSection) => void;
const migrations: Migration[] = [];

/** Registers a one-time migration, e.g. Phase 10's localStorage preferences. */
export function registerUiConfigMigration(migration: Migration) {
  migrations.push(migration);
}

/**
 * Loads the document. Called once per window before rendering.
 *
 * Outside Tauri (a plain browser, tests) it falls back to memory, so the UI
 * always renders.
 */
export async function initUiConfig(source?: UiConfigBackend): Promise<void> {
  unsubscribe?.();
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  backend = source ?? (isTauriRuntime() ? tauriBackend() : memoryBackend());

  let snapshot: UiConfigSnapshot;
  try {
    snapshot = await backend.load();
  } catch (error) {
    backend = memoryBackend();
    snapshot = await backend.load();
    state = { ...state, saveError: error instanceof Error ? error.message : String(error) };
  }

  state = {
    ...state,
    status: 'ready',
    document: snapshot.document,
    load: snapshot.load,
    readOnly: snapshot.readOnly,
    path: snapshot.path,
  };
  const own = backend.origin();
  unsubscribe = backend.subscribe((change) => {
    if (change.origin === own) return;
    if (!(UI_SECTIONS as readonly string[]).includes(change.section)) return;
    setState({ document: { ...state.document, [change.section]: change.value } });
  });

  for (const migration of migrations) migration(readSection, writeSection);
  emit();
}

export function readSection(section: UiSection): unknown {
  return state.document[section];
}

/**
 * Replaces a section, immediately for this window, and sends it to the
 * backend after {@link SAVE_DEBOUNCE_MS}.
 */
export function writeSection(section: UiSection, value: unknown) {
  setState({ document: { ...state.document, [section]: value } });
  const current = backend;
  if (!current) return;
  const previous = timers.get(section);
  if (previous) clearTimeout(previous);
  timers.set(
    section,
    setTimeout(() => {
      timers.delete(section);
      void current
        .save(section, state.document[section])
        .then(() => {
          if (state.saveError) setState({ saveError: null });
        })
        .catch((error: unknown) =>
          setState({ saveError: error instanceof Error ? error.message : String(error) }),
        );
    }, SAVE_DEBOUNCE_MS),
  );
}

/** Sends every pending section now. For tests and before closing a window. */
export async function flushUiConfig(): Promise<void> {
  const current = backend;
  if (!current) return;
  const pending = [...timers.keys()];
  for (const section of pending) {
    clearTimeout(timers.get(section));
    timers.delete(section);
    await current.save(section, state.document[section]);
  }
}

export function subscribeUiConfig(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getState() {
  return state;
}

/** The whole store state: status, load outcome, read-only reason, save error. */
export function useUiConfigStatus() {
  return useSyncExternalStore(subscribeUiConfig, getState, getState);
}

/**
 * One section, normalised by `normalize`. The normalised value is cached per
 * raw value, so components re-render only when *their* section changes.
 */
export function useUiSection<T>(section: UiSection, normalize: (raw: unknown) => T): T {
  const raw = useSyncExternalStore(
    subscribeUiConfig,
    () => state.document[section],
    () => state.document[section],
  );
  return cachedNormalize(section, raw, normalize);
}

const cache = new Map<UiSection, { raw: unknown; normalize: unknown; value: unknown }>();

function cachedNormalize<T>(section: UiSection, raw: unknown, normalize: (raw: unknown) => T): T {
  const hit = cache.get(section);
  if (hit && hit.raw === raw && hit.normalize === normalize) return hit.value as T;
  const value = normalize(raw);
  cache.set(section, { raw, normalize, value });
  return value;
}

/** Resets to an in-memory document synchronously. Tests only. */
export function resetUiConfigForTesting(document: Record<string, unknown> = {}) {
  unsubscribe?.();
  unsubscribe = null;
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  cache.clear();
  backend = null;
  state = {
    status: 'ready',
    document: { version: 1, ...document },
    load: { kind: 'loaded' },
    readOnly: null,
    path: 'memory',
    saveError: null,
  };
  emit();
}
