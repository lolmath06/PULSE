//! Turning what Windows reports into stable identities.
//!
//! Pure string and structure handling, so all of it compiles and is tested on
//! Fedora. The FFI that produces the inputs lives next door.
//!
//! # `PhysicalDrive0` is not an identity
//!
//! It is the number the storage stack currently assigns to a device, and it
//! moves: plug in a USB disk before boot and yesterday's `PhysicalDrive1` is
//! today's `PhysicalDrive2`. PULSE uses it to *open* a device — it is the only
//! way to address one — and never to remember it.
//!
//! `C:` is not an identity either, for the same reason on the volume side: a
//! drive letter is an assignment, and a volume keeps its GUID path when its
//! letter changes or is removed entirely.
//!
//! # What is used instead
//!
//! For a device, in order: the serial number the device reports through
//! `IOCTL_STORAGE_QUERY_PROPERTY`, then the device instance ID SetupAPI
//! assigns. For a volume: its volume GUID path, which Windows guarantees for
//! the life of the volume.
//!
//! The device order matters for portability. A drive's serial number is the
//! *same string* on both operating systems, so
//! `storage.capacity.total@storage:serial-s677nx0w` means the same physical
//! drive whether Fedora's sysfs or Windows's storage stack reported it — and a
//! dashboard built on one opens on the other. A device instance ID is a
//! Windows construct with no Linux counterpart, so it is the fallback rather
//! than the preference.

use crate::metrics::model::SourceId;
use crate::metrics::wellknown::storage::{
    device_identity, guid_volume_source_id, StorageIdentity, VolumeIdentity,
};

/// Whether a serial number is specific enough to identify a device.
///
/// USB bridges are the problem. A great many of them report a hardcoded
/// placeholder — every unit of a given enclosure model shipping with
/// `0123456789ABCDEF` or `000000000000` — and two different disks in two
/// identical enclosures would then collapse onto one identity, so a widget
/// bound to one would show the other.
///
/// A serial that is entirely one repeated character, or one of the known
/// placeholders, is refused; identity falls through to the device instance ID,
/// which encodes the port and does distinguish them.
pub fn is_usable_serial(serial: &str) -> bool {
    let trimmed = serial.trim();

    if trimmed.len() < 4 {
        return false;
    }

    // `000000000000`, `....`, `____`: one character repeated is a placeholder,
    // not a serial.
    let mut characters = trimmed.chars();
    let first = characters.next().unwrap_or(' ');
    if characters.all(|c| c == first) {
        return false;
    }

    const PLACEHOLDERS: &[&str] = &[
        "0123456789abcdef",
        "123456789abcdef",
        "0123456789",
        "123456789",
        "abcdefgh",
        "noserial",
        "none",
        "default",
        "disabled",
    ];

    let lowered = trimmed.to_ascii_lowercase();
    !PLACEHOLDERS.contains(&lowered.as_str())
}

/// Builds the identity of one physical disk.
///
/// `serial` is what the device reported, `instance_id` what SetupAPI assigns,
/// and `fallback_name` the `PhysicalDriveN` spelling used only when neither is
/// usable — with the weakness recorded in the returned [`StorageIdentity`]
/// rather than hidden.
pub fn disk_identity(
    serial: Option<&str>,
    instance_id: Option<&str>,
    fallback_name: &str,
) -> Option<(SourceId, StorageIdentity)> {
    let serial = serial.filter(|value| is_usable_serial(value));

    // No `wwid` argument: Windows's storage stack does not surface a
    // world-wide name through the device descriptor, so the serial is the best
    // hardware identifier available and takes the first slot.
    device_identity(None, serial, instance_id, fallback_name)
}

/// Builds the identity of one volume from its GUID path.
///
/// Returns `None` when the path is not one — which would mean Windows handed
/// back something the volume enumeration does not produce, and deriving an
/// identity from it anyway would be guessing.
pub fn volume_identity(guid_path: &str) -> Option<(SourceId, VolumeIdentity)> {
    let trimmed = guid_path.trim();

    if !trimmed.starts_with(r"\\?\Volume{") {
        return None;
    }

    let source = guid_volume_source_id(trimmed)?;
    Some((source, VolumeIdentity::Guid(trimmed.to_string())))
}

/// The `\\.\PhysicalDriveN` path used to open a disk.
///
/// A *handle* to a device, never an identity — see the module documentation.
pub fn physical_drive_path(number: u32) -> String {
    format!(r"\\.\PhysicalDrive{number}")
}

