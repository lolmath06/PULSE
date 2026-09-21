//! The `storage.*` metrics: declarations shared by every platform.
//!
//! This module owns every storage metric key, unit, kind and user-facing
//! string PULSE ships. Platform backends supply **only raw numbers and
//! descriptors**; they never choose a key or a unit, which is what keeps
//! `storage.io.read.iops@storage:wwid-…` meaning the same thing whether it
//! came from `/proc/diskstats` on Fedora or `IOCTL_DISK_PERFORMANCE` on
//! Windows.
//!
//! # The catalog is sized by the machine
//!
//! For `D` physical devices and `V` volumes:
//!
//! ```text
//! 2     machine-wide     storage.device.count, storage.volume.count
//! 13D   per device       capacity, six I/O rates, six health values
//! 4V    per volume       total, used, available, usage percent
//! ```
//!
//! Nothing hardcodes `D` or `V`. A laptop with one NVMe drive, a workstation
//! with four disks and a VM with a single virtio device all run the same code,
//! and the tests derive the expected size from the catalog rather than
//! asserting a number.
//!
//! # Devices and volumes are two source kinds
//!
//! `storage:` names hardware, `volume:` names a filesystem. See
//! [`descriptor`] for why conflating them is the classic storage-monitor bug,
//! and `docs/metrics/storage.md` for the full model.
//!
//! # Unsupported metrics keep their definitions
//!
//! A SATA SSD declares `storage.health.percentage_used@<its source>` carrying
//! an `unsupported` availability and a reason, rather than being dropped. That
//! is what lets a dashboard built on a machine with NVMe health open on a
//! machine without it and explain itself — and start working again when the
//! user moves the widget to a drive that does report it.

pub mod descriptor;
pub mod health;
pub mod io;

use crate::metrics::model::{
    Availability, MetricCategory, MetricDefinition, MetricDefinitionBuilder, MetricKey, MetricKind,
    MetricRef, MetricUnit, ProviderId, SourceId,
};

pub use descriptor::{
    device_identity, device_number_volume_source_id, guid_volume_source_id, normalize_identifier,
    os_name_source_id, partition_volume_source_id, serial_source_id, system_source_id,
    wwid_source_id, IdentityStability, StorageBus, StorageCapabilities, StorageDeviceDescriptor,
    StorageIdentity, StorageVolumeDescriptor, VolumeCapabilities, VolumeIdentity,
};
pub use health::{
    counter_value, kelvin_to_celsius, parse_smart_log, CriticalWarning, NvmeHealth, SMART_LOG_LEN,
    SMART_LOG_PAGE_ID,
};
pub use io::{
    average_latency_ms, per_second, NeedsAnotherSample, StorageIo, StorageIoCounters,
    StorageIoRates, StorageIoSnapshot, StorageIoTracker, KERNEL_SECTOR_BYTES,
};

// --- machine-wide keys ----------------------------------------------------

/// `storage.device.count` — how many physical storage devices were
/// inventoried.
pub const DEVICE_COUNT: &str = "storage.device.count";
/// `storage.volume.count` — how many mounted filesystems were inventoried.
pub const VOLUME_COUNT: &str = "storage.volume.count";

// --- per-device keys ------------------------------------------------------

/// `storage.capacity.total` — the device's total addressable capacity, in
/// bytes.
pub const CAPACITY_TOTAL: &str = "storage.capacity.total";

/// `storage.io.read.bytes_per_second` — read throughput.
pub const IO_READ_BYTES: &str = "storage.io.read.bytes_per_second";
/// `storage.io.write.bytes_per_second` — write throughput.
pub const IO_WRITE_BYTES: &str = "storage.io.write.bytes_per_second";
/// `storage.io.read.iops` — read operations completing per second.
pub const IO_READ_IOPS: &str = "storage.io.read.iops";
/// `storage.io.write.iops` — write operations completing per second.
pub const IO_WRITE_IOPS: &str = "storage.io.write.iops";
/// `storage.io.read.latency` — mean time a read took, in milliseconds.
pub const IO_READ_LATENCY: &str = "storage.io.read.latency";
/// `storage.io.write.latency` — mean time a write took, in milliseconds.
pub const IO_WRITE_LATENCY: &str = "storage.io.write.latency";

/// `storage.health.temperature` — the controller's composite temperature, in
/// degrees Celsius.
pub const HEALTH_TEMPERATURE: &str = "storage.health.temperature";
/// `storage.health.percentage_used` — the controller's estimate of endurance
/// consumed, as a percentage. May legitimately exceed 100.
pub const HEALTH_PERCENTAGE_USED: &str = "storage.health.percentage_used";
/// `storage.health.available_spare` — remaining spare blocks, as a percentage
/// of the reserve the device shipped with. **Not free space.**
pub const HEALTH_AVAILABLE_SPARE: &str = "storage.health.available_spare";
/// `storage.health.power_on_hours` — hours the controller has been powered on.
pub const HEALTH_POWER_ON_HOURS: &str = "storage.health.power_on_hours";
/// `storage.health.unsafe_shutdowns` — shutdowns that lost power without
/// notice.
pub const HEALTH_UNSAFE_SHUTDOWNS: &str = "storage.health.unsafe_shutdowns";
/// `storage.health.media_errors` — media and data integrity errors the
/// controller detected.
pub const HEALTH_MEDIA_ERRORS: &str = "storage.health.media_errors";

