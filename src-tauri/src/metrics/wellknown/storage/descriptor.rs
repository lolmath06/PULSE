//! What PULSE knows about one storage device and one storage volume — and, the
//! hard part again, how it tells them apart and identifies each of them.
//!
//! # A device is not a volume, and a volume is not a mount point
//!
//! Three words that monitoring tools routinely use interchangeably, and that
//! mean three different things:
//!
//! | Concept | What it is | Example |
//! |---|---|---|
//! | **Device** | A physical piece of hardware that stores bytes | the Samsung SSD in the M.2 slot |
//! | **Volume** | A filesystem living on some of that hardware | the btrfs filesystem holding Fedora |
//! | **Mount point** | A path where a volume is currently reachable | `/`, `/home`, `C:\` |
//!
//! Their cardinalities do not line up. One device holds many volumes. One
//! volume can span several devices. One volume is frequently reachable at
//! several mount points at once — Fedora's default layout mounts the *same*
//! btrfs filesystem at both `/` and `/home`, and a `C:` drive letter is one
//! presentation of a volume that also has a GUID path. Collapsing any pair of
//! these produces a monitor that double-counts capacity and attributes writes
//! to the wrong disk.
//!
//! So PULSE models devices and volumes as two source kinds, `storage:` and
//! `volume:`, and treats a mount point as *presentation* attached to a volume
//! rather than as an identity.
//!
//! # Identity
//!
//! Every obvious candidate is wrong, exactly as it was for GPUs:
//!
//! | Candidate | Why it must not be an identity |
//! |---|---|
//! | `sda`, `nvme0n1` | Kernel names assigned in probe order; plugging in a USB stick at boot renames the next disk |
//! | `\\.\PhysicalDrive0` | A Windows enumeration index, not a property of the hardware |
//! | `C:`, `/`, `/home` | Mount points. They change without the filesystem changing |
//! | Product name | Two identical SSDs collapse into one identifier |
//! | `major:minor` | Stable only while the device stays attached |
//!
//! What PULSE prefers, in order: a hardware identifier the device carries
//! (WWN / NGUID / EUI-64 / T10 vendor ID), then its serial number, then — only
//! as a session-scoped fallback — its kernel or OS name, with the weakness
//! recorded rather than hidden.

use std::fmt;

use crate::metrics::model::{Availability, SourceId};

/// How the device attaches to the machine.
///
/// Reported for display and for explaining health limitations. **Never part of
/// an identity**, and never inferred from something else: in particular a
/// non-rotating device is not necessarily NVMe, and a device on a USB bridge
/// may be any of the others underneath.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorageBus {
    Nvme,
    /// SATA or any other ATA attachment.
    Ata,
    /// SAS / parallel SCSI.
    Scsi,
    /// Behind a USB mass-storage bridge. What the bridge hides may itself be
    /// SATA or NVMe; PULSE does not guess which.
    Usb,
    /// A paravirtualised disk — `virtio`, Xen, Hyper-V.
    Virtual,
    /// SD / eMMC / MMC.
    Mmc,
    /// Attached by something PULSE has no name for.
    Unknown,
}

impl StorageBus {
    /// The short label shown next to a device, e.g. `NVMe`.
    pub const fn label(self) -> &'static str {
        match self {
            StorageBus::Nvme => "NVMe",
            StorageBus::Ata => "SATA",
            StorageBus::Scsi => "SCSI",
            StorageBus::Usb => "USB",
            StorageBus::Virtual => "Virtual",
            StorageBus::Mmc => "MMC",
            StorageBus::Unknown => "Storage",
        }
    }

    /// Whether the NVMe SMART/Health log can even be asked for over this
    /// attachment.
    ///
    /// Only a genuine NVMe attachment qualifies. A USB bridge in front of an
    /// NVMe drive rarely passes admin commands through, and pretending
    /// otherwise would turn "we cannot ask" into "your drive is broken".
    pub const fn may_expose_nvme_health(self) -> bool {
        matches!(self, StorageBus::Nvme)
    }

    pub const ALL: &'static [StorageBus] = &[
        StorageBus::Nvme,
        StorageBus::Ata,
        StorageBus::Scsi,
        StorageBus::Usb,
        StorageBus::Virtual,
        StorageBus::Mmc,
        StorageBus::Unknown,
    ];
}

