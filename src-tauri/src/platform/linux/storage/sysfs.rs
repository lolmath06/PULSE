//! The physical storage inventory, read from `/sys/class/block`.
//!
//! Plain file reads, no subprocess. PULSE never shells out to `lsblk`,
//! `blkid`, `udevadm`, `smartctl` or `nvme`: those tools read exactly the
//! files below, and spawning one would add a dependency, a parsing surface and
//! a process per refresh for information already sitting in `sysfs`.
//!
//! # What counts as a device
//!
//! `/sys/class/block` lists every block device the kernel knows, which is a
//! much larger set than "disks this machine has":
//!
//! ```text
//! nvme0n1      a disk                              counted
//! nvme0n1p8    a partition of that disk            not a device of its own
//! sda          a disk behind a USB bridge          counted
//! loop0        a file presented as a block device  not hardware
//! zram0        compressed RAM                      not storage
//! dm-0         a device-mapper mapping             not an extra disk
//! vda          a paravirtualised disk              counted
//! ```
//!
//! The two easy mistakes are counting partitions — turning one NVMe drive into
//! five "disks" — and filtering on "is it real hardware", which would drop the
//! `virtio` disk that *is* the only storage a VM has. The filter is therefore
//! about **whether the entry is a usable block device in its own right**, not
//! about whether silicon is involved.
//!
//! # What is read once
//!
//! Everything here is static for the life of the process: model, serial, WWID,
//! capacity, bus, rotational and removable flags do not change while a device
//! stays attached. The provider reads them at startup and never again; a
//! refresh touches `/proc/diskstats`, `statvfs` and the health sources only.

use std::fs;
use std::path::{Path, PathBuf};

use crate::metrics::wellknown::storage::{
    device_identity, StorageBus, StorageDeviceDescriptor, StorageIdentity,
};

/// Where the kernel lists every block device.
pub const BLOCK_ROOT: &str = "/sys/class/block";

/// Kernel name prefixes that are block devices but not storage this machine
/// has.
///
/// - `loop` — a file made to look like a disk. Fedora has eight by default for
///   snap packages, and counting them would report nine disks on a laptop with
///   one.
/// - `ram` — the legacy RAM disk driver.
/// - `zram` — compressed swap in RAM. Real, useful, and not a storage device.
/// - `dm-` — a device-mapper mapping (LVM, LUKS). It is a *view* of storage
///   that is already counted through the disk underneath, so counting it again
///   would double the machine's capacity.
/// - `md` — likewise for software RAID.
/// - `sr`, `fd` — optical and floppy drives, which have no capacity of their
///   own and no meaningful I/O baseline.
const EXCLUDED_PREFIXES: &[&str] = &["loop", "ram", "zram", "dm-", "md", "sr", "fd"];

/// One block device entry, with the sysfs paths its attributes live under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDevice {
    /// The kernel name: `nvme0n1`, `sda`.
    pub name: String,
    /// `/sys/class/block/<name>`.
    pub path: PathBuf,
}

/// Whether a kernel name is a device PULSE inventories.
///
/// Pure, so the rule is tested directly rather than through a filesystem.
pub fn is_inventoried_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    !EXCLUDED_PREFIXES.iter().any(|prefix| {
        // `sr0` is excluded, `sda` is not: the prefix must be followed by a
        // digit (or `-`, for `dm-0`), otherwise `ram` would also exclude a
        // hypothetical `ramses0` and `sd` would be caught by nothing at all.
        name.strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
    })
}

