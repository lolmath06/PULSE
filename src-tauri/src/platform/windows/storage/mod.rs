//! The Windows storage provider.
//!
//! **One provider owns every storage metric on the machine**, exactly as on
//! Fedora:
//!
//! ```text
//! windows.storage
//!  ├── inventory   SetupAPI + IOCTL_STORAGE_QUERY_PROPERTY   disks, identity, bus
//!  ├── capacity    IOCTL_DISK_GET_DRIVE_GEOMETRY_EX          whole-device size
//!  ├── volumes     FindFirstVolumeW + volume GUID paths      filesystems
//!  ├── mapping     IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS      volume → disk
//!  ├── I/O         IOCTL_DISK_PERFORMANCE                    cumulative counters
//!  └── health      IOCTL_STORAGE_QUERY_PROPERTY + NVMe log   SMART / Health
//! ```
//!
//! # Why not performance counters
//!
//! `\PhysicalDisk(0 C:)\Disk Reads/sec` is the obvious source and is not used.
//! Its instance names are **presentation strings** built from a disk number
//! and whichever drive letters happen to sit on it — they change when a letter
//! is reassigned, they are localised on some systems, and a disk holding
//! several volumes produces a name PULSE would have to parse to attribute
//! anything. Correlating that back to a `StorageDeviceDescriptor` would be a
//! guess dressed as a measurement.
//!
//! `IOCTL_DISK_PERFORMANCE` is asked of the device itself, by handle. There is
//! no name to parse and no correlation to get wrong. Its counters are only
//! collected when the disk performance provider is enabled — it is on by
//! default for physical disks on every supported Windows version — and when it
//! is not, the control fails and PULSE reports the I/O metrics as unsupported
//! rather than inventing them.
//!
//! # Degradation
//!
//! Every layer fails alone, as on Fedora. A USB bridge that answers no
//! descriptor still yields a disk with a capacity and counters. A driver that
//! refuses the NVMe log leaves everything else working.
//!
//! # Not executed
//!
//! This code compiles and type checks for `x86_64-pc-windows-msvc` through
//! `tools/windows-check`, and every pure part of it — descriptor parsing,
//! identity, capacity, counter conversion, extent mapping — is unit-tested on
//! Fedora. **It has not been run on a Windows machine.** See
//! `docs/platforms/windows.md`.

pub mod device;
pub mod identity;
pub mod ioctl;
pub mod volumes;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricRef, MetricSample, ProviderId, SourceId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::storage::{
    self as wellknown, NvmeHealth, StorageBus, StorageCapabilities, StorageDeviceDescriptor,
    StorageIo, StorageIoSnapshot, StorageIoTracker, StorageTelemetry, StorageVolumeDescriptor,
    VolumeCapabilities, VolumeUsage,
};

/// Identifier of the Windows storage provider.
pub const PROVIDER_ID: &str = "windows.storage";

/// One inventoried disk, with what is needed to read it again.
#[derive(Debug, Clone)]
pub struct Disk {
    pub descriptor: StorageDeviceDescriptor,
    /// The `PhysicalDriveN` number used to reopen the device. A handle, never
    /// an identity.
    pub number: u32,
    pub performance_available: bool,
}

/// One inventoried volume.
#[derive(Debug, Clone)]
pub struct Volume {
    pub descriptor: StorageVolumeDescriptor,
    /// The GUID path `GetDiskFreeSpaceExW` is called on.
    pub guid_path: String,
}