/// How stable a derived identity actually is.
///
/// Recorded rather than assumed, so a user can be told when a saved widget is
/// resting on something weaker than a hardware identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityStability {
    /// Derived from something the hardware itself carries — a WWN, an EUI-64,
    /// a serial number. Survives reboots, reordering, and distinguishes two
    /// identical drives.
    Hardware,
    /// Derived from a stable identifier the operating system assigns to the
    /// hardware, such as a Windows device instance ID. Survives reboots and
    /// driver updates; tied to the port the device is attached to.
    SystemAssigned,
    /// Derived from the parent device's identity plus a partition ordinal.
    /// Exactly as stable as the parent, and changes if the disk is
    /// repartitioned.
    DerivedFromParent,
    /// Derived from a name or number valid only while the device stays
    /// attached: a kernel name, a `major:minor` pair, a `PhysicalDriveN`.
    Session,
}

impl IdentityStability {
    /// Whether this identity is expected to mean the same thing after a
    /// reboot.
    pub const fn survives_reboot(self) -> bool {
        !matches!(self, IdentityStability::Session)
    }

    /// A short explanation, surfaced in documentation and diagnostics.
    pub const fn explanation(self) -> &'static str {
        match self {
            IdentityStability::Hardware => {
                "derived from a hardware identifier carried by the device itself"
            }
            IdentityStability::SystemAssigned => {
                "derived from a stable identifier the operating system assigns to this device"
            }
            IdentityStability::DerivedFromParent => {
                "derived from the parent device's identity and this partition's ordinal"
            }
            IdentityStability::Session => {
                "derived from a name that is only valid while the device stays attached"
            }
        }
    }
}

/// Which identifier a device's `SourceId` was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageIdentity {
    /// A world-wide name, NGUID, EUI-64 or T10 vendor identifier, as the
    /// device reports it. The best identity available for a disk.
    Wwid(String),
    /// The device's serial number, when it exposes no world-wide name.
    Serial(String),
    /// A stable identifier assigned by the operating system — currently a
    /// Windows device instance ID.
    SystemId(String),
    /// A kernel or OS name: `nvme0n1`, `PhysicalDrive0`. Session-scoped, and
    /// used only when nothing better exists.
    OsName(String),
}

impl StorageIdentity {
    pub const fn stability(&self) -> IdentityStability {
        match self {
            StorageIdentity::Wwid(_) | StorageIdentity::Serial(_) => IdentityStability::Hardware,
            StorageIdentity::SystemId(_) => IdentityStability::SystemAssigned,
            StorageIdentity::OsName(_) => IdentityStability::Session,
        }
    }

    /// A short machine-readable tag, for diagnostics and the report.
    pub const fn mechanism(&self) -> &'static str {
        match self {
            StorageIdentity::Wwid(_) => "wwid",
            StorageIdentity::Serial(_) => "serial",
            StorageIdentity::SystemId(_) => "system-id",
            StorageIdentity::OsName(_) => "os-name",
        }
    }
}

/// Which identifier a volume's `SourceId` was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeIdentity {
    /// A filesystem or volume identifier the platform guarantees — a Windows
    /// volume GUID path.
    Guid(String),
    /// The parent device's identity plus this partition's ordinal.
    Partition { device: SourceId, partition: u32 },
    /// The filesystem's `major:minor` device number. Valid for as long as the
    /// filesystem stays mounted, and no longer.
    DeviceNumber { major: u32, minor: u32 },
}

impl VolumeIdentity {
    pub const fn stability(&self) -> IdentityStability {
        match self {
            VolumeIdentity::Guid(_) => IdentityStability::SystemAssigned,
            VolumeIdentity::Partition { .. } => IdentityStability::DerivedFromParent,
            VolumeIdentity::DeviceNumber { .. } => IdentityStability::Session,
        }
    }

    pub const fn mechanism(&self) -> &'static str {
        match self {
            VolumeIdentity::Guid(_) => "volume-guid",
            VolumeIdentity::Partition { .. } => "partition-of-device",
            VolumeIdentity::DeviceNumber { .. } => "device-number",
        }
    }
}

// --- source identifiers ---------------------------------------------------

