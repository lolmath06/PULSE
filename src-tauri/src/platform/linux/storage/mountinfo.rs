//! `/proc/self/mountinfo` — the mount table, parsed properly.
//!
//! # Why not `/etc/mtab` or `/proc/mounts`
//!
//! `/etc/mtab` is a file a userspace tool maintains; on Fedora it is a symlink
//! to `/proc/self/mounts`, but that is a convention rather than a guarantee,
//! and either way both are the *old* format. `mountinfo` carries three things
//! they do not:
//!
//! - the **device number** (`major:minor`) of the filesystem, which is what
//!   makes two mounts of the same filesystem recognisable as one filesystem;
//! - the **root within the filesystem** each mount exposes, which is how a
//!   bind mount and a btrfs subvolume are told apart from a separate volume;
//! - the **mount topology** — parent ids, shared/peer groups — so a mount
//!   namespace is represented rather than flattened.
//!
//! Without the device number, a monitor counts Fedora's default layout as two
//! filesystems (`/` and `/home`) and reports the same 400 GB twice.
//!
//! # The format
//!
//! ```text
//! 36 35 98:0 /mnt1 /mnt2 rw,noatime master:1 - ext3 /dev/root rw,errors=continue
//! │  │  │    │     │     │          │       │ │    │          │
//! │  │  │    │     │     │          │       │ │    │          └ superblock options
//! │  │  │    │     │     │          │       │ │    └ mount source
//! │  │  │    │     │     │          │       │ └ filesystem type
//! │  │  │    │     │     │          │       └ separator
//! │  │  │    │     │     │          └ optional fields, variable in number
//! │  │  │    │     │     └ mount options
//! │  │  │    │     └ mount point
//! │  │  │    └ root within the filesystem
//! │  │  └ major:minor
//! │  └ parent mount id
//! └ mount id
//! ```
//!
//! The optional-field block is why the separator exists: fields before `-` are
//! variable in number, so nothing may be located by a fixed index from the
//! left past field six, and the type, source and options must be located
//! relative to the `-`.
//!
//! # Escaping
//!
//! Paths in `mountinfo` escape space, tab, newline and backslash as octal:
//! a mount at `/mnt/My Disk` appears as `/mnt/My\040Disk`. Splitting on
//! whitespace without unescaping would cut that path in half and shift every
//! field after it — which is how a naive parser ends up reporting the
//! filesystem type as `Disk`.

use std::fs;
use std::path::Path;

/// Where the kernel publishes this process's mount table.
pub const MOUNTINFO_PATH: &str = "/proc/self/mountinfo";

/// Filesystem types that are not user storage.
///
/// These are kernel interfaces, RAM-backed scratch space or container
/// plumbing. Publishing them as volumes would fill the Storage card with a
/// dozen rows nobody asked about and report `/dev` as a 4 MB disk.
///
/// `tmpfs` is on the list deliberately, and it is the debatable one: `/tmp`
/// and `/dev/shm` are real, useful filesystems whose usage a user may well
/// want. They are excluded here because they are **RAM**, already accounted
/// for by the memory metrics, and showing them beside persistent disks invites
/// the reading that the machine has more storage than it does. A future mode
/// may surface them explicitly; see `docs/metrics/storage.md`.
const PSEUDO_FILESYSTEMS: &[&str] = &[
    "autofs",
    "bpf",
    "binfmt_misc",
    "cgroup",
    "cgroup2",
    "configfs",
    "debugfs",
    "devpts",
    "devtmpfs",
    "efivarfs",
    "fuse.gvfsd-fuse",
    "fuse.portal",
    "fusectl",
    "hugetlbfs",
    "mqueue",
    "nsfs",
    "overlay",
    "proc",
    "pstore",
    "ramfs",
    "rpc_pipefs",
    "securityfs",
    "selinuxfs",
    "squashfs",
    "sunrpc",
    "sysfs",
    "tmpfs",
    "tracefs",
];

