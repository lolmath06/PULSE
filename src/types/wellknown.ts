import type { MetricRef, SourceId } from '@/types/metrics';

/**
 * The metric references PULSE currently ships.
 *
 * Mirrors `src-tauri/src/metrics/wellknown/`. These strings are the stable
 * contract a saved dashboard will store, and they are identical on Fedora and
 * Windows — only the backing provider differs, which the UI never sees.
 *
 * A Rust contract test asserts both platforms declare exactly these.
 */

// --- metric keys ----------------------------------------------------------
// Keys are referenced on their own wherever a metric exists once per logical
// processor, because there the source is discovered at runtime rather than
// known in advance.

/** Aggregate CPU utilisation, 0–100 percent. */
export const CPU_USAGE_TOTAL_KEY = 'cpu.usage.total';
/** Utilisation of one logical processor, 0–100 percent. */
export const CPU_USAGE_LOGICAL_KEY = 'cpu.usage.logical';
/** Clock the OS currently reports for one logical processor, in hertz. */
export const CPU_FREQUENCY_CURRENT_KEY = 'cpu.frequency.current';
/** Maximum clock the platform reports for one logical processor, in hertz. */
export const CPU_FREQUENCY_MAX_KEY = 'cpu.frequency.max';
/** Number of logical processors (hardware threads). */
export const CPU_COUNT_LOGICAL_KEY = 'cpu.count.logical';
/** Number of physical execution cores. */
export const CPU_COUNT_PHYSICAL_KEY = 'cpu.count.physical';
/** Number of processor packages (sockets). */
export const CPU_COUNT_PACKAGE_KEY = 'cpu.count.package';
/** Temperature of one processor package, in degrees Celsius. */
export const CPU_TEMPERATURE_PACKAGE_KEY = 'cpu.temperature.package';

// --- sources --------------------------------------------------------------

/** The machine-wide CPU source. */
export const CPU_SYSTEM_SOURCE: SourceId = 'cpu:system';

/** Prefix of a logical processor's source instance: `cpu:logical-0`. */
const CPU_LOGICAL_PREFIX = 'cpu:logical-';

/**
 * Builds the canonical source of one logical processor, e.g. `cpu:logical-7`.
 *
 * Mirrors `metrics::wellknown::cpu::topology::LogicalId::source_id`.
 */
export function cpuLogicalSourceId(ordinal: number): SourceId {
  return `${CPU_LOGICAL_PREFIX}${ordinal}`;
}

/**
 * Recovers a logical processor's ordinal from its source identifier.
 *
 * Returns `null` for any other source, including `cpu:system`. Parsing is
 * strict — `cpu:logical-01` and `cpu:logical-1x` are rejected rather than
 * coerced — so a malformed identifier can never be silently folded onto a
 * real processor's row.
 */
export function cpuLogicalOrdinal(sourceId: SourceId): number | null {
  if (!sourceId.startsWith(CPU_LOGICAL_PREFIX)) return null;

  const suffix = sourceId.slice(CPU_LOGICAL_PREFIX.length);
  if (!/^(0|[1-9][0-9]*)$/.test(suffix)) return null;

  const ordinal = Number(suffix);
  return Number.isSafeInteger(ordinal) ? ordinal : null;
}

/** Prefix of a processor package's source instance: `cpu:package-0`. */
const CPU_PACKAGE_PREFIX = 'cpu:package-';

/**
 * Builds the canonical source of one processor package, e.g. `cpu:package-0`.
 *
 * Mirrors `metrics::wellknown::cpu::topology::PackageId::source_id`.
 */
export function cpuPackageSourceId(index: number): SourceId {
  return `${CPU_PACKAGE_PREFIX}${index}`;
}

/**
 * Recovers a package's index from its source identifier.
 *
 * Strict for the same reason as {@link cpuLogicalOrdinal}: a malformed
 * identifier must never be folded onto a real package's row.
 */
export function cpuPackageIndex(sourceId: SourceId): number | null {
  if (!sourceId.startsWith(CPU_PACKAGE_PREFIX)) return null;

  const suffix = sourceId.slice(CPU_PACKAGE_PREFIX.length);
  if (!/^(0|[1-9][0-9]*)$/.test(suffix)) return null;

  const index = Number(suffix);
  return Number.isSafeInteger(index) ? index : null;
}

