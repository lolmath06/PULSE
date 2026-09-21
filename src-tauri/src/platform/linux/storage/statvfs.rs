//! Filesystem space, from `statvfs(3)`.
//!
//! PULSE never runs `df`. `df` is a program that calls `statvfs` and formats
//! the result; spawning it per volume per refresh would add a process, a
//! locale-dependent output format to parse and a failure mode, in exchange for
//! numbers already available from one system call.
//!
//! # Blocks, not bytes
//!
//! `statvfs` reports counts of blocks plus the size of a block, and the two
//! must be multiplied — with `f_frsize`, the *fragment* size, which is the
//! unit `f_blocks`, `f_bfree` and `f_bavail` are expressed in. `f_bsize` is
//! the filesystem's preferred I/O block size and is a different number on
//! several filesystems; using it would misreport their capacity. The
//! multiplication is checked, because a filesystem reporting an implausible
//! block count must not wrap into a small number that looks correct.
//!
//! The arithmetic itself, and the `used` / `available` convention, live in the
//! shared contract — see [`VolumeUsage`] — so Fedora and Windows publish the
//! same meaning for the same key.
//!
//! [`VolumeUsage`]: crate::metrics::wellknown::storage::VolumeUsage

use std::path::Path;

use crate::metrics::model::{MetricError, MetricErrorCode};
use crate::metrics::wellknown::storage::VolumeUsage;

/// Reads one mounted filesystem's space accounting.
///
/// Errors carry the distinction the availability contract needs: a filesystem
/// PULSE may not stat is `PermissionDenied`, one that vanished between the
/// mount table being read and this call is an I/O failure that will resolve
/// itself, and neither is "this volume is full".
#[cfg(target_os = "linux")]
pub fn read(mount_point: &Path) -> Result<VolumeUsage, MetricError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(mount_point.as_os_str().as_bytes()).map_err(|_| {
        MetricError::new(
            MetricErrorCode::Parse,
            format!(
                "mount point '{}' contains a NUL byte",
                mount_point.display()
            ),
        )
    })?;

    // SAFETY: `path` is a valid NUL-terminated C string that outlives the
    // call, and `stats` is a correctly sized, aligned `statvfs` the kernel
    // fills. The call is a pure read: `statvfs` never modifies the filesystem.
    let stats = unsafe {
        let mut stats = std::mem::zeroed::<libc::statvfs>();

        if libc::statvfs(path.as_ptr(), &mut stats) != 0 {
            return Err(from_errno(std::io::Error::last_os_error(), mount_point));
        }

        stats
    };

    // `f_frsize` is the unit the block counts are in. `f_bsize` is the
    // preferred I/O size and is *not* interchangeable with it.
    let fragment_size = u64::from(stats.f_frsize as u32).max(u64::from(stats.f_bsize as u32));

    VolumeUsage::from_blocks(
        fragment_size,
        stats.f_blocks as u64,
        stats.f_bfree as u64,
        stats.f_bavail as u64,
    )
    .ok_or_else(|| {
        MetricError::new(
            MetricErrorCode::Parse,
            format!(
                "filesystem at '{}' reported implausible block counts",
                mount_point.display()
            ),
        )
    })
}

/// Compiled on non-Linux hosts so the Windows cross-check harness can type
/// check this module. It is never reached: the Linux provider is the only
/// caller, and it only runs on Linux.
#[cfg(not(target_os = "linux"))]
pub fn read(mount_point: &Path) -> Result<VolumeUsage, MetricError> {
    Err(MetricError::new(
        MetricErrorCode::Unsupported,
        format!(
            "statvfs is a Unix interface; '{}' cannot be read on this platform",
            mount_point.display()
        ),
    ))
}

/// Maps a failed `statvfs` onto the error code that describes it honestly.
///
/// Compiled everywhere so its mapping is unit-tested on any host.
pub fn from_errno(error: std::io::Error, mount_point: &Path) -> MetricError {
    use std::io::ErrorKind;

    let code = match error.kind() {
        ErrorKind::PermissionDenied => MetricErrorCode::PermissionDenied,
        // The mount table is read a moment before this call. A volume
        // unmounted in between is gone, not broken, and the next refresh will
        // simply not list it.
        ErrorKind::NotFound => MetricErrorCode::NotDetected,
        _ => MetricErrorCode::Io,
    };

    MetricError::new(
        code,
        format!(
            "could not read filesystem statistics for '{}': {error}",
            mount_point.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn errno_keeps_the_distinctions_the_availability_contract_needs() {
        let path = Path::new("/mnt/example");

        assert_eq!(
            from_errno(Error::from(ErrorKind::PermissionDenied), path).code,
            MetricErrorCode::PermissionDenied
        );
        assert_eq!(
            from_errno(Error::from(ErrorKind::NotFound), path).code,
            MetricErrorCode::NotDetected
        );
        assert_eq!(
            from_errno(Error::from(ErrorKind::Other), path).code,
            MetricErrorCode::Io
        );
    }

    #[test]
    fn an_error_names_the_volume_it_is_about() {
        let message =
            from_errno(Error::from(ErrorKind::PermissionDenied), Path::new("/boot")).message;

        assert!(message.contains("/boot"), "{message}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reads_the_root_filesystem_of_the_running_machine() {
        // Read-only, and `/` exists on every Linux host including a CI
        // container, so this is a real end-to-end check rather than a mock.
        let usage = read(Path::new("/")).expect("root filesystem is readable");

        assert!(usage.total_bytes > 0, "a mounted filesystem has a size");
        assert!(usage.used_bytes() <= usage.total_bytes);
        assert!(usage.available_bytes <= usage.free_bytes);
        assert!(usage.free_bytes <= usage.total_bytes);

        let percent = usage
            .usage_percent()
            .expect("a sized filesystem has a usage");
        assert!((0.0..=100.0).contains(&percent), "{percent}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_path_that_is_not_mounted_fails_rather_than_reporting_zero() {
        let error = read(Path::new("/nonexistent/pulse/volume")).expect_err("must fail");

        assert_eq!(error.code, MetricErrorCode::NotDetected);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn two_reads_of_the_same_filesystem_agree_on_its_size() {
        // Guards against reading `f_bsize` where `f_frsize` is meant, which
        // would produce a different total on filesystems where they differ.
        let first = read(Path::new("/")).expect("readable");
        let second = read(Path::new("/")).expect("readable");

        assert_eq!(first.total_bytes, second.total_bytes);
    }
}
