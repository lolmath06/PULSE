//! The Fedora storage provider.
//!
//! **One provider owns every storage metric on the machine**, whatever mix of
//! devices, buses and filesystems it holds:
//!
//! ```text
//! linux.storage
//!  ├── inventory   /sys/class/block          devices, identity, capacity, bus
//!  ├── volumes     /proc/self/mountinfo      filesystems and their mount points
//!  ├── usage       statvfs                   size, used, available
//!  ├── I/O         /proc/diskstats           one read for every device
//!  └── health      nvme hwmon + admin ioctl  composite temperature, SMART log
//! ```
//!
//! The volume layer is deliberately **not** a `linux.filesystem` provider of
//! its own, and the health layer is not a `storage.smart` one. Splitting them
//! would let two providers publish metrics about the same disk and the same
//! filesystem, and the engine — which rejects two providers claiming one
//! `MetricRef` — would refuse whichever registered second. Owning the whole
//! family in one provider is what lets the layers be merged before anything is
//! published.
//!
//! So on a machine with storage:
//!
//! ```text
//! Providers = 4        linux.cpu, linux.memory, linux.gpu, linux.storage
//! ```
//!
//! # Degradation
//!
//! Every layer is optional and fails alone. No `/proc/diskstats` leaves the
//! inventory and the volumes intact. No permission for the NVMe health log
//! leaves the temperature, the counters and the capacity working. No storage
//! at all leaves `storage.device.count` reporting zero, which is a fact rather
//! than a failure.
//!
//! # What a refresh costs
//!
//! ```text
//! 1       read       /proc/diskstats        every device's counters
//! 1       read       /proc/self/mountinfo   (inventory only, at startup)
//! V       syscalls   statvfs                one per volume
//! D_nvme  reads      hwmon temp input       one per NVMe device
//! D_nvme  ioctls     health log             one per NVMe device, when permitted
//! ```
//!
//! Identity, model, serial, capacity, bus and topology are read **once at
//! startup** and cached for the life of the process. They cannot change while
//! a device stays attached, and re-reading seven sysfs files per metric per
//! device on every refresh — the obvious shape, and the wrong one — would turn
//! a four-disk machine into a hundred file opens a click.

pub mod diskstats;
pub mod mountinfo;
pub mod nvme;
pub mod statvfs;
pub mod sysfs;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricRef, MetricSample, ProviderId, SourceId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::storage::{
    self as wellknown, NvmeHealth, StorageBus, StorageCapabilities, StorageDeviceDescriptor,
    StorageIo, StorageIoSnapshot, StorageIoTracker, StorageTelemetry, StorageVolumeDescriptor,
    VolumeCapabilities, VolumeIdentity, VolumeUsage,
};

/// Identifier of the Linux storage provider.
pub const PROVIDER_ID: &str = "linux.storage";

/// Where a device's health values come from.
#[derive(Debug, Clone)]
enum HealthSource {
    /// An NVMe controller: a hwmon composite temperature that any user can
    /// read, and a health log that needs privilege.
    Nvme {
        temperature_input: Option<PathBuf>,
        controller: Option<PathBuf>,
    },
    /// Nothing can be read, and this says why in terms that fit the device.
    None(Availability),
}

/// One device, with the backends that measure it.
#[derive(Debug)]
struct Device {
    descriptor: StorageDeviceDescriptor,
    /// The kernel name, used to find this device's line in `/proc/diskstats`.
    kernel_name: String,
    health: HealthSource,
}

/// One volume, with the path its usage is read through.
#[derive(Debug)]
struct Volume {
    descriptor: StorageVolumeDescriptor,
    /// The mount point `statvfs` is called on: the volume's primary one.
    probe_path: PathBuf,
}

/// What the provider discovered at startup.
struct StorageInventory {
    devices: Vec<Device>,
    volumes: Vec<Volume>,
}

impl std::fmt::Debug for StorageInventory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageInventory")
            .field("devices", &self.devices.len())
            .field("volumes", &self.volumes.len())
            .finish()
    }
}

impl StorageInventory {
    fn discover() -> Self {
        Self::discover_in(
            Path::new(sysfs::BLOCK_ROOT),
            &mountinfo::read(),
            &diskstats::read(),
        )
    }

    /// Builds the inventory from a sysfs root, a mount table and a parsed
    /// `diskstats`.
    ///
    /// Taking all three as parameters is what makes the whole correlation —
    /// device discovery, volume grouping, parent attribution — testable
    /// against fixture trees, on any host, without a particular disk being
    /// present.
    fn discover_in(
        block_root: &Path,
        mounts: &[mountinfo::MountEntry],
        stats: &BTreeMap<String, diskstats::DiskstatsEntry>,
    ) -> Self {
        let devices: Vec<Device> = sysfs::discover_in(block_root)
            .iter()
            .filter_map(|block| {
                let attributes = sysfs::read_attributes(block_root, &block.name);
                let (mut descriptor, _) = sysfs::describe(&attributes)?;

                let health = health_source(&descriptor, &block.path);
                descriptor.capabilities = capabilities_for(&descriptor, &health, stats);

                Some(Device {
                    descriptor,
                    kernel_name: block.name.clone(),
                    health,
                })
            })
            .collect();

        let volumes = group_volumes(mounts, &devices, stats);

        Self { devices, volumes }
    }