/** Aggregate CPU utilisation, 0–100 percent. */
export const CPU_USAGE_TOTAL: MetricRef = {
  key: CPU_USAGE_TOTAL_KEY,
  sourceId: CPU_SYSTEM_SOURCE,
};

/** Number of logical processors this machine has. */
export const CPU_COUNT_LOGICAL: MetricRef = {
  key: CPU_COUNT_LOGICAL_KEY,
  sourceId: CPU_SYSTEM_SOURCE,
};

/** Number of physical cores this machine has. */
export const CPU_COUNT_PHYSICAL: MetricRef = {
  key: CPU_COUNT_PHYSICAL_KEY,
  sourceId: CPU_SYSTEM_SOURCE,
};

/** Number of processor packages this machine has. */
export const CPU_COUNT_PACKAGE: MetricRef = {
  key: CPU_COUNT_PACKAGE_KEY,
  sourceId: CPU_SYSTEM_SOURCE,
};

/** The machine-wide CPU metrics the CPU details card always requests. */
export const CPU_TOPOLOGY_METRICS: readonly MetricRef[] = [
  CPU_COUNT_PHYSICAL,
  CPU_COUNT_LOGICAL,
  CPU_COUNT_PACKAGE,
] as const;

/** Installed physical memory, in bytes. */
export const MEMORY_TOTAL: MetricRef = {
  key: 'memory.total',
  sourceId: 'memory:system',
};

/** Physical memory in use (`total - available`), in bytes. */
export const MEMORY_USED: MetricRef = {
  key: 'memory.used',
  sourceId: 'memory:system',
};

/** Physical memory obtainable without swapping, in bytes. */
export const MEMORY_AVAILABLE: MetricRef = {
  key: 'memory.available',
  sourceId: 'memory:system',
};

/** Share of physical memory in use, 0–100 percent. */
export const MEMORY_USAGE_PERCENT: MetricRef = {
  key: 'memory.usage.percent',
  sourceId: 'memory:system',
};

// --- GPU ------------------------------------------------------------------

/** Number of hardware graphics adapters inventoried. */
export const GPU_COUNT_KEY = 'gpu.count';
/** Share of a GPU's graphics engine in use, 0–100 percent. */
export const GPU_USAGE_CORE_KEY = 'gpu.usage.core';
/** Dedicated video memory installed on an adapter, in bytes. */
export const GPU_MEMORY_TOTAL_KEY = 'gpu.memory.total';
/** Dedicated video memory in use, in bytes. */
export const GPU_MEMORY_USED_KEY = 'gpu.memory.used';
/** Dedicated video memory still allocatable, in bytes. */
export const GPU_MEMORY_FREE_KEY = 'gpu.memory.free';
/** Share of dedicated video memory in use, 0–100 percent. */
export const GPU_MEMORY_USAGE_PERCENT_KEY = 'gpu.memory.usage.percent';
/** Current graphics clock, in hertz. */
export const GPU_FREQUENCY_CORE_KEY = 'gpu.frequency.core';
/** Current video memory clock, in hertz. */
export const GPU_FREQUENCY_MEMORY_KEY = 'gpu.frequency.memory';
/** GPU die temperature, in degrees Celsius. */
export const GPU_TEMPERATURE_CORE_KEY = 'gpu.temperature.core';
/** Hottest point on the GPU package, in degrees Celsius. */
export const GPU_TEMPERATURE_HOTSPOT_KEY = 'gpu.temperature.hotspot';
/** Video memory temperature, in degrees Celsius. */
export const GPU_TEMPERATURE_MEMORY_KEY = 'gpu.temperature.memory';
/** Fan speed, in revolutions per minute. */
export const GPU_FAN_SPEED_KEY = 'gpu.fan.speed';

/** The machine-wide GPU source. */
export const GPU_SYSTEM_SOURCE: SourceId = 'gpu:system';