/// Turns one disk's report into a descriptor.
///
/// Pure, so the whole mapping — identity choice, bus, capacity, the
/// capabilities each backend implies — is tested on Fedora against synthetic
/// reports rather than on a Windows machine with that hardware in it.
pub fn describe_disk(report: &device::DiskReport) -> Option<Disk> {
    let fallback = format!("PhysicalDrive{}", report.number);

    let (source_id, storage_identity) = identity::disk_identity(
        report.descriptor.serial.as_deref(),
        report.instance_id.as_deref(),
        &fallback,
    )?;

    let bus = report.descriptor.bus.0;

    let mut capabilities = StorageCapabilities::all_available();

    if report.capacity_bytes.is_none() {
        capabilities.capacity_total =
            Availability::unsupported("this device reported no drive geometry");
    }

    if !report.performance_available {
        capabilities = capabilities.with_io(Availability::unsupported(
            "Windows is not collecting performance counters for this disk, so it keeps \
             no cumulative I/O totals PULSE could difference",
        ));
    }

    if !bus.may_expose_nvme_health() {
        capabilities = capabilities.with_health(reason_for_bus(bus));
    }

    Some(Disk {
        number: report.number,
        performance_available: report.performance_available,
        descriptor: StorageDeviceDescriptor {
            source_id,
            display_name: identity::display_name(
                report.descriptor.vendor.as_deref(),
                report.descriptor.product.as_deref(),
                &fallback,
            ),
            os_name: fallback,
            identity: storage_identity,
            bus,
            capacity_bytes: report.capacity_bytes,
            // Windows's storage descriptor reports removable *media* rather
            // than whether the medium rotates, so the rotational question is
            // left unanswered instead of guessed at from it.
            rotational: None,
            removable: Some(report.descriptor.removable),
            backend: "setupapi",
            capabilities,
        },
    })
}

/// Why a device on this bus reports no standardised health data.
///
/// Worded identically to the Fedora provider's, because the situation is
/// identical: the limitation is PULSE's health coverage, not the operating
/// system's.
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
            "a virtual disk has no controller of its own to report health",
        ),
        StorageBus::Mmc => Availability::unsupported(
            "PULSE has no MMC health backend yet; the device is inventoried and its \
             activity is measured",
        ),
        StorageBus::Nvme | StorageBus::Unknown => Availability::unsupported(
            "Windows did not report how this device attaches, so PULSE does not guess \
             which health protocol to speak to it",
        ),
    }
}

/// Turns one volume's report into a descriptor, attributing it to a disk.
///
/// The parent comes from the volume's own disk extents, never from a name. A
/// volume whose extents could not be read, or that spans several disks, is
/// left without a single parent — the UI shows it in its own section rather
/// than attributing it to one of the disks arbitrarily.
pub fn describe_volume(report: &volumes::VolumeReport, disks: &[Disk]) -> Option<Volume> {
    let (source_id, volume_identity) = identity::volume_identity(&report.guid_path)?;

    // Exactly one disk means an unambiguous parent. Zero means PULSE could not
    // ask. More than one means the volume genuinely spans disks, and naming
    // one of them would be a lie.
    let parent: Option<SourceId> = match report.disk_numbers.as_slice() {
        [number] => disks
            .iter()
            .find(|disk| disk.number == *number)
            .map(|disk| disk.descriptor.source_id.clone()),
        _ => None,
    };

    Some(Volume {
        guid_path: report.guid_path.clone(),
        descriptor: StorageVolumeDescriptor {
            source_id,
            display_name: report.display_name(),
            identity: volume_identity,
            filesystem: report.filesystem.clone(),
            mount_points: report.path_names.clone(),
            device: parent,
            read_only: report.read_only,
            backend: "volume-guid",
            capabilities: VolumeCapabilities::all_available(),
        },
    })
}

/// What the provider discovered at startup.
#[derive(Debug)]
pub struct StorageInventory {
    pub disks: Vec<Disk>,
    pub volumes: Vec<Volume>,
}

impl StorageInventory {
    /// Builds the inventory from raw reports.
    ///
    /// Pure, and therefore the seam the tests use: the FFI produces reports,
    /// this turns them into the catalog, and the two are checked separately.
    pub fn from_reports(
        disk_reports: &[device::DiskReport],
        volume_reports: &[volumes::VolumeReport],
    ) -> Self {
        let mut disks: Vec<Disk> = disk_reports.iter().filter_map(describe_disk).collect();

        // Two interfaces onto one disk would otherwise become two rows.
        disks.sort_by(|left, right| left.descriptor.source_id.cmp(&right.descriptor.source_id));
        disks.dedup_by(|left, right| left.descriptor.source_id == right.descriptor.source_id);

        let mut volumes: Vec<Volume> = volume_reports
            .iter()
            .filter_map(|report| describe_volume(report, &disks))
            .collect();
        volumes.sort_by(|left, right| left.descriptor.source_id.cmp(&right.descriptor.source_id));

        Self { disks, volumes }
    }