    /// Reads every device's counters in one pass.
    fn io_snapshot(&self, at: &Instant, origin: &Instant) -> StorageIoSnapshot {
        let stats = diskstats::read();
        let mut snapshot = StorageIoSnapshot::new(at.duration_since(*origin).as_millis() as u64);

        for device in &self.devices {
            if let Some(entry) = stats.get(&device.kernel_name) {
                snapshot.insert(device.descriptor.source_id.clone(), entry.counters);
            }
        }

        snapshot
    }
}

/// Decides where one device's health values can come from.
///
/// Only a genuine NVMe attachment is tried. A SATA disk needs an ATA SMART
/// backend PULSE does not have yet, and a USB bridge would have to pass those
/// commands through, which most do not — both are stated as the specific
/// reason rather than a generic "unavailable".
fn health_source(descriptor: &StorageDeviceDescriptor, block_path: &Path) -> HealthSource {
    if !descriptor.bus.may_expose_nvme_health() {
        return HealthSource::None(reason_for_bus(descriptor.bus));
    }

    HealthSource::Nvme {
        temperature_input: nvme::composite_temperature_input(&block_path.join("device")),
        controller: nvme::controller_path(&descriptor.os_name),
    }
}

/// Why a device on this bus reports no standardised health data.
fn reason_for_bus(bus: StorageBus) -> Availability {
    match bus {
        StorageBus::Ata | StorageBus::Scsi => Availability::unsupported(
            "PULSE reports only the standardised NVMe health log so far. ATA SMART \
             attributes are vendor-defined, and normalising them incorrectly would \
             publish confident but wrong figures about this drive.",
        ),
        StorageBus::Usb => Availability::unsupported(
            "this device is behind a USB bridge, which does not pass the drive's own \
             health commands through",
        ),
        StorageBus::Virtual => Availability::not_detected(
            "a paravirtualised disk has no controller of its own to report health",
        ),
        StorageBus::Mmc => Availability::unsupported(
            "PULSE has no MMC health backend yet; the device is inventoried and its \
             activity is measured",
        ),
        StorageBus::Nvme | StorageBus::Unknown => Availability::unsupported(
            "PULSE could not determine how this device attaches, so it does not \
             guess which health protocol to speak to it",
        ),
    }
}

/// Derives a device's capabilities from what its backends can actually answer.
///
/// Probed once at startup: which interfaces exist does not change while the
/// machine runs, even though the values behind them do. The one thing checked
/// per-metric rather than per-device is the health log's *permission*, which is
/// resolved lazily on the first read — see [`LinuxStorageProvider::telemetry_for`].
fn capabilities_for(
    descriptor: &StorageDeviceDescriptor,
    health: &HealthSource,
    stats: &BTreeMap<String, diskstats::DiskstatsEntry>,
) -> StorageCapabilities {
    let mut capabilities = StorageCapabilities::all_available();

    if descriptor.capacity_bytes.is_none() {
        capabilities.capacity_total =
            Availability::unsupported("this device reports no size in sysfs");
    }

    if !stats.contains_key(&descriptor.os_name) {
        capabilities = capabilities.with_io(Availability::not_detected(format!(
            "'{}' has no entry in /proc/diskstats, so the kernel keeps no I/O \
             counters for it",
            descriptor.os_name
        )));
    }

    match health {
        HealthSource::None(reason) => capabilities.with_health(reason.clone()),
        HealthSource::Nvme {
            temperature_input,
            controller,
        } => {
            // The controller's own log needs privilege PULSE does not ask for.
            // Whether it will be granted is not knowable without trying, so
            // the declaration is optimistic and the sample tells the truth.
            if controller.is_none() {
                capabilities = capabilities.with_health(Availability::not_detected(
                    "this NVMe namespace has no controller device PULSE could name",
                ));
            }

            if temperature_input.is_none() && controller.is_some() {
                capabilities.health_temperature = Availability::not_detected(
                    "the kernel publishes no composite temperature for this controller",
                );
            }

            capabilities
        }
    }
}

