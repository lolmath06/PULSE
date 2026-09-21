//! `/proc/diskstats` — every device's cumulative I/O counters, in one read.
//!
//! # Why this file rather than `/sys/block/<dev>/stat`
//!
//! The two carry the same numbers. `/sys/block/<dev>/stat` needs one open,
//! read and close **per device**, and gives no way to know which device a
//! given read belongs to except the path it came from. `/proc/diskstats` gives
//! every device in the machine in a single read, with the major/minor pair and
//! the kernel name on each line.
//!
//! That matters for more than speed. Rates are derived from the interval
//! between two snapshots, so every device's counters must be captured at the
//! *same* instant — otherwise two disks' figures describe two slightly
//! different windows, and a busy machine makes the skew visible. One read is
//! one instant.
//!
//! # The format
//!
//! Each line is `major minor name` followed by a series of counters. Kernel
//! 2.6 had 11, 4.18 added 4 for discards, 5.5 added 2 for flushes, and more
//! may follow. The parser therefore requires the **first eleven** and ignores
//! anything after them, rather than matching an exact column count that a
//! kernel upgrade would break.
//!
//! The fields PULSE uses, by index after the name:
//!
//! ```text
//! 0  reads completed
//! 2  sectors read          512-byte sectors, always
//! 3  milliseconds reading
//! 4  writes completed
//! 6  sectors written       512-byte sectors, always
//! 7  milliseconds writing
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::metrics::wellknown::storage::{StorageIoCounters, KERNEL_SECTOR_BYTES};

/// Where the kernel publishes the counters.
pub const DISKSTATS_PATH: &str = "/proc/diskstats";

/// The number of counter columns the parser requires.
///
/// Everything PULSE reads lives within the first eleven, which every kernel
/// since 2.6 provides. Requiring more would break on an old kernel; requiring
/// an exact count would break on a new one.
const REQUIRED_COUNTERS: usize = 11;

/// One line of `/proc/diskstats`, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskstatsEntry {
    pub major: u32,
    pub minor: u32,
    /// The kernel name, e.g. `nvme0n1`.
    pub name: String,
    pub counters: StorageIoCounters,
}

/// Parses one line, converting sectors to bytes.
///
/// Returns `None` for a line that is malformed, truncated or carries a
/// non-numeric field. A single bad line — which a kernel bug or a driver quirk
/// can produce — must not cost PULSE every other device's counters, so the
/// caller skips it and keeps going.
///
/// # Sectors are 512 bytes here
///
/// The kernel's block statistics count **512-byte sectors**, whatever the
/// device's logical block size. Multiplying by `queue/logical_block_size` is
/// the same mistake as for `/sys/block/<dev>/size`, and on a 4Kn drive it
/// would report eight times the real throughput.
pub fn parse_line(line: &str) -> Option<DiskstatsEntry> {
    let mut fields = line.split_ascii_whitespace();

    let major: u32 = fields.next()?.parse().ok()?;
    let minor: u32 = fields.next()?.parse().ok()?;
    let name = fields.next()?.to_string();

    let counters: Vec<u64> = fields
        .map(|field| field.parse::<u64>().ok())
        .collect::<Option<Vec<u64>>>()?;

    if counters.len() < REQUIRED_COUNTERS {
        return None;
    }

    Some(DiskstatsEntry {
        major,
        minor,
        name,
        counters: StorageIoCounters {
            read_operations: counters[0],
            read_bytes: counters[2].checked_mul(KERNEL_SECTOR_BYTES)?,
            read_time_ms: counters[3],
            write_operations: counters[4],
            write_bytes: counters[6].checked_mul(KERNEL_SECTOR_BYTES)?,
            write_time_ms: counters[7],
        },
    })
}

/// Parses a whole `/proc/diskstats`, keyed by kernel name.
///
/// Every line is attempted; the ones that do not parse are skipped. Partitions
/// are kept in the map — the caller decides which entries it cares about,
/// because the same parse also serves volume-to-device correlation, which
/// needs the partitions' major/minor pairs.
pub fn parse(contents: &str) -> BTreeMap<String, DiskstatsEntry> {
    contents
        .lines()
        .filter_map(parse_line)
        .map(|entry| (entry.name.clone(), entry))
        .collect()
}