/**
 * The GPU metrics that describe **performance**: what the engine is doing and
 * how fast, as opposed to how hot it is.
 *
 * Kept apart from the thermal set because the two fail independently. An
 * open-source driver commonly exposes a temperature and no utilisation
 * counter, and telling such a user "GPU telemetry unavailable" would be wrong
 * on both counts — the GPU is detected, and one half of its telemetry works.
 */
export const GPU_PERFORMANCE_KEYS: readonly string[] = [
  GPU_USAGE_CORE_KEY,
  GPU_MEMORY_TOTAL_KEY,
  GPU_MEMORY_USED_KEY,
  GPU_MEMORY_FREE_KEY,
  GPU_MEMORY_USAGE_PERCENT_KEY,
  GPU_FREQUENCY_CORE_KEY,
  GPU_FREQUENCY_MEMORY_KEY,
] as const;

/** The GPU metrics that describe **temperature and cooling**. */
export const GPU_THERMAL_KEYS: readonly string[] = [
  GPU_TEMPERATURE_CORE_KEY,
  GPU_TEMPERATURE_HOTSPOT_KEY,
  GPU_TEMPERATURE_MEMORY_KEY,
  GPU_FAN_SPEED_KEY,
] as const;

/**
 * Every metric PULSE publishes per GPU.
 *
 * Mirrors `metrics::wellknown::gpu::PER_GPU_KEYS`. The frontend never assumes
 * how many GPUs exist — it discovers their sources from the catalog.
 */
export const GPU_PER_DEVICE_KEYS: readonly string[] = [
  ...GPU_PERFORMANCE_KEYS,
  ...GPU_THERMAL_KEYS,
] as const;

/** How many hardware GPUs this machine has. */
export const GPU_COUNT: MetricRef = {
  key: GPU_COUNT_KEY,
  sourceId: GPU_SYSTEM_SOURCE,
};

/**
 * Whether a source identifies one physical GPU.
 *
 * Every GPU source is `gpu:<instance>`; `gpu:system` is the machine-wide
 * aggregate and is deliberately excluded. Nothing here parses the instance —
 * it is a stable identifier whose internal shape (an NVML UUID, a PCI address,
 * a device-model tuple) is the backend's business, not the interface's.
 */
export function isGpuDeviceSource(sourceId: SourceId): boolean {
  return sourceId.startsWith('gpu:') && sourceId !== GPU_SYSTEM_SOURCE;
}

/** Everything the "Live system sample" card requests, in display order. */
export const LIVE_SAMPLE_METRICS: readonly MetricRef[] = [
  CPU_USAGE_TOTAL,
  MEMORY_TOTAL,
  MEMORY_USED,
  MEMORY_AVAILABLE,
  MEMORY_USAGE_PERCENT,
] as const;

/** Builds the `key@sourceId` string used to index a sample response. */
export function metricRefId(metric: MetricRef): string {
  return `${metric.key}@${metric.sourceId}`;
}

// --- storage --------------------------------------------------------------
//
// Mirrors `metrics::wellknown::storage`. Two source kinds, deliberately:
// `storage:` names a **physical device** and `volume:` names a **filesystem**.
// They are related and not interchangeable — a disk holds many filesystems, a
// filesystem can span disks, and a widget bound to one must never resolve to
// the other.

/** Number of physical storage devices inventoried. */
export const STORAGE_DEVICE_COUNT_KEY = 'storage.device.count';
/** Number of mounted filesystems inventoried. */
export const STORAGE_VOLUME_COUNT_KEY = 'storage.volume.count';

/** Total addressable capacity of a physical device, in bytes. */
export const STORAGE_CAPACITY_TOTAL_KEY = 'storage.capacity.total';

/** Bytes read from a device per second. */
export const STORAGE_IO_READ_BYTES_KEY = 'storage.io.read.bytes_per_second';
/** Bytes written to a device per second. */
export const STORAGE_IO_WRITE_BYTES_KEY = 'storage.io.write.bytes_per_second';
/** Read operations completing per second. */
export const STORAGE_IO_READ_IOPS_KEY = 'storage.io.read.iops';
/** Write operations completing per second. */
export const STORAGE_IO_WRITE_IOPS_KEY = 'storage.io.write.iops';
/** Mean time a read took, in milliseconds. */
export const STORAGE_IO_READ_LATENCY_KEY = 'storage.io.read.latency';
/** Mean time a write took, in milliseconds. */
export const STORAGE_IO_WRITE_LATENCY_KEY = 'storage.io.write.latency';