/// Groups mounts into volumes, one per filesystem.
///
/// # One filesystem, however many paths
///
/// Mounts are grouped by **device number**, which is the kernel's identity for
/// a mounted filesystem. Fedora's default layout mounts one btrfs filesystem
/// at both `/` and `/home`; a bind mount adds a third path to a filesystem
/// already listed. All of them are one volume with several mount points, and
/// counting them separately would report the machine's capacity two or three
/// times over.
///
/// Mount points are sorted shortest-first, so the primary one is the path a
/// user thinks of the volume as — `/` rather than `/home`, `/mnt/data` rather
/// than a bind mount buried under it.
fn group_volumes(
    mounts: &[mountinfo::MountEntry],
    devices: &[Device],
    stats: &BTreeMap<String, diskstats::DiskstatsEntry>,
) -> Vec<Volume> {
    let mut grouped: BTreeMap<(u32, u32), Vec<&mountinfo::MountEntry>> = BTreeMap::new();

    for mount in mounts.iter().filter(|mount| mount.is_user_storage()) {
        grouped
            .entry((mount.major, mount.minor))
            .or_default()
            .push(mount);
    }

    grouped
        .into_iter()
        .filter_map(|((major, minor), mut entries)| {
            entries.sort_by(|left, right| {
                left.mount_point
                    .len()
                    .cmp(&right.mount_point.len())
                    .then_with(|| left.mount_point.cmp(&right.mount_point))
            });

            let primary = entries.first()?;
            let (device, source_id, identity) = correlate(primary, devices, stats, major, minor)?;

            let mount_points: Vec<String> = entries
                .iter()
                .map(|entry| entry.mount_point.clone())
                .collect();

            Some(Volume {
                probe_path: PathBuf::from(&primary.mount_point),
                descriptor: StorageVolumeDescriptor {
                    source_id,
                    display_name: primary.mount_point.clone(),
                    identity,
                    filesystem: Some(primary.filesystem.clone()),
                    mount_points,
                    device,
                    read_only: primary.read_only,
                    backend: "mountinfo",
                    capabilities: VolumeCapabilities::all_available(),
                },
            })
        })
        .collect()
}

/// Works out which physical device a volume lives on, and derives its identity.
///
/// The correlation goes through `/proc/diskstats`: the mount's source names a
/// block device, that device's line gives its `major:minor`, and the disk whose
/// partitions include that minor is the parent. Nothing is inferred from names
/// — `nvme0n1p8` *looks* like a partition of `nvme0n1`, but a rule built on
/// that would misattribute any driver that names things differently.
///
/// A volume PULSE cannot correlate keeps a session-scoped identity built from
/// its device number and is shown in a separate section of the card. That is
/// the honest outcome: attributing it to the wrong disk would be worse than
/// admitting the link is unknown.
fn correlate(
    mount: &mountinfo::MountEntry,
    devices: &[Device],
    stats: &BTreeMap<String, diskstats::DiskstatsEntry>,
    major: u32,
    minor: u32,
) -> Option<(Option<SourceId>, SourceId, VolumeIdentity)> {
    let fallback = || {
        let source = wellknown::device_number_volume_source_id(major, minor)?;
        Some((None, source, VolumeIdentity::DeviceNumber { major, minor }))
    };

    // The filesystem's own device number is what identifies it. For most
    // filesystems it is the partition's; btrfs presents an anonymous one, so
    // the mount source is followed instead.
    let Some(block_name) = mount.block_device_name() else {
        return fallback();
    };
    let Some(entry) = stats.get(block_name) else {
        return fallback();
    };

    // The parent disk is the one whose diskstats entry shares this major and
    // whose name the partition's line is filed under. A whole-disk mount —
    // `mkfs` straight onto `/dev/sdb` — has the disk itself as its entry.
    let parent = devices.iter().find(|device| {
        stats
            .get(&device.kernel_name)
            .is_some_and(|disk| disk.major == entry.major && is_child_of(&entry.name, &disk.name))
    });

    let Some(parent) = parent else {
        return fallback();
    };

    // The partition ordinal comes from the minor numbers' offset within the
    // disk, which the kernel assigns contiguously.
    let partition = entry.minor.saturating_sub(
        stats
            .get(&parent.kernel_name)
            .map(|disk| disk.minor)
            .unwrap_or(0),
    );

    let source = wellknown::partition_volume_source_id(&parent.descriptor.source_id, partition)?;

    Some((
        Some(parent.descriptor.source_id.clone()),
        source,
        VolumeIdentity::Partition {
            device: parent.descriptor.source_id.clone(),
            partition,
        },
    ))
}

