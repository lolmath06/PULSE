import type { Availability, MetricDefinition, MetricUnit } from '@/types/metrics';
import type { WidgetInstance } from '@/dashboard/model';
import { createWidget, findBlueprint } from '@/dashboard/library';

/** Shared fixtures for the dashboard and overlay tests. */

export const AVAILABLE: Availability = { status: 'available' };

export function definition(
  key: string,
  sourceId: string,
  options: { label?: string; unit?: MetricUnit; availability?: Availability } = {},
): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel: options.label ?? sourceId,
    displayName: key,
    description: '',
    category: 'cpu',
    unit:
      options.unit ??
      (key.includes('temperature')
        ? 'celsius'
        : key.includes('bytes')
          ? 'bytesPerSecond'
          : 'percent'),
    valueType: 'number',
    kind: 'gauge',
    availability: options.availability ?? AVAILABLE,
    providerId: 'mock',
  };
}

export const CATALOG: MetricDefinition[] = [
  definition('cpu.usage.total', 'cpu:system', { label: 'CPU' }),
  definition('cpu.usage.logical', 'cpu:logical-0', { label: 'CPU 0' }),
  definition('cpu.usage.logical', 'cpu:logical-1', { label: 'CPU 1' }),
  definition('cpu.temperature.package', 'cpu:package-0', { label: 'Package 0' }),
  definition('memory.usage.percent', 'memory:system', { label: 'Memory' }),
  definition('memory.used', 'memory:system', { label: 'Memory', unit: 'bytes' }),
  definition('gpu.usage.core', 'gpu:pci-0000-01-00-0', {
    label: 'RTX',
    availability: { status: 'unsupported', reason: 'libnvidia-ml.so.1 is not installed' },
  }),
  definition('gpu.temperature.core', 'gpu:pci-0000-01-00-0', {
    label: 'RTX',
    availability: { status: 'unsupported', reason: 'no sensor' },
  }),
  definition('storage.io.read.bytes_per_second', 'storage:serial-aaa', { label: 'SSD A' }),
  definition('storage.io.write.bytes_per_second', 'storage:serial-aaa', { label: 'SSD A' }),
  definition('storage.io.read.bytes_per_second', 'storage:serial-bbb', { label: 'SSD B' }),
  definition('storage.io.write.bytes_per_second', 'storage:serial-bbb', { label: 'SSD B' }),
  definition('storage.health.temperature', 'storage:serial-aaa', { label: 'SSD A' }),
  definition('network.receive.bytes_per_second', 'network:sys-veth', { label: 'veth0 · Virtual' }),
  definition('network.transmit.bytes_per_second', 'network:sys-veth', { label: 'veth0 · Virtual' }),
  definition('network.mtu', 'network:sys-veth', { label: 'veth0 · Virtual' }),
  definition('network.receive.bytes_per_second', 'network:mac-001122334455', {
    label: 'wlp3s0 · Wi-Fi',
  }),
  definition('network.transmit.bytes_per_second', 'network:mac-001122334455', {
    label: 'wlp3s0 · Wi-Fi',
  }),
  definition('network.mtu', 'network:mac-001122334455', { label: 'wlp3s0 · Wi-Fi' }),
  definition('process.count.total', 'process:system', { unit: 'count' }),
  definition('process.count.running', 'process:system', { unit: 'count' }),
  definition('process.thread.count.total', 'process:system', { unit: 'count' }),
];

/** What `get_source_refs` would answer for {@link CATALOG}. */
export const SOURCE_REFS: Record<string, string> = {
  'cpu:system': 'cpu:system',
  'cpu:logical-0': 'cpu:logical-0',
  'cpu:logical-1': 'cpu:logical-1',
  'cpu:package-0': 'cpu:package-0',
  'memory:system': 'memory:system',
  'gpu:pci-0000-01-00-0': 'gpu:1111111111111111',
  'storage:serial-aaa': 'storage:aaaaaaaaaaaaaaaa',
  'storage:serial-bbb': 'storage:bbbbbbbbbbbbbbbb',
  'network:sys-veth': 'network:cccccccccccccccc',
  'network:mac-001122334455': 'network:dddddddddddddddd',
  'process:system': 'process:system',
};

export function widgetFrom(
  blueprintId: string,
  patch: Partial<WidgetInstance> = {},
): WidgetInstance {
  return { ...createWidget(findBlueprint(blueprintId)!), ...patch };
}