/// One parsed mount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountEntry {
    pub mount_id: u32,
    pub parent_id: u32,
    pub major: u32,
    pub minor: u32,
    /// The directory within the filesystem this mount exposes. `/` for a
    /// whole-filesystem mount, something else for a bind mount or a btrfs
    /// subvolume.
    pub root: String,
    /// Where it is reachable in this namespace.
    pub mount_point: String,
    /// `ext4`, `btrfs`, `vfat`, `tmpfs`, …
    pub filesystem: String,
    /// What was mounted: `/dev/nvme0n1p8`, `tmpfs`, `overlay`.
    pub source: String,
    pub read_only: bool,
}

impl MountEntry {
    /// Whether this mount exposes a filesystem PULSE treats as user storage.
    pub fn is_user_storage(&self) -> bool {
        !PSEUDO_FILESYSTEMS.contains(&self.filesystem.as_str())
    }

    /// The kernel device name behind `source`, when it is a block device.
    ///
    /// `/dev/nvme0n1p8` yields `nvme0n1p8`. A source that is not a path under
    /// `/dev` — `tmpfs`, `overlay`, an NFS export — yields `None`, which is the
    /// honest answer: there is no block device to correlate with.
    pub fn block_device_name(&self) -> Option<&str> {
        let name = self.source.strip_prefix("/dev/")?;

        // `/dev/mapper/fedora-root` is a symlink into the mapper namespace,
        // not a block device name; resolving it needs the device number
        // instead, which the caller already has.
        (!name.contains('/') && !name.is_empty()).then_some(name)
    }
}

/// Decodes the octal escapes `mountinfo` uses for space, tab, newline and
/// backslash.
///
/// `\040` is a space, `\011` a tab, `\012` a newline, `\134` a backslash. An
/// incomplete or non-octal escape is left verbatim rather than dropped: a path
/// PULSE cannot fully decode is still better shown as written than silently
/// mangled.
pub fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let digits = &value[index + 1..index + 4];
            if let Ok(code) = u8::from_str_radix(digits, 8) {
                out.push(code as char);
                index += 4;
                continue;
            }
        }

        // Not an escape, or one that did not decode: keep the character.
        let character = value[index..].chars().next().unwrap_or('\\');
        out.push(character);
        index += character.len_utf8();
    }

    out
}

/// Parses one `mountinfo` line.
///
/// Returns `None` for anything that does not have the documented shape.
/// Locating the type, source and options relative to the `-` separator rather
/// than by index is what makes the parser survive the variable optional-field
/// block.
pub fn parse_line(line: &str) -> Option<MountEntry> {
    let (before, after) = line.split_once(" - ")?;

    let head: Vec<&str> = before.split(' ').collect();
    if head.len() < 6 {
        return None;
    }

    let (major, minor) = head[2].split_once(':')?;

    let tail: Vec<&str> = after.split(' ').collect();
    if tail.len() < 3 {
        return None;
    }

    let options = head[5];

    Some(MountEntry {
        mount_id: head[0].parse().ok()?,
        parent_id: head[1].parse().ok()?,
        major: major.parse().ok()?,
        minor: minor.parse().ok()?,
        root: unescape(head[3]),
        mount_point: unescape(head[4]),
        filesystem: tail[0].to_string(),
        source: unescape(tail[1]),
        // The first mount option is `ro` or `rw`; anything else is a
        // malformed line, and treating it as writable is the safer default
        // for a read-only monitor that will never act on the answer.
        read_only: options == "ro" || options.starts_with("ro,"),
    })
}

/// Parses a whole mount table, in file order.
///
/// Pseudo-filesystems are **kept** here and filtered by the caller, so the
/// filter is one testable rule in one place rather than a parser behaviour.
pub fn parse(contents: &str) -> Vec<MountEntry> {
    contents.lines().filter_map(parse_line).collect()
}

/// Reads and parses this process's mount table.
pub fn read_from(path: &Path) -> Vec<MountEntry> {
    fs::read_to_string(path)
        .map(|contents| parse(&contents))
        .unwrap_or_default()
}

/// Reads this machine's mount table.
pub fn read() -> Vec<MountEntry> {
    read_from(Path::new(MOUNTINFO_PATH))
}