/// Whether `candidate` is `disk` itself or one of its partitions.
///
/// The kernel files a partition's `diskstats` line under a name that begins
/// with its disk's, which is a property of how block devices are registered
/// rather than a naming convention — `nvme0n1p8` under `nvme0n1`, `sda4` under
/// `sda`. The suffix is required to be numeric or `p`-prefixed so `sdaa` is not
/// taken for a partition of `sda`.
fn is_child_of(candidate: &str, disk: &str) -> bool {
    if candidate == disk {
        return true;
    }

    let Some(suffix) = candidate.strip_prefix(disk) else {
        return false;
    };

    let digits = suffix.strip_prefix('p').unwrap_or(suffix);

    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// Publishes every storage metric on Fedora.
///
/// Requires no elevated privileges to start or to report inventory, volumes
/// and I/O: `/sys/class/block`, `/proc/diskstats` and `/proc/self/mountinfo`
/// are world-readable, and `statvfs` works for any user. Only the NVMe health
/// log needs privilege, and its absence costs nothing else.
pub struct LinuxStorageProvider {
    id: ProviderId,
    inventory: StorageInventory,
    io: StorageIoTracker,
    /// The origin of the monotonic clock rates are measured against.
    ///
    /// A monotonic instant rather than a wall clock: an NTP correction between
    /// two refreshes must not turn into a throughput spike or a negative
    /// interval.
    origin: Instant,
}

impl std::fmt::Debug for LinuxStorageProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxStorageProvider")
            .field("id", &self.id)
            .field("inventory", &self.inventory)
            .finish()
    }
}

impl LinuxStorageProvider {
    pub fn new() -> Self {
        let origin = Instant::now();
        let inventory = StorageInventory::discover();
        let io = StorageIoTracker::new();

        // Prime the baseline at startup so the *second* request — typically
        // the user's first Refresh — already has a real interval to work
        // from. The first request still reports "waiting for another sample",
        // honestly, rather than a fabricated 0 B/s.
        io.prime(&inventory.io_snapshot(&Instant::now(), &origin));

        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
            inventory,
            io,
            origin,
        }
    }

    /// The devices this provider published its catalog from.
    pub fn devices(&self) -> Vec<StorageDeviceDescriptor> {
        self.inventory
            .devices
            .iter()
            .map(|device| device.descriptor.clone())
            .collect()
    }

    /// The volumes this provider published its catalog from.
    pub fn volumes(&self) -> Vec<StorageVolumeDescriptor> {
        self.inventory
            .volumes
            .iter()
            .map(|volume| volume.descriptor.clone())
            .collect()
    }

    /// Reads one device's health, when a backend can.
    fn health_for(
        &self,
        device: &Device,
    ) -> (Option<NvmeHealth>, Option<f64>, Option<MetricError>) {
        match &device.health {
            HealthSource::None(_) => (None, None, None),
            HealthSource::Nvme {
                temperature_input,
                controller,
            } => {
                let temperature = temperature_input
                    .as_deref()
                    .and_then(|input| nvme::read_temperature(input).ok());

                let Some(controller) = controller else {
                    return (None, temperature, None);
                };

                match nvme::read_health(controller) {
                    Ok(health) => (Some(health), temperature, None),
                    Err(error) => (None, temperature, Some(error)),
                }
            }
        }
    }
}

impl Default for LinuxStorageProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for LinuxStorageProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        Ok(wellknown::definitions(
            &self.id,
            &self.devices(),
            &self.volumes(),
        ))
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        // One `/proc/diskstats` read serves every device in this request,
        // however many of their metrics were asked for.
        let snapshot = self.inventory.io_snapshot(&Instant::now(), &self.origin);
        let rates = self.io.update(&snapshot)?;

        // Which devices and volumes this request actually touches, so an
        // unrelated disk is never woken and an unrelated volume never stat'ed.
        let mut telemetry: BTreeMap<&str, (StorageTelemetry, Option<MetricError>)> =
            BTreeMap::new();
        let mut usage: BTreeMap<&str, Result<VolumeUsage, MetricError>> = BTreeMap::new();

        for reference in requested {
            let source = reference.source_id.as_str();

            if let Some(device) = self.device_for(source) {
                telemetry.entry(source).or_insert_with(|| {
                    let (health, temperature, error) =
                        if wellknown::requests_health(requested, source) {
                            self.health_for(device)
                        } else {
                            (None, None, None)
                        };

                    (
                        StorageTelemetry {
                            capacity_bytes: device.descriptor.capacity_bytes,
                            io: match rates.get(&device.descriptor.source_id) {
                                Some(StorageIo::Ready(rates)) => Some(*rates),
                                _ => None,
                            },
                            health,
                            temperature_celsius: temperature,
                        },
                        error,
                    )
                });
            } else if let Some(volume) = self.volume_for(source) {
                usage
                    .entry(source)
                    .or_insert_with(|| statvfs::read(&volume.probe_path));
            }
        }

        Ok(requested
            .iter()
            .map(|reference| self.sample_one(reference, &telemetry, &usage, &rates))
            .collect())
    }
}

impl LinuxStorageProvider {
    fn device_for(&self, source: &str) -> Option<&Device> {
        self.inventory
            .devices
            .iter()
            .find(|device| device.descriptor.source_id.as_str() == source)
    }

    fn volume_for(&self, source: &str) -> Option<&Volume> {
        self.inventory
            .volumes
            .iter()
            .find(|volume| volume.descriptor.source_id.as_str() == source)
    }