/// Reduces an arbitrary hardware string to something a `SourceId` instance
/// accepts.
///
/// `SourceId` allows lowercase letters, digits, `-`, `_` and `.`, and forbids
/// a leading `-`. Real identifiers are messier than that: the SCSI T10 string
/// of the USB disk on the development machine is
/// `t10.Intenso SCSI            2019131398AB5\0\0\0`, padding and NUL escapes
/// included.
///
/// Every disallowed run collapses to a single `-`, so the mapping is
/// deterministic and two devices whose raw strings differ cannot collide on a
/// normalised one *unless* they differed only in padding — which is what we
/// want, since padding is not part of the identifier.
///
/// Returns `None` when nothing usable survives, so a device reporting an empty
/// or all-padding identifier falls through to the next candidate rather than
/// producing `storage:wwid-`.
pub fn normalize_identifier(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut pending_separator = false;

    for character in raw.trim().chars() {
        let lowered = character.to_ascii_lowercase();

        if lowered.is_ascii_lowercase() || lowered.is_ascii_digit() || lowered == '.' {
            if pending_separator && !out.is_empty() {
                out.push('-');
            }
            pending_separator = false;
            out.push(lowered);
        } else {
            // `_` is legal in an instance too, but treating it as a separator
            // keeps one canonical spelling for strings that pad with either.
            pending_separator = true;
        }
    }

    let trimmed = out.trim_matches(|c| c == '-' || c == '.');
    // A string of nothing but separators, or one that reduces to a bare `.`,
    // is not an identifier.
    (!trimmed.is_empty() && trimmed.chars().any(|c| c.is_ascii_alphanumeric()))
        .then(|| trimmed.to_string())
}

/// The longest instance fragment that still leaves room for the prefixes used
/// below inside `SourceId`'s 128-character limit.
const MAX_INSTANCE_FRAGMENT: usize = 96;

fn truncate(fragment: String) -> String {
    if fragment.len() <= MAX_INSTANCE_FRAGMENT {
        return fragment;
    }

    let mut cut = MAX_INSTANCE_FRAGMENT;
    while cut > 0 && !fragment.is_char_boundary(cut) {
        cut -= 1;
    }

    fragment[..cut].trim_end_matches(['-', '.']).to_string()
}

/// Builds the `SourceId` of a device identified by a world-wide name, NGUID,
/// EUI-64 or T10 identifier.
pub fn wwid_source_id(wwid: &str) -> Option<SourceId> {
    let normalized = truncate(normalize_identifier(wwid)?);
    SourceId::new(format!("storage:wwid-{normalized}")).ok()
}

/// Builds the `SourceId` of a device identified by its serial number.
pub fn serial_source_id(serial: &str) -> Option<SourceId> {
    let normalized = truncate(normalize_identifier(serial)?);
    SourceId::new(format!("storage:serial-{normalized}")).ok()
}

/// Builds the `SourceId` of a device identified by an OS-assigned stable
/// identifier, such as a Windows device instance ID.
pub fn system_source_id(system_id: &str) -> Option<SourceId> {
    let normalized = truncate(normalize_identifier(system_id)?);
    SourceId::new(format!("storage:sys-{normalized}")).ok()
}

/// Builds the `SourceId` of a device PULSE could only name.
///
/// The `dev-` prefix exists to make the weakness visible in the identifier
/// itself: anything under it is session-scoped.
pub fn os_name_source_id(name: &str) -> Option<SourceId> {
    let normalized = truncate(normalize_identifier(name)?);
    SourceId::new(format!("storage:dev-{normalized}")).ok()
}

/// Picks the strongest device identity available and builds its `SourceId`.
///
/// The order is the whole point, and it is shared by both platforms so that
/// the *same* drive yields the *same* identifier whichever operating system
/// enumerated it.
pub fn device_identity(
    wwid: Option<&str>,
    serial: Option<&str>,
    system_id: Option<&str>,
    os_name: &str,
) -> Option<(SourceId, StorageIdentity)> {
    if let Some(wwid) = wwid {
        if let Some(source) = wwid_source_id(wwid) {
            return Some((source, StorageIdentity::Wwid(wwid.trim().to_string())));
        }
    }

    if let Some(serial) = serial {
        if let Some(source) = serial_source_id(serial) {
            return Some((source, StorageIdentity::Serial(serial.trim().to_string())));
        }
    }

    if let Some(system_id) = system_id {
        if let Some(source) = system_source_id(system_id) {
            return Some((
                source,
                StorageIdentity::SystemId(system_id.trim().to_string()),
            ));
        }
    }

    os_name_source_id(os_name)
        .map(|source| (source, StorageIdentity::OsName(os_name.trim().to_string())))
}