/// The per-device keys describing **capacity**.
pub const CAPACITY_KEYS: &[&str] = &[CAPACITY_TOTAL];

/// The per-device keys describing **activity**, all of which need two samples.
pub const IO_KEYS: &[&str] = &[
    IO_READ_BYTES,
    IO_WRITE_BYTES,
    IO_READ_IOPS,
    IO_WRITE_IOPS,
    IO_READ_LATENCY,
    IO_WRITE_LATENCY,
];

/// The per-device keys describing **the controller's own health reporting**.
///
/// Kept apart from [`IO_KEYS`] because the two fail independently and for
/// entirely different reasons: a USB disk has perfectly good I/O counters and
/// no reachable SMART data, while a permissions problem can remove health
/// alone from an NVMe drive whose counters keep working.
pub const HEALTH_KEYS: &[&str] = &[
    HEALTH_TEMPERATURE,
    HEALTH_PERCENTAGE_USED,
    HEALTH_AVAILABLE_SPARE,
    HEALTH_POWER_ON_HOURS,
    HEALTH_UNSAFE_SHUTDOWNS,
    HEALTH_MEDIA_ERRORS,
];

/// Every per-device key, in declaration order.
pub const PER_DEVICE_KEYS: &[&str] = &[
    CAPACITY_TOTAL,
    IO_READ_BYTES,
    IO_WRITE_BYTES,
    IO_READ_IOPS,
    IO_WRITE_IOPS,
    IO_READ_LATENCY,
    IO_WRITE_LATENCY,
    HEALTH_TEMPERATURE,
    HEALTH_PERCENTAGE_USED,
    HEALTH_AVAILABLE_SPARE,
    HEALTH_POWER_ON_HOURS,
    HEALTH_UNSAFE_SHUTDOWNS,
    HEALTH_MEDIA_ERRORS,
];

// --- per-volume keys ------------------------------------------------------

/// `storage.volume.capacity.total` — the filesystem's total size, in bytes.
pub const VOLUME_CAPACITY_TOTAL: &str = "storage.volume.capacity.total";
/// `storage.volume.capacity.used` — space the filesystem reports as consumed,
/// in bytes. Defined as `total - free`; see [`VolumeUsage`].
pub const VOLUME_CAPACITY_USED: &str = "storage.volume.capacity.used";
/// `storage.volume.capacity.available` — space actually obtainable by this
/// user, in bytes. See [`VolumeUsage`].
pub const VOLUME_CAPACITY_AVAILABLE: &str = "storage.volume.capacity.available";
/// `storage.volume.usage.percent` — share of the filesystem consumed.
pub const VOLUME_USAGE_PERCENT: &str = "storage.volume.usage.percent";

/// Every per-volume key, in declaration order.
pub const PER_VOLUME_KEYS: &[&str] = &[
    VOLUME_CAPACITY_TOTAL,
    VOLUME_CAPACITY_USED,
    VOLUME_CAPACITY_AVAILABLE,
    VOLUME_USAGE_PERCENT,
];

// --- sources --------------------------------------------------------------

/// The machine-wide storage source.
///
/// A *logical* identifier meaning "this machine's storage taken together" —
/// stable by construction and identical on every platform, like `cpu:system`
/// and `gpu:system`. Every other `storage:` source identifies one physical
/// device; see [`descriptor`] for how those are derived.
pub const SOURCE: &str = "storage:system";

/// The user-facing label for `storage:system`.
const SYSTEM_LABEL: &str = "Storage";

fn key(name: &str) -> MetricKey {
    MetricKey::new(name).expect("well-known storage key must be valid")
}

fn system_source() -> SourceId {
    SourceId::new(SOURCE).expect("well-known storage source must be valid")
}

/// Builds the `storage.device.count` reference.
pub fn device_count_ref() -> MetricRef {
    MetricRef::new(key(DEVICE_COUNT), system_source())
}

/// Builds the `storage.volume.count` reference.
pub fn volume_count_ref() -> MetricRef {
    MetricRef::new(key(VOLUME_COUNT), system_source())
}

/// Builds a reference for any per-device or per-volume key.
pub fn storage_ref(name: &str, source: &SourceId) -> MetricRef {
    MetricRef::new(key(name), source.clone())
}

// --- the used/available convention ----------------------------------------

/// One filesystem's space accounting, in bytes.
///
/// # `used` and `available` do not add up to `total`, and that is correct
///
/// Unix filesystems reserve a slice of their blocks for the superuser, so that
/// a full disk still leaves root enough room to fix it. `statvfs` therefore
/// reports **two** different notions of emptiness:
///
/// ```text
/// f_bfree   blocks that hold no data at all
/// f_bavail  blocks an unprivileged process may actually allocate
/// ```
///
/// PULSE publishes them as:
///
/// ```text
/// used      = total - free        space that holds data
/// available = user-available      space this user can really write to
/// ```
///
/// So `used + available <= total`, with the reserve as the difference. This is
/// the same convention `df` uses, which matters: a user comparing PULSE
/// against `df` on the same filesystem must see the same numbers, and a
/// monitor that quietly reported `total - available` as "used" would show
/// several gigabytes of phantom consumption on every ext4 volume.
///
/// Windows has no such reserve, so both notions coincide there and the same
/// formula gives the same answer. **The meaning of each key is identical on
/// both platforms**, which is the requirement; only the gap between them
/// differs, and on Windows it is zero.
///
/// `usage_percent` is computed from `used / total`, consistently with `used`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeUsage {
    /// Total size of the filesystem, in bytes.
    pub total_bytes: u64,
    /// Blocks holding no data, in bytes. Includes the superuser reserve.
    pub free_bytes: u64,
    /// Blocks an unprivileged process may allocate, in bytes.
    pub available_bytes: u64,
}

