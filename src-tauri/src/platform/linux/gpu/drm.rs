//! Generic GPU inventory on Fedora, from `/sys/class/drm`.
//!
//! This is the layer that answers "which graphics adapters does this machine
//! have", for every vendor, before any telemetry backend is consulted. It runs
//! even when no vendor backend exists at all — a card with an open-source
//! driver and no counters still appears, correctly named and identified.
//!
//! # What is and is not a GPU
//!
//! `/sys/class/drm` mixes three kinds of entry, and only the first is a device:
//!
//! ```text
//! card0              a DRM device        ← counted
//! card0-DP-1         a display connector ← skipped
//! card0-eDP-1        a display connector ← skipped
//! renderD128         a render node for card0, the same hardware ← skipped
//! version            not a device at all ← skipped
//! ```
//!
//! Counting connectors would report a laptop with four outputs as having five
//! GPUs; counting render nodes would double every card.
//!
//! An entry is a device only when it is named `card<N>` with `<N>` numeric, and
//! it is a *hardware* GPU only when it resolves to a real PCI device. That last
//! check is what excludes `vkms`, `simpledrm` and other virtual devices, which
//! have no PCI parent.
//!
//! # No subprocess
//!
//! Everything is read with `std::fs`. No `lspci`, no `glxinfo`, no
//! `nvidia-smi`, no shell. The names come from the system's own PCI ID
//! database, read as a file — the same data `lspci` prints, without running it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::metrics::model::Availability;
use crate::metrics::wellknown::gpu::{
    pci_source_id, GpuCapabilities, GpuDescriptor, GpuIdentity, GpuVendor, PciAddress,
};

/// Where the kernel exposes DRM devices.
pub const DRM_ROOT: &str = "/sys/class/drm";

/// The system PCI ID database, as shipped by `hwdata`.
///
/// Optional: when absent, devices are named from their raw IDs instead. Read
/// as an ordinary file — this is the same data `lspci` prints, obtained
/// without running it.
const PCI_IDS_PATHS: &[&str] = &[
    "/usr/share/hwdata/pci.ids",
    "/usr/share/misc/pci.ids",
    "/usr/share/pci.ids",
];

/// One graphics adapter as sysfs describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrmCard {
    /// The DRM minor number, e.g. `0` for `card0`.
    ///
    /// **Enumeration order, not identity.** It reflects probe order and can
    /// differ between boots, which is why it never reaches a `SourceId`.
    pub index: u32,
    pub pci: PciAddress,
    pub vendor_id: u16,
    pub device_id: u16,
    /// The kernel driver bound to the device, e.g. `nvidia`, `amdgpu`,
    /// `nouveau`, `i915`. Decides which telemetry backend can serve it.
    pub driver: Option<String>,
}

impl DrmCard {
    pub fn vendor(&self) -> GpuVendor {
        GpuVendor::from_pci_id(self.vendor_id)
    }

    /// The path of this card's device directory, for backends that read more.
    pub fn device_path(&self, root: &Path) -> PathBuf {
        root.join(format!("card{}", self.index)).join("device")
    }
}

/// Reads a sysfs attribute, trimmed.
fn read_attribute(path: PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
}

/// Parses a `0x10de`-style hexadecimal ID.
pub fn parse_hex_id(value: &str) -> Option<u16> {
    let trimmed = value.trim();
    let digits = trimmed.strip_prefix("0x").unwrap_or(trimmed);

    u16::from_str_radix(digits, 16).ok()
}

/// Extracts the DRM minor number from an entry name.
///
/// Returns `None` for connectors (`card0-DP-1`), render nodes (`renderD128`)
/// and anything else — the filter that keeps the count honest.
pub fn parse_card_name(name: &str) -> Option<u32> {
    let suffix = name.strip_prefix("card")?;

    // `card0-DP-1` starts with `card` too; only a purely numeric suffix is a
    // device.
    suffix.parse::<u32>().ok()
}

