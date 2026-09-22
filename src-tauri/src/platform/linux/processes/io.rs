//! Parsing `/proc/<pid>/io`.
//!
//! # `read_bytes`, not `rchar`
//!
//! The file offers two pairs of counters and they measure different things:
//!
//! | Field | Counts |
//! |---|---|
//! | `rchar` / `wchar` | every byte passed to `read(2)` / `write(2)` |
//! | `read_bytes` / `write_bytes` | bytes that actually moved to or from a block device |
//!
//! `rchar` includes bytes served from the page cache, read from a pipe, from a
//! socket, from a tty and from `/proc` itself. A process re-reading a cached
//! file would be reported at several gigabytes per second off a disk whose
//! activity light never blinked — and PULSE's own storage metrics, which come
//! from the block layer, would disagree with its own process table.
//!
//! So PULSE uses `read_bytes`/`write_bytes` and documents it. The cost is that
//! a process doing purely cached or purely network I/O reports `0 B/s`, which
//! is the correct answer to *"what is this process doing to my storage"*.
//!
//! # This file is often refused, and that is not an error
//!
//! `/proc/<pid>/io` is `0400` and owned by the process's user. PULSE runs
//! unprivileged, so every process belonging to another user — most of the
//! system daemons on a normal Fedora desktop — refuses it with `EACCES`. That
//! is a `permissionDenied` on two columns of that row, never a reason to hide
//! the row and never a reason to print `0 B/s`.

use crate::processes::RawProcessIo;

/// Parses the two counters PULSE uses out of `/proc/<pid>/io`.
///
/// Returns `None` when either is missing or unparsable — which happens for a
/// kernel thread, for a kernel built without `CONFIG_TASK_IO_ACCOUNTING`, and
/// for a process that exited mid-read.
pub fn parse(content: &str) -> Option<RawProcessIo> {
    let field = |name: &str| {
        content.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == name).then(|| value.trim().parse::<u64>().ok())?
        })
    };

    Some(RawProcessIo {
        read_bytes: field("read_bytes")?,
        write_bytes: field("write_bytes")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = "rchar: 323934931\n\
                        wchar: 323929600\n\
                        syscr: 632687\n\
                        syscw: 632675\n\
                        read_bytes: 0\n\
                        write_bytes: 323932160\n\
                        cancelled_write_bytes: 0\n";

    #[test]
    fn reads_the_block_device_counters() {
        let io = parse(REAL).expect("parses");

        assert_eq!(io.read_bytes, 0);
        assert_eq!(io.write_bytes, 323_932_160);
    }

    #[test]
    fn never_reads_rchar_or_wchar_as_storage_traffic() {
        // The whole point of the module. `rchar` here is 323 934 931 — three
        // orders of magnitude away from the real read figure of 0.
        let io = parse(REAL).expect("parses");

        assert_ne!(io.read_bytes, 323_934_931, "rchar is not storage traffic");
        assert_ne!(io.write_bytes, 323_929_600, "wchar is not storage traffic");
    }

    #[test]
    fn a_genuine_zero_parses_as_zero() {
        let io = parse("read_bytes: 0\nwrite_bytes: 0\n").expect("parses");
        assert_eq!(io.read_bytes, 0);
        assert_eq!(io.write_bytes, 0);
    }

    #[test]
    fn enormous_counters_parse() {
        let content = format!("read_bytes: {}\nwrite_bytes: {}\n", u64::MAX, u64::MAX);
        let io = parse(&content).expect("parses");
        assert_eq!(io.read_bytes, u64::MAX);
    }

    #[test]
    fn a_kernel_without_io_accounting_yields_nothing_rather_than_zero() {
        // Missing counters must not become `0 B/s`, which reads as measured
        // and idle.
        assert!(parse("rchar: 10\nwchar: 20\n").is_none());
        assert!(parse("read_bytes: 5\n").is_none());
        assert!(parse("write_bytes: 5\n").is_none());
    }

    #[test]
    fn malformed_content_is_skipped_rather_than_fatal() {
        for malformed in [
            "",
            "\n\n",
            "read_bytes:\nwrite_bytes:\n",
            "read_bytes: not a number\nwrite_bytes: 3\n",
            "read_bytes: -1\nwrite_bytes: 3\n",
            "garbage without colons",
        ] {
            assert!(
                parse(malformed).is_none(),
                "expected '{malformed}' rejected"
            );
        }
    }

    #[test]
    fn a_similarly_named_field_is_not_mistaken_for_the_real_one() {
        // `cancelled_write_bytes` ends in `write_bytes`; a substring match
        // would pick it up.
        let content = "cancelled_write_bytes: 999\nread_bytes: 1\nwrite_bytes: 2\n";
        let io = parse(content).expect("parses");
        assert_eq!(io.write_bytes, 2);
    }
}