    fn discover() -> Self {
        Self::from_reports(&device::discover(), &volumes::discover())
    }

    /// Reads every disk's counters.
    ///
    /// One device control per disk: Windows has no single file listing them
    /// all the way `/proc/diskstats` does. They are issued back to back so the
    /// interval they describe is as close to common as the platform allows.
    fn io_snapshot(&self, at: &Instant, origin: &Instant) -> StorageIoSnapshot {
        let mut snapshot = StorageIoSnapshot::new(at.duration_since(*origin).as_millis() as u64);

        for disk in &self.disks {
            if !disk.performance_available {
                continue;
            }
            if let Ok(counters) = device::read_performance(disk.number) {
                snapshot.insert(disk.descriptor.source_id.clone(), counters);
            }
        }

        snapshot
    }
}

/// Publishes every storage metric on Windows.
pub struct WindowsStorageProvider {
    id: ProviderId,
    inventory: StorageInventory,
    io: StorageIoTracker,
    origin: Instant,
}

impl std::fmt::Debug for WindowsStorageProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsStorageProvider")
            .field("id", &self.id)
            .field("disks", &self.inventory.disks.len())
            .field("volumes", &self.inventory.volumes.len())
            .finish()
    }
}

impl WindowsStorageProvider {
    pub fn new() -> Self {
        let origin = Instant::now();
        let inventory = StorageInventory::discover();
        let io = StorageIoTracker::new();

        // The same baseline-at-startup as Fedora, for the same reason: the
        // user's first Refresh should produce a real interval, and the first
        // request should say it is waiting rather than report 0 B/s.
        io.prime(&inventory.io_snapshot(&Instant::now(), &origin));

        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
            inventory,
            io,
            origin,
        }
    }

    pub fn devices(&self) -> Vec<StorageDeviceDescriptor> {
        self.inventory
            .disks
            .iter()
            .map(|disk| disk.descriptor.clone())
            .collect()
    }

    pub fn volumes(&self) -> Vec<StorageVolumeDescriptor> {
        self.inventory
            .volumes
            .iter()
            .map(|volume| volume.descriptor.clone())
            .collect()
    }

    fn disk_for(&self, source: &str) -> Option<&Disk> {
        self.inventory
            .disks
            .iter()
            .find(|disk| disk.descriptor.source_id.as_str() == source)
    }

    fn volume_for(&self, source: &str) -> Option<&Volume> {
        self.inventory
            .volumes
            .iter()
            .find(|volume| volume.descriptor.source_id.as_str() == source)
    }

    /// Reads one disk's health, when its bus can carry the request.
    fn health_for(&self, disk: &Disk) -> (Option<NvmeHealth>, Option<MetricError>) {
        if !disk.descriptor.bus.may_expose_nvme_health() {
            return (None, None);
        }

        match device::read_nvme_health(disk.number) {
            Ok(health) => (Some(health), None),
            Err(error) => (None, Some(error)),
        }
    }
}

impl Default for WindowsStorageProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for WindowsStorageProvider {
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
        let snapshot = self.inventory.io_snapshot(&Instant::now(), &self.origin);
        let rates = self.io.update(&snapshot)?;

        let mut telemetry: BTreeMap<&str, (StorageTelemetry, Option<MetricError>)> =
            BTreeMap::new();
        let mut usage: BTreeMap<&str, Result<VolumeUsage, MetricError>> = BTreeMap::new();