/// Enumerates graphics adapters under a DRM root.
///
/// Takes the root as a parameter so the whole traversal is tested against a
/// fixture tree rather than against the live `/sys`, which cannot be made to
/// contain two cards, a missing symlink or a virtual device on demand.
///
/// Sorted by PCI address, so the inventory order is stable across runs
/// independently of directory iteration order.
pub fn discover_in(root: &Path) -> Vec<DrmCard> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut cards: Vec<DrmCard> = Vec::new();

    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(index) = parse_card_name(&name) else {
            continue;
        };

        let device = root.join(&name).join("device");

        // A hardware GPU resolves to a real PCI device. Virtual devices
        // (`vkms`, `simpledrm`) do not, and are excluded here.
        let Some(pci) = resolve_pci_address(&device) else {
            continue;
        };
        let Some(vendor_id) = read_attribute(device.join("vendor")).and_then(|v| parse_hex_id(&v))
        else {
            continue;
        };
        let device_id = read_attribute(device.join("device"))
            .and_then(|v| parse_hex_id(&v))
            .unwrap_or(0);

        let driver = fs::read_link(device.join("driver")).ok().and_then(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        });

        cards.push(DrmCard {
            index,
            pci,
            vendor_id,
            device_id,
            driver,
        });
    }

    cards.sort_by_key(|card| card.pci);
    cards.dedup_by_key(|card| card.pci);
    cards
}

/// Enumerates the adapters of the running system.
pub fn discover() -> Vec<DrmCard> {
    discover_in(Path::new(DRM_ROOT))
}

/// Resolves a card's `device` symlink to a PCI bus address.
///
/// The symlink target's final component is the address — `/sys/devices/pci0000:00/
/// 0000:00:01.0/0000:01:00.0` — which is exactly the identity PULSE wants.
fn resolve_pci_address(device: &Path) -> Option<PciAddress> {
    let target = fs::canonicalize(device).ok()?;
    let name = target.file_name()?.to_str()?;

    PciAddress::parse(name)
}

/// A minimal reader for the system PCI ID database.
///
/// Only what a device name needs: the vendor line and its indented device
/// lines. Subsystem entries, device classes and comments are skipped.
///
/// ```text
/// 10de  NVIDIA Corporation
/// \t2820  AD106M [GeForce RTX 4070 Max-Q / Mobile]
/// ```
#[derive(Debug, Default, Clone)]
pub struct PciIds {
    names: BTreeMap<(u16, u16), String>,
    vendors: BTreeMap<u16, String>,
}

impl PciIds {
    /// Parses the database's text.
    pub fn parse(content: &str) -> Self {
        let mut ids = Self::default();
        let mut current_vendor: Option<u16> = None;

        for line in content.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            // Two levels of indentation are subsystem entries, which PULSE
            // does not use.
            if line.starts_with("\t\t") {
                continue;
            }

            if let Some(rest) = line.strip_prefix('\t') {
                let Some(vendor) = current_vendor else {
                    continue;
                };
                if let Some((id, name)) = split_id_and_name(rest) {
                    ids.names.insert((vendor, id), name);
                }
                continue;
            }

            // A non-indented line starts a new vendor — or a device-class
            // section, which is not hexadecimal and simply fails to parse.
            match split_id_and_name(line) {
                Some((id, name)) => {
                    ids.vendors.insert(id, name);
                    current_vendor = Some(id);
                }
                None => current_vendor = None,
            }
        }

        ids
    }

    /// Loads the database from the first path that exists.
    ///
    /// Returns an empty database when none does; names then fall back to the
    /// raw IDs, which is unhelpful but never wrong.
    pub fn load() -> Self {
        for path in PCI_IDS_PATHS {
            if let Ok(content) = fs::read_to_string(path) {
                return Self::parse(&content);
            }
        }

        Self::default()
    }

    /// The marketing name of a device, when the database knows it.
    pub fn device_name(&self, vendor_id: u16, device_id: u16) -> Option<&str> {
        self.names.get(&(vendor_id, device_id)).map(String::as_str)
    }

    pub fn vendor_name(&self, vendor_id: u16) -> Option<&str> {
        self.vendors.get(&vendor_id).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.vendors.is_empty()
    }
}