#[cfg(test)]
pub(crate) mod fixtures {
    /// An excerpt of the development machine's real mount table, covering
    /// every case the parser has to get right: one btrfs filesystem mounted
    /// twice as two subvolumes, an ext4 `/boot`, a vfat ESP nested inside it,
    /// a tmpfs, the kernel pseudo-filesystems, a squashfs snap on a loop
    /// device, an exFAT USB partition and a Docker overlay.
    pub const REAL: &str = r"66 1 0:33 /root / rw,relatime shared:1 - btrfs /dev/nvme0n1p8 rw,seclabel,compress=zstd:1,ssd,subvol=/root
35 66 0:6 / /dev rw,nosuid shared:2 - devtmpfs devtmpfs rw,seclabel,size=4096k
38 66 0:24 / /sys rw,nosuid,nodev,noexec,relatime shared:5 - sysfs sysfs rw,seclabel
45 66 0:23 / /proc rw,nosuid,nodev,noexec,relatime shared:13 - proc proc rw
46 66 0:27 / /run rw,nosuid,nodev shared:14 - tmpfs tmpfs rw,seclabel,size=6522516k
51 66 7:0 / /var/lib/snapd/snap/bare/5 ro,nodev,relatime shared:48 - squashfs /dev/loop0 ro,errors=continue
92 66 0:33 /home /home rw,relatime shared:64 - btrfs /dev/nvme0n1p8 rw,seclabel,compress=zstd:1,ssd,subvol=/home
94 66 0:42 / /tmp rw,nosuid,nodev shared:66 - tmpfs tmpfs rw,seclabel,nr_inodes=1048576
96 66 259:7 / /boot rw,relatime shared:68 - ext4 /dev/nvme0n1p7 rw,seclabel
98 96 259:1 / /boot/efi rw,relatime shared:70 - vfat /dev/nvme0n1p1 rw,fmask=0077,codepage=437
517 47 8:4 / /mnt/PROMETHEUS_DATA rw,relatime shared:1127 - exfat /dev/sda4 rw,uid=1000,iocharset=utf8
1291 66 0:80 / /var/lib/docker/overlay2/cab18b/merged rw,relatime shared:1171 - overlay overlay rw,lowerdir=/a:/b,upperdir=/c
26 49 8:3 / /mnt/PROMETHEUS_DEV rw,relatime shared:1504 - ext4 /dev/sda3 rw,seclabel
1371 46 8:2 / /run/media/matheo/PROM_RESCUE rw,nosuid,nodev,relatime shared:1552 - ext4 /dev/sda2 rw,seclabel,errors=remount-ro";
}

#[cfg(test)]
mod tests {
    use super::fixtures::REAL;
    use super::*;