    /// Answers one requested reference.
    fn sample_one(
        &self,
        reference: &MetricRef,
        telemetry: &BTreeMap<&str, (StorageTelemetry, Option<MetricError>)>,
        usage: &BTreeMap<&str, Result<VolumeUsage, MetricError>>,
        rates: &BTreeMap<SourceId, StorageIo>,
    ) -> MetricSample {
        let key = reference.key.as_str();

        match key {
            wellknown::DEVICE_COUNT => {
                return MetricSample::number(reference.clone(), self.inventory.devices.len() as f64)
            }
            wellknown::VOLUME_COUNT => {
                return MetricSample::number(reference.clone(), self.inventory.volumes.len() as f64)
            }
            _ => {}
        }

        let source = reference.source_id.as_str();

        if let Some(device) = self.device_for(source) {
            let Some((telemetry, health_error)) = telemetry.get(source) else {
                return MetricSample::unavailable(
                    reference.clone(),
                    Availability::temporarily_unavailable(
                        "this device was not read during this sample",
                    ),
                );
            };

            return match telemetry.value_for(key) {
                Some(value) => MetricSample::number(reference.clone(), value),
                // Never a fabricated zero. The reason is chosen from what
                // actually went wrong for *this* metric on *this* device.
                None => MetricSample::unavailable(
                    reference.clone(),
                    device_reason(device, key, health_error.as_ref(), rates),
                ),
            };
        }

        if let Some(volume) = self.volume_for(source) {
            return match usage.get(source) {
                Some(Ok(usage)) => match usage.value_for(key) {
                    Some(value) => MetricSample::number(reference.clone(), value),
                    None => MetricSample::unavailable(
                        reference.clone(),
                        Availability::not_detected(
                            "this filesystem reports no size, so it has no usage",
                        ),
                    ),
                },
                Some(Err(error)) => MetricSample::unavailable(
                    reference.clone(),
                    crate::metrics::wellknown::availability_for(error.clone()),
                ),
                None => MetricSample::unavailable(
                    reference.clone(),
                    Availability::temporarily_unavailable(format!(
                        "'{}' was not read during this sample",
                        volume.descriptor.display_name
                    )),
                ),
            };
        }

        MetricSample::unavailable(
            reference.clone(),
            Availability::not_registered(format!(
                "'{reference}' does not name a device or volume this provider inventoried"
            )),
        )
    }
}

/// Explains why one device could not answer one metric.
fn device_reason(
    device: &Device,
    key: &str,
    health_error: Option<&MetricError>,
    rates: &BTreeMap<SourceId, StorageIo>,
) -> Availability {
    if wellknown::IO_KEYS.contains(&key) {
        // A device with counters that simply has no interval yet, or whose
        // interval contained no completed operation, is not a device with a
        // problem — and the two say different things.
        return match rates.get(&device.descriptor.source_id) {
            Some(StorageIo::NeedsAnotherSample(reason)) => {
                Availability::temporarily_unavailable(reason.reason())
            }
            Some(StorageIo::Ready(_)) => Availability::temporarily_unavailable(
                "no operation of this kind completed during the interval, so there is \
                 no latency to report",
            ),
            None => device.descriptor.capabilities.io_read_bytes.clone(),
        };
    }

    if wellknown::HEALTH_KEYS.contains(&key) {
        if let Some(error) = health_error {
            return crate::metrics::wellknown::availability_for(error.clone());
        }
    }

    capability_for(&device.descriptor.capabilities, key)
}

/// The declared availability of one metric on one device.
fn capability_for(capabilities: &StorageCapabilities, key: &str) -> Availability {
    match key {
        wellknown::CAPACITY_TOTAL => capabilities.capacity_total.clone(),
        wellknown::IO_READ_BYTES => capabilities.io_read_bytes.clone(),
        wellknown::IO_WRITE_BYTES => capabilities.io_write_bytes.clone(),
        wellknown::IO_READ_IOPS => capabilities.io_read_iops.clone(),
        wellknown::IO_WRITE_IOPS => capabilities.io_write_iops.clone(),
        wellknown::IO_READ_LATENCY => capabilities.io_read_latency.clone(),
        wellknown::IO_WRITE_LATENCY => capabilities.io_write_latency.clone(),
        wellknown::HEALTH_TEMPERATURE => capabilities.health_temperature.clone(),
        wellknown::HEALTH_PERCENTAGE_USED => capabilities.health_percentage_used.clone(),
        wellknown::HEALTH_AVAILABLE_SPARE => capabilities.health_available_spare.clone(),
        wellknown::HEALTH_POWER_ON_HOURS => capabilities.health_power_on_hours.clone(),
        wellknown::HEALTH_UNSAFE_SHUTDOWNS => capabilities.health_unsafe_shutdowns.clone(),
        wellknown::HEALTH_MEDIA_ERRORS => capabilities.health_media_errors.clone(),
        other => Availability::not_registered(format!("'{other}' is not a storage metric")),
    }
}