/// Splits `10de  NVIDIA Corporation` into its ID and name.
fn split_id_and_name(line: &str) -> Option<(u16, String)> {
    let (id, name) = line.split_once("  ")?;
    let id = u16::from_str_radix(id.trim(), 16).ok()?;
    let name = name.trim();

    (!name.is_empty()).then(|| (id, name.to_string()))
}

/// Builds the user-facing name of a card.
///
/// Presentation only — it never participates in identity, so a database update
/// that rewords a device cannot invalidate a saved dashboard.
pub fn display_name(card: &DrmCard, ids: &PciIds) -> String {
    let vendor = card.vendor();

    match ids.device_name(card.vendor_id, card.device_id) {
        Some(name) => format!("{} {name}", vendor.label()),
        // Honest fallback: the raw identifiers, which at least let a user look
        // the device up.
        None => format!(
            "{} GPU {:04x}:{:04x}",
            vendor.label(),
            card.vendor_id,
            card.device_id
        ),
    }
}

/// Turns an enumerated card into a descriptor with no telemetry.
///
/// The starting point every Linux GPU gets: correctly identified, correctly
/// named, and honest that nothing can be measured until a backend claims it.
pub fn describe(card: &DrmCard, ids: &PciIds, reason: &Availability) -> Option<GpuDescriptor> {
    Some(GpuDescriptor {
        source_id: pci_source_id(card.pci)?,
        display_name: display_name(card, ids),
        vendor: card.vendor(),
        identity: GpuIdentity::Pci(card.pci),
        pci: Some(card.pci),
        backend: "drm",
        capabilities: GpuCapabilities::none_available(reason),
    })
}

// The fixture tree builds a `/sys`-shaped directory with symlinks, which is a
// Unix concept; the traversal it exercises only ever runs on Linux. Every
// *pure* test below — name filtering, ID parsing, the PCI database, naming and
// describing — stays compiled everywhere, so a Windows build still checks them.
#[cfg(all(test, unix))]
pub mod fixtures {
    //! Builds a fake `/sys/class/drm` tree, so traversal is tested against
    //! shapes the live system cannot be made to produce on demand.

    use std::fs;
    use std::path::{Path, PathBuf};

    /// A scratch DRM tree that cleans itself up.
    pub struct DrmTree {
        pub root: PathBuf,
        devices: PathBuf,
    }

    impl DrmTree {
        pub fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "pulse-drm-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&base);

            let root = base.join("class/drm");
            let devices = base.join("devices/pci0000:00");
            fs::create_dir_all(&root).expect("fixture root");
            fs::create_dir_all(&devices).expect("fixture devices");

            Self { root, devices }
        }

        /// Adds a `card<N>` backed by a real PCI device directory.
        pub fn add_card(
            &self,
            index: u32,
            bdf: &str,
            vendor_id: &str,
            device_id: &str,
            driver: Option<&str>,
        ) -> &Self {
            let device = self.devices.join(bdf);
            fs::create_dir_all(&device).expect("device dir");
            fs::write(device.join("vendor"), format!("{vendor_id}\n")).expect("vendor");
            fs::write(device.join("device"), format!("{device_id}\n")).expect("device");

            if let Some(driver) = driver {
                let driver_dir = self.devices.join("drivers").join(driver);
                fs::create_dir_all(&driver_dir).expect("driver dir");
                let _ = std::os::unix::fs::symlink(&driver_dir, device.join("driver"));
            }

            let card = self.root.join(format!("card{index}"));
            fs::create_dir_all(&card).expect("card dir");
            let _ = std::os::unix::fs::symlink(&device, card.join("device"));

            self
        }

