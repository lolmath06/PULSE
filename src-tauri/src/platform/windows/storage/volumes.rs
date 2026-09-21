//! Enumerating Windows volumes, and finding out which disks they sit on.
//!
//! # A volume is not a drive letter
//!
//! `FindFirstVolumeW` / `FindNextVolumeW` walk the machine's **volumes**, each
//! identified by a GUID path like
//! `\\?\Volume{d2b1f8e0-…-100000000000}\`. That identifier belongs to the
//! volume and outlives every letter it is ever given.
//!
//! Enumerating drive letters instead would miss the volumes that have none —
//! the EFI system partition, Windows's recovery partition, a data volume
//! mounted into a folder — and would give the ones that do an identity that
//! changes the moment a letter is reassigned.
//!
//! `GetVolumePathNamesForVolumeNameW` then asks where each volume is currently
//! reachable, which may be several paths at once: a letter *and* one or more
//! mount folders. Those are presentation, attached to the volume.
//!
//! # Volume to disk
//!
//! `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` asks the volume itself which
//! physical disks it occupies, rather than inferring it from a name. A volume
//! normally has one extent on one disk; a striped or spanned volume has
//! several, and PULSE reads the count rather than assuming one. A volume whose
//! extents cannot be read is shown without a parent rather than attributed to
//! a guess.

use crate::metrics::model::MetricError;
use crate::metrics::wellknown::storage::VolumeUsage;

/// What one volume reported about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeReport {
    /// The volume GUID path. The volume's identity.
    pub guid_path: String,
    /// Every path the volume is currently reachable at: a drive letter, mount
    /// folders, or none at all.
    pub path_names: Vec<String>,
    /// The filesystem Windows names: `NTFS`, `exFAT`, `FAT32`, `ReFS`.
    pub filesystem: Option<String>,
    pub read_only: bool,
    /// The `PhysicalDriveN` numbers this volume occupies, in the order the
    /// extents were reported.
    pub disk_numbers: Vec<u32>,
}

impl VolumeReport {
    /// The path shown first: the shortest, which is the drive letter when
    /// there is one.
    pub fn primary_path(&self) -> Option<&str> {
        self.path_names
            .iter()
            .min_by_key(|path| (path.len(), path.as_str()))
            .map(String::as_str)
    }

    /// The name shown for this volume.
    ///
    /// Its drive letter when it has one, because that is what a user
    /// recognises; otherwise a shortened form of its GUID, so a volume with no
    /// letter is still nameable rather than blank. **Presentation only** — the
    /// identity is the GUID path.
    pub fn display_name(&self) -> String {
        if let Some(path) = self.primary_path() {
            return path.trim_end_matches('\\').to_string();
        }

        // `\\?\Volume{d2b1f8e0-…}\` → `Volume d2b1f8e0`, which is enough to
        // tell two unlettered volumes apart in a list.
        let short = self
            .guid_path
            .trim_start_matches(r"\\?\Volume{")
            .split('-')
            .next()
            .unwrap_or("")
            .to_string();

        if short.is_empty() {
            "Unnamed volume".to_string()
        } else {
            format!("Volume {short}")
        }
    }
}

/// Converts `GetDiskFreeSpaceExW`'s three totals into the shared accounting.
///
/// Windows reports:
///
/// ```text
/// lpFreeBytesAvailableToCaller     free space this user may use
/// lpTotalNumberOfBytes             the volume's size
/// lpTotalNumberOfFreeBytes         free space on the volume
/// ```
///
/// which maps exactly onto the Unix `statvfs` triple PULSE already publishes:
/// `used = total - free`, `available = what this caller may write`. On a
/// volume with no quota the last two coincide, which is the normal Windows
/// case — and the *meaning* of each published key is then identical on both
/// operating systems, with the gap simply being zero here. Under a disk quota
/// it is not zero, and `available` correctly reports what the user can
/// actually write.
pub fn usage_from_free_space(
    total_bytes: u64,
    free_bytes: u64,
    available_to_caller: u64,
) -> Option<VolumeUsage> {
    // Expressed in bytes already, so the shared constructor is called with a
    // block size of one rather than reimplementing its clamping here.
    VolumeUsage::from_blocks(1, total_bytes, free_bytes, available_to_caller)
}

// --- the Windows implementation -------------------------------------------

#[cfg(target_os = "windows")]
mod imp {
    use super::*;