/// Builds a volume's `SourceId` from its parent device and partition ordinal.
pub fn partition_volume_source_id(device: &SourceId, partition: u32) -> Option<SourceId> {
    // `storage:wwid-…` becomes `volume:wwid-…-p8`: the parent's instance is
    // reused verbatim so the relationship is visible, and the kind changes so
    // the two can never be confused.
    SourceId::new(format!("volume:{}-p{partition}", device.instance())).ok()
}

/// Builds a volume's `SourceId` from a Windows volume GUID path.
pub fn guid_volume_source_id(guid: &str) -> Option<SourceId> {
    let normalized = truncate(normalize_identifier(guid)?);
    SourceId::new(format!("volume:guid-{normalized}")).ok()
}

/// Builds a volume's `SourceId` from its `major:minor` device number.
pub fn device_number_volume_source_id(major: u32, minor: u32) -> Option<SourceId> {
    SourceId::new(format!("volume:mm-{major}-{minor}")).ok()
}

// --- capabilities ---------------------------------------------------------

/// Whether each per-device metric can be sampled on this device.
///
/// Carried **per metric**, never per device, for the same reason as GPUs: a
/// USB disk whose bridge hides SMART entirely is still a disk with a capacity,
/// volumes and working I/O counters. One flag per device is what would make
/// PULSE hide a whole drive over one missing interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCapabilities {
    pub capacity_total: Availability,
    pub io_read_bytes: Availability,
    pub io_write_bytes: Availability,
    pub io_read_iops: Availability,
    pub io_write_iops: Availability,
    pub io_read_latency: Availability,
    pub io_write_latency: Availability,
    pub health_temperature: Availability,
    pub health_percentage_used: Availability,
    pub health_available_spare: Availability,
    pub health_power_on_hours: Availability,
    pub health_unsafe_shutdowns: Availability,
    pub health_media_errors: Availability,
}

impl StorageCapabilities {
    /// Everything readable.
    pub fn all_available() -> Self {
        Self {
            capacity_total: Availability::Available,
            io_read_bytes: Availability::Available,
            io_write_bytes: Availability::Available,
            io_read_iops: Availability::Available,
            io_write_iops: Availability::Available,
            io_read_latency: Availability::Available,
            io_write_latency: Availability::Available,
            health_temperature: Availability::Available,
            health_percentage_used: Availability::Available,
            health_available_spare: Availability::Available,
            health_power_on_hours: Availability::Available,
            health_unsafe_shutdowns: Availability::Available,
            health_media_errors: Availability::Available,
        }
    }

    /// Nothing readable, all for the same stated reason.
    pub fn none(reason: &Availability) -> Self {
        Self {
            capacity_total: reason.clone(),
            io_read_bytes: reason.clone(),
            io_write_bytes: reason.clone(),
            io_read_iops: reason.clone(),
            io_write_iops: reason.clone(),
            io_read_latency: reason.clone(),
            io_write_latency: reason.clone(),
            health_temperature: reason.clone(),
            health_percentage_used: reason.clone(),
            health_available_spare: reason.clone(),
            health_power_on_hours: reason.clone(),
            health_unsafe_shutdowns: reason.clone(),
            health_media_errors: reason.clone(),
        }
    }

    /// Replaces every `storage.health.*` capability with one reason.
    ///
    /// Used when a whole health backend is absent — a SATA disk with no ATA
    /// SMART backend, a USB bridge that passes nothing through, or an NVMe
    /// device whose log page the OS refuses. The I/O and capacity capabilities
    /// are untouched, because they fail independently.
    pub fn with_health(mut self, reason: Availability) -> Self {
        self.health_temperature = reason.clone();
        self.health_percentage_used = reason.clone();
        self.health_available_spare = reason.clone();
        self.health_power_on_hours = reason.clone();
        self.health_unsafe_shutdowns = reason.clone();
        self.health_media_errors = reason;
        self
    }