impl VolumeUsage {
    /// Builds a usage from `statvfs`-style block counts.
    ///
    /// Every multiplication is checked: a filesystem reporting an implausible
    /// block count must not overflow into a small number that looks valid.
    /// `available` is clamped to `free`, because a filesystem cannot offer
    /// more to an unprivileged user than it has empty.
    pub fn from_blocks(
        block_size: u64,
        total_blocks: u64,
        free_blocks: u64,
        available_blocks: u64,
    ) -> Option<Self> {
        if block_size == 0 {
            return None;
        }

        let total_bytes = total_blocks.checked_mul(block_size)?;
        let free_bytes = free_blocks.checked_mul(block_size)?.min(total_bytes);
        let available_bytes = available_blocks.checked_mul(block_size)?.min(free_bytes);

        Some(Self {
            total_bytes,
            free_bytes,
            available_bytes,
        })
    }

    /// Space holding data: `total - free`.
    pub const fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.free_bytes)
    }

    /// Share of the filesystem holding data, 0–100.
    ///
    /// `None` for a zero-sized filesystem, where the question has no answer —
    /// and where dividing would produce `NaN`, which the metric contract
    /// refuses anyway.
    pub fn usage_percent(&self) -> Option<f64> {
        if self.total_bytes == 0 {
            return None;
        }

        let percent = (self.used_bytes() as f64 / self.total_bytes as f64) * 100.0;
        percent.is_finite().then(|| percent.clamp(0.0, 100.0))
    }

    /// The value for one per-volume metric key.
    pub fn value_for(&self, name: &str) -> Option<f64> {
        match name {
            VOLUME_CAPACITY_TOTAL => Some(self.total_bytes as f64),
            VOLUME_CAPACITY_USED => Some(self.used_bytes() as f64),
            VOLUME_CAPACITY_AVAILABLE => Some(self.available_bytes as f64),
            VOLUME_USAGE_PERCENT => self.usage_percent(),
            _ => None,
        }
    }
}

// --- telemetry ------------------------------------------------------------

/// Everything one device's backends produced for one refresh.
///
/// Assembled by a platform provider and resolved here, so both operating
/// systems publish the same number for the same key.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StorageTelemetry {
    /// Total capacity in bytes. Static, but carried here so a single
    /// `value_for` call answers every key.
    pub capacity_bytes: Option<u64>,
    /// The rates derived from this refresh's counters, or `None` when the
    /// device needs another sample.
    pub io: Option<StorageIoRates>,
    /// The device's standardised health values, when a backend could read
    /// them.
    pub health: Option<NvmeHealth>,
    /// A composite temperature obtained outside the health log — on Linux, the
    /// kernel's `nvme` hwmon node, which an unprivileged process can read even
    /// when the admin passthrough it would otherwise need is refused.
    ///
    /// Used only when [`StorageTelemetry::health`] carries none, so the
    /// controller's own figure always wins and the two can never disagree.
    pub temperature_celsius: Option<f64>,
}

impl StorageTelemetry {
    /// The value for one per-device metric key, or `None` when it was not
    /// measured.
    pub fn value_for(&self, name: &str) -> Option<f64> {
        match name {
            CAPACITY_TOTAL => self.capacity_bytes.map(|bytes| bytes as f64),

            IO_READ_BYTES => self.io.map(|io| io.read_bytes_per_second),
            IO_WRITE_BYTES => self.io.map(|io| io.write_bytes_per_second),
            IO_READ_IOPS => self.io.map(|io| io.read_iops),
            IO_WRITE_IOPS => self.io.map(|io| io.write_iops),
            // `and_then`, not `map`: an interval with no completed operation
            // has no latency, and must not fall back to zero.
            IO_READ_LATENCY => self.io.and_then(|io| io.read_latency_ms),
            IO_WRITE_LATENCY => self.io.and_then(|io| io.write_latency_ms),

            HEALTH_TEMPERATURE => self
                .health
                .and_then(|health| health.temperature_celsius)
                .or(self.temperature_celsius),
            HEALTH_PERCENTAGE_USED => self.health.and_then(|health| health.percentage_used),
            HEALTH_AVAILABLE_SPARE => self
                .health
                .and_then(|health| health.available_spare_percent),
            HEALTH_POWER_ON_HOURS => {
                counter_value(self.health.and_then(|health| health.power_on_hours))
            }
            HEALTH_UNSAFE_SHUTDOWNS => {
                counter_value(self.health.and_then(|health| health.unsafe_shutdowns))
            }
            HEALTH_MEDIA_ERRORS => {
                counter_value(self.health.and_then(|health| health.media_errors))
            }

            _ => None,
        }
    }
}