/// The name shown for a disk.
///
/// **Presentation only.** Two identical drives produce the same string, which
/// is exactly why it is not an identity. Vendor and product come back as
/// separate space-padded fields, and many NVMe devices report a product string
/// that already contains the vendor, so the two are joined only when the
/// product does not already start with the vendor.
pub fn display_name(vendor: Option<&str>, product: Option<&str>, fallback: &str) -> String {
    let clean = |value: Option<&str>| -> Option<String> {
        let trimmed = value?.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };

    match (clean(vendor), clean(product)) {
        (Some(vendor), Some(product)) => {
            if product
                .to_ascii_lowercase()
                .starts_with(&vendor.to_ascii_lowercase())
            {
                product
            } else {
                format!("{vendor} {product}")
            }
        }
        (None, Some(product)) => product,
        (Some(vendor), None) => vendor,
        (None, None) => fallback.to_string(),
    }
}

/// Splits the NUL-separated, double-NUL-terminated list
/// `GetVolumePathNamesForVolumeNameW` writes.
///
/// A volume can be reachable at several paths at once — a drive letter *and* a
/// directory it is mounted into — so the result is a list, not a string.
/// Taking only the first would hide the mount folder a user deliberately
/// created.
pub fn split_path_names(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|&unit| unit == 0)
        .filter(|segment| !segment.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Reads a NUL-terminated UTF-16 string from a buffer.
pub fn utf16_to_string(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::wellknown::storage::IdentityStability;

    // --- serial plausibility ----------------------------------------------

    #[test]
    fn a_real_serial_is_usable() {
        for serial in ["S677NX0W", "WD-WCC4N1234567", "50026B7684E1D9A2", "21A0B2"] {
            assert!(is_usable_serial(serial), "'{serial}' should be usable");
        }
    }

    #[test]
    fn a_usb_bridge_placeholder_serial_is_refused() {
        // Two different disks in two identical enclosures must not collapse
        // onto one identity.
        for serial in [
            "0123456789ABCDEF",
            "000000000000",
            "0000",
            "............",
            "NoSerial",
            "None",
            "   ",
            "",
            "12",
        ] {
            assert!(!is_usable_serial(serial), "'{serial}' should be refused");
        }
    }

    // --- disk identity ----------------------------------------------------

    #[test]
    fn a_serial_outranks_a_device_instance_id() {
        let (source, identity) = disk_identity(
            Some("S677NX0W"),
            Some(r"SCSI\Disk&Ven_NVMe&Prod_SAMSUNG\5&2a1b3c&0&000000"),
            "PhysicalDrive0",
        )
        .expect("identified");

        assert_eq!(source.as_str(), "storage:serial-s677nx0w");
        assert_eq!(identity.stability(), IdentityStability::Hardware);
    }

    #[test]
    fn the_same_drive_gets_the_same_identity_on_both_operating_systems() {
        // The portability promise. Fedora reads the serial from sysfs and
        // Windows from the device descriptor; it is the same string, so it is
        // the same `SourceId`, so a saved widget survives the move.
        let windows = disk_identity(Some("S677NX0W"), Some("whatever"), "PhysicalDrive0")
            .expect("identified")
            .0;
        let linux = crate::metrics::wellknown::storage::device_identity(
            None,
            Some("S677NX0W"),
            None,
            "nvme0n1",
        )
        .expect("identified")
        .0;

        assert_eq!(windows, linux);
    }

    #[test]
    fn a_placeholder_serial_falls_through_to_the_device_instance_id() {
        let (source, identity) = disk_identity(
            Some("0123456789ABCDEF"),
            Some(r"USBSTOR\Disk&Ven_Intenso&Prod_External&Rev_0209\7&1a2b3c&0&Port_0004"),
            "PhysicalDrive2",
        )
        .expect("identified");

        assert!(source
            .as_str()
            .starts_with("storage:sys-usbstor-disk-ven-intenso"));
        assert_eq!(identity.stability(), IdentityStability::SystemAssigned);
        assert!(identity.stability().survives_reboot());
    }

    #[test]
    fn two_identical_enclosures_stay_two_devices() {
        // Same placeholder serial, same model, different ports.
        let first = disk_identity(
            Some("000000000000"),
            Some(r"USBSTOR\Disk&Ven_X&Prod_Y\7&1a&0&Port_0004"),
            "PhysicalDrive2",
        )
        .expect("identified");
        let second = disk_identity(
            Some("000000000000"),
            Some(r"USBSTOR\Disk&Ven_X&Prod_Y\7&1a&0&Port_0005"),
            "PhysicalDrive3",
        )
        .expect("identified");

        assert_ne!(first.0, second.0);
    }

    #[test]
    fn a_disk_with_nothing_usable_keeps_a_session_scoped_identity_and_says_so() {
        let (source, identity) = disk_identity(None, None, "PhysicalDrive0").expect("identified");

        assert_eq!(source.as_str(), "storage:dev-physicaldrive0");
        assert_eq!(identity.stability(), IdentityStability::Session);
        assert!(!identity.stability().survives_reboot());
    }

    #[test]
    fn the_drive_path_is_a_handle_not_an_identity() {
        assert_eq!(physical_drive_path(0), r"\\.\PhysicalDrive0");
        assert_eq!(physical_drive_path(12), r"\\.\PhysicalDrive12");

        // …and it never appears in a source id derived from real hardware.
        let source = disk_identity(Some("S677NX0W"), None, "PhysicalDrive0")
            .expect("identified")
            .0;
        assert!(!source.as_str().contains("physicaldrive"));
    }

    // --- volume identity --------------------------------------------------

    #[test]
    fn a_volume_guid_path_becomes_a_stable_identity() {
        let path = r"\\?\Volume{d2b1f8e0-1111-2222-3333-100000000000}\";
        let (source, identity) = volume_identity(path).expect("identified");

        assert_eq!(source.kind(), "volume");
        assert_eq!(
            source.as_str(),
            "volume:guid-volume-d2b1f8e0-1111-2222-3333-100000000000"
        );
        assert_eq!(identity, VolumeIdentity::Guid(path.to_string()));
        assert!(identity.stability().survives_reboot());
    }

    #[test]
    fn a_drive_letter_is_not_a_volume_identity() {
        // `C:` is where a volume is reachable today. Removing the letter, or
        // reassigning it, does not make it a different volume.
        assert_eq!(volume_identity("C:"), None);
        assert_eq!(volume_identity(r"C:\"), None);
        assert_eq!(volume_identity(r"\\?\Harddisk0Partition1"), None);
        assert_eq!(volume_identity(""), None);
    }

    #[test]
    fn two_volumes_keep_two_identities() {
        let first = volume_identity(r"\\?\Volume{aaaaaaaa-0000-0000-0000-000000000000}\").unwrap();
        let second = volume_identity(r"\\?\Volume{bbbbbbbb-0000-0000-0000-000000000000}\").unwrap();

        assert_ne!(first.0, second.0);
    }

    // --- presentation -----------------------------------------------------

    #[test]
    fn the_display_name_never_repeats_the_vendor() {
        assert_eq!(
            display_name(Some("NVMe"), Some("SAMSUNG MZVL22T0HBLB"), "PhysicalDrive0"),
            "NVMe SAMSUNG MZVL22T0HBLB"
        );
        assert_eq!(
            display_name(
                Some("Samsung"),
                Some("Samsung SSD 990 PRO"),
                "PhysicalDrive0"
            ),
            "Samsung SSD 990 PRO"
        );
        assert_eq!(
            display_name(None, Some("WDC WDS100T2B0A"), "x"),
            "WDC WDS100T2B0A"
        );
        assert_eq!(display_name(Some("Intenso"), None, "x"), "Intenso");
        assert_eq!(display_name(None, None, "PhysicalDrive3"), "PhysicalDrive3");
        assert_eq!(
            display_name(Some("  "), Some("  "), "PhysicalDrive3"),
            "PhysicalDrive3"
        );
    }

    // --- Win32 string handling --------------------------------------------

    #[test]
    fn a_volumes_path_list_keeps_every_path() {
        // `C:\` and a mount folder are both real ways to reach one volume.
        let buffer: Vec<u16> = "C:\\\0D:\\Mounts\\Data\\\0\0".encode_utf16().collect();

        assert_eq!(split_path_names(&buffer), [r"C:\", r"D:\Mounts\Data\"]);
    }

    #[test]
    fn a_volume_with_no_path_names_yields_an_empty_list() {
        // A volume with no drive letter and no mount folder is real, mounted
        // and unreachable by path — a Windows recovery partition, typically.
        assert!(split_path_names(&[0, 0]).is_empty());
        assert!(split_path_names(&[]).is_empty());
    }

    #[test]
    fn utf16_stops_at_the_terminator() {
        let buffer: Vec<u16> = r"\\?\Volume{1234}\"
            .encode_utf16()
            .chain([0, 0xFFFF])
            .collect();

        assert_eq!(utf16_to_string(&buffer), r"\\?\Volume{1234}\");
        assert_eq!(utf16_to_string(&[]), "");
    }
}
