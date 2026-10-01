import type { Availability, MetricDefinition, MetricKey, SourceId } from '@/types/metrics';
import { hasKey, i18n } from '@/i18n/i18n';

/**
 * Presentation of the backend's metric catalog in the active language.
 *
 * The backend describes each metric in English (`displayName`,
 * `description`) next to its stable identity. Nothing here reads that prose
 * to infer meaning: names and descriptions are looked up by the **metric
 * key**, a machine identifier, and the backend's English is used only when
 * this PULSE has no translation for the key (a metric added by a newer
 * backend). Hardware names — `NVIDIA GeForce RTX 4070`, `nvme0n1` — are
 * shown exactly as the backend reports them.
 */

export function metricNameKey(key: MetricKey): string {
  return `metrics.catalog.${key}.name`;
}

export function metricDescriptionKey(key: MetricKey): string {
  return `metrics.catalog.${key}.description`;
}

/** A metric's name in the active language, or the backend's when unknown. */
export function metricName(definition: Pick<MetricDefinition, 'metric' | 'displayName'>): string {
  const key = metricNameKey(definition.metric.key);
  return hasKey(key) ? i18n.t(key) : definition.displayName;
}

/** A metric's description in the active language, or the backend's when unknown. */
export function metricDescription(
  definition: Pick<MetricDefinition, 'metric' | 'description'>,
): string {
  const key = metricDescriptionKey(definition.metric.key);
  return hasKey(key) ? i18n.t(key) : definition.description;
}

/** Logical (machine-wide) sources, whose label is PULSE's word, not hardware's. */
const LOGICAL_SOURCES: Readonly<Record<string, string>> = {
  'cpu:system': 'metrics.sources.cpuSystem',
  'memory:system': 'metrics.sources.memorySystem',
  'gpu:system': 'metrics.sources.gpuSystem',
  'process:system': 'metrics.sources.processSystem',
  'network:system': 'metrics.sources.networkSystem',
  'storage:system': 'metrics.sources.storageSystem',
};

/**
 * A source's label in the active language. Logical sources and processor
 * packages are PULSE's own words and are translated by their source id;
 * `CPU 7` is the name every tool uses and stays as is; a device keeps the
 * name the backend found for it.
 */
export function sourceLabel(definition: {
  readonly metric: { readonly sourceId: SourceId };
  readonly sourceLabel: string;
}): string {
  const sourceId = definition.metric.sourceId;
  const logical = LOGICAL_SOURCES[sourceId];
  if (logical) return i18n.t(logical);
  const packageMatch = /^cpu:package-(0|[1-9][0-9]*)$/.exec(sourceId);
  if (packageMatch) return i18n.t('metrics.sources.cpuPackage', { index: Number(packageMatch[1]) });
  return definition.sourceLabel;
}

/**
 * The words before an availability's reason, by status — `Not detected`,
 * `Permission required`. The backend's reason follows verbatim: it is the
 * technical detail (`no hwmon sensor exposes a package temperature`) that
 * makes the status actionable, and rewording it would lose information.
 */
export function availabilityLabel(availability: Availability): string {
  return i18n.t(`metrics.availability.${availability.status}`);
}

/** Every catalog key PULSE ships a translation for. */
export function isTranslatedMetric(key: MetricKey): boolean {
  return hasKey(metricNameKey(key));
}

/**
 * Lower-cased and stripped of diacritics, so `temperature` finds
 * `Température` and `Speicher` finds `speicher`. Pure Unicode folding: no
 * locale rules, so it behaves the same in every language.
 */
export function foldForSearch(text: string): string {
  return text
    .normalize('NFD')
    .replace(/\p{M}+/gu, '')
    .toLowerCase();
}

/**
 * Everything a catalog search matches a metric on: its name in the active
 * language, the backend's English name (so an English term keeps working in
 * any language), the source as shown and as reported, and the canonical key.
 */
export function metricSearchText(definition: MetricDefinition): string {
  return foldForSearch(
    [
      metricName(definition),
      definition.displayName,
      sourceLabel(definition),
      definition.sourceLabel,
      definition.metric.key,
    ].join(' '),
  );
}

/** Whether a metric matches a typed query; every word must match. */
export function matchesMetricQuery(definition: MetricDefinition, query: string): boolean {
  const words = foldForSearch(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const haystack = metricSearchText(definition);
  return words.every((word) => haystack.includes(word));
}