    use super::super::identity;
    use super::super::ioctl;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_MORE_DATA};
    use windows_sys::Win32::Storage::FileSystem::{
        FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW,
        GetVolumeInformationW, GetVolumePathNamesForVolumeNameW,
    };
    use windows_sys::Win32::System::SystemServices::FILE_READ_ONLY_VOLUME;

    /// The GUID path of every volume on the machine.
    fn enumerate_volume_names() -> Vec<String> {
        let mut buffer = [0_u16; 260];

        // SAFETY: `buffer` is a 260-unit array whose length is passed to the
        // call, which writes a NUL-terminated path into it.
        let find = unsafe { FindFirstVolumeW(buffer.as_mut_ptr(), buffer.len() as u32) };

        if find.is_null() || find == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return Vec::new();
        }

        let mut names = vec![identity::utf16_to_string(&buffer)];

        loop {
            buffer.fill(0);

            // SAFETY: `find` is a live search handle from the call above, and
            // `buffer` is sized as declared.
            let ok = unsafe { FindNextVolumeW(find, buffer.as_mut_ptr(), buffer.len() as u32) };
            if ok == 0 {
                break;
            }

            names.push(identity::utf16_to_string(&buffer));
        }

        // SAFETY: `find` is a live search handle and is not used afterwards.
        unsafe {
            FindVolumeClose(find);
        }

        names.retain(|name| !name.is_empty());
        names.sort();
        names.dedup();
        names
    }

    /// Where one volume is currently reachable.
    fn path_names_for(guid_path: &str) -> Vec<String> {
        let wide: Vec<u16> = guid_path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut buffer = vec![0_u16; 512];
        let mut required: u32 = 0;

        // SAFETY: both buffers are owned here, outlive the call, and have
        // their lengths passed alongside their pointers.
        let ok = unsafe {
            GetVolumePathNamesForVolumeNameW(
                wide.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut required,
            )
        };

        if ok == 0 {
            // SAFETY: reads a thread-local error code set by the call above.
            if unsafe { GetLastError() } != ERROR_MORE_DATA {
                return Vec::new();
            }

            // A volume mounted into many folders needs more room. Ask again
            // with exactly what it said it needed.
            buffer = vec![0_u16; required as usize];

            // SAFETY: as above, with the size the call itself asked for.
            let ok = unsafe {
                GetVolumePathNamesForVolumeNameW(
                    wide.as_ptr(),
                    buffer.as_mut_ptr(),
                    buffer.len() as u32,
                    &mut required,
                )
            };
            if ok == 0 {
                return Vec::new();
            }
        }

        identity::split_path_names(&buffer)
    }

    /// One volume's filesystem name and read-only flag.
    fn volume_information(guid_path: &str) -> (Option<String>, bool) {
        let wide: Vec<u16> = guid_path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut filesystem = [0_u16; 64];
        let mut flags: u32 = 0;

        // SAFETY: `wide` is a NUL-terminated path and `filesystem` is sized as
        // declared; the null pointers are the optional out-parameters PULSE
        // does not need.
        let ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut flags,
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            )
        };

        if ok == 0 {
            // An unformatted or unmounted volume answers nothing. That is a
            // fact about the volume, not a failure.
            return (None, false);
        }

        let name = identity::utf16_to_string(&filesystem);

        (
            (!name.is_empty()).then_some(name),
            flags & FILE_READ_ONLY_VOLUME != 0,
        )
    }

    /// Which physical disks a volume occupies.
    fn disk_numbers_for(guid_path: &str) -> Vec<u32> {
        // The device control needs a path without the trailing backslash.
        let device_path = guid_path.trim_end_matches('\\');

        let Ok(handle) = super::super::device::DeviceHandle::open(device_path) else {
            return Vec::new();
        };

        // Room for a generously spanned volume: header plus sixteen extents.
        let mut buffer = [0_u8; 8 + 24 * 16];
        let Ok(returned) = handle.control(ioctl::IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &mut buffer)
        else {
            return Vec::new();
        };

        let mut numbers: Vec<u32> = ioctl::parse_disk_extents(&buffer[..returned])
            .into_iter()
            .map(|extent| extent.disk_number)
            .collect();

        numbers.dedup();
        numbers
    }

    /// Every volume on the machine, with the disks it occupies.
    pub fn discover() -> Vec<VolumeReport> {
        enumerate_volume_names()
            .into_iter()
            .map(|guid_path| {
                let (filesystem, read_only) = volume_information(&guid_path);

                VolumeReport {
                    path_names: path_names_for(&guid_path),
                    disk_numbers: disk_numbers_for(&guid_path),
                    filesystem,
                    read_only,
                    guid_path,
                }
            })
            .collect()
    }

    /// Reads one volume's space accounting.
    pub fn read_usage(guid_path: &str) -> Result<VolumeUsage, MetricError> {
        use crate::metrics::model::MetricErrorCode;

        let wide: Vec<u16> = guid_path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut available: u64 = 0;
        let mut total: u64 = 0;
        let mut free: u64 = 0;

        // SAFETY: `wide` is a NUL-terminated path outliving the call, and the
        // three out-parameters are owned locals.
        let ok =
            unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) };

        if ok == 0 {
            // SAFETY: reads a thread-local error code set by the call above.
            return Err(super::super::device::from_win32(
                unsafe { GetLastError() },
                "this volume's free space",
            ));
        }

        usage_from_free_space(total, free, available).ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                format!("volume '{guid_path}' reported implausible space totals"),
            )
        })
    }
}