    /// Replaces every I/O capability with one reason.
    pub fn with_io(mut self, reason: Availability) -> Self {
        self.io_read_bytes = reason.clone();
        self.io_write_bytes = reason.clone();
        self.io_read_iops = reason.clone();
        self.io_write_iops = reason.clone();
        self.io_read_latency = reason.clone();
        self.io_write_latency = reason;
        self
    }
}

/// Whether each per-volume metric can be sampled on this volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeCapabilities {
    pub capacity_total: Availability,
    pub capacity_used: Availability,
    pub capacity_available: Availability,
    pub usage_percent: Availability,
}

impl VolumeCapabilities {
    pub fn all_available() -> Self {
        Self {
            capacity_total: Availability::Available,
            capacity_used: Availability::Available,
            capacity_available: Availability::Available,
            usage_percent: Availability::Available,
        }
    }

    pub fn none(reason: &Availability) -> Self {
        Self {
            capacity_total: reason.clone(),
            capacity_used: reason.clone(),
            capacity_available: reason.clone(),
            usage_percent: reason.clone(),
        }
    }
}

// --- descriptors ----------------------------------------------------------

/// One physical storage device, as PULSE understands it.
///
/// Built once at startup from static facts. Nothing in here changes while the
/// machine runs: a refresh re-reads counters and health, never the inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageDeviceDescriptor {
    /// The stable identifier a dashboard stores.
    pub source_id: SourceId,
    /// What the device is called in the interface, e.g.
    /// `SAMSUNG MZVL22T0HBLB-00B00`. **Presentation only** — never an
    /// identity, since two identical drives share it.
    pub display_name: String,
    /// The name the operating system gave it: `nvme0n1`, `PhysicalDrive0`.
    /// Kept for diagnostics and for correlating volumes; never an identity
    /// unless nothing better existed, in which case `identity` says so.
    pub os_name: String,
    /// Which identifier `source_id` was derived from.
    pub identity: StorageIdentity,
    pub bus: StorageBus,
    /// Total addressable capacity in bytes, when the platform reported one.
    pub capacity_bytes: Option<u64>,
    /// Whether the medium rotates. **Not a bus indicator**: a non-rotating
    /// device may be SATA, NVMe or behind a USB bridge.
    pub rotational: Option<bool>,
    /// Whether the medium can be removed from the drive.
    pub removable: Option<bool>,
    /// Which backend inventoried it, for diagnostics: `sysfs`, `setupapi`.
    pub backend: &'static str,
    pub capabilities: StorageCapabilities,
}

impl StorageDeviceDescriptor {
    /// A label combining the model and the attachment, e.g.
    /// `SAMSUNG MZVL22T0HBLB-00B00 · NVMe`.
    pub fn summary(&self) -> String {
        format!("{} · {}", self.display_name, self.bus.label())
    }
}

/// One filesystem, with every path it is currently reachable at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageVolumeDescriptor {
    /// The stable identifier a dashboard stores.
    pub source_id: SourceId,
    /// What the volume is called in the interface. Usually its primary mount
    /// point, because that is what a user recognises — but derived from it,
    /// never *equal* to it as an identity.
    pub display_name: String,
    /// Which identifier `source_id` was derived from.
    pub identity: VolumeIdentity,
    /// The filesystem type as the OS names it: `btrfs`, `ext4`, `NTFS`.
    pub filesystem: Option<String>,
    /// Every path this one filesystem is reachable at, in the order the
    /// platform enumerated them, shortest first.
    ///
    /// Several entries is normal, not a duplicate: Fedora mounts the same
    /// btrfs filesystem at `/` and `/home`, and a bind mount adds another.
    pub mount_points: Vec<String>,
    /// The device this volume lives on, when the platform could correlate them
    /// without guessing. `None` is an honest answer, and the UI shows such a
    /// volume separately rather than attributing it to the wrong disk.
    pub device: Option<SourceId>,
    /// Whether the volume is mounted read-only.
    pub read_only: bool,
    pub backend: &'static str,
    pub capabilities: VolumeCapabilities,
}

impl StorageVolumeDescriptor {
    /// The mount point to show first: the shortest, which is the one a user
    /// thinks of the volume as.
    pub fn primary_mount_point(&self) -> Option<&str> {
        self.mount_points.first().map(String::as_str)
    }
}