/** The controller's composite temperature, in degrees Celsius. */
export const STORAGE_HEALTH_TEMPERATURE_KEY = 'storage.health.temperature';
/** Estimated write endurance consumed, as a percentage. May exceed 100. */
export const STORAGE_HEALTH_PERCENTAGE_USED_KEY = 'storage.health.percentage_used';
/** Remaining reserve of replacement blocks, as a percentage. Not free space. */
export const STORAGE_HEALTH_AVAILABLE_SPARE_KEY = 'storage.health.available_spare';
/** Hours the controller has been powered on. */
export const STORAGE_HEALTH_POWER_ON_HOURS_KEY = 'storage.health.power_on_hours';
/** Shutdowns that lost power without notice. */
export const STORAGE_HEALTH_UNSAFE_SHUTDOWNS_KEY = 'storage.health.unsafe_shutdowns';
/** Media and data integrity errors the controller detected. */
export const STORAGE_HEALTH_MEDIA_ERRORS_KEY = 'storage.health.media_errors';

/** Total size of a filesystem, in bytes. */
export const STORAGE_VOLUME_CAPACITY_TOTAL_KEY = 'storage.volume.capacity.total';
/** Space on a filesystem that holds data (`total - free`), in bytes. */
export const STORAGE_VOLUME_CAPACITY_USED_KEY = 'storage.volume.capacity.used';
/** Space this user can actually write to, in bytes. */
export const STORAGE_VOLUME_CAPACITY_AVAILABLE_KEY = 'storage.volume.capacity.available';
/** Share of a filesystem that holds data, 0–100 percent. */
export const STORAGE_VOLUME_USAGE_PERCENT_KEY = 'storage.volume.usage.percent';

/** The machine-wide storage source. */
export const STORAGE_SYSTEM_SOURCE: SourceId = 'storage:system';

/**
 * The per-device metrics describing **activity**.
 *
 * All six need two samples to exist at all, which is why they are grouped:
 * before a baseline exists they are absent together, and the card says so once
 * rather than six times.
 */
export const STORAGE_IO_KEYS: readonly string[] = [
  STORAGE_IO_READ_BYTES_KEY,
  STORAGE_IO_WRITE_BYTES_KEY,
  STORAGE_IO_READ_IOPS_KEY,
  STORAGE_IO_WRITE_IOPS_KEY,
  STORAGE_IO_READ_LATENCY_KEY,
  STORAGE_IO_WRITE_LATENCY_KEY,
] as const;

/**
 * The per-device metrics describing **the controller's own health reporting**.
 *
 * Kept apart from the I/O set because the two fail for entirely different
 * reasons: a USB disk has working counters and unreachable SMART data, while a
 * permission problem removes health alone from an NVMe drive whose counters
 * keep working.
 */
export const STORAGE_HEALTH_KEYS: readonly string[] = [
  STORAGE_HEALTH_TEMPERATURE_KEY,
  STORAGE_HEALTH_PERCENTAGE_USED_KEY,
  STORAGE_HEALTH_AVAILABLE_SPARE_KEY,
  STORAGE_HEALTH_POWER_ON_HOURS_KEY,
  STORAGE_HEALTH_UNSAFE_SHUTDOWNS_KEY,
  STORAGE_HEALTH_MEDIA_ERRORS_KEY,
] as const;

/**
 * Every metric PULSE publishes per physical storage device.
 *
 * Mirrors `metrics::wellknown::storage::PER_DEVICE_KEYS`. The frontend never
 * assumes how many disks exist — it discovers their sources from the catalog.
 */
export const STORAGE_PER_DEVICE_KEYS: readonly string[] = [
  STORAGE_CAPACITY_TOTAL_KEY,
  ...STORAGE_IO_KEYS,
  ...STORAGE_HEALTH_KEYS,
] as const;

