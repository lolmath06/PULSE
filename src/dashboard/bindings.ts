import { useEffect, useState } from 'react';
import type { MetricDefinition, MetricRef } from '@/types/metrics';
import { invokeCommand, isTauriRuntime } from '@/services/tauri';
import { discoverNetworkInterfaces, isPrimaryKind, orderInterfaces } from '@/utils/network';
import type { WidgetBinding } from '@/dashboard/model';
import { keyLabel } from '@/dashboard/metricInfo';
import { bindingLabelText } from '@/dashboard/geometry';
import { t } from '@/i18n/i18n';

/**
 * Turning a saved binding into a concrete metric reference.
 *
 * # Privacy
 *
 * A binding never stores a raw device source id — those can be derived from a
 * MAC address or a drive serial. It stores the *persistable reference* the
 * backend computes (`get_source_refs`): logical sources as they are
 * (`cpu:system`, `cpu:logical-3`), device sources as the same digest the
 * history database uses. Resolution maps it back through the live catalog.
 *
 * # Automatic
 *
 * `auto` picks a reasonable source each time — the first readable GPU, the
 * first drive, a hardware network interface — and the UI says it is an
 * automatic choice, not a claim about which one is "the" disk or "the"
 * Internet link.
 *
 * # Missing sources
 *
 * A fixed source that no longer exists (an unplugged disk, a replaced NIC)
 * resolves to `Source unavailable`, with the current candidates so the user
 * can remap. Nothing crashes and the widget keeps its configuration.
 */

export interface SourceRefs {
  /** sourceId → persistable reference. */
  readonly toRef: ReadonlyMap<string, string>;
  /** persistable reference → sourceId. */
  readonly toSource: ReadonlyMap<string, string>;
}

export const EMPTY_SOURCE_REFS: SourceRefs = { toRef: new Map(), toSource: new Map() };

export function sourceRefsFrom(map: Readonly<Record<string, string>>): SourceRefs {
  const toRef = new Map(Object.entries(map));
  const toSource = new Map([...toRef.entries()].map(([source, ref]) => [ref, source]));
  return { toRef, toSource };
}

let shared: Promise<SourceRefs> | null = null;
let override: SourceRefs | null = null;

/** The persistable-reference map, fetched once per window. */
export function useSourceRefs(): SourceRefs {
  const [refs, setRefs] = useState<SourceRefs>(override ?? EMPTY_SOURCE_REFS);
  useEffect(() => {
    if (override || !isTauriRuntime()) return;
    let cancelled = false;
    shared ??= invokeCommand<Record<string, string>>('get_source_refs').then(sourceRefsFrom);
    shared
      .then((value) => {
        if (!cancelled) setRefs(value);
      })
      .catch(() => {
        shared = null;
      });
    return () => {
      cancelled = true;
    };
  }, []);
  return refs;
}

export function setSourceRefsForTesting(refs: SourceRefs | null) {
  override = refs;
  shared = null;
}

/**
 * The persistable form of a source. Outside the app (no map yet), logical
 * sources are kept and device sources are refused rather than stored raw.
 */
export function persistableRef(sourceId: string, refs: SourceRefs): string | null {
  const mapped = refs.toRef.get(sourceId);
  if (mapped) return mapped;
  const [kind, instance] = sourceId.split(':');
  if (instance === 'system' || kind === 'cpu' || kind === 'memory' || kind === 'process') {
    return sourceId;
  }
  return null;
}

export type ResolvedBinding =
  | {
      readonly ok: true;
      readonly binding: WidgetBinding;
      readonly ref: MetricRef;
      readonly definition: MetricDefinition;
      readonly auto: boolean;
      /** What the widget shows as this metric's name. */
      readonly label: string;
    }
  | {
      readonly ok: false;
      readonly binding: WidgetBinding;
      readonly reason: string;
      readonly label: string;
      /** Sources that exist now, to remap to. */
      readonly candidates: readonly MetricDefinition[];
    };

function available(definition: MetricDefinition): boolean {
  return definition.availability.status === 'available';
}

/** The automatic choice among `candidates` (all the same key). */
export function autoPick(
  key: string,
  candidates: readonly MetricDefinition[],
  catalog: readonly MetricDefinition[],
): MetricDefinition | undefined {
  if (candidates.length <= 1) return candidates[0];
  const readable = candidates.filter(available);
  const pool = readable.length > 0 ? readable : candidates;

  if (key.startsWith('network.')) {
    const interfaces = orderInterfaces(discoverNetworkInterfaces(catalog), () => false);
    for (const entry of interfaces) {
      if (!isPrimaryKind(entry.kind)) continue;
      const match = pool.find((definition) => definition.metric.sourceId === entry.sourceId);
      if (match) return match;
    }
  }
  // Deterministic otherwise: the catalog's own (sourceId) order.
  return [...pool].sort((a, b) => a.metric.sourceId.localeCompare(b.metric.sourceId))[0];
}

export function resolveBinding(
  binding: WidgetBinding,
  catalog: readonly MetricDefinition[],
  refs: SourceRefs,
): ResolvedBinding {
  const candidates = catalog.filter((definition) => definition.metric.key === binding.key);
  const baseLabel = bindingLabelText(binding) ?? keyLabel(binding.key, candidates[0]);

  if (candidates.length === 0) {
    return {
      ok: false,
      binding,
      reason: t('metrics.notReported'),
      label: baseLabel,
      candidates,
    };
  }

  if (binding.source.mode === 'fixed') {
    const wanted = binding.source.ref;
    const sourceId = refs.toSource.get(wanted) ?? wanted;
    const match = candidates.find((definition) => definition.metric.sourceId === sourceId);
    if (!match) {
      return {
        ok: false,
        binding,
        reason: t('metrics.sourceUnavailable'),
        label: baseLabel,
        candidates,
      };
    }
    return {
      ok: true,
      binding,
      ref: match.metric,
      definition: match,
      auto: false,
      label: baseLabel,
    };
  }

  const pick = autoPick(binding.key, candidates, catalog)!;
  return { ok: true, binding, ref: pick.metric, definition: pick, auto: true, label: baseLabel };
}

export function resolveBindings(
  bindings: readonly WidgetBinding[],
  catalog: readonly MetricDefinition[],
  refs: SourceRefs,
): ResolvedBinding[] {
  return bindings.map((binding) => resolveBinding(binding, catalog, refs));
}