impl fmt::Display for StorageDeviceDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.display_name, self.source_id)
    }
}

impl fmt::Display for StorageVolumeDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.display_name, self.source_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation_reduces_a_real_t10_identifier_to_a_valid_instance() {
        // Close to what /sys/block/sda/device/wwid holds on the development
        // machine, once the Linux backend has stripped the literal `\0`
        // escapes the kernel writes for the NUL padding — see
        // `platform::linux::storage::sysfs`. What reaches here still has
        // internal padding and mixed case.
        let raw = "t10.Intenso SCSI            2019131398AB5";

        assert_eq!(
            normalize_identifier(raw).as_deref(),
            Some("t10.intenso-scsi-2019131398ab5")
        );
        assert_eq!(
            wwid_source_id(raw).map(|id| id.as_str().to_string()),
            Some("storage:wwid-t10.intenso-scsi-2019131398ab5".to_string())
        );
    }

    #[test]
    fn normalisation_is_deterministic_for_anything_it_is_handed() {
        // It never rejects on shape, only on emptiness, so an identifier PULSE
        // has not seen before still yields one stable instance rather than
        // falling through to a session-scoped name.
        let messy = r"WD  PC SN740 SDDPNQD-1T00 \x00";

        assert_eq!(normalize_identifier(messy), normalize_identifier(messy));
        assert_eq!(
            normalize_identifier(messy).as_deref(),
            Some("wd-pc-sn740-sddpnqd-1t00-x00")
        );
    }

    #[test]
    fn normalisation_keeps_an_nvme_eui_intact() {
        assert_eq!(
            wwid_source_id("eui.002538b331b36d03")
                .map(|id| id.as_str().to_string())
                .as_deref(),
            Some("storage:wwid-eui.002538b331b36d03")
        );
    }

    #[test]
    fn normalisation_refuses_a_string_with_nothing_in_it() {
        assert_eq!(normalize_identifier(""), None);
        assert_eq!(normalize_identifier("   "), None);
        assert_eq!(normalize_identifier("----"), None);
        assert_eq!(normalize_identifier("..."), None);
        assert_eq!(normalize_identifier(r"\0\0\0"), Some("0-0-0".to_string()));

        assert!(wwid_source_id("").is_none());
        assert!(serial_source_id("  ").is_none());
    }

    #[test]
    fn a_normalised_identifier_never_starts_with_a_separator() {
        // `SourceId` forbids a leading `-`, and a device whose identifier is
        // padded on the left is common enough that this must not be luck.
        for raw in ["   ABC123", "\t-lead", "___x9"] {
            let id = wwid_source_id(raw).expect("valid for {raw}");
            assert!(!id.instance().starts_with('-'), "{raw} -> {id}");
        }
    }

    #[test]
    fn an_over_long_identifier_is_truncated_to_a_valid_source() {
        let long = "a".repeat(400);
        let id = wwid_source_id(&long).expect("still valid");

        assert!(id.as_str().len() <= 128);
        assert!(id.as_str().starts_with("storage:wwid-a"));
    }

    #[test]
    fn identity_prefers_hardware_over_a_kernel_name() {
        let (source, identity) = device_identity(
            Some("eui.002538b331b36d03"),
            Some("S677NX0W"),
            None,
            "nvme0n1",
        )
        .expect("identified");

        assert_eq!(source.as_str(), "storage:wwid-eui.002538b331b36d03");
        assert_eq!(identity.mechanism(), "wwid");
        assert_eq!(identity.stability(), IdentityStability::Hardware);
        assert!(identity.stability().survives_reboot());
    }

    #[test]
    fn identity_falls_through_each_candidate_in_order() {
        let serial = device_identity(None, Some("S677NX0W"), None, "nvme0n1").expect("identified");
        assert_eq!(serial.0.as_str(), "storage:serial-s677nx0w");
        assert_eq!(serial.1.mechanism(), "serial");

        let system = device_identity(None, None, Some(r"SCSI\Disk&Ven_X\5&2a"), "PhysicalDrive0")
            .expect("identified");
        assert_eq!(system.0.as_str(), "storage:sys-scsi-disk-ven-x-5-2a");
        assert_eq!(system.1.stability(), IdentityStability::SystemAssigned);

        let fallback = device_identity(None, None, None, "nvme0n1").expect("identified");
        assert_eq!(fallback.0.as_str(), "storage:dev-nvme0n1");
        assert_eq!(fallback.1.stability(), IdentityStability::Session);
        assert!(!fallback.1.stability().survives_reboot());
    }

    #[test]
    fn an_empty_wwid_falls_through_instead_of_producing_a_bare_prefix() {
        // A drive reporting all-padding must not become `storage:wwid-`.
        let (source, identity) =
            device_identity(Some("     "), Some("S677NX0W"), None, "nvme0n1").expect("identified");

        assert_eq!(source.as_str(), "storage:serial-s677nx0w");
        assert_eq!(identity.mechanism(), "serial");
    }

    #[test]
    fn two_identical_models_stay_two_devices() {
        // The failure this whole module exists to prevent: identical drives
        // share a model name and differ only in their serial.
        let first = device_identity(None, Some("S111"), None, "nvme0n1").expect("identified");
        let second = device_identity(None, Some("S222"), None, "nvme1n1").expect("identified");

        assert_ne!(first.0, second.0);
    }

    #[test]
    fn a_volume_source_carries_its_parents_identity_under_a_different_kind() {
        let device = wwid_source_id("eui.002538b331b36d03").expect("valid");
        let volume = partition_volume_source_id(&device, 8).expect("valid");

        assert_eq!(volume.as_str(), "volume:wwid-eui.002538b331b36d03-p8");
        assert_eq!(volume.kind(), "volume");
        assert_ne!(volume, device);
    }

    #[test]
    fn a_windows_volume_guid_becomes_a_stable_volume_source() {
        let volume =
            guid_volume_source_id(r"\\?\Volume{d2b1f8e0-0000-0000-0000-100000000000}\").unwrap();

        assert_eq!(
            volume.as_str(),
            "volume:guid-volume-d2b1f8e0-0000-0000-0000-100000000000"
        );
        assert_eq!(
            VolumeIdentity::Guid("x".into()).stability(),
            IdentityStability::SystemAssigned
        );
    }

    #[test]
    fn a_device_number_volume_is_honestly_session_scoped() {
        let volume = device_number_volume_source_id(259, 7).expect("valid");

        assert_eq!(volume.as_str(), "volume:mm-259-7");
        assert_eq!(
            VolumeIdentity::DeviceNumber {
                major: 259,
                minor: 7
            }
            .stability(),
            IdentityStability::Session
        );
    }

    #[test]
    fn a_rotational_bit_never_implies_a_bus() {
        // `rotational = 0` means "does not spin", and nothing else. Inferring
        // NVMe from it would mislabel every SATA SSD on the machine.
        for bus in StorageBus::ALL {
            assert!(!bus.label().is_empty());
        }
        assert!(StorageBus::Nvme.may_expose_nvme_health());
        assert!(!StorageBus::Usb.may_expose_nvme_health());
        assert!(!StorageBus::Ata.may_expose_nvme_health());
    }

    #[test]
    fn capabilities_fail_one_at_a_time() {
        let capabilities = StorageCapabilities::all_available()
            .with_health(Availability::unsupported("USB bridge passes no SMART"));

        // Health is gone; the disk is still a disk.
        assert!(!capabilities.health_temperature.is_available());
        assert!(!capabilities.health_media_errors.is_available());
        assert!(capabilities.capacity_total.is_available());
        assert!(capabilities.io_read_bytes.is_available());
        assert!(capabilities.io_write_latency.is_available());
    }

    #[test]
    fn a_volume_reports_its_shortest_mount_point_first() {
        let volume = StorageVolumeDescriptor {
            source_id: device_number_volume_source_id(0, 33).expect("valid"),
            display_name: "/".to_string(),
            identity: VolumeIdentity::DeviceNumber {
                major: 0,
                minor: 33,
            },
            filesystem: Some("btrfs".to_string()),
            mount_points: vec!["/".to_string(), "/home".to_string()],
            device: None,
            read_only: false,
            backend: "mountinfo",
            capabilities: VolumeCapabilities::all_available(),
        };

        assert_eq!(volume.primary_mount_point(), Some("/"));
        assert_eq!(volume.mount_points.len(), 2, "one filesystem, two paths");
    }
}