/// Reads and parses `/proc/diskstats`.
///
/// An unreadable file yields an empty map rather than an error: the provider
/// reports each device's I/O as temporarily unavailable, which is more useful
/// than failing the whole sample and taking capacity and health down with it.
pub fn read_from(path: &Path) -> BTreeMap<String, DiskstatsEntry> {
    fs::read_to_string(path)
        .map(|contents| parse(&contents))
        .unwrap_or_default()
}

/// Reads this machine's counters.
pub fn read() -> BTreeMap<String, DiskstatsEntry> {
    read_from(Path::new(DISKSTATS_PATH))
}

#[cfg(test)]
pub(crate) mod fixtures {
    /// A verbatim excerpt of the development machine's `/proc/diskstats`,
    /// with the 20-column format a 6.11 kernel writes.
    pub const REAL: &str = " 259       0 nvme0n1 1644392 82662 74859674 631522 8136883 160455 214707066 47987341 0 3526819 50856289 228772 0 549907520 1848797 131906 388627
 259       1 nvme0n1p1 1244 1168 15216 299 2 0 2 8 0 615 840 50 0 501336 533 0 0
 259       2 nvme0n1p2 57 0 4416 34 0 0 0 0 0 32 34 0 0 0 0 0 0
 259       7 nvme0n1p7 233 6 11514 52 25 17 304 116 0 106 181 24 0 1079576 12 0 0
 259       8 nvme0n1p8 1642533 81488 74807120 631042 8136855 160438 214706760 47987216 0 3680659 50466510 228698 0 548326608 1848251 0 0
 252       0 zram0 1273563 0 10190432 3832 2648163 0 21185304 13712 0 49955 17544 0 0 0 0 0 0
   7       0 loop0 10 0 20 0 0 0 0 0 0 1 0 0 0 0 0 0 0
   8       0 sda 7108064 2784006 1165682685 41551651 1531569 1744130 1266500257 168416043 0 19011098 211003330 0 0 0 0 11715 1035636
   8       2 sda2 623 0 8690 1981 13 2 104 180 0 1391 2162 0 0 0 0 0 0
   8       3 sda3 871855 42469 73406714 15334026 630033 249356 495822768 63675368 0 7303296 79009394 0 0 0 0 0 0
   8       4 sda4 6235351 2741533 1092254553 26211969 901519 1494772 770677381 104740493 0 14392519 130952463 0 0 0 0 0 0";

    /// The eleven-column format older kernels write, with no discard or flush
    /// counters at all.
    pub const LEGACY_ELEVEN_COLUMNS: &str =
        "   8       0 sdb 100 10 2000 500 50 5 800 250 0 700 750";
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn parses_a_real_nvme_line() {
        let entry = parse_line(
            " 259       0 nvme0n1 1644392 82662 74859674 631522 8136883 160455 214707066 47987341 0 3526819 50856289 228772 0 549907520 1848797 131906 388627",
        )
        .expect("valid line");

        assert_eq!((entry.major, entry.minor), (259, 0));
        assert_eq!(entry.name, "nvme0n1");
        assert_eq!(entry.counters.read_operations, 1_644_392);
        assert_eq!(entry.counters.write_operations, 8_136_883);
        assert_eq!(entry.counters.read_time_ms, 631_522);
        assert_eq!(entry.counters.write_time_ms, 47_987_341);
    }

    #[test]
    fn sectors_are_converted_at_512_bytes_each() {
        let entry = parse_line(&format!(
            "259 0 nvme0n1 1 0 {sectors} 3 4 0 {written} 7 0 8 9",
            sectors = 74_859_674_u64,
            written = 214_707_066_u64
        ))
        .expect("valid");

        assert_eq!(entry.counters.read_bytes, 74_859_674 * 512);
        assert_eq!(entry.counters.write_bytes, 214_707_066 * 512);
    }

    #[test]
    fn sectors_are_never_scaled_by_a_logical_block_size() {
        // The 4Kn trap: these counters are in 512-byte units whatever the
        // device's logical block size, so scaling them would report eight
        // times the real throughput.
        let entry = parse_line("259 0 nvme0n1 1 0 1000 3 4 0 2000 7 0 8 9").expect("valid");

        assert_eq!(entry.counters.read_bytes, 512_000);
        assert_ne!(entry.counters.read_bytes, 1000 * 4096);
    }