/// Reads a sysfs attribute, trimming the trailing newline.
///
/// Returns `None` for a missing file, an unreadable one or an empty value —
/// all of which mean the same thing to the caller: this device does not report
/// that attribute.
fn attribute(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let trimmed = value.trim();

    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Strips the literal `\0` escapes the kernel writes for NUL padding.
///
/// `/sys/block/sda/device/wwid` on a USB bridge ends
/// `…2019131398AB5\0\0\0`: the SCSI layer emits the identifier's trailing NUL
/// bytes as two-character escapes. They are padding, not part of the
/// identifier, and leaving them in would make the derived `SourceId` depend on
/// how many NULs the bridge happened to pad with.
pub fn strip_nul_escapes(value: &str) -> &str {
    let mut end = value.len();

    while end >= 2 && &value[end - 2..end] == r"\0" {
        end -= 2;
    }

    value[..end].trim_end()
}

/// Reads an attribute as an unsigned integer.
fn numeric_attribute(path: &Path) -> Option<u64> {
    attribute(path)?.parse().ok()
}

/// Reads a `0`/`1` attribute as a boolean.
fn flag_attribute(path: &Path) -> Option<bool> {
    match numeric_attribute(path)? {
        0 => Some(false),
        1 => Some(true),
        // Anything else is not a flag, and guessing would be worse than
        // reporting the attribute as absent.
        _ => None,
    }
}

/// The capacity of a block device, in bytes.
///
/// # `size` is in 512-byte sectors, always
///
/// `/sys/block/<dev>/size` is documented as a count of **512-byte sectors**,
/// regardless of the device's logical block size. The tempting formula
///
/// ```text
/// size × queue/logical_block_size      // WRONG
/// ```
///
/// is correct only by accident on the 512-byte-logical drives that make up
/// most hardware, and reports **eight times the real capacity** on a 4Kn
/// drive — a 2 TB SSD shown as 16 TB. The logical block size is a property of
/// how the device is addressed and has nothing to do with the unit `size` is
/// expressed in.
pub fn capacity_bytes(sectors: u64) -> Option<u64> {
    sectors.checked_mul(SYSFS_SECTOR_BYTES)
}

/// The unit `/sys/block/<dev>/size` is expressed in. Fixed by the kernel ABI.
pub const SYSFS_SECTOR_BYTES: u64 = 512;

/// Derives the attachment from a device's resolved sysfs path.
///
/// The path is the parent chain the kernel built while probing, so it names
/// every layer the device sits behind:
///
/// ```text
/// …/pci0000:00/…/nvme/nvme0/nvme0n1                            NVMe
/// …/pci0000:00/0000:00:14.0/usb2/2-4/…/block/sda               USB
/// …/pci0000:00/0000:00:17.0/ata3/host2/…/block/sdb             SATA
/// …/pci0000:00/…/virtio3/block/vda                             virtio
/// ```
///
/// A USB bridge is reported as USB even when an NVMe or SATA drive sits behind
/// it, because from PULSE's side of the bridge that is genuinely all that can
/// be known — and it is exactly why health data is usually unreachable there.
/// USB is therefore checked **first**: a drive reached through a bridge is not
/// a directly-attached drive, whatever the bridge chose to call itself.
pub fn bus_from_device_path(path: &str) -> StorageBus {
    let segments: Vec<&str> = path.split('/').collect();

    let has = |predicate: &dyn Fn(&str) -> bool| segments.iter().any(|segment| predicate(segment));

    if has(&|segment: &str| segment.starts_with("usb") || segment == "usb") {
        return StorageBus::Usb;
    }
    if has(&|segment: &str| segment == "nvme" || segment.starts_with("nvme")) {
        return StorageBus::Nvme;
    }
    if has(&|segment: &str| {
        segment
            .strip_prefix("ata")
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
    }) {
        return StorageBus::Ata;
    }
    if has(&|segment: &str| segment.starts_with("virtio") || segment.starts_with("vbd")) {
        return StorageBus::Virtual;
    }
    if has(&|segment: &str| segment.starts_with("mmc")) {
        return StorageBus::Mmc;
    }
    if has(&|segment: &str| {
        segment
            .strip_prefix("host")
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
    }) {
        return StorageBus::Scsi;
    }

    StorageBus::Unknown
}

/// Everything one device reports about itself.
///
/// Separated from [`describe`] so the whole mapping — identity choice, label
/// construction, capacity arithmetic — is testable from a fixture tree without
/// touching the real `/sys`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockAttributes {
    pub name: String,
    /// `/sys/block/<dev>/size`, in 512-byte sectors.
    pub size_sectors: Option<u64>,
    pub rotational: Option<bool>,
    pub removable: Option<bool>,
    pub model: Option<String>,
    pub vendor: Option<String>,
    pub serial: Option<String>,
    pub wwid: Option<String>,
    /// What the enclosure calls itself, when the device sits behind a bridge
    /// that has its own descriptors. Presentation only.
    pub enclosure: Option<String>,
    /// The resolved parent chain, used only to derive the bus.
    pub device_path: String,
}

/// SCSI inquiry model strings that name a protocol rather than a product.
///
/// A USB bridge answers the SCSI inquiry on the drive's behalf, and a great
/// many of them fill the model field with a placeholder. The development
/// machine's external disk reports vendor `Intenso` and model `SCSI`, which
/// renders as "Intenso SCSI" — technically what the device said, and useless
/// as a way to recognise which disk is which.
const GENERIC_MODELS: &[&str] = &[
    "scsi",
    "disk",
    "usb",
    "usb device",
    "usb disk",
    "external",
    "mass storage",
    "mass storage device",
    "storage device",
    "generic",
];