/** Every metric PULSE publishes per volume. */
export const STORAGE_PER_VOLUME_KEYS: readonly string[] = [
  STORAGE_VOLUME_CAPACITY_TOTAL_KEY,
  STORAGE_VOLUME_CAPACITY_USED_KEY,
  STORAGE_VOLUME_CAPACITY_AVAILABLE_KEY,
  STORAGE_VOLUME_USAGE_PERCENT_KEY,
] as const;

/** How many physical storage devices this machine has. */
export const STORAGE_DEVICE_COUNT: MetricRef = {
  key: STORAGE_DEVICE_COUNT_KEY,
  sourceId: STORAGE_SYSTEM_SOURCE,
};

/** How many mounted filesystems this machine has. */
export const STORAGE_VOLUME_COUNT: MetricRef = {
  key: STORAGE_VOLUME_COUNT_KEY,
  sourceId: STORAGE_SYSTEM_SOURCE,
};

/**
 * Whether a source identifies one physical storage device.
 *
 * Every device source is `storage:<instance>`; `storage:system` is the
 * machine-wide aggregate and is deliberately excluded. Nothing here parses the
 * instance — whether it encodes a WWN, a serial or a kernel name is the
 * backend's business, not the interface's.
 */
export function isStorageDeviceSource(sourceId: SourceId): boolean {
  return sourceId.startsWith('storage:') && sourceId !== STORAGE_SYSTEM_SOURCE;
}

/**
 * Whether a source identifies one filesystem.
 *
 * A separate kind from `storage:`, so a device and a volume can never be
 * confused for one another however their instances happen to be spelled.
 */
export function isStorageVolumeSource(sourceId: SourceId): boolean {
  return sourceId.startsWith('volume:');
}

// --- network --------------------------------------------------------------
//
// Mirrors `metrics::wellknown::network`. Two things to keep straight:
//
// - **Direction is from the machine's point of view.** `receive` is inbound
//   and renders as *Download*; `transmit` is outbound and renders as *Upload*.
// - **Traffic is in bytes, link capacity in bits.** A 1 Gbit/s link carrying
//   12 MiB/s is two different units, and mixing them is a factor-of-eight
//   error that looks entirely plausible.

/** Number of network interfaces PULSE monitors. Loopback is excluded. */
export const NETWORK_INTERFACE_COUNT_KEY = 'network.interface.count';
/** How many of those currently carry a link. */
export const NETWORK_INTERFACE_UP_COUNT_KEY = 'network.interface.up_count';

/** Bytes arriving at this machine per second. Rendered as **Download**. */
export const NETWORK_RECEIVE_BYTES_KEY = 'network.receive.bytes_per_second';
/** Bytes leaving this machine per second. Rendered as **Upload**. */
export const NETWORK_TRANSMIT_BYTES_KEY = 'network.transmit.bytes_per_second';
/** Packets arriving per second. */
export const NETWORK_RECEIVE_PACKETS_KEY = 'network.receive.packets_per_second';
/** Packets leaving per second. */
export const NETWORK_TRANSMIT_PACKETS_KEY = 'network.transmit.packets_per_second';
/** Malformed inbound frames rejected per second. */
export const NETWORK_RECEIVE_ERRORS_KEY = 'network.receive.errors_per_second';
/** Outbound frames that failed to transmit, per second. */
export const NETWORK_TRANSMIT_ERRORS_KEY = 'network.transmit.errors_per_second';
/** Intact inbound frames discarded per second. **Not Internet packet loss.** */
export const NETWORK_RECEIVE_DROPPED_KEY = 'network.receive.dropped_per_second';
/** Outbound frames discarded before transmission, per second. */
export const NETWORK_TRANSMIT_DROPPED_KEY = 'network.transmit.dropped_per_second';

/** The link's negotiated inbound capacity, in bits per second. */
export const NETWORK_LINK_RECEIVE_SPEED_KEY = 'network.link.receive_speed';
/** The link's negotiated outbound capacity, in bits per second. */
export const NETWORK_LINK_TRANSMIT_SPEED_KEY = 'network.link.transmit_speed';
/** The largest payload this interface carries in one frame, in bytes. */
export const NETWORK_MTU_KEY = 'network.mtu';