    #[test]
    fn tolerates_the_extra_columns_newer_kernels_add() {
        // 20 columns on a 6.11 kernel today, more tomorrow. What PULSE reads
        // lives in the first eleven, and the rest is ignored rather than
        // rejected.
        let parsed = parse(REAL);

        assert!(parsed.contains_key("nvme0n1"));
        assert_eq!(parsed["nvme0n1"].counters.read_operations, 1_644_392);
        assert_eq!(parsed["sda"].counters.write_operations, 1_531_569);
    }

    #[test]
    fn accepts_the_eleven_column_format_of_older_kernels() {
        let entry = parse_line(LEGACY_ELEVEN_COLUMNS).expect("valid");

        assert_eq!(entry.name, "sdb");
        assert_eq!(entry.counters.read_operations, 100);
        assert_eq!(entry.counters.read_bytes, 2000 * 512);
        assert_eq!(entry.counters.write_time_ms, 250);
    }

    #[test]
    fn refuses_a_line_with_too_few_counters() {
        // Ten columns: the write time PULSE needs is not there, and guessing
        // it would fabricate a latency.
        assert_eq!(
            parse_line("8 0 sdb 100 10 2000 500 50 5 800 250 0 700"),
            None
        );
        assert_eq!(parse_line("8 0 sdb"), None);
        assert_eq!(parse_line(""), None);
    }

    #[test]
    fn refuses_a_malformed_line_without_losing_the_others() {
        let contents = "259 0 nvme0n1 1 0 10 3 4 0 20 7 0 8 9\n\
                        this is not a diskstats line at all\n\
                        8 0 sda x y z\n\
                        \n\
                        8 16 sdb 2 0 30 3 4 0 40 7 0 8 9";

        let parsed = parse(contents);

        assert_eq!(parsed.len(), 2, "the good lines survive");
        assert!(parsed.contains_key("nvme0n1"));
        assert!(parsed.contains_key("sdb"));
        assert!(!parsed.contains_key("sda"));
    }

    #[test]
    fn partitions_are_parsed_too_so_volumes_can_be_correlated() {
        let parsed = parse(REAL);

        // The disk and its partitions both appear, with distinct minors.
        assert_eq!(parsed["nvme0n1"].minor, 0);
        assert_eq!(parsed["nvme0n1p7"].minor, 7);
        assert_eq!(parsed["nvme0n1p8"].minor, 8);
        assert_eq!(parsed["nvme0n1p8"].major, 259);
    }

    #[test]
    fn pseudo_devices_are_parsed_here_and_filtered_by_the_inventory() {
        // `/proc/diskstats` lists them; excluding them is the inventory's job,
        // and doing it in two places would risk the two disagreeing.
        let parsed = parse(REAL);

        assert!(parsed.contains_key("zram0"));
        assert!(parsed.contains_key("loop0"));
    }

    #[test]
    fn an_overflowing_sector_count_is_refused_rather_than_wrapped() {
        let line = format!("8 0 sdb 1 0 {} 3 4 0 5 7 0 8 9", u64::MAX);
        assert_eq!(parse_line(&line), None);
    }

    #[test]
    fn a_negative_counter_is_refused() {
        // The kernel never writes one, but a value that does not parse as
        // unsigned must not become a huge positive number.
        assert_eq!(parse_line("8 0 sdb 1 0 -5 3 4 0 5 7 0 8 9"), None);
    }

    #[test]
    fn an_unreadable_file_yields_an_empty_map_rather_than_a_failure() {
        assert!(read_from(Path::new("/nonexistent/pulse/diskstats")).is_empty());
    }

    #[test]
    fn a_counter_reset_is_visible_as_smaller_values_not_handled_here() {
        // The parser reports what the kernel says. Detecting a rollback is the
        // tracker's job, and doing it in both places would let the two
        // disagree about what a reset is.
        let before = parse_line("8 0 sdb 1000 0 5000 100 200 0 900 50 0 1 2").expect("valid");
        let after = parse_line("8 0 sdb 5 0 10 1 2 0 4 1 0 1 2").expect("valid");

        assert!(after.counters.went_backwards(&before.counters));
    }
}