    fn by_mount_point<'a>(entries: &'a [MountEntry], path: &str) -> &'a MountEntry {
        entries
            .iter()
            .find(|entry| entry.mount_point == path)
            .unwrap_or_else(|| panic!("'{path}' must be parsed"))
    }

    // --- the format -------------------------------------------------------

    #[test]
    fn parses_the_documented_example() {
        let entry = parse_line(
            "36 35 98:0 /mnt1 /mnt2 rw,noatime master:1 - ext3 /dev/root rw,errors=continue",
        )
        .expect("valid");

        assert_eq!(entry.mount_id, 36);
        assert_eq!(entry.parent_id, 35);
        assert_eq!((entry.major, entry.minor), (98, 0));
        assert_eq!(entry.root, "/mnt1");
        assert_eq!(entry.mount_point, "/mnt2");
        assert_eq!(entry.filesystem, "ext3");
        assert_eq!(entry.source, "/dev/root");
        assert!(!entry.read_only);
    }

    #[test]
    fn locates_the_type_relative_to_the_separator_not_by_index() {
        // The optional-field block varies in length. A parser indexing from
        // the left would read `shared:1` as the filesystem type on the first
        // line and `unbindable` on the second.
        let none = parse_line("1 2 8:1 / /a rw - ext4 /dev/sda1 rw").expect("valid");
        let several = parse_line(
            "1 2 8:1 / /a rw shared:1 master:2 propagate_from:3 unbindable - ext4 /dev/sda1 rw",
        )
        .expect("valid");

        assert_eq!(none.filesystem, "ext4");
        assert_eq!(several.filesystem, "ext4");
        assert_eq!(several.source, "/dev/sda1");
    }

    #[test]
    fn refuses_a_line_with_no_separator_or_too_few_fields() {
        assert_eq!(
            parse_line("36 35 98:0 /mnt1 /mnt2 rw ext3 /dev/root rw"),
            None
        );
        assert_eq!(parse_line("36 35 98:0 - ext3 /dev/root rw"), None);
        assert_eq!(parse_line("36 35 nope / /a rw - ext4 /dev/sda1 rw"), None);
        assert_eq!(parse_line(""), None);
    }

    #[test]
    fn a_read_only_mount_is_recognised() {
        let ro = parse_line("1 2 8:1 / /a ro,nodev,relatime - squashfs /dev/loop0 ro").expect("v");
        let rw = parse_line("1 2 8:1 / /a rw,relatime - ext4 /dev/sda1 rw").expect("v");

        assert!(ro.read_only);
        assert!(!rw.read_only);
        // `rootflags` must not be mistaken for `ro`.
        let tricky = parse_line("1 2 8:1 / /a rootcontext=x,rw - ext4 /dev/sda1 rw").expect("v");
        assert!(!tricky.read_only);
    }

    // --- escaping ---------------------------------------------------------

    #[test]
    fn decodes_the_octal_escapes_paths_use() {
        assert_eq!(unescape(r"/mnt/My\040Disk"), "/mnt/My Disk");
        assert_eq!(unescape(r"/a\011b"), "/a\tb");
        assert_eq!(unescape(r"/a\012b"), "/a\nb");
        assert_eq!(unescape(r"/a\134b"), r"/a\b");
        assert_eq!(unescape("/plain/path"), "/plain/path");
    }

    #[test]
    fn a_space_in_a_mount_point_does_not_shift_every_later_field() {
        // The failure a whitespace-splitting parser produces: the filesystem
        // type comes out as `Disk`.
        let entry = parse_line(
            r"42 66 8:5 / /run/media/matheo/My\040Backup\040Disk rw,nosuid - ext4 /dev/sdb1 rw",
        )
        .expect("valid");

        assert_eq!(entry.mount_point, "/run/media/matheo/My Backup Disk");
        assert_eq!(entry.filesystem, "ext4");
        assert_eq!(entry.source, "/dev/sdb1");
    }

    #[test]
    fn an_undecodable_escape_is_kept_verbatim_rather_than_dropped() {
        assert_eq!(unescape(r"/a\09"), r"/a\09");
        assert_eq!(unescape(r"/trailing\"), r"/trailing\");
        assert_eq!(unescape(r"/a\999b"), r"/a\999b");
    }

    // --- the real machine -------------------------------------------------

    #[test]
    fn parses_the_whole_real_mount_table() {
        let entries = parse(REAL);
        assert_eq!(entries.len(), REAL.lines().count());
    }

    #[test]
    fn one_btrfs_filesystem_mounted_twice_shares_one_device_number() {
        // Fedora's default layout. `/` and `/home` are two subvolumes of the
        // *same* filesystem, and counting them as two volumes would report the
        // machine's capacity twice.
        let entries = parse(REAL);
        let root = by_mount_point(&entries, "/");
        let home = by_mount_point(&entries, "/home");

        assert_eq!((root.major, root.minor), (home.major, home.minor));
        assert_eq!(root.source, home.source);
        assert_eq!(root.filesystem, "btrfs");
        // What differs is which subvolume each exposes, which is presentation.
        assert_eq!(root.root, "/root");
        assert_eq!(home.root, "/home");
        assert_ne!(root.root, home.root);
    }

    #[test]
    fn a_bind_mount_of_the_same_filesystem_shares_its_device_number() {
        // The same rule catches a plain bind mount, which is the other way a
        // volume gets duplicated.
        let contents = "40 1 259:8 / /srv/data rw,relatime - ext4 /dev/nvme0n1p8 rw\n\
                        41 1 259:8 /projects /home/matheo/projects rw,relatime - ext4 /dev/nvme0n1p8 rw";
        let entries = parse(contents);

        assert_eq!(entries.len(), 2);
        assert_eq!(
            (entries[0].major, entries[0].minor),
            (entries[1].major, entries[1].minor)
        );
        assert_eq!(entries[0].source, entries[1].source);
    }

    #[test]
    fn distinct_filesystems_keep_distinct_device_numbers() {
        let entries = parse(REAL);

        let boot = by_mount_point(&entries, "/boot");
        let efi = by_mount_point(&entries, "/boot/efi");

        assert_eq!((boot.major, boot.minor), (259, 7));
        assert_eq!((efi.major, efi.minor), (259, 1));
        assert_ne!((boot.major, boot.minor), (efi.major, efi.minor));
        assert_eq!(efi.filesystem, "vfat");
        // Nested in the path, and a separate filesystem all the same.
        assert_eq!(efi.parent_id, boot.mount_id);
    }

    // --- filtering --------------------------------------------------------

    #[test]
    fn kernel_pseudo_filesystems_are_not_user_storage() {
        let entries = parse(REAL);

        for path in ["/dev", "/sys", "/proc", "/run", "/tmp"] {
            assert!(
                !by_mount_point(&entries, path).is_user_storage(),
                "'{path}' must be filtered"
            );
        }
    }

    #[test]
    fn a_squashfs_snap_and_a_docker_overlay_are_filtered() {
        let entries = parse(REAL);

        assert!(!by_mount_point(&entries, "/var/lib/snapd/snap/bare/5").is_user_storage());
        assert!(
            !by_mount_point(&entries, "/var/lib/docker/overlay2/cab18b/merged").is_user_storage()
        );
    }

    #[test]
    fn real_filesystems_survive_the_filter() {
        let entries = parse(REAL);

        let kept: Vec<&str> = entries
            .iter()
            .filter(|entry| entry.is_user_storage())
            .map(|entry| entry.mount_point.as_str())
            .collect();

        assert_eq!(
            kept,
            [
                "/",
                "/home",
                "/boot",
                "/boot/efi",
                "/mnt/PROMETHEUS_DATA",
                "/mnt/PROMETHEUS_DEV",
                "/run/media/matheo/PROM_RESCUE",
            ]
        );
    }

    #[test]
    fn an_exfat_usb_partition_is_user_storage() {
        // An external disk is exactly the kind of volume the user cares about,
        // and its filesystem type is not one Linux ships for itself.
        let entries = parse(REAL);
        let entry = by_mount_point(&entries, "/mnt/PROMETHEUS_DATA");

        assert!(entry.is_user_storage());
        assert_eq!(entry.filesystem, "exfat");
        assert_eq!(entry.block_device_name(), Some("sda4"));
    }

    // --- correlation ------------------------------------------------------

    #[test]
    fn a_block_device_source_yields_its_kernel_name() {
        let entries = parse(REAL);

        assert_eq!(
            by_mount_point(&entries, "/").block_device_name(),
            Some("nvme0n1p8")
        );
        assert_eq!(
            by_mount_point(&entries, "/boot/efi").block_device_name(),
            Some("nvme0n1p1")
        );
    }

    #[test]
    fn a_source_that_is_not_a_block_device_correlates_to_nothing() {
        let entries = parse(REAL);

        assert_eq!(by_mount_point(&entries, "/tmp").block_device_name(), None);
        assert_eq!(
            by_mount_point(&entries, "/var/lib/docker/overlay2/cab18b/merged").block_device_name(),
            None
        );

        // A device-mapper path is not a kernel block-device name either; the
        // device number is the way to resolve it.
        let mapper = parse_line("1 2 253:0 / /a rw - ext4 /dev/mapper/fedora-root rw").expect("v");
        assert_eq!(mapper.block_device_name(), None);
    }

    #[test]
    fn an_unreadable_mount_table_yields_nothing_rather_than_a_failure() {
        assert!(read_from(Path::new("/nonexistent/pulse/mountinfo")).is_empty());
    }
}