/** A 0–100 link quality figure, only where the platform computes one. */
export const NETWORK_WIFI_SIGNAL_QUALITY_KEY = 'network.wifi.signal.quality';
/** Received signal strength, in dBm. */
export const NETWORK_WIFI_SIGNAL_RSSI_KEY = 'network.wifi.signal.rssi';
/** Negotiated inbound Wi-Fi rate, in bits per second. */
export const NETWORK_WIFI_LINK_RECEIVE_RATE_KEY = 'network.wifi.link.receive_rate';
/** Negotiated outbound Wi-Fi rate, in bits per second. */
export const NETWORK_WIFI_LINK_TRANSMIT_RATE_KEY = 'network.wifi.link.transmit_rate';

/** The machine-wide network source. */
export const NETWORK_SYSTEM_SOURCE: SourceId = 'network:system';

/**
 * The per-interface metrics describing **traffic**.
 *
 * All eight need two samples to exist at all, which is why they are grouped:
 * before a baseline exists they are absent together, and the card says so once
 * rather than eight times.
 */
export const NETWORK_TRAFFIC_KEYS: readonly string[] = [
  NETWORK_RECEIVE_BYTES_KEY,
  NETWORK_TRANSMIT_BYTES_KEY,
  NETWORK_RECEIVE_PACKETS_KEY,
  NETWORK_TRANSMIT_PACKETS_KEY,
  NETWORK_RECEIVE_ERRORS_KEY,
  NETWORK_TRANSMIT_ERRORS_KEY,
  NETWORK_RECEIVE_DROPPED_KEY,
  NETWORK_TRANSMIT_DROPPED_KEY,
] as const;

/** The per-interface metrics describing the **link** itself. */
export const NETWORK_LINK_KEYS: readonly string[] = [
  NETWORK_LINK_RECEIVE_SPEED_KEY,
  NETWORK_LINK_TRANSMIT_SPEED_KEY,
  NETWORK_MTU_KEY,
] as const;

/**
 * Every metric PULSE publishes per interface.
 *
 * Mirrors `metrics::wellknown::network::PER_INTERFACE_KEYS`.
 */
export const NETWORK_PER_INTERFACE_KEYS: readonly string[] = [
  ...NETWORK_TRAFFIC_KEYS,
  ...NETWORK_LINK_KEYS,
] as const;

/**
 * Every metric PULSE publishes per Wi-Fi interface.
 *
 * Declared **only** on wireless interfaces: an RSSI on an Ethernet port is not
 * a missing measurement, there is no radio.
 */
export const NETWORK_PER_WIFI_KEYS: readonly string[] = [
  NETWORK_WIFI_SIGNAL_QUALITY_KEY,
  NETWORK_WIFI_SIGNAL_RSSI_KEY,
  NETWORK_WIFI_LINK_RECEIVE_RATE_KEY,
  NETWORK_WIFI_LINK_TRANSMIT_RATE_KEY,
] as const;

/** How many interfaces PULSE monitors. */
export const NETWORK_INTERFACE_COUNT: MetricRef = {
  key: NETWORK_INTERFACE_COUNT_KEY,
  sourceId: NETWORK_SYSTEM_SOURCE,
};

/** How many of them currently carry a link. */
export const NETWORK_INTERFACE_UP_COUNT: MetricRef = {
  key: NETWORK_INTERFACE_UP_COUNT_KEY,
  sourceId: NETWORK_SYSTEM_SOURCE,
};

/**
 * Whether a source identifies one network interface.
 *
 * Every interface source is `network:<instance>`; `network:system` is the
 * machine-wide aggregate and is deliberately excluded. Nothing here parses the
 * instance — whether it encodes a permanent MAC, a Windows GUID or an
 * interface name is the backend's business.
 */
export function isNetworkInterfaceSource(sourceId: SourceId): boolean {
  return sourceId.startsWith('network:') && sourceId !== NETWORK_SYSTEM_SOURCE;
}