#[cfg(target_os = "windows")]
pub use imp::{discover, read_usage};

// --- the non-Windows stand-ins --------------------------------------------

#[cfg(not(target_os = "windows"))]
pub fn discover() -> Vec<VolumeReport> {
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
pub fn read_usage(guid_path: &str) -> Result<VolumeUsage, MetricError> {
    use crate::metrics::model::MetricErrorCode;

    Err(MetricError::new(
        MetricErrorCode::Unsupported,
        format!("'{guid_path}' can only be read through the Windows volume API"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(guid: &str, paths: &[&str], disks: &[u32]) -> VolumeReport {
        VolumeReport {
            guid_path: guid.to_string(),
            path_names: paths.iter().map(|path| path.to_string()).collect(),
            filesystem: Some("NTFS".to_string()),
            read_only: false,
            disk_numbers: disks.to_vec(),
        }
    }

    #[test]
    fn a_lettered_volume_is_named_by_its_letter() {
        let report = volume(
            r"\\?\Volume{d2b1f8e0-1111-2222-3333-100000000000}\",
            &[r"C:\"],
            &[0],
        );

        assert_eq!(report.display_name(), "C:");
        assert_eq!(report.primary_path(), Some(r"C:\"));
    }

    #[test]
    fn a_volume_with_no_letter_is_still_nameable() {
        // A recovery or EFI partition. It exists, it takes space, and it must
        // not appear as a blank row.
        let report = volume(
            r"\\?\Volume{aabbccdd-1111-2222-3333-100000000000}\",
            &[],
            &[0],
        );

        assert_eq!(report.display_name(), "Volume aabbccdd");
        assert_eq!(report.primary_path(), None);
    }

    #[test]
    fn a_volume_reachable_at_several_paths_shows_the_shortest_first() {
        // A drive letter and a mount folder are two ways to reach one volume,
        // not two volumes.
        let report = volume(
            r"\\?\Volume{d2b1f8e0-1111-2222-3333-100000000000}\",
            &[r"D:\Mounts\Data\", r"E:\"],
            &[1],
        );

        assert_eq!(report.primary_path(), Some(r"E:\"));
        assert_eq!(report.display_name(), "E:");
        assert_eq!(report.path_names.len(), 2);
    }

    #[test]
    fn a_malformed_guid_path_still_produces_a_name() {
        let report = volume("", &[], &[]);
        assert_eq!(report.display_name(), "Unnamed volume");
    }

    #[test]
    fn a_spanned_volume_keeps_every_disk_it_occupies() {
        let report = volume(
            r"\\?\Volume{d2b1f8e0-1111-2222-3333-100000000000}\",
            &[r"S:\"],
            &[1, 2],
        );

        assert_eq!(report.disk_numbers, [1, 2]);
    }

    #[test]
    fn windows_totals_map_onto_the_shared_accounting() {
        // 500 GB volume, 200 GB free, all of it usable by this caller.
        let usage = usage_from_free_space(500_000_000_000, 200_000_000_000, 200_000_000_000)
            .expect("plausible");

        assert_eq!(usage.total_bytes, 500_000_000_000);
        assert_eq!(usage.used_bytes(), 300_000_000_000);
        assert_eq!(usage.available_bytes, 200_000_000_000);
        assert_eq!(usage.usage_percent(), Some(60.0));
    }

    #[test]
    fn used_is_total_minus_free_here_too() {
        // The same convention as Fedora, so the key means one thing. Under a
        // quota the caller sees less than is free, and `used` is unaffected.
        let quota = usage_from_free_space(1000, 400, 100).expect("plausible");

        assert_eq!(quota.used_bytes(), 600, "not 900");
        assert_eq!(quota.available_bytes, 100);
        assert_eq!(quota.usage_percent(), Some(60.0));
    }

    #[test]
    fn an_empty_volume_is_zero_percent_used() {
        let usage = usage_from_free_space(1000, 1000, 1000).expect("plausible");

        assert_eq!(usage.used_bytes(), 0);
        assert_eq!(usage.usage_percent(), Some(0.0));
    }

    #[test]
    fn a_zero_sized_volume_has_no_usage_rather_than_a_nan() {
        let usage = usage_from_free_space(0, 0, 0).expect("valid");
        assert_eq!(usage.usage_percent(), None);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_stand_ins_report_the_interface_as_windows_only() {
        use crate::metrics::model::MetricErrorCode;

        assert!(discover().is_empty());
        assert_eq!(
            read_usage(r"\\?\Volume{1234}\")
                .expect_err("unsupported")
                .code,
            MetricErrorCode::Unsupported
        );
    }
}