/// Builds the Fedora storage provider.
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(LinuxStorageProvider::new())
}

// The fixture trees below model `/sys/class/block`, whose device entries are
// symlinks into the kernel's device chain. Creating one needs an unprivileged
// `symlink`, which Unix has and Windows does not, so these run wherever this
// Linux-only provider could actually run. The Windows cross-check harness
// still type checks every line of them.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::platform::linux::storage::sysfs::fixtures::BlockFixture;

    /// The development machine's real layout, as fixtures.
    fn real_stats() -> BTreeMap<String, diskstats::DiskstatsEntry> {
        diskstats::parse(diskstats::fixtures::REAL)
    }

    fn real_mounts() -> Vec<mountinfo::MountEntry> {
        mountinfo::parse(mountinfo::fixtures::REAL)
    }

    fn real_block_tree() -> BlockFixture {
        let fixture = BlockFixture::new("provider");

        fixture.disk_on(
            "nvme0n1",
            "pci0000:00/0000:00:0e.0/pci10000:e0/10000:e1:00.0/nvme/nvme0",
            &[
                ("size", "4000797360"),
                ("queue/rotational", "0"),
                ("removable", "0"),
                ("wwid", "eui.002538b331b36d03"),
                ("device/model", "SAMSUNG MZVL22T0HBLB-00B00"),
                ("device/serial", "S677NX0W"),
            ],
        );
        for ordinal in 1..=8 {
            fixture.partition(&format!("nvme0n1p{ordinal}"), ordinal, 1000);
        }

        fixture.disk_on(
            "sda",
            "pci0000:00/0000:00:14.0/usb2/2-4/2-4:1.0/host0/target0:0:0/0:0:0:0/block",
            &[
                ("size", "1953525168"),
                ("queue/rotational", "1"),
                (
                    "device/wwid",
                    r"t10.Intenso SCSI            2019131398AB5\0\0\0",
                ),
                ("device/vendor", "Intenso"),
                ("device/model", "SCSI"),
            ],
        );
        for ordinal in 1..=4 {
            fixture.partition(&format!("sda{ordinal}"), ordinal, 1000);
        }

        for index in 0..8 {
            fixture.disk(&format!("loop{index}"), &[("size", "8")]);
        }
        fixture.disk("zram0", &[("size", "16")]);

        fixture
    }

    fn real_inventory(fixture: &BlockFixture) -> StorageInventory {
        StorageInventory::discover_in(fixture.root(), &real_mounts(), &real_stats())
    }

    // --- devices ----------------------------------------------------------

    #[test]
    fn the_inventory_counts_disks_not_partitions_or_pseudo_devices() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let names: Vec<&str> = inventory
            .devices
            .iter()
            .map(|device| device.kernel_name.as_str())
            .collect();

        assert_eq!(names, ["nvme0n1", "sda"], "two disks, twelve partitions");
    }

    #[test]
    fn each_device_gets_a_hardware_identity() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let sources: Vec<&str> = inventory
            .devices
            .iter()
            .map(|device| device.descriptor.source_id.as_str())
            .collect();

        assert_eq!(
            sources,
            [
                "storage:wwid-eui.002538b331b36d03",
                "storage:wwid-t10.intenso-scsi-2019131398ab5"
            ]
        );
    }

    #[test]
    fn a_device_with_no_diskstats_entry_declares_its_io_as_undetected() {
        let fixture = BlockFixture::new("no-stats");
        fixture.disk("sdz", &[("size", "1000"), ("device/serial", "X1")]);

        let inventory = StorageInventory::discover_in(fixture.root(), &[], &BTreeMap::new());

        let capabilities = &inventory.devices[0].descriptor.capabilities;
        assert_eq!(capabilities.io_read_bytes.status_str(), "notDetected");
        // …while the capacity it does report is unaffected.
        assert!(capabilities.capacity_total.is_available());
    }

    // --- health -----------------------------------------------------------

    #[test]
    fn only_an_nvme_device_is_asked_for_a_health_log() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let nvme_device = &inventory.devices[0];
        let usb_device = &inventory.devices[1];

        assert!(matches!(nvme_device.health, HealthSource::Nvme { .. }));
        assert!(matches!(usb_device.health, HealthSource::None(_)));
    }

    #[test]
    fn a_usb_disk_explains_its_missing_health_without_disappearing() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let usb = &inventory.devices[1].descriptor;

        assert_eq!(usb.bus, StorageBus::Usb);
        assert_eq!(
            usb.capabilities.health_percentage_used.status_str(),
            "unsupported"
        );
        // The device is still fully present and measurable.
        assert!(usb.capabilities.capacity_total.is_available());
        assert!(usb.capabilities.io_read_iops.is_available());
        assert_eq!(usb.capacity_bytes, Some(1_000_204_886_016));
    }

    #[test]
    fn a_sata_device_says_ata_smart_is_not_implemented_rather_than_absent() {
        // The user must be able to tell "PULSE does not do this yet" from
        // "your drive does not report it".
        let reason = reason_for_bus(StorageBus::Ata);

        assert_eq!(reason.status_str(), "unsupported");
        let Availability::Unsupported { reason } = reason else {
            panic!("expected unsupported");
        };
        assert!(reason.contains("ATA SMART"), "{reason}");
    }

    #[test]
    fn every_bus_has_its_own_explanation() {
        let mut reasons: Vec<String> = StorageBus::ALL
            .iter()
            .map(|bus| format!("{:?}", reason_for_bus(*bus)))
            .collect();
        reasons.sort();
        reasons.dedup();

        // Two pairs share a sentence on purpose: NVMe and Unknown ("PULSE
        // could not work out how to talk to this device"), and ATA and SCSI,
        // which are the same missing backend. Everything else is specific.
        assert_eq!(reasons.len(), StorageBus::ALL.len() - 2);
        assert!(
            reasons.len() >= 5,
            "a generic catch-all would collapse these"
        );
    }

    // --- volumes ----------------------------------------------------------

    #[test]
    fn one_filesystem_mounted_twice_is_one_volume_with_two_mount_points() {
        // Fedora's default btrfs layout. Counting `/` and `/home` separately
        // would report the machine's capacity twice.
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let root = inventory
            .volumes
            .iter()
            .find(|volume| volume.descriptor.mount_points.contains(&"/".to_string()))
            .expect("the root filesystem is a volume");

        assert_eq!(root.descriptor.mount_points, ["/", "/home"]);
        assert_eq!(root.descriptor.display_name, "/");
        assert_eq!(root.probe_path, Path::new("/"));
        assert_eq!(root.descriptor.filesystem.as_deref(), Some("btrfs"));
    }

    #[test]
    fn a_bind_mount_does_not_create_a_second_volume() {
        let mounts = mountinfo::parse(
            "40 1 259:8 / /srv/data rw,relatime - ext4 /dev/nvme0n1p8 rw\n\
             41 1 259:8 /projects /srv/data/projects rw,relatime - ext4 /dev/nvme0n1p8 rw\n\
             42 1 259:7 / /boot rw,relatime - ext4 /dev/nvme0n1p7 rw",
        );

        let fixture = real_block_tree();
        let inventory = StorageInventory::discover_in(fixture.root(), &mounts, &real_stats());

        assert_eq!(inventory.volumes.len(), 2, "one bind mount is not a volume");

        let data = inventory
            .volumes
            .iter()
            .find(|volume| volume.descriptor.display_name == "/srv/data")
            .expect("declared once");
        assert_eq!(
            data.descriptor.mount_points,
            ["/srv/data", "/srv/data/projects"]
        );
    }

    #[test]
    fn the_real_machine_yields_the_expected_volume_set() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let mut primary: Vec<&str> = inventory
            .volumes
            .iter()
            .map(|volume| volume.descriptor.display_name.as_str())
            .collect();
        primary.sort_unstable();

        assert_eq!(
            primary,
            [
                "/",
                "/boot",
                "/boot/efi",
                "/mnt/PROMETHEUS_DATA",
                "/mnt/PROMETHEUS_DEV",
                "/run/media/matheo/PROM_RESCUE",
            ],
            "six filesystems: tmpfs, proc, sysfs, squashfs and overlay are filtered"
        );
    }

    #[test]
    fn a_volume_is_attributed_to_the_disk_it_actually_lives_on() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let nvme = "storage:wwid-eui.002538b331b36d03";
        let usb = "storage:wwid-t10.intenso-scsi-2019131398ab5";

        let parent_of = |mount: &str| -> Option<String> {
            inventory
                .volumes
                .iter()
                .find(|volume| volume.descriptor.display_name == mount)
                .and_then(|volume| volume.descriptor.device.clone())
                .map(|source| source.as_str().to_string())
        };

        assert_eq!(parent_of("/").as_deref(), Some(nvme));
        assert_eq!(parent_of("/boot").as_deref(), Some(nvme));
        assert_eq!(parent_of("/boot/efi").as_deref(), Some(nvme));
        assert_eq!(parent_of("/mnt/PROMETHEUS_DATA").as_deref(), Some(usb));
        assert_eq!(parent_of("/mnt/PROMETHEUS_DEV").as_deref(), Some(usb));
    }

    #[test]
    fn a_volume_source_is_derived_from_its_parents_identity() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let boot = inventory
            .volumes
            .iter()
            .find(|volume| volume.descriptor.display_name == "/boot")
            .expect("declared");

        assert_eq!(
            boot.descriptor.source_id.as_str(),
            "volume:wwid-eui.002538b331b36d03-p7"
        );
        assert_eq!(
            boot.descriptor.identity.stability(),
            wellknown::IdentityStability::DerivedFromParent
        );
    }

    #[test]
    fn a_volume_with_no_correlatable_parent_keeps_an_honest_identity() {
        // An NFS export, an unlisted device-mapper volume: real, mounted, and
        // not attributable to any disk PULSE inventoried. Guessing a parent
        // would be worse than saying none.
        let mounts =
            mountinfo::parse("50 1 0:99 / /mnt/share rw,relatime - nfs4 server:/export rw");

        let fixture = real_block_tree();
        let inventory = StorageInventory::discover_in(fixture.root(), &mounts, &real_stats());

        let share = &inventory
            .volumes
            .iter()
            .find(|volume| volume.descriptor.display_name == "/mnt/share")
            .expect("declared")
            .descriptor;

        assert_eq!(share.device, None);
        assert_eq!(share.source_id.as_str(), "volume:mm-0-99");
        assert_eq!(
            share.identity.stability(),
            wellknown::IdentityStability::Session
        );
    }

    #[test]
    fn a_partition_ordinal_comes_from_the_minor_numbers_not_the_name() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let data = inventory
            .volumes
            .iter()
            .find(|volume| volume.descriptor.display_name == "/mnt/PROMETHEUS_DATA")
            .expect("declared");

        // sda has minor 0, sda4 has minor 4.
        assert_eq!(
            data.descriptor.source_id.as_str(),
            "volume:wwid-t10.intenso-scsi-2019131398ab5-p4"
        );
    }

    #[test]
    fn a_similarly_named_disk_is_not_taken_for_a_partition() {
        // `sdaa` starts with `sda`, and is a different disk.
        assert!(is_child_of("sda4", "sda"));
        assert!(is_child_of("nvme0n1p8", "nvme0n1"));
        assert!(is_child_of("sda", "sda"));
        assert!(!is_child_of("sdaa", "sda"));
        assert!(!is_child_of("sdb1", "sda"));
        assert!(!is_child_of("nvme0n2", "nvme0n1"));
    }

    // --- the catalog ------------------------------------------------------

    #[test]
    fn the_catalog_size_follows_the_inventory() {
        let fixture = real_block_tree();
        let inventory = real_inventory(&fixture);

        let provider = ProviderId::new(PROVIDER_ID).expect("valid");
        let devices: Vec<StorageDeviceDescriptor> = inventory
            .devices
            .iter()
            .map(|device| device.descriptor.clone())
            .collect();
        let volumes: Vec<StorageVolumeDescriptor> = inventory
            .volumes
            .iter()
            .map(|volume| volume.descriptor.clone())
            .collect();

        assert_eq!(
            wellknown::definitions(&provider, &devices, &volumes).len(),
            2 + wellknown::PER_DEVICE_KEYS.len() * 2 + wellknown::PER_VOLUME_KEYS.len() * 6
        );
    }

    #[test]
    fn a_machine_with_no_block_devices_still_builds_an_inventory() {
        let fixture = BlockFixture::new("empty");
        let inventory = StorageInventory::discover_in(fixture.root(), &[], &BTreeMap::new());

        assert!(inventory.devices.is_empty());
        assert!(inventory.volumes.is_empty());
    }

    // --- sampling on the real machine -------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_provider_answers_every_metric_it_declares() {
        let provider = LinuxStorageProvider::new();
        let definitions = provider.describe().expect("describes");

        let references: Vec<MetricRef> = definitions
            .iter()
            .map(|definition| definition.metric.clone())
            .collect();
        let samples = provider.sample(&references).expect("samples");

        assert_eq!(samples.len(), references.len());
        for (sample, reference) in samples.iter().zip(&references) {
            assert_eq!(&sample.metric, reference);
            // Every answer is either a value or a stated reason; never both
            // and never neither.
            assert_eq!(
                sample.has_value(),
                sample.availability.is_available(),
                "{reference} answered inconsistently"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_machine_reports_at_least_one_device_and_one_volume() {
        // Any machine that can run this test booted from something.
        let provider = LinuxStorageProvider::new();

        assert!(
            !provider.devices().is_empty(),
            "a booted machine has a disk"
        );
        assert!(!provider.volumes().is_empty(), "and a mounted root");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn no_pseudo_device_reaches_the_real_inventory() {
        let provider = LinuxStorageProvider::new();

        for device in provider.devices() {
            assert!(
                sysfs::is_inventoried_name(&device.os_name),
                "'{}' should not have been inventoried",
                device.os_name
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_unknown_source_is_answered_rather_than_rejected() {
        let provider = LinuxStorageProvider::new();
        let reference =
            MetricRef::parse(wellknown::CAPACITY_TOTAL, "storage:dev-nonexistent").expect("valid");

        let samples = provider
            .sample(std::slice::from_ref(&reference))
            .expect("samples");

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].availability.status_str(), "notRegistered");
        assert!(!samples[0].has_value());
    }
}
