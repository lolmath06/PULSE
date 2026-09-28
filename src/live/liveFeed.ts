import { useEffect, useSyncExternalStore } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { MetricRef } from '@/types/metrics';
import { metricRefId } from '@/types/wellknown';
import { invokeCommand, isTauriRuntime } from '@/services/tauri';

/**
 * This window's side of the live widget feed.
 *
 * Every widget that shows live values *retains* its metric references here.
 * The feed keeps a reference count per metric and sends the backend **one**
 * subscription for the whole window — the union — so twenty CPU widgets are one
 * reference, and the backend unions the windows again. There is no timer in
 * this file and none in any widget: values arrive with the backend's
 * `live-sample` event, once per second, only while something is retained and
 * the window is visible.
 */

export const LIVE_EVENT = 'live-sample';
export const LIVE_CADENCE_MS = 1_000;
/** Five minutes at one point per second — the backend's ring size. */
export const LIVE_CAPACITY = 300;

export interface LivePoint {
  readonly t: number;
  readonly v: number | null;
}

interface LiveTick {
  readonly t: number;
  readonly values: readonly { readonly metric: MetricRef; readonly v: number | null }[];
}

interface Refused {
  readonly metric: MetricRef;
  readonly reason: string;
}

/** The backend, injectable for tests. */
export interface LiveBackend {
  setSubscription(metrics: readonly MetricRef[]): Promise<{ refused: readonly Refused[] }>;
  buffer(
    metrics: readonly MetricRef[],
  ): Promise<readonly { metric: MetricRef; points: LivePoint[] }[]>;
  onTick(listener: (tick: LiveTick) => void): () => void;
}

export function tauriLiveBackend(): LiveBackend {
  return {
    setSubscription: (metrics) =>
      invokeCommand<{ refused: Refused[] }>('set_live_subscription', { metrics }),
    buffer: (metrics) =>
      invokeCommand<{ metric: MetricRef; points: LivePoint[] }[]>('get_live_buffer', { metrics }),
    onTick(listener) {
      const pending = listen<LiveTick>(LIVE_EVENT, (event) => listener(event.payload));
      return () => void pending.then((unlisten) => unlisten()).catch(() => undefined);
    },
  };
}

let backend: LiveBackend | null = null;
const retained = new Map<string, { ref: MetricRef; count: number }>();
const rings = new Map<string, LivePoint[]>();
const refused = new Map<string, string>();
let version = 0;
let detachTick: (() => void) | null = null;
let syncQueued = false;
let lastSent = '';
const listeners = new Set<() => void>();

function current(): LiveBackend | null {
  if (!backend && isTauriRuntime()) backend = tauriLiveBackend();
  return backend;
}

function notify() {
  version += 1;
  for (const listener of [...listeners]) listener();
}

function visible(): boolean {
  return typeof document === 'undefined' || !document.hidden;
}

function onTick(tick: LiveTick) {
  let changed = false;
  for (const { metric, v } of tick.values) {
    const id = metricRefId(metric);
    if (!retained.has(id)) continue;
    const ring = rings.get(id) ?? [];
    ring.push({ t: tick.t, v });
    if (ring.length > LIVE_CAPACITY) ring.splice(0, ring.length - LIVE_CAPACITY);
    rings.set(id, ring);
    changed = true;
  }
  if (changed) notify();
}

/** Sends the union once per microtask burst, and only when it changed. */
function queueSync() {
  if (syncQueued) return;
  syncQueued = true;
  queueMicrotask(() => {
    syncQueued = false;
    const source = current();
    if (!source) return;
    const refs = visible() ? [...retained.values()].map((entry) => entry.ref) : [];
    const key = refs.map(metricRefId).sort().join('|');
    if (key === lastSent) return;
    lastSent = key;

    if (refs.length > 0 && !detachTick) detachTick = source.onTick(onTick);
    if (refs.length === 0 && detachTick) {
      detachTick();
      detachTick = null;
    }

    const missing = refs.filter((ref) => !rings.has(metricRefId(ref)));
    void source
      .setSubscription(refs)
      .then((result) => {
        refused.clear();
        for (const entry of result.refused) refused.set(metricRefId(entry.metric), entry.reason);
        notify();
      })
      .catch(() => undefined);
    if (missing.length > 0) {
      void source
        .buffer(missing)
        .then((series) => {
          for (const entry of series) {
            const id = metricRefId(entry.metric);
            if (!retained.has(id)) continue;
            const existing = rings.get(id) ?? [];
            const newest = existing[0]?.t ?? Infinity;
            rings.set(id, [...entry.points.filter((p) => p.t < newest), ...existing]);
          }
          notify();
        })
        .catch(() => undefined);
    }
  });
}

if (typeof document !== 'undefined') {
  // A hidden window (minimised main window, hidden Mini) unsubscribes; the
  // backend stops sampling what nobody sees. Overlays stay visible.
  document.addEventListener('visibilitychange', () => {
    lastSent = '\u0000';
    queueSync();
  });
}

/** Adds references for one widget. Returns the release function. */
export function retainLive(refs: readonly MetricRef[]): () => void {
  for (const ref of refs) {
    const id = metricRefId(ref);
    const entry = retained.get(id);
    if (entry) entry.count += 1;
    else retained.set(id, { ref, count: 1 });
  }
  queueSync();
  return () => {
    for (const ref of refs) {
      const id = metricRefId(ref);
      const entry = retained.get(id);
      if (!entry) continue;
      entry.count -= 1;
      if (entry.count <= 0) {
        retained.delete(id);
        rings.delete(id);
      }
    }
    queueSync();
  };
}

export interface LiveSeriesState {
  /** Points per reference id, oldest first; `v: null` is a gap. */
  readonly points: ReadonlyMap<string, readonly LivePoint[]>;
  /** Why a reference is not sampled live, per reference id. */
  readonly refused: ReadonlyMap<string, string>;
  readonly version: number;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

const getVersion = () => version;

/**
 * Live points for `refs`, retained while mounted and `enabled`.
 *
 * `refs` is keyed by content, so a new array with the same metrics does not
 * resubscribe.
 */
export function useLiveSeries(refs: readonly MetricRef[], enabled = true): LiveSeriesState {
  const key = JSON.stringify(refs.map((ref) => [ref.key, ref.sourceId]));
  useEffect(() => {
    if (!enabled) return;
    const parsed = (JSON.parse(key) as [string, string][]).map(([k, sourceId]) => ({
      key: k,
      sourceId,
    }));
    if (parsed.length === 0) return;
    return retainLive(parsed);
  }, [key, enabled]);

  const current = useSyncExternalStore(subscribe, getVersion, getVersion);
  const points = new Map<string, readonly LivePoint[]>();
  const refusedHere = new Map<string, string>();
  for (const ref of refs) {
    const id = metricRefId(ref);
    points.set(id, rings.get(id) ?? []);
    const reason = refused.get(id);
    if (reason) refusedHere.set(id, reason);
  }
  return { points, refused: refusedHere, version: current };
}

/** Diagnostics: how many distinct references this window subscribes to. */
export function liveSubscriptionSize(): number {
  return retained.size;
}

/** Test seams. */
export function setLiveBackendForTesting(next: LiveBackend | null) {
  detachTick?.();
  detachTick = null;
  backend = next;
  retained.clear();
  rings.clear();
  refused.clear();
  lastSent = '';
  version = 0;
}

export function deliverLiveTickForTesting(tick: LiveTick) {
  onTick(tick);
}