fn is_generic_model(model: &str) -> bool {
    GENERIC_MODELS.contains(&model.trim().to_ascii_lowercase().as_str())
}

/// What the enclosure a device sits in calls itself.
///
/// Walks up the resolved device chain looking for a node that carries USB
/// device descriptors — `idVendor` alongside `manufacturer` and `product`.
/// That is the enclosure, and its own descriptors are usually far more
/// recognisable than the SCSI inquiry strings its bridge invents.
///
/// **Presentation only**, like every other name here: two identical enclosures
/// report identical descriptors, so this never touches identity.
pub fn enclosure_name(device_path: &Path) -> Option<String> {
    let mut current = Some(device_path);

    while let Some(path) = current {
        if path.join("idVendor").exists() {
            let manufacturer = attribute(&path.join("manufacturer"));
            let product = attribute(&path.join("product"));

            return match (manufacturer, product) {
                (Some(manufacturer), Some(product)) => {
                    if product
                        .to_ascii_lowercase()
                        .starts_with(&manufacturer.to_ascii_lowercase())
                    {
                        Some(product)
                    } else {
                        Some(format!("{manufacturer} {product}"))
                    }
                }
                (None, Some(product)) => Some(product),
                (Some(manufacturer), None) => Some(manufacturer),
                (None, None) => None,
            };
        }

        current = path.parent();
    }

    None
}

/// Reads one device's static attributes.
///
/// Every read is independently optional: a device that reports no serial, no
/// model or no WWID is still a device, and the identity logic falls through to
/// whatever it does report.
pub fn read_attributes(root: &Path, name: &str) -> BlockAttributes {
    let base = root.join(name);
    let device = base.join("device");

    // NVMe namespaces carry the WWID at the block level; SCSI devices carry it
    // under the SCSI device. Both spellings are checked, in that order.
    let wwid = attribute(&base.join("wwid"))
        .or_else(|| attribute(&device.join("wwid")))
        .map(|raw| strip_nul_escapes(&raw).to_string())
        .filter(|value| !value.is_empty());

    let device_path = fs::canonicalize(&base).unwrap_or_else(|_| base.clone());

    BlockAttributes {
        name: name.to_string(),
        size_sectors: numeric_attribute(&base.join("size")),
        rotational: flag_attribute(&base.join("queue/rotational")),
        removable: flag_attribute(&base.join("removable")),
        model: attribute(&device.join("model")),
        vendor: attribute(&device.join("vendor")),
        serial: attribute(&device.join("serial")),
        wwid,
        enclosure: enclosure_name(&device_path),
        device_path: device_path.to_string_lossy().into_owned(),
    }
}

/// Builds the user-facing name of a device.
///
/// **Presentation only.** Two identical drives produce the same string, which
/// is precisely why this is never an identity.
///
/// SCSI devices report vendor and model in separate, space-padded fields; NVMe
/// reports one model string that usually already contains the vendor. Both are
/// collapsed here, and a device that reports neither falls back to its kernel
/// name so the row is never blank.
///
/// When the model names a *protocol* rather than a product — a USB bridge
/// answering the SCSI inquiry with `SCSI` on the drive's behalf — the
/// enclosure's own USB descriptors are preferred instead, because
/// `Intenso USB3.0 Device` tells the user which disk this is and
/// `Intenso SCSI` does not.
pub fn display_name(attributes: &BlockAttributes) -> String {
    let clean = |value: &Option<String>| -> Option<String> {
        let trimmed = value.as_deref()?.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };

    let vendor = clean(&attributes.vendor);
    let model = clean(&attributes.model);

    // A model that is a placeholder, or missing entirely, yields to whatever
    // the enclosure calls itself.
    if model.as_deref().map(is_generic_model).unwrap_or(true) {
        if let Some(enclosure) = clean(&attributes.enclosure) {
            return enclosure;
        }
    }

    match (vendor, model) {
        // No enclosure to fall back on: the bridge's own inquiry strings still
        // beat showing `sda`.
        (Some(vendor), Some(model)) => {
            if model
                .to_ascii_lowercase()
                .starts_with(&vendor.to_ascii_lowercase())
            {
                model
            } else {
                format!("{vendor} {model}")
            }
        }
        (None, Some(model)) => model,
        (Some(vendor), None) => vendor,
        (None, None) => attributes.name.clone(),
    }
}