// --- declarations ---------------------------------------------------------

/// Declares every storage metric PULSE ships for a discovered inventory.
///
/// Both the Linux and the Windows storage provider call this, so their
/// declarations are identical apart from `providerId` and the availabilities
/// each platform genuinely discovered. A contract test asserts exactly that.
///
/// Sorted by metric reference, matching the order the engine's catalog holds,
/// so the output is deterministic for a given inventory regardless of the
/// order the platform enumerated it.
pub fn definitions(
    provider: &ProviderId,
    devices: &[StorageDeviceDescriptor],
    volumes: &[StorageVolumeDescriptor],
) -> Vec<MetricDefinition> {
    let mut definitions = Vec::with_capacity(
        2 + PER_DEVICE_KEYS.len() * devices.len() + PER_VOLUME_KEYS.len() * volumes.len(),
    );

    definitions.push(
        MetricDefinitionBuilder::new(
            device_count_ref(),
            provider.clone(),
            MetricCategory::Storage,
            MetricUnit::Count,
            // A discrete fact about the machine, not a reading that rises and
            // falls. `State` is not directly averageable, so history can never
            // produce "1.4 disks".
            MetricKind::State,
        )
        .source_label(SYSTEM_LABEL)
        .display_name("Storage devices")
        .description(
            "Number of physical storage devices PULSE inventoried. Partitions, \
             loop devices, RAM disks and device-mapper volumes are not counted as \
             devices of their own.",
        )
        .build(),
    );

    definitions.push(
        MetricDefinitionBuilder::new(
            volume_count_ref(),
            provider.clone(),
            MetricCategory::Storage,
            MetricUnit::Count,
            MetricKind::State,
        )
        .source_label(SYSTEM_LABEL)
        .display_name("Volumes")
        .description(
            "Number of mounted filesystems PULSE inventoried. A filesystem reachable \
             at several paths counts once; pseudo-filesystems such as proc and sysfs \
             are not counted.",
        )
        .build(),
    );

    for device in devices {
        definitions.extend(per_device_definitions(provider, device));
    }
    for volume in volumes {
        definitions.extend(per_volume_definitions(provider, volume));
    }

    definitions.sort_by(|left, right| left.metric.cmp(&right.metric));
    definitions
}

/// The thirteen metrics of one physical device.
fn per_device_definitions(
    provider: &ProviderId,
    device: &StorageDeviceDescriptor,
) -> Vec<MetricDefinition> {
    let label = device.summary();
    let capabilities = &device.capabilities;

    let build = |reference: MetricRef,
                 unit: MetricUnit,
                 kind: MetricKind,
                 display: &str,
                 description: &str,
                 availability: &Availability| {
        MetricDefinitionBuilder::new(
            reference,
            provider.clone(),
            MetricCategory::Storage,
            unit,
            kind,
        )
        .source_label(&label)
        .display_name(display)
        .description(description)
        .availability(availability.clone())
        .build()
    };

    let source = &device.source_id;

    vec![
        build(
            storage_ref(CAPACITY_TOTAL, source),
            MetricUnit::Bytes,
            // Installed capacity is a hardware fact, not a reading.
            MetricKind::State,
            "Capacity",
            "Total addressable capacity of the physical device. This is the whole \
             device, not the sum of the filesystems on it.",
            &capabilities.capacity_total,
        ),
        build(
            storage_ref(IO_READ_BYTES, source),
            MetricUnit::BytesPerSecond,
            MetricKind::Gauge,
            "Read",
            "Bytes read from the device per second, derived from the operating \
             system's cumulative counters between two samples.",
            &capabilities.io_read_bytes,
        ),
        build(
            storage_ref(IO_WRITE_BYTES, source),
            MetricUnit::BytesPerSecond,
            MetricKind::Gauge,
            "Write",
            "Bytes written to the device per second, derived from the operating \
             system's cumulative counters between two samples.",
            &capabilities.io_write_bytes,
        ),
        build(
            storage_ref(IO_READ_IOPS, source),
            MetricUnit::OperationsPerSecond,
            MetricKind::Gauge,
            "Read IOPS",
            "Read operations completing per second. Zero is a real measurement: it \
             means no read completed during the interval.",
            &capabilities.io_read_iops,
        ),
        build(
            storage_ref(IO_WRITE_IOPS, source),
            MetricUnit::OperationsPerSecond,
            MetricKind::Gauge,
            "Write IOPS",
            "Write operations completing per second. Zero is a real measurement: it \
             means no write completed during the interval.",
            &capabilities.io_write_iops,
        ),
        build(
            storage_ref(IO_READ_LATENCY, source),
            MetricUnit::Milliseconds,
            MetricKind::Gauge,
            "Read latency",
            "Mean time a read took, as the operating system observed it — queueing \
             included, and measured from the host rather than inside the device. An \
             interval in which no read completed has no latency at all, and reports \
             none rather than zero.",
            &capabilities.io_read_latency,
        ),
        build(
            storage_ref(IO_WRITE_LATENCY, source),
            MetricUnit::Milliseconds,
            MetricKind::Gauge,
            "Write latency",
            "Mean time a write took, as the operating system observed it. An interval \
             in which no write completed reports no value rather than zero.",
            &capabilities.io_write_latency,
        ),
        build(
            storage_ref(HEALTH_TEMPERATURE, source),
            MetricUnit::Celsius,
            MetricKind::Gauge,
            "Temperature",
            "The storage controller's composite temperature — a single figure the \
             device derives from its own sensors, not one individual sensor and not \
             any other component's reading.",
            &capabilities.health_temperature,
        ),
        build(
            storage_ref(HEALTH_PERCENTAGE_USED, source),
            MetricUnit::Percent,
            MetricKind::Gauge,
            "Used endurance",
            "The controller's estimate of how much of the device's rated write \
             endurance has been consumed. Reported exactly as the device states it, \
             including values above 100 % once the rating has been exceeded, and \
             never converted into a health score.",
            &capabilities.health_percentage_used,
        ),
        build(
            storage_ref(HEALTH_AVAILABLE_SPARE, source),
            MetricUnit::Percent,
            MetricKind::Gauge,
            "Available spare",
            "Share of the device's reserve of replacement blocks that remains. This \
             is not free space on the filesystem and not unallocated capacity: a \
             completely full drive normally still reports 100 % spare.",
            &capabilities.health_available_spare,
        ),
        build(
            storage_ref(HEALTH_POWER_ON_HOURS, source),
            MetricUnit::Hours,
            // Monotonic for the life of the device: a counter, not a gauge.
            MetricKind::Counter,
            "Power-on hours",
            "Hours the device's controller has been powered on, as it counts them.",
            &capabilities.health_power_on_hours,
        ),
        build(
            storage_ref(HEALTH_UNSAFE_SHUTDOWNS, source),
            MetricUnit::Count,
            MetricKind::Counter,
            "Unsafe shutdowns",
            "Times the device lost power without being told to shut down first.",
            &capabilities.health_unsafe_shutdowns,
        ),
        build(
            storage_ref(HEALTH_MEDIA_ERRORS, source),
            MetricUnit::Count,
            MetricKind::Counter,
            "Media errors",
            "Media and data integrity errors the controller has detected and \
             counted over the device's life.",
            &capabilities.health_media_errors,
        ),
    ]
}