        // Only the devices and volumes this request touches: a disk nobody
        // asked about is never opened.
        for reference in requested {
            let source = reference.source_id.as_str();

            if let Some(disk) = self.disk_for(source) {
                telemetry.entry(source).or_insert_with(|| {
                    let (health, error) = if wellknown::requests_health(requested, source) {
                        self.health_for(disk)
                    } else {
                        (None, None)
                    };

                    (
                        StorageTelemetry {
                            capacity_bytes: disk.descriptor.capacity_bytes,
                            io: match rates.get(&disk.descriptor.source_id) {
                                Some(StorageIo::Ready(rates)) => Some(*rates),
                                _ => None,
                            },
                            health,
                            // Windows exposes no unprivileged composite
                            // temperature outside the health log itself, so
                            // there is no second source to fall back on.
                            temperature_celsius: None,
                        },
                        error,
                    )
                });
            } else if let Some(volume) = self.volume_for(source) {
                usage
                    .entry(source)
                    .or_insert_with(|| volumes::read_usage(&volume.guid_path));
            }
        }

        Ok(requested
            .iter()
            .map(|reference| self.sample_one(reference, &telemetry, &usage, &rates))
            .collect())
    }
}

impl WindowsStorageProvider {
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
                return MetricSample::number(reference.clone(), self.inventory.disks.len() as f64)
            }
            wellknown::VOLUME_COUNT => {
                return MetricSample::number(reference.clone(), self.inventory.volumes.len() as f64)
            }
            _ => {}
        }

        let source = reference.source_id.as_str();

        if let Some(disk) = self.disk_for(source) {
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
                None => MetricSample::unavailable(
                    reference.clone(),
                    disk_reason(disk, key, health_error.as_ref(), rates),
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

/// Explains why one disk could not answer one metric.
///
/// The same rules as Fedora's, so a user moving between the two sees the same
/// distinction between "waiting for a second sample", "nothing happened during
/// this interval" and "this cannot be measured here".
fn disk_reason(
    disk: &Disk,
    key: &str,
    health_error: Option<&MetricError>,
    rates: &BTreeMap<SourceId, StorageIo>,
) -> Availability {
    if wellknown::IO_KEYS.contains(&key) {
        return match rates.get(&disk.descriptor.source_id) {
            Some(StorageIo::NeedsAnotherSample(reason)) => {
                Availability::temporarily_unavailable(reason.reason())
            }
            Some(StorageIo::Ready(_)) => Availability::temporarily_unavailable(
                "no operation of this kind completed during the interval, so there is \
                 no latency to report",
            ),
            None => disk.descriptor.capabilities.io_read_bytes.clone(),
        };
    }

    if wellknown::HEALTH_KEYS.contains(&key) {
        if let Some(error) = health_error {
            return crate::metrics::wellknown::availability_for(error.clone());
        }
    }

    capability_for(&disk.descriptor.capabilities, key)
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

/// Builds the Windows storage provider.
#[cfg(target_os = "windows")]
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(WindowsStorageProvider::new())
}

// Referenced only by the Windows-gated `provider()`; the import is kept
// unconditional so the module's `use` list does not depend on the target.
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
fn _arc_is_used(provider: WindowsStorageProvider) -> Arc<dyn MetricProvider> {
    Arc::new(provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::wellknown::storage::IdentityStability;

    fn disk_report(
        number: u32,
        bus: u32,
        vendor: Option<&str>,
        product: Option<&str>,
        serial: Option<&str>,
        instance: Option<&str>,
        capacity: Option<u64>,
    ) -> device::DiskReport {
        let buffer = ioctl::fixtures::device_descriptor(bus, false, vendor, product, None, serial);

        device::DiskReport {
            number,
            instance_id: instance.map(str::to_string),
            descriptor: ioctl::parse_device_descriptor(&buffer).expect("valid"),
            capacity_bytes: capacity,
            performance_available: true,
        }
    }

    fn volume_report(guid: &str, paths: &[&str], disks: &[u32]) -> volumes::VolumeReport {
        volumes::VolumeReport {
            guid_path: guid.to_string(),
            path_names: paths.iter().map(|path| path.to_string()).collect(),
            filesystem: Some("NTFS".to_string()),
            read_only: false,
            disk_numbers: disks.to_vec(),
        }
    }

    const NVME: u32 = 0x11;
    const SATA: u32 = 0x0B;
    const USB: u32 = 0x07;

    // --- disks ------------------------------------------------------------

    #[test]
    fn an_nvme_disk_is_described_from_its_descriptor() {
        let report = disk_report(
            0,
            NVME,
            Some("NVMe"),
            Some("SAMSUNG MZVL22T0HBLB-00B00"),
            Some("S677NX0W"),
            Some(r"SCSI\Disk&Ven_NVMe&Prod_SAMSUNG\5&2a"),
            Some(2_048_408_248_320),
        );

        let disk = describe_disk(&report).expect("described");

        assert_eq!(
            disk.descriptor.source_id.as_str(),
            "storage:serial-s677nx0w"
        );
        assert_eq!(disk.descriptor.bus, StorageBus::Nvme);
        assert_eq!(disk.descriptor.capacity_bytes, Some(2_048_408_248_320));
        assert_eq!(disk.descriptor.os_name, "PhysicalDrive0");
        assert_eq!(disk.number, 0);
        // A whole-device NVMe drive can be asked for its health log.
        assert!(disk
            .descriptor
            .capabilities
            .health_percentage_used
            .is_available());
    }

    #[test]
    fn a_sata_disk_keeps_its_health_definitions_as_unsupported() {
        let report = disk_report(
            1,
            SATA,
            Some("ATA"),
            Some("WDC WDS100T2B0A"),
            Some("184512"),
            None,
            Some(1_000_204_886_016),
        );
        let disk = describe_disk(&report).expect("described");

        assert_eq!(disk.descriptor.bus, StorageBus::Ata);
        assert_eq!(
            disk.descriptor
                .capabilities
                .health_percentage_used
                .status_str(),
            "unsupported"
        );
        // Everything else about it still works.
        assert!(disk.descriptor.capabilities.capacity_total.is_available());
        assert!(disk.descriptor.capabilities.io_read_iops.is_available());
    }

    #[test]
    fn a_usb_bridge_without_a_serial_is_still_a_disk() {
        let report = disk_report(
            2,
            USB,
            Some("Intenso"),
            Some("External USB3.0"),
            None,
            Some(r"USBSTOR\Disk&Ven_Intenso\7&1a&0&Port_0004"),
            Some(1_000_204_886_016),
        );

        let disk = describe_disk(&report).expect("described");

        assert_eq!(disk.descriptor.bus, StorageBus::Usb);
        assert_eq!(
            disk.descriptor.identity.stability(),
            IdentityStability::SystemAssigned
        );
        assert_eq!(disk.descriptor.display_name, "Intenso External USB3.0");
        assert!(disk.descriptor.capabilities.capacity_total.is_available());
    }

    #[test]
    fn a_disk_with_no_performance_counters_says_so_without_disappearing() {
        let mut report = disk_report(
            3,
            SATA,
            None,
            Some("Some disk"),
            Some("AB12CD"),
            None,
            Some(1000),
        );
        report.performance_available = false;

        let disk = describe_disk(&report).expect("described");

        assert_eq!(
            disk.descriptor.capabilities.io_read_iops.status_str(),
            "unsupported"
        );
        assert_eq!(
            disk.descriptor.capabilities.io_write_latency.status_str(),
            "unsupported"
        );
        assert!(disk.descriptor.capabilities.capacity_total.is_available());
    }

    #[test]
    fn a_disk_with_no_geometry_keeps_its_capacity_definition() {
        let report = disk_report(4, NVME, None, Some("Odd disk"), Some("AB12CD"), None, None);
        let disk = describe_disk(&report).expect("described");

        assert_eq!(disk.descriptor.capacity_bytes, None);
        assert_eq!(
            disk.descriptor.capabilities.capacity_total.status_str(),
            "unsupported"
        );
    }

    #[test]
    fn two_identical_models_stay_two_disks() {
        let inventory = StorageInventory::from_reports(
            &[
                disk_report(
                    0,
                    NVME,
                    None,
                    Some("Twin SSD"),
                    Some("S111"),
                    None,
                    Some(1000),
                ),
                disk_report(
                    1,
                    NVME,
                    None,
                    Some("Twin SSD"),
                    Some("S222"),
                    None,
                    Some(1000),
                ),
            ],
            &[],
        );

        assert_eq!(inventory.disks.len(), 2);
        assert_ne!(
            inventory.disks[0].descriptor.source_id,
            inventory.disks[1].descriptor.source_id
        );
        assert_eq!(
            inventory.disks[0].descriptor.display_name,
            inventory.disks[1].descriptor.display_name
        );
    }

    #[test]
    fn one_disk_behind_two_interfaces_is_not_counted_twice() {
        let inventory = StorageInventory::from_reports(
            &[
                disk_report(
                    0,
                    NVME,
                    None,
                    Some("One SSD"),
                    Some("S111"),
                    None,
                    Some(1000),
                ),
                disk_report(
                    0,
                    NVME,
                    None,
                    Some("One SSD"),
                    Some("S111"),
                    None,
                    Some(1000),
                ),
            ],
            &[],
        );

        assert_eq!(inventory.disks.len(), 1);
    }

    #[test]
    fn the_inventory_order_does_not_depend_on_enumeration_order() {
        let forward = StorageInventory::from_reports(
            &[
                disk_report(0, NVME, None, Some("A"), Some("SAAA"), None, Some(1)),
                disk_report(1, SATA, None, Some("B"), Some("SBBB"), None, Some(1)),
            ],
            &[],
        );
        let backward = StorageInventory::from_reports(
            &[
                disk_report(1, SATA, None, Some("B"), Some("SBBB"), None, Some(1)),
                disk_report(0, NVME, None, Some("A"), Some("SAAA"), None, Some(1)),
            ],
            &[],
        );

        let sources = |inventory: &StorageInventory| -> Vec<String> {
            inventory
                .disks
                .iter()
                .map(|disk| disk.descriptor.source_id.as_str().to_string())
                .collect()
        };

        assert_eq!(sources(&forward), sources(&backward));
    }

    // --- volumes ----------------------------------------------------------

    #[test]
    fn a_volume_is_attributed_through_its_disk_extents() {
        let disks = [disk_report(
            0,
            NVME,
            None,
            Some("SSD"),
            Some("S111"),
            None,
            Some(1000),
        )];
        let inventory = StorageInventory::from_reports(
            &disks,
            &[volume_report(
                r"\\?\Volume{d2b1f8e0-1111-2222-3333-100000000000}\",
                &[r"C:\"],
                &[0],
            )],
        );

        let volume = &inventory.volumes[0].descriptor;

        assert_eq!(volume.display_name, "C:");
        assert_eq!(
            volume.device.as_ref().map(|source| source.as_str()),
            Some("storage:serial-s111")
        );
        assert_eq!(volume.source_id.kind(), "volume");
    }

    #[test]
    fn a_volume_spanning_two_disks_is_attributed_to_neither() {
        // `1 volume = 1 disk` holds on most machines. Hardcoding it would
        // attribute a striped volume's whole capacity to one of its halves.
        let disks = [
            disk_report(0, NVME, None, Some("A"), Some("SAAA"), None, Some(1000)),
            disk_report(1, NVME, None, Some("B"), Some("SBBB"), None, Some(1000)),
        ];
        let inventory = StorageInventory::from_reports(
            &disks,
            &[volume_report(
                r"\\?\Volume{aaaaaaaa-0000-0000-0000-000000000000}\",
                &[r"S:\"],
                &[0, 1],
            )],
        );

        assert_eq!(inventory.volumes[0].descriptor.device, None);
    }

    #[test]
    fn a_volume_whose_extents_are_unreadable_is_attributed_to_nothing() {
        let disks = [disk_report(
            0,
            NVME,
            None,
            Some("SSD"),
            Some("S111"),
            None,
            Some(1000),
        )];
        let inventory = StorageInventory::from_reports(
            &disks,
            &[volume_report(
                r"\\?\Volume{bbbbbbbb-0000-0000-0000-000000000000}\",
                &[r"D:\"],
                &[],
            )],
        );

        assert_eq!(inventory.volumes[0].descriptor.device, None);
    }

    #[test]
    fn a_volume_with_no_drive_letter_is_still_inventoried() {
        // The EFI system partition and Windows's recovery partition have none,
        // and they take real space.
        let inventory = StorageInventory::from_reports(
            &[],
            &[volume_report(
                r"\\?\Volume{cccccccc-0000-0000-0000-000000000000}\",
                &[],
                &[0],
            )],
        );

        assert_eq!(inventory.volumes.len(), 1);
        assert_eq!(
            inventory.volumes[0].descriptor.display_name,
            "Volume cccccccc"
        );
        assert!(inventory.volumes[0].descriptor.mount_points.is_empty());
    }

    #[test]
    fn a_volume_reachable_at_a_letter_and_a_folder_is_one_volume() {
        let inventory = StorageInventory::from_reports(
            &[],
            &[volume_report(
                r"\\?\Volume{dddddddd-0000-0000-0000-000000000000}\",
                &[r"E:\", r"C:\Mounts\Data\"],
                &[],
            )],
        );

        assert_eq!(inventory.volumes.len(), 1);
        assert_eq!(inventory.volumes[0].descriptor.mount_points.len(), 2);
        assert_eq!(inventory.volumes[0].descriptor.display_name, "E:");
    }

    #[test]
    fn something_that_is_not_a_volume_guid_is_not_inventoried() {
        let inventory =
            StorageInventory::from_reports(&[], &[volume_report(r"C:\", &[r"C:\"], &[0])]);

        assert!(inventory.volumes.is_empty());
    }

    // --- the catalog ------------------------------------------------------

    #[test]
    fn the_catalog_size_follows_the_inventory() {
        let inventory = StorageInventory::from_reports(
            &[
                disk_report(0, NVME, None, Some("A"), Some("SAAA"), None, Some(1000)),
                disk_report(1, USB, None, Some("B"), Some("SBBB"), None, Some(1000)),
            ],
            &[
                volume_report(r"\\?\Volume{aaaaaaaa-0-0-0-0}\", &[r"C:\"], &[0]),
                volume_report(r"\\?\Volume{bbbbbbbb-0-0-0-0}\", &[r"D:\"], &[1]),
                volume_report(r"\\?\Volume{cccccccc-0-0-0-0}\", &[], &[0]),
            ],
        );

        let provider = ProviderId::new(PROVIDER_ID).expect("valid");
        let devices: Vec<StorageDeviceDescriptor> = inventory
            .disks
            .iter()
            .map(|disk| disk.descriptor.clone())
            .collect();
        let volumes: Vec<StorageVolumeDescriptor> = inventory
            .volumes
            .iter()
            .map(|volume| volume.descriptor.clone())
            .collect();

        assert_eq!(
            wellknown::definitions(&provider, &devices, &volumes).len(),
            2 + wellknown::PER_DEVICE_KEYS.len() * 2 + wellknown::PER_VOLUME_KEYS.len() * 3
        );
    }

    #[test]
    fn a_machine_with_no_storage_still_builds_an_inventory() {
        let inventory = StorageInventory::from_reports(&[], &[]);

        assert!(inventory.disks.is_empty());
        assert!(inventory.volumes.is_empty());
    }

    #[test]
    fn every_bus_has_its_own_explanation() {
        let mut reasons: Vec<String> = StorageBus::ALL
            .iter()
            .map(|bus| format!("{:?}", reason_for_bus(*bus)))
            .collect();
        reasons.sort();
        reasons.dedup();

        // NVMe/Unknown share one and ATA/SCSI share another, exactly as on
        // Fedora.
        assert_eq!(reasons.len(), StorageBus::ALL.len() - 2);
    }
}