/// Turns one device's attributes into a descriptor, capabilities aside.
///
/// Returns `None` only when no identity at all could be derived, which cannot
/// happen for a device that has a kernel name — the signature keeps the
/// failure representable rather than unwrapping.
pub fn describe(
    attributes: &BlockAttributes,
) -> Option<(StorageDeviceDescriptor, StorageIdentity)> {
    let (source_id, identity) = device_identity(
        attributes.wwid.as_deref(),
        attributes.serial.as_deref(),
        None,
        &attributes.name,
    )?;

    let descriptor = StorageDeviceDescriptor {
        source_id,
        display_name: display_name(attributes),
        os_name: attributes.name.clone(),
        identity: identity.clone(),
        bus: bus_from_device_path(&attributes.device_path),
        capacity_bytes: attributes.size_sectors.and_then(capacity_bytes),
        rotational: attributes.rotational,
        removable: attributes.removable,
        backend: "sysfs",
        // Filled in by the provider, which knows which backends answered.
        capabilities: crate::metrics::wellknown::storage::StorageCapabilities::all_available(),
    };

    Some((descriptor, identity))
}

/// Lists the block devices PULSE inventories under `root`.
///
/// Sorted by kernel name so the inventory is deterministic across runs even
/// though `read_dir` is not.
///
/// A partition is recognised by the `partition` attribute the kernel creates
/// for it, rather than by pattern-matching names like `nvme0n1p8` or `sda1` —
/// naming schemes differ between drivers, and a rule built on them would
/// eventually classify a disk as a partition or the reverse.
pub fn discover_in(root: &Path) -> Vec<BlockDevice> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut devices: Vec<BlockDevice> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_inventoried_name(&name) {
                return None;
            }

            let path = root.join(&name);
            // The kernel writes this attribute for partitions and nothing
            // else. Its presence is the definition.
            if path.join("partition").exists() {
                return None;
            }

            Some(BlockDevice { name, path })
        })
        .collect();

    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices
}