/// The four metrics of one volume.
fn per_volume_definitions(
    provider: &ProviderId,
    volume: &StorageVolumeDescriptor,
) -> Vec<MetricDefinition> {
    let label = volume.display_name.clone();
    let capabilities = &volume.capabilities;

    let build = |reference: MetricRef,
                 unit: MetricUnit,
                 kind: MetricKind,
                 display: &str,
                 description: &str,
                 availability: &Availability| {
        MetricDefinitionBuilder::new(
            reference,
            provider.clone(),
            MetricCategory::Storage,
            unit,
            kind,
        )
        .source_label(&label)
        .display_name(display)
        .description(description)
        .availability(availability.clone())
        .build()
    };

    let source = &volume.source_id;

    vec![
        build(
            storage_ref(VOLUME_CAPACITY_TOTAL, source),
            MetricUnit::Bytes,
            // The size of a filesystem is a property of how it was made.
            MetricKind::State,
            "Volume size",
            "Total size of the filesystem. Several filesystems on one device do not \
             add up to the device's capacity, and a filesystem may span devices.",
            &capabilities.capacity_total,
        ),
        build(
            storage_ref(VOLUME_CAPACITY_USED, source),
            MetricUnit::Bytes,
            MetricKind::Gauge,
            "Used",
            "Space on the filesystem that holds data, defined as total minus free \
             blocks. Used and available do not sum to the total on filesystems that \
             reserve blocks for the superuser.",
            &capabilities.capacity_used,
        ),
        build(
            storage_ref(VOLUME_CAPACITY_AVAILABLE, source),
            MetricUnit::Bytes,
            MetricKind::Gauge,
            "Available",
            "Space this user can actually write to. Lower than the free space on \
             filesystems that keep a reserve for the superuser, which is deliberate: \
             the reserve is not available to you.",
            &capabilities.capacity_available,
        ),
        build(
            storage_ref(VOLUME_USAGE_PERCENT, source),
            MetricUnit::Percent,
            MetricKind::Gauge,
            "Volume usage",
            "Share of the filesystem that holds data, computed from used and total \
             bytes.",
            &capabilities.usage_percent,
        ),
    ]
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// A plausible NVMe SSD, identified by its EUI-64.
    pub fn nvme(eui: &str, model: &str) -> StorageDeviceDescriptor {
        StorageDeviceDescriptor {
            source_id: wwid_source_id(eui).expect("valid"),
            display_name: model.to_string(),
            os_name: "nvme0n1".to_string(),
            identity: StorageIdentity::Wwid(eui.to_string()),
            bus: StorageBus::Nvme,
            capacity_bytes: Some(2_048_408_248_320),
            rotational: Some(false),
            removable: Some(false),
            backend: "test",
            capabilities: StorageCapabilities::all_available(),
        }
    }

    /// A plausible filesystem on a partition of `device`.
    pub fn volume(
        device: &StorageDeviceDescriptor,
        partition: u32,
        mount: &str,
    ) -> StorageVolumeDescriptor {
        StorageVolumeDescriptor {
            source_id: partition_volume_source_id(&device.source_id, partition).expect("valid"),
            display_name: mount.to_string(),
            identity: VolumeIdentity::Partition {
                device: device.source_id.clone(),
                partition,
            },
            filesystem: Some("btrfs".to_string()),
            mount_points: vec![mount.to_string()],
            device: Some(device.source_id.clone()),
            read_only: false,
            backend: "test",
            capabilities: VolumeCapabilities::all_available(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::metrics::model::MetricValueType;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn provider() -> ProviderId {
        ProviderId::new("linux.storage").expect("valid")
    }

    // --- references -------------------------------------------------------

    #[test]
    fn the_machine_wide_references_are_valid_and_stable() {
        assert_eq!(
            device_count_ref().to_string(),
            "storage.device.count@storage:system"
        );
        assert_eq!(
            volume_count_ref().to_string(),
            "storage.volume.count@storage:system"
        );
    }

    #[test]
    fn per_device_references_carry_the_devices_own_source() {
        let device = nvme("eui.002538b331b36d03", "SAMSUNG MZVL22T0HBLB-00B00");

        assert_eq!(
            storage_ref(IO_READ_IOPS, &device.source_id).to_string(),
            "storage.io.read.iops@storage:wwid-eui.002538b331b36d03"
        );
        assert_eq!(
            storage_ref(HEALTH_PERCENTAGE_USED, &device.source_id).to_string(),
            "storage.health.percentage_used@storage:wwid-eui.002538b331b36d03"
        );
    }

    #[test]
    fn every_declared_key_is_a_valid_metric_key() {
        for name in PER_DEVICE_KEYS
            .iter()
            .chain(PER_VOLUME_KEYS)
            .chain(&[DEVICE_COUNT, VOLUME_COUNT])
        {
            assert!(MetricKey::new(*name).is_ok(), "'{name}' must be valid");
        }
    }

    #[test]
    fn the_key_groups_partition_the_per_device_set() {
        let mut grouped: Vec<&str> = CAPACITY_KEYS
            .iter()
            .chain(IO_KEYS)
            .chain(HEALTH_KEYS)
            .copied()
            .collect();
        let mut all: Vec<&str> = PER_DEVICE_KEYS.to_vec();

        grouped.sort_unstable();
        all.sort_unstable();

        assert_eq!(grouped, all);
        assert_eq!(PER_DEVICE_KEYS.len(), 13);
        assert_eq!(PER_VOLUME_KEYS.len(), 4);
    }

    // --- catalog shape ----------------------------------------------------

    #[test]
    fn the_catalog_grows_with_the_machine() {
        for devices in 0..=4_usize {
            for volumes in 0..=4_usize {
                let inventory: Vec<StorageDeviceDescriptor> = (0..devices)
                    .map(|index| nvme(&format!("eui.{index:016x}"), "Test SSD"))
                    .collect();
                let mounted: Vec<StorageVolumeDescriptor> = (0..volumes)
                    .map(|index| {
                        let parent = nvme(&format!("eui.{index:016x}"), "Test SSD");
                        volume(&parent, 1, &format!("/mnt/{index}"))
                    })
                    .collect();

                assert_eq!(
                    definitions(&provider(), &inventory, &mounted).len(),
                    2 + PER_DEVICE_KEYS.len() * devices + PER_VOLUME_KEYS.len() * volumes,
                    "wrong catalog size for {devices} devices and {volumes} volumes"
                );
            }
        }
    }

    #[test]
    fn a_machine_with_no_storage_still_declares_both_counts() {
        let definitions = definitions(&provider(), &[], &[]);

        assert_eq!(definitions.len(), 2);
        // Zero devices is a known fact, not a failure to detect.
        assert!(definitions.iter().all(|d| d.availability.is_available()));
    }

    #[test]
    fn the_declaration_order_is_deterministic() {
        let devices = vec![nvme("eui.bbbb", "Test SSD"), nvme("eui.aaaa", "Test SSD")];
        let forward: Vec<String> = definitions(&provider(), &devices, &[])
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();

        let mut sorted = forward.clone();
        sorted.sort();
        assert_eq!(forward, sorted);

        let mut reversed = devices.clone();
        reversed.reverse();
        let backward: Vec<String> = definitions(&provider(), &reversed, &[])
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();
        assert_eq!(forward, backward, "enumeration order must not matter");
    }

    #[test]
    fn the_types_match_the_documented_contract() {
        let device = nvme("eui.0001", "Test SSD");
        let mounted = volume(&device, 1, "/");
        let declared = definitions(&provider(), &[device], &[mounted]);

        let unit_of = |name: &str| {
            declared
                .iter()
                .find(|d| d.metric.key.as_str() == name)
                .map(|d| (d.unit, d.kind, d.value_type))
                .unwrap_or_else(|| panic!("'{name}' declared"))
        };

        assert_eq!(
            unit_of(CAPACITY_TOTAL),
            (
                MetricUnit::Bytes,
                MetricKind::State,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(IO_READ_BYTES),
            (
                MetricUnit::BytesPerSecond,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(IO_WRITE_IOPS),
            (
                MetricUnit::OperationsPerSecond,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(IO_READ_LATENCY),
            (
                MetricUnit::Milliseconds,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(HEALTH_TEMPERATURE),
            (
                MetricUnit::Celsius,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(HEALTH_PERCENTAGE_USED),
            (
                MetricUnit::Percent,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(HEALTH_POWER_ON_HOURS),
            (
                MetricUnit::Hours,
                MetricKind::Counter,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(HEALTH_MEDIA_ERRORS),
            (
                MetricUnit::Count,
                MetricKind::Counter,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(VOLUME_CAPACITY_TOTAL),
            (
                MetricUnit::Bytes,
                MetricKind::State,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(VOLUME_CAPACITY_USED),
            (
                MetricUnit::Bytes,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            unit_of(VOLUME_USAGE_PERCENT),
            (
                MetricUnit::Percent,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
    }

    #[test]
    fn a_device_with_no_health_backend_keeps_its_health_definitions() {
        // The point of the catalog contract: a SATA SSD's wear metric is
        // declared and explained, not removed.
        let mut device = nvme("eui.0002", "Some SATA SSD");
        device.bus = StorageBus::Ata;
        device.capabilities = StorageCapabilities::all_available().with_health(
            Availability::unsupported("PULSE has no ATA SMART backend yet"),
        );

        let declared = definitions(&provider(), &[device], &[]);

        for name in HEALTH_KEYS {
            let definition = declared
                .iter()
                .find(|d| d.metric.key.as_str() == *name)
                .unwrap_or_else(|| panic!("'{name}' must stay in the catalog"));

            assert_eq!(definition.availability.status_str(), "unsupported");
            // Still fully typed, so a dashboard can resolve it.
            assert!(!definition.description.is_empty());
        }

        // …while everything else on the same device still works.
        assert!(declared
            .iter()
            .filter(|d| IO_KEYS.contains(&d.metric.key.as_str()))
            .all(|d| d.availability.is_available()));
    }

    #[test]
    fn the_source_label_names_the_device_rather_than_its_identifier() {
        let device = nvme("eui.0003", "SAMSUNG MZVL22T0HBLB-00B00");
        let declared = definitions(&provider(), &[device], &[]);

        let label = &declared
            .iter()
            .find(|d| d.metric.key.as_str() == CAPACITY_TOTAL)
            .expect("declared")
            .source_label;

        assert_eq!(label, "SAMSUNG MZVL22T0HBLB-00B00 · NVMe");
    }

    // --- the used/available convention ------------------------------------

    #[test]
    fn used_is_total_minus_free_and_available_is_what_the_user_can_write() {
        // The real root filesystem of the development machine, in 4 KiB
        // blocks: 101875712 total, 42970402 free, 40917442 available.
        let usage =
            VolumeUsage::from_blocks(4096, 101_875_712, 42_970_402, 40_917_442).expect("plausible");

        assert_eq!(usage.total_bytes, 101_875_712 * 4096);
        assert_eq!(usage.available_bytes, 40_917_442 * 4096);
        assert_eq!(usage.used_bytes(), (101_875_712 - 42_970_402) * 4096);

        // The reserve is the gap, and it is not zero.
        assert!(usage.used_bytes() + usage.available_bytes < usage.total_bytes);
    }

    #[test]
    fn used_is_never_computed_from_available() {
        // Computing `total - available` would report the superuser reserve as
        // consumed — several gigabytes of phantom usage on every ext4 volume.
        let usage = VolumeUsage::from_blocks(4096, 1000, 500, 400).expect("valid");

        assert_eq!(usage.used_bytes(), 500 * 4096);
        assert_ne!(usage.used_bytes(), 600 * 4096);
    }

    #[test]
    fn a_filesystem_with_no_reserve_has_used_and_available_summing_to_total() {
        // Windows, and Unix filesystems configured without a reserve.
        let usage = VolumeUsage::from_blocks(4096, 1000, 250, 250).expect("valid");

        assert_eq!(
            usage.used_bytes() + usage.available_bytes,
            usage.total_bytes
        );
        assert_eq!(usage.usage_percent(), Some(75.0));
    }

    #[test]
    fn usage_percent_is_computed_from_used_consistently() {
        let usage = VolumeUsage::from_blocks(1, 100, 25, 10).expect("valid");
        assert_eq!(usage.usage_percent(), Some(75.0));

        let empty = VolumeUsage::from_blocks(1, 100, 100, 100).expect("valid");
        assert_eq!(empty.usage_percent(), Some(0.0));

        let full = VolumeUsage::from_blocks(1, 100, 0, 0).expect("valid");
        assert_eq!(full.usage_percent(), Some(100.0));
    }

    #[test]
    fn a_zero_sized_filesystem_has_no_usage_rather_than_a_nan() {
        let usage = VolumeUsage::from_blocks(4096, 0, 0, 0).expect("valid");

        assert_eq!(usage.total_bytes, 0);
        assert_eq!(usage.usage_percent(), None);
        assert_eq!(usage.value_for(VOLUME_USAGE_PERCENT), None);
        assert_eq!(usage.value_for(VOLUME_CAPACITY_TOTAL), Some(0.0));
    }

    #[test]
    fn implausible_block_counts_are_refused_rather_than_overflowing() {
        assert_eq!(VolumeUsage::from_blocks(0, 10, 5, 5), None, "no block size");
        assert_eq!(
            VolumeUsage::from_blocks(u64::MAX, u64::MAX, 0, 0),
            None,
            "overflow must not wrap into a small plausible number"
        );
    }

    #[test]
    fn free_and_available_are_clamped_into_the_filesystem() {
        // A filesystem cannot have more free blocks than it has blocks, nor
        // offer an unprivileged user more than is free.
        let usage = VolumeUsage::from_blocks(4096, 100, 500, 900).expect("valid");

        assert_eq!(usage.free_bytes, usage.total_bytes);
        assert_eq!(usage.available_bytes, usage.total_bytes);
        assert_eq!(usage.used_bytes(), 0);
    }

    // --- telemetry resolution ---------------------------------------------

    #[test]
    fn telemetry_resolves_every_per_device_key() {
        let telemetry = StorageTelemetry {
            capacity_bytes: Some(2 * 1024 * GIB),
            io: Some(StorageIoRates {
                read_bytes_per_second: 131_072_000.0,
                write_bytes_per_second: 18_874_368.0,
                read_iops: 940.0,
                write_iops: 120.0,
                read_latency_ms: Some(0.7),
                write_latency_ms: Some(1.2),
            }),
            health: Some(parse_smart_log(&health::fixtures::healthy()).expect("valid")),
            temperature_celsius: None,
        };

        assert_eq!(
            telemetry.value_for(CAPACITY_TOTAL),
            Some(2.0 * 1024.0 * GIB as f64)
        );
        assert_eq!(telemetry.value_for(IO_READ_BYTES), Some(131_072_000.0));
        assert_eq!(telemetry.value_for(IO_WRITE_IOPS), Some(120.0));
        assert_eq!(telemetry.value_for(IO_READ_LATENCY), Some(0.7));
        assert_eq!(telemetry.value_for(HEALTH_PERCENTAGE_USED), Some(3.0));
        assert_eq!(telemetry.value_for(HEALTH_POWER_ON_HOURS), Some(421.0));
        assert_eq!(telemetry.value_for(HEALTH_UNSAFE_SHUTDOWNS), Some(7.0));
        assert_eq!(telemetry.value_for(HEALTH_MEDIA_ERRORS), Some(0.0));

        // Not one of ours.
        assert_eq!(telemetry.value_for("storage.nonsense"), None);
    }

    #[test]
    fn a_device_awaiting_its_baseline_reports_no_io_values_at_all() {
        let telemetry = StorageTelemetry {
            capacity_bytes: Some(GIB),
            io: None,
            health: None,
            temperature_celsius: None,
        };

        for name in IO_KEYS {
            assert_eq!(
                telemetry.value_for(name),
                None,
                "'{name}' must not fabricate a value before a baseline exists"
            );
        }
        // Capacity is static and unaffected.
        assert_eq!(telemetry.value_for(CAPACITY_TOTAL), Some(GIB as f64));
    }

    #[test]
    fn an_idle_interval_publishes_zero_rates_but_no_latency() {
        let telemetry = StorageTelemetry {
            capacity_bytes: None,
            io: Some(StorageIoRates {
                read_bytes_per_second: 0.0,
                write_bytes_per_second: 0.0,
                read_iops: 0.0,
                write_iops: 0.0,
                read_latency_ms: None,
                write_latency_ms: None,
            }),
            health: None,
            temperature_celsius: None,
        };

        assert_eq!(telemetry.value_for(IO_READ_BYTES), Some(0.0));
        assert_eq!(telemetry.value_for(IO_READ_IOPS), Some(0.0));
        assert_eq!(telemetry.value_for(IO_WRITE_IOPS), Some(0.0));
        assert_eq!(telemetry.value_for(IO_READ_LATENCY), None);
        assert_eq!(telemetry.value_for(IO_WRITE_LATENCY), None);
    }

    #[test]
    fn a_hwmon_temperature_fills_in_when_the_health_log_is_unreachable() {
        // The Fedora case: the admin passthrough needs root, but the kernel's
        // own nvme hwmon node reports the same composite figure to anyone.
        let telemetry = StorageTelemetry {
            capacity_bytes: None,
            io: None,
            health: None,
            temperature_celsius: Some(52.85),
        };

        assert_eq!(telemetry.value_for(HEALTH_TEMPERATURE), Some(52.85));
        // …and nothing else is invented from it.
        assert_eq!(telemetry.value_for(HEALTH_PERCENTAGE_USED), None);
        assert_eq!(telemetry.value_for(HEALTH_AVAILABLE_SPARE), None);
    }

    #[test]
    fn the_controllers_own_temperature_wins_over_a_secondary_sensor() {
        let telemetry = StorageTelemetry {
            capacity_bytes: None,
            io: None,
            health: Some(parse_smart_log(&health::fixtures::healthy()).expect("valid")),
            temperature_celsius: Some(99.0),
        };

        assert_eq!(
            telemetry.value_for(HEALTH_TEMPERATURE),
            Some(316.0 - 273.15),
            "two sources for one sensor must not be able to disagree"
        );
    }

    #[test]
    fn no_metric_is_a_health_verdict() {
        // PULSE publishes measurements. There is deliberately no key that
        // scores a drive, and nothing derives one from percentage used.
        for name in PER_DEVICE_KEYS {
            assert!(!name.contains("score"), "'{name}' looks like a verdict");
            assert!(!name.contains("status"), "'{name}' looks like a verdict");
        }
    }
}