        /// Adds a display connector, which must not be counted as a GPU.
        pub fn add_connector(&self, name: &str) -> &Self {
            fs::create_dir_all(self.root.join(name)).expect("connector");
            self
        }

        /// Adds a render node, which is the same hardware as its card.
        pub fn add_render_node(&self, name: &str) -> &Self {
            fs::create_dir_all(self.root.join(name)).expect("render node");
            self
        }

        /// Adds a `card<N>` with no `device` symlink at all — a virtual device
        /// such as `vkms`.
        pub fn add_virtual_card(&self, index: u32) -> &Self {
            fs::create_dir_all(self.root.join(format!("card{index}"))).expect("virtual card");
            self
        }

        pub fn path(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for DrmTree {
        fn drop(&mut self) {
            if let Some(base) = self.root.parent().and_then(Path::parent) {
                let _ = fs::remove_dir_all(base);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- entry-name filtering --------------------------------------------

    #[test]
    fn only_numeric_card_entries_are_devices() {
        assert_eq!(parse_card_name("card0"), Some(0));
        assert_eq!(parse_card_name("card1"), Some(1));
        assert_eq!(parse_card_name("card12"), Some(12));
    }

    #[test]
    fn connectors_and_render_nodes_are_not_devices() {
        // The filter that stops a four-output laptop reporting five GPUs.
        for name in [
            "card0-DP-1",
            "card0-eDP-1",
            "card0-HDMI-A-1",
            "card1-DP-2",
            "renderD128",
            "renderD129",
            "version",
            "",
            "card",
            "cardX",
        ] {
            assert_eq!(parse_card_name(name), None, "'{name}' is not a GPU");
        }
    }

    #[test]
    fn parses_the_hexadecimal_ids_sysfs_writes() {
        assert_eq!(parse_hex_id("0x10de"), Some(0x10DE));
        assert_eq!(parse_hex_id("0x1002\n"), Some(0x1002));
        assert_eq!(parse_hex_id("8086"), Some(0x8086));
        assert_eq!(parse_hex_id(""), None);
        assert_eq!(parse_hex_id("zzzz"), None);
    }

    // --- PCI ID database --------------------------------------------------

    const SAMPLE_IDS: &str = "\
# Comment line
10de  NVIDIA Corporation
\t2820  AD106M [GeForce RTX 4070 Max-Q / Mobile]
\t\t1028 0b12  Some subsystem
\t2684  AD102 [GeForce RTX 4090]
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t73ff  Navi 23 [Radeon RX 6600/6600 XT]
8086  Intel Corporation
\t56a0  DG2 [Arc A770]

C 03  Display controller
\t00  VGA compatible controller
";

    #[test]
    fn parses_vendors_and_devices_from_the_database() {
        let ids = PciIds::parse(SAMPLE_IDS);

        assert_eq!(
            ids.device_name(0x10DE, 0x2820),
            Some("AD106M [GeForce RTX 4070 Max-Q / Mobile]")
        );
        assert_eq!(
            ids.device_name(0x1002, 0x73FF),
            Some("Navi 23 [Radeon RX 6600/6600 XT]")
        );
        assert_eq!(ids.device_name(0x8086, 0x56A0), Some("DG2 [Arc A770]"));
        assert_eq!(ids.vendor_name(0x10DE), Some("NVIDIA Corporation"));
    }

    #[test]
    fn subsystem_and_class_sections_are_ignored() {
        let ids = PciIds::parse(SAMPLE_IDS);

        // A subsystem line must not be mistaken for a device of the vendor.
        assert_eq!(ids.device_name(0x10DE, 0x1028), None);
        // The device-class section must not leak in as a vendor.
        assert_eq!(ids.vendor_name(0x0003), None);
    }

    #[test]
    fn an_unknown_device_has_no_name() {
        let ids = PciIds::parse(SAMPLE_IDS);
        assert_eq!(ids.device_name(0x10DE, 0xFFFF), None);
    }

    #[test]
    fn an_absent_database_is_empty_rather_than_an_error() {
        assert!(PciIds::parse("").is_empty());
    }

    // --- naming and describing -------------------------------------------

    #[test]
    fn names_a_card_from_the_database_when_it_is_available() {
        let ids = PciIds::parse(SAMPLE_IDS);
        let card = DrmCard {
            index: 0,
            pci: PciAddress::new(0, 1, 0, 0),
            vendor_id: 0x10DE,
            device_id: 0x2820,
            driver: Some("nouveau".into()),
        };

        assert_eq!(
            display_name(&card, &ids),
            "NVIDIA AD106M [GeForce RTX 4070 Max-Q / Mobile]"
        );
    }

    #[test]
    fn falls_back_to_the_raw_identifiers_without_a_database() {
        let card = DrmCard {
            index: 0,
            pci: PciAddress::new(0, 1, 0, 0),
            vendor_id: 0x10DE,
            device_id: 0x2820,
            driver: None,
        };

        // Unhelpful, but never wrong, and still lets a user look it up.
        assert_eq!(
            display_name(&card, &PciIds::default()),
            "NVIDIA GPU 10de:2820"
        );
    }

    #[test]
    fn a_described_card_is_identified_by_its_slot_not_its_name() {
        let ids = PciIds::parse(SAMPLE_IDS);
        let card = DrmCard {
            index: 0,
            pci: PciAddress::new(0, 1, 0, 0),
            vendor_id: 0x10DE,
            device_id: 0x2820,
            driver: Some("nouveau".into()),
        };
        let reason = Availability::unsupported("the nouveau driver exposes no counters");

        let descriptor = describe(&card, &ids, &reason).expect("describable");

        assert_eq!(descriptor.source_id.as_str(), "gpu:pci-0000-01-00-0");
        assert_eq!(descriptor.vendor, GpuVendor::Nvidia);
        assert_eq!(descriptor.backend, "drm");
        assert!(!descriptor.has_telemetry());
        assert!(descriptor.display_name.contains("GeForce"));
    }

    #[test]
    fn two_identical_cards_in_different_slots_get_different_identities() {
        let ids = PciIds::parse(SAMPLE_IDS);
        let reason = Availability::unsupported("no backend");
        let build = |bus: u8| {
            describe(
                &DrmCard {
                    index: 0,
                    pci: PciAddress::new(0, bus, 0, 0),
                    vendor_id: 0x1002,
                    device_id: 0x73FF,
                    driver: Some("amdgpu".into()),
                },
                &ids,
                &reason,
            )
            .expect("describable")
        };

        let first = build(0x01);
        let second = build(0x0B);

        assert_eq!(first.display_name, second.display_name);
        assert_ne!(first.source_id, second.source_id);
    }

    // --- host check: invariants only --------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_drm_tree_on_this_host_is_readable_and_consistent() {
        // Asserts shape, never a specific card: this must pass on a headless
        // build machine and on a workstation with four GPUs alike.
        let cards = discover();

        for card in &cards {
            assert_ne!(card.vendor_id, 0, "a real device has a vendor");
            assert!(
                pci_source_id(card.pci).is_some(),
                "every discovered card must yield a valid identity"
            );
        }

        let mut addresses: Vec<PciAddress> = cards.iter().map(|card| card.pci).collect();
        let total = addresses.len();
        addresses.dedup();
        assert_eq!(total, addresses.len(), "no card counted twice");
    }
}

/// Traversal tests, which need the symlink-based fixture tree.
#[cfg(all(test, unix))]
mod traversal_tests {
    use super::fixtures::DrmTree;
    use super::*;

    #[test]
    fn a_card_and_its_render_node_are_one_gpu() {
        let tree = DrmTree::new("single");
        tree.add_card(0, "0000:01:00.0", "0x10de", "0x2820", Some("nouveau"))
            .add_render_node("renderD128")
            .add_connector("card0-DP-1")
            .add_connector("card0-eDP-1")
            .add_connector("card0-HDMI-A-1");

        let cards = discover_in(tree.path());

        assert_eq!(cards.len(), 1, "one physical adapter");
        assert_eq!(cards[0].pci, PciAddress::new(0, 1, 0, 0));
        assert_eq!(cards[0].vendor_id, 0x10DE);
        assert_eq!(cards[0].device_id, 0x2820);
        assert_eq!(cards[0].driver.as_deref(), Some("nouveau"));
        assert_eq!(cards[0].vendor(), GpuVendor::Nvidia);
    }

    #[test]
    fn two_cards_are_both_discovered() {
        let tree = DrmTree::new("dual");
        tree.add_card(0, "0000:00:02.0", "0x8086", "0xa788", Some("i915"))
            .add_card(1, "0000:01:00.0", "0x10de", "0x2820", Some("nvidia"))
            .add_render_node("renderD128")
            .add_render_node("renderD129");

        let cards = discover_in(tree.path());

        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].vendor(), GpuVendor::Intel, "sorted by bus address");
        assert_eq!(cards[1].vendor(), GpuVendor::Nvidia);
    }

    #[test]
    fn non_contiguous_card_indices_are_handled() {
        // `card0` removed and re-probed as `card3` is perfectly normal.
        let tree = DrmTree::new("sparse");
        tree.add_card(3, "0000:01:00.0", "0x10de", "0x2820", Some("nvidia"))
            .add_card(7, "0000:0b:00.0", "0x1002", "0x73ff", Some("amdgpu"));

        let cards = discover_in(tree.path());

        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].index, 3);
        assert_eq!(cards[1].index, 7);
    }

    #[test]
    fn a_card_without_a_device_symlink_is_not_a_hardware_gpu() {
        // `vkms` and `simpledrm` have no PCI parent and must not be counted.
        let tree = DrmTree::new("virtual");
        tree.add_card(0, "0000:01:00.0", "0x10de", "0x2820", Some("nouveau"))
            .add_virtual_card(1);

        let cards = discover_in(tree.path());

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].index, 0);
    }

    #[test]
    fn a_card_without_a_vendor_id_is_skipped() {
        let tree = DrmTree::new("novendor");
        tree.add_card(0, "0000:01:00.0", "not-hex", "0x2820", None);

        assert!(discover_in(tree.path()).is_empty());
    }

    #[test]
    fn a_card_without_a_driver_is_still_inventoried() {
        // No driver bound yet is not a reason to hide the hardware.
        let tree = DrmTree::new("nodriver");
        tree.add_card(0, "0000:01:00.0", "0x1002", "0x73ff", None);

        let cards = discover_in(tree.path());

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].driver, None);
        assert_eq!(cards[0].vendor(), GpuVendor::Amd);
    }

    #[test]
    fn a_missing_drm_root_yields_an_empty_inventory_not_a_panic() {
        assert!(discover_in(Path::new("/nonexistent/pulse/drm")).is_empty());
    }

    #[test]
    fn the_inventory_order_is_deterministic() {
        let tree = DrmTree::new("order");
        tree.add_card(5, "0000:0b:00.0", "0x1002", "0x73ff", Some("amdgpu"))
            .add_card(2, "0000:01:00.0", "0x10de", "0x2820", Some("nvidia"))
            .add_card(9, "0000:03:00.0", "0x8086", "0x56a0", Some("i915"));

        let first: Vec<PciAddress> = discover_in(tree.path()).iter().map(|c| c.pci).collect();
        let second: Vec<PciAddress> = discover_in(tree.path()).iter().map(|c| c.pci).collect();

        assert_eq!(first, second);
        assert_eq!(
            first,
            vec![
                PciAddress::new(0, 0x01, 0, 0),
                PciAddress::new(0, 0x03, 0, 0),
                PciAddress::new(0, 0x0B, 0, 0),
            ]
        );
    }
}