/// Lists the block devices on this machine.
pub fn discover() -> Vec<BlockDevice> {
    discover_in(Path::new(BLOCK_ROOT))
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A throwaway `/sys` tree: a `class/block` directory to enumerate, and a
    /// `devices` directory the entries in it can point into.
    ///
    /// The two are kept apart exactly as the kernel keeps them, so nothing the
    /// fixture creates to model a device chain can be mistaken for a block
    /// device of its own. Nothing in the test suite writes to the real `/sys`,
    /// and nothing needs a machine with a particular disk in it.
    pub struct BlockFixture {
        base: PathBuf,
        root: PathBuf,
    }

    impl BlockFixture {
        pub fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "pulse-block-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&base);

            let root = base.join("class").join("block");
            fs::create_dir_all(&root).expect("create fixture root");

            Self { base, root }
        }

        /// The directory that stands in for `/sys/class/block`.
        pub fn root(&self) -> &Path {
            &self.root
        }

        /// Creates a disk whose entry is a symlink into a fake kernel device
        /// chain, the way `/sys/class/block` really works.
        ///
        /// `chain` is the path under the fixture's own `devices/` directory,
        /// e.g. `pci0000:00/0000:00:14.0/usb2/2-4/host0/block`. Resolving the
        /// symlink is what lets the bus-detection rule be tested without a
        /// machine that has that hardware in it.
        #[cfg(unix)]
        pub fn disk_on(&self, name: &str, chain: &str, attributes: &[(&str, &str)]) -> PathBuf {
            let target = self.base.join("devices").join(chain).join(name);
            fs::create_dir_all(&target).expect("create device chain");

            let link = self.root.join(name);
            let _ = fs::remove_file(&link);
            std::os::unix::fs::symlink(&target, link).expect("link device");

            self.write_attributes(&target, attributes);
            target
        }

        /// Creates a disk with the given attributes.
        pub fn disk(&self, name: &str, attributes: &[(&str, &str)]) -> PathBuf {
            let path = self.root.join(name);
            self.write_attributes(&path, attributes);
            path
        }

        fn write_attributes(&self, path: &Path, attributes: &[(&str, &str)]) {
            fs::create_dir_all(path.join("queue")).expect("create device");
            fs::create_dir_all(path.join("device")).expect("create device node");

            for (attribute, value) in attributes {
                let target = path.join(attribute);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).expect("create attribute directory");
                }
                fs::write(target, format!("{value}\n")).expect("write attribute");
            }
        }

        /// Creates a partition of a disk, recognisable by its `partition`
        /// attribute.
        pub fn partition(&self, name: &str, ordinal: u32, sectors: u64) -> PathBuf {
            let path = self.disk(name, &[]);
            fs::write(path.join("partition"), format!("{ordinal}\n")).expect("write partition");
            fs::write(path.join("size"), format!("{sectors}\n")).expect("write size");
            path
        }
    }

    impl Drop for BlockFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::BlockFixture;
    use super::*;

    // --- what counts as a device ------------------------------------------

    #[test]
    fn pseudo_devices_are_not_counted_as_disks() {
        for name in [
            "loop0", "loop7", "ram0", "ram15", "zram0", "dm-0", "dm-12", "md0", "sr0", "fd0",
        ] {
            assert!(!is_inventoried_name(name), "'{name}' must be excluded");
        }
    }

    #[test]
    fn real_and_virtual_disks_are_both_counted() {
        // Virtualisation is not a reason to hide the only disk a VM has.
        for name in [
            "nvme0n1", "nvme1n1", "sda", "sdb", "sdaa", "vda", "xvda", "mmcblk0", "hda",
        ] {
            assert!(is_inventoried_name(name), "'{name}' must be inventoried");
        }
    }

    #[test]
    fn the_exclusion_matches_a_prefix_plus_a_number_not_a_bare_prefix() {
        // `sda` starts with `sd`, not with an excluded prefix; `ramdisk` is
        // not `ram0`.
        assert!(is_inventoried_name("sda"));
        assert!(is_inventoried_name("ramdisk"));
        assert!(is_inventoried_name("loopback"));
        assert!(!is_inventoried_name("loop0"));
        assert!(!is_inventoried_name(""));
    }

    #[test]
    fn a_disk_with_partitions_counts_once() {
        // The failure this prevents: one NVMe drive reported as nine disks.
        let fixture = BlockFixture::new("partitions");
        fixture.disk("nvme0n1", &[("size", "4000797360")]);
        for ordinal in 1..=8 {
            fixture.partition(&format!("nvme0n1p{ordinal}"), ordinal, 1000);
        }

        let discovered = discover_in(fixture.root());

        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].name, "nvme0n1");
    }

    #[test]
    fn loop_and_zram_devices_never_reach_the_inventory() {
        let fixture = BlockFixture::new("pseudo");
        fixture.disk("nvme0n1", &[("size", "1000")]);
        for index in 0..8 {
            fixture.disk(&format!("loop{index}"), &[("size", "8")]);
        }
        fixture.disk("zram0", &[("size", "16")]);
        fixture.disk("dm-0", &[("size", "2000")]);

        let names: Vec<String> = discover_in(fixture.root())
            .into_iter()
            .map(|device| device.name)
            .collect();

        assert_eq!(names, ["nvme0n1"]);
    }

    #[test]
    fn a_mixed_machine_is_enumerated_deterministically() {
        let fixture = BlockFixture::new("mixed");
        for name in ["sdb", "nvme0n1", "vda", "sda"] {
            fixture.disk(name, &[("size", "1000")]);
        }

        let names: Vec<String> = discover_in(fixture.root())
            .into_iter()
            .map(|device| device.name)
            .collect();

        assert_eq!(names, ["nvme0n1", "sda", "sdb", "vda"]);
    }

    #[test]
    fn a_missing_sysfs_tree_yields_an_empty_inventory_rather_than_a_panic() {
        assert!(discover_in(Path::new("/nonexistent/pulse/block")).is_empty());
    }

    // --- capacity ---------------------------------------------------------

    #[test]
    fn capacity_treats_size_as_512_byte_sectors() {
        // The real NVMe drive on the development machine.
        assert_eq!(capacity_bytes(4_000_797_360), Some(2_048_408_248_320));
        // And the real USB disk.
        assert_eq!(capacity_bytes(1_953_525_168), Some(1_000_204_886_016));
    }

    #[test]
    fn capacity_never_multiplies_by_the_logical_block_size() {
        // The classic bug: a 4Kn drive whose `size` is already in 512-byte
        // sectors, multiplied by its 4096-byte logical block size, is reported
        // at eight times its real capacity.
        let sectors = 4_000_797_360_u64;
        let correct = capacity_bytes(sectors).expect("valid");
        let wrong = sectors * 4096;

        assert_eq!(correct, 2_048_408_248_320);
        assert_eq!(wrong, correct * 8);
        assert_ne!(correct, wrong);
    }

    #[test]
    fn capacity_refuses_to_overflow() {
        assert_eq!(capacity_bytes(u64::MAX), None);
        assert_eq!(capacity_bytes(0), Some(0));
    }

    // --- bus --------------------------------------------------------------

    #[test]
    fn the_bus_comes_from_the_kernel_device_chain() {
        assert_eq!(
            bus_from_device_path(
                "/sys/devices/pci0000:00/0000:00:0e.0/pci10000:e0/10000:e1:00.0/nvme/nvme0/nvme0n1"
            ),
            StorageBus::Nvme
        );
        assert_eq!(
            bus_from_device_path(
                "/sys/devices/pci0000:00/0000:00:14.0/usb2/2-4/2-4:1.0/host0/target0:0:0/0:0:0:0/block/sda"
            ),
            StorageBus::Usb
        );
        assert_eq!(
            bus_from_device_path(
                "/sys/devices/pci0000:00/0000:00:17.0/ata3/host2/target2:0:0/2:0:0:0/block/sdb"
            ),
            StorageBus::Ata
        );
        assert_eq!(
            bus_from_device_path("/sys/devices/pci0000:00/0000:00:07.0/virtio3/block/vda"),
            StorageBus::Virtual
        );
        assert_eq!(
            bus_from_device_path("/sys/devices/platform/soc/mmc_host/mmc0/mmc0:0001/block/mmcblk0"),
            StorageBus::Mmc
        );
        assert_eq!(
            bus_from_device_path("/sys/devices/virtual/block/nothing"),
            StorageBus::Unknown
        );
    }

    #[test]
    fn a_drive_behind_a_usb_bridge_is_reported_as_usb() {
        // Even though the bridge presents itself as SCSI, and even if an NVMe
        // drive sits behind it: USB is what PULSE can actually see, and it is
        // why health data will not be reachable.
        let path = "/sys/devices/pci0000:00/0000:00:14.0/usb2/2-4/2-4:1.0/host0/target0:0:0/0:0:0:0/block/sda";

        assert_eq!(bus_from_device_path(path), StorageBus::Usb);
        assert!(!bus_from_device_path(path).may_expose_nvme_health());
    }

    #[test]
    fn a_non_rotating_device_is_not_assumed_to_be_nvme() {
        // `rotational = 0` says the medium does not spin and nothing else.
        let attributes = BlockAttributes {
            name: "sdb".to_string(),
            rotational: Some(false),
            device_path:
                "/sys/devices/pci0000:00/0000:00:17.0/ata3/host2/target2:0:0/2:0:0:0/block/sdb"
                    .to_string(),
            ..BlockAttributes::default()
        };

        let (descriptor, _) = describe(&attributes).expect("described");
        assert_eq!(descriptor.bus, StorageBus::Ata);
        assert_eq!(descriptor.rotational, Some(false));
    }

    // --- attributes and identity ------------------------------------------

    #[test]
    fn reads_the_attributes_a_real_nvme_drive_exposes() {
        let fixture = BlockFixture::new("nvme");
        fixture.disk(
            "nvme0n1",
            &[
                ("size", "4000797360"),
                ("queue/rotational", "0"),
                ("removable", "0"),
                ("wwid", "eui.002538b331b36d03"),
                ("device/model", "SAMSUNG MZVL22T0HBLB-00B00              "),
                ("device/serial", "S677NX0W"),
            ],
        );

        let attributes = read_attributes(fixture.root(), "nvme0n1");

        assert_eq!(attributes.size_sectors, Some(4_000_797_360));
        assert_eq!(attributes.rotational, Some(false));
        assert_eq!(attributes.removable, Some(false));
        assert_eq!(attributes.wwid.as_deref(), Some("eui.002538b331b36d03"));
        assert_eq!(attributes.serial.as_deref(), Some("S677NX0W"));

        let (descriptor, identity) = describe(&attributes).expect("described");

        assert_eq!(
            descriptor.source_id.as_str(),
            "storage:wwid-eui.002538b331b36d03"
        );
        assert_eq!(identity.mechanism(), "wwid");
        assert_eq!(descriptor.display_name, "SAMSUNG MZVL22T0HBLB-00B00");
        assert_eq!(descriptor.capacity_bytes, Some(2_048_408_248_320));
        assert_eq!(descriptor.os_name, "nvme0n1");
    }

    #[test]
    fn strips_the_nul_escapes_a_scsi_wwid_is_padded_with() {
        // Verbatim from /sys/block/sda/device/wwid on the development machine.
        assert_eq!(
            strip_nul_escapes(r"t10.Intenso SCSI            2019131398AB5\0\0\0"),
            "t10.Intenso SCSI            2019131398AB5"
        );
        assert_eq!(strip_nul_escapes("clean"), "clean");
        assert_eq!(strip_nul_escapes(r"\0\0"), "");
    }

    #[test]
    fn a_usb_bridge_wwid_still_yields_a_hardware_identity() {
        let fixture = BlockFixture::new("usb");
        fixture.disk(
            "sda",
            &[
                ("size", "1953525168"),
                ("queue/rotational", "1"),
                ("removable", "0"),
                (
                    "device/wwid",
                    r"t10.Intenso SCSI            2019131398AB5\0\0\0",
                ),
                ("device/vendor", "Intenso "),
                ("device/model", "SCSI            "),
            ],
        );

        let attributes = read_attributes(fixture.root(), "sda");
        let (descriptor, identity) = describe(&attributes).expect("described");

        assert_eq!(
            descriptor.source_id.as_str(),
            "storage:wwid-t10.intenso-scsi-2019131398ab5"
        );
        assert_eq!(
            identity.stability(),
            crate::metrics::wellknown::storage::IdentityStability::Hardware
        );
        assert_eq!(descriptor.display_name, "Intenso SCSI");
        assert_eq!(descriptor.rotational, Some(true));
    }

    #[test]
    fn a_device_with_no_wwid_falls_back_to_its_serial() {
        let fixture = BlockFixture::new("serial-only");
        fixture.disk(
            "sdb",
            &[("size", "100"), ("device/serial", "WD-WCC4N1234567")],
        );

        let (descriptor, identity) =
            describe(&read_attributes(fixture.root(), "sdb")).expect("described");

        assert_eq!(
            descriptor.source_id.as_str(),
            "storage:serial-wd-wcc4n1234567"
        );
        assert_eq!(identity.mechanism(), "serial");
    }

    #[test]
    fn a_device_with_neither_falls_back_to_its_kernel_name_and_says_so() {
        let fixture = BlockFixture::new("anonymous");
        fixture.disk("vda", &[("size", "100")]);

        let (descriptor, identity) =
            describe(&read_attributes(fixture.root(), "vda")).expect("described");

        assert_eq!(descriptor.source_id.as_str(), "storage:dev-vda");
        assert_eq!(identity.mechanism(), "os-name");
        assert!(!identity.stability().survives_reboot());
        // No model either: the row shows the kernel name rather than nothing.
        assert_eq!(descriptor.display_name, "vda");
    }

    #[test]
    fn two_identical_models_get_two_identities() {
        let fixture = BlockFixture::new("twins");
        for (name, serial) in [("nvme0n1", "S111AAAA"), ("nvme1n1", "S222BBBB")] {
            fixture.disk(
                name,
                &[
                    ("size", "1000"),
                    ("device/model", "SAMSUNG MZVL22T0HBLB-00B00"),
                    ("device/serial", serial),
                ],
            );
        }

        let first = describe(&read_attributes(fixture.root(), "nvme0n1")).expect("described");
        let second = describe(&read_attributes(fixture.root(), "nvme1n1")).expect("described");

        assert_eq!(first.0.display_name, second.0.display_name);
        assert_ne!(
            first.0.source_id, second.0.source_id,
            "same model, two disks"
        );
    }

    #[test]
    fn a_missing_attribute_is_absent_rather_than_zero() {
        let fixture = BlockFixture::new("sparse");
        fixture.disk("sda", &[]);

        let attributes = read_attributes(fixture.root(), "sda");

        assert_eq!(attributes.size_sectors, None);
        assert_eq!(attributes.rotational, None);
        assert_eq!(attributes.removable, None);
        assert_eq!(attributes.model, None);

        let (descriptor, _) = describe(&attributes).expect("described");
        assert_eq!(
            descriptor.capacity_bytes, None,
            "an unread capacity is not a zero-byte disk"
        );
    }

    #[test]
    fn a_removable_device_is_reported_as_such() {
        let fixture = BlockFixture::new("removable");
        fixture.disk("sdc", &[("size", "100"), ("removable", "1")]);

        let (descriptor, _) = describe(&read_attributes(fixture.root(), "sdc")).expect("described");
        assert_eq!(descriptor.removable, Some(true));
    }

    #[test]
    fn a_nonsensical_flag_is_dropped_rather_than_coerced() {
        let fixture = BlockFixture::new("bad-flag");
        fixture.disk("sdd", &[("size", "100"), ("queue/rotational", "banana")]);

        assert_eq!(read_attributes(fixture.root(), "sdd").rotational, None);
    }

    #[test]
    fn the_display_name_never_repeats_the_vendor() {
        let named = |vendor: Option<&str>, model: Option<&str>| {
            display_name(&BlockAttributes {
                name: "sda".to_string(),
                vendor: vendor.map(str::to_string),
                model: model.map(str::to_string),
                ..BlockAttributes::default()
            })
        };

        assert_eq!(
            named(Some("Samsung"), Some("Samsung SSD 990 PRO")),
            "Samsung SSD 990 PRO"
        );
        // With no enclosure descriptors to fall back on, the bridge's own
        // inquiry strings are still better than showing `sda`.
        assert_eq!(named(Some("Intenso"), Some("SCSI")), "Intenso SCSI");
        assert_eq!(named(None, Some("WDC WDS100T2B0A")), "WDC WDS100T2B0A");
        assert_eq!(named(Some("ATA"), None), "ATA");
        assert_eq!(named(None, None), "sda");
        assert_eq!(
            named(Some("  "), Some("  ")),
            "sda",
            "padding is not a name"
        );
    }

    #[test]
    fn a_bridge_that_names_a_protocol_yields_to_the_enclosures_own_name() {
        // Verbatim from the development machine: the USB bridge answers the
        // SCSI inquiry with vendor `Intenso` and model `SCSI`, while the
        // enclosure's USB descriptors say `Intenso` / `USB3.0 Device`. The
        // second is the one that tells a user which disk this is.
        let attributes = BlockAttributes {
            name: "sda".to_string(),
            vendor: Some("Intenso".to_string()),
            model: Some("SCSI".to_string()),
            enclosure: Some("Intenso USB3.0 Device".to_string()),
            ..BlockAttributes::default()
        };

        assert_eq!(display_name(&attributes), "Intenso USB3.0 Device");
    }

    #[test]
    fn a_real_model_is_never_replaced_by_the_enclosures_name() {
        // A drive that reports its own product name keeps it, even in an
        // enclosure that also has descriptors.
        let attributes = BlockAttributes {
            name: "sdb".to_string(),
            vendor: Some("ATA".to_string()),
            model: Some("WDC WDS100T2B0A".to_string()),
            enclosure: Some("Generic USB3.0 Device".to_string()),
            ..BlockAttributes::default()
        };

        assert_eq!(display_name(&attributes), "ATA WDC WDS100T2B0A");
    }

    #[test]
    fn a_device_with_no_model_at_all_uses_its_enclosure() {
        let attributes = BlockAttributes {
            name: "sdc".to_string(),
            enclosure: Some("Samsung Portable SSD T7".to_string()),
            ..BlockAttributes::default()
        };

        assert_eq!(display_name(&attributes), "Samsung Portable SSD T7");
    }

    #[test]
    fn the_generic_model_list_matches_on_the_whole_string_only() {
        // `SCSI` is a placeholder; `SCSI SSD 2TB` is a product name that
        // happens to start with one.
        assert!(is_generic_model("SCSI"));
        assert!(is_generic_model("  disk  "));
        assert!(is_generic_model("Mass Storage Device"));
        assert!(!is_generic_model("SCSI SSD 2TB"));
        assert!(!is_generic_model("SAMSUNG MZVL22T0HBLB-00B00"));
    }

    #[test]
    fn the_enclosure_name_is_found_by_walking_up_to_the_usb_device() {
        let fixture = BlockFixture::new("enclosure");
        let device = fixture.disk("sda", &[("size", "100")]);

        // The USB device node sits two levels above the block device, exactly
        // as it does under `/sys/devices`.
        let enclosure = device.parent().expect("has a parent").join("usb-enclosure");
        std::fs::create_dir_all(&enclosure).expect("create enclosure");
        for (attribute, value) in [
            ("idVendor", "152d"),
            ("manufacturer", "Intenso"),
            ("product", "USB3.0 Device"),
        ] {
            std::fs::write(enclosure.join(attribute), format!("{value}\n")).expect("write");
        }

        let nested = enclosure.join("host0").join("block").join("sda");
        std::fs::create_dir_all(&nested).expect("create nested");

        assert_eq!(
            enclosure_name(&nested).as_deref(),
            Some("Intenso USB3.0 Device")
        );
    }

    #[test]
    fn a_device_with_no_enclosure_above_it_reports_none() {
        assert_eq!(enclosure_name(Path::new("/nonexistent/pulse/device")), None);
    }

    #[test]
    fn an_enclosure_name_never_repeats_its_manufacturer() {
        let fixture = BlockFixture::new("enclosure-repeat");
        let root = fixture.root().join("samsung-enclosure");
        std::fs::create_dir_all(&root).expect("create");
        for (attribute, value) in [
            ("idVendor", "04e8"),
            ("manufacturer", "Samsung"),
            ("product", "Samsung Portable SSD T7"),
        ] {
            std::fs::write(root.join(attribute), format!("{value}\n")).expect("write");
        }

        assert_eq!(
            enclosure_name(&root).as_deref(),
            Some("Samsung Portable SSD T7")
        );
    }
}
