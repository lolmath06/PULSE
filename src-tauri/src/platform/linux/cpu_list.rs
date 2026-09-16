//! The kernel's "CPU list" format.
//!
//! Several files under `/sys/devices/system/cpu/` describe sets of CPUs with a
//! compact comma-and-dash syntax:
//!
//! ```text
//! 0-31          every CPU from 0 to 31
//! 0-3,8-11      two ranges, with a gap — CPUs 4-7 are offline or absent
//! 0,2,4,6       individual CPUs
//! 3             a single CPU
//! (empty)       no CPUs at all
//! ```
//!
//! **Gaps are normal, not an anomaly.** Offlining a CPU
//! (`echo 0 > /sys/devices/system/cpu/cpu4/online`), CPU hotplug on a virtual
//! machine, and some firmware configurations all produce non-contiguous lists.
//! Code that assumes `0..n` breaks on exactly those machines, so PULSE parses
//! the list the kernel actually wrote.
//!
//! Parsing is a pure function over a string so every shape above — and every
//! malformed one — is testable without a Linux host.

use crate::metrics::model::{MetricError, MetricErrorCode};
use crate::metrics::wellknown::cpu::LogicalId;

/// Upper bound on the CPU numbers PULSE will accept from a list.
///
/// `CONFIG_NR_CPUS` tops out well below this on every shipping kernel. The
/// bound exists so a corrupted or hostile file cannot make PULSE try to
/// materialise four billion entries from a range like `0-4294967295`.
const MAX_CPU_ORDINAL: u32 = 8_192;

/// Parses a kernel CPU list into ascending, de-duplicated ordinals.
///
/// Returns an empty vector for empty input — a legitimate answer, meaning "no
/// CPUs in this set", not an error.
///
/// Malformed input is rejected rather than silently partially parsed: a list
/// PULSE cannot fully understand would silently drop processors from the
/// catalog, which is worse than falling back to another source.
pub fn parse_cpu_list(content: &str) -> Result<Vec<LogicalId>, MetricError> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut ordinals: Vec<u32> = Vec::new();

    for entry in trimmed.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            return Err(malformed(content, "empty entry between commas"));
        }

        match entry.split_once('-') {
            None => ordinals.push(ordinal(content, entry)?),
            Some((start, end)) => {
                let start = ordinal(content, start.trim())?;
                let end = ordinal(content, end.trim())?;

                if end < start {
                    return Err(malformed(
                        content,
                        &format!("range '{entry}' ends before it starts"),
                    ));
                }

                ordinals.extend(start..=end);
            }
        }
    }

    ordinals.sort_unstable();
    ordinals.dedup();

    Ok(ordinals.into_iter().map(LogicalId::new).collect())
}

fn ordinal(content: &str, raw: &str) -> Result<u32, MetricError> {
    let value: u32 = raw
        .parse()
        .map_err(|_| malformed(content, &format!("'{raw}' is not a CPU number")))?;

    if value > MAX_CPU_ORDINAL {
        return Err(malformed(
            content,
            &format!("CPU number {value} exceeds the supported maximum of {MAX_CPU_ORDINAL}"),
        ));
    }

    Ok(value)
}

fn malformed(content: &str, detail: &str) -> MetricError {
    MetricError::new(
        MetricErrorCode::Parse,
        format!("malformed CPU list '{}': {detail}", content.trim()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(content: &str) -> Vec<u32> {
        parse_cpu_list(content)
            .expect("valid CPU list")
            .into_iter()
            .map(LogicalId::get)
            .collect()
    }

    #[test]
    fn parses_a_single_cpu() {
        assert_eq!(parse("0"), [0]);
        assert_eq!(parse("3"), [3]);
        assert_eq!(parse("127"), [127]);
    }

    #[test]
    fn parses_a_contiguous_range() {
        assert_eq!(parse("0-3"), [0, 1, 2, 3]);
        assert_eq!(parse("0-31").len(), 32);
        assert_eq!(parse("4-4"), [4], "a one-element range is valid");
    }

    #[test]
    fn parses_a_list_with_gaps() {
        // The case a naive `0..n` assumption gets wrong.
        assert_eq!(parse("0-3,8-11"), [0, 1, 2, 3, 8, 9, 10, 11]);
        assert_eq!(parse("0,2,4"), [0, 2, 4]);
        assert_eq!(parse("1,3-5,9"), [1, 3, 4, 5, 9]);
    }

    #[test]
    fn an_empty_list_means_no_cpus_not_an_error() {
        assert!(parse_cpu_list("").expect("valid").is_empty());
        assert!(parse_cpu_list("\n").expect("valid").is_empty());
        assert!(parse_cpu_list("   ").expect("valid").is_empty());
    }

    #[test]
    fn tolerates_the_trailing_newline_sysfs_writes() {
        assert_eq!(parse("0-3\n"), [0, 1, 2, 3]);
        assert_eq!(parse(" 0-3 \n"), [0, 1, 2, 3]);
        assert_eq!(parse("0 - 3"), [0, 1, 2, 3]);
    }

    #[test]
    fn output_is_sorted_and_deduplicated() {
        // Not a shape sysfs produces, but the parser must not propagate it.
        assert_eq!(parse("8,0,4"), [0, 4, 8]);
        assert_eq!(parse("0-3,2-5"), [0, 1, 2, 3, 4, 5]);
        assert_eq!(parse("1,1,1"), [1]);
    }

    #[test]
    fn rejects_malformed_lists_rather_than_parsing_them_partially() {
        for content in [
            "abc", "0-", "-3", "0,,3", "0-3,", ",0-3", "0..3", "0-3-5", "-1", "0.5", "0 3", "cpu0",
        ] {
            let error = parse_cpu_list(content)
                .err()
                .unwrap_or_else(|| panic!("'{content}' must be rejected"));
            assert_eq!(error.code, MetricErrorCode::Parse);
        }
    }

    #[test]
    fn rejects_a_backwards_range() {
        let error = parse_cpu_list("7-3").expect_err("must be rejected");
        assert!(error.message.contains("ends before it starts"));
    }

    #[test]
    fn refuses_an_absurd_range_instead_of_exhausting_memory() {
        // A corrupted file must not make PULSE allocate four billion entries.
        assert!(parse_cpu_list("0-4294967295").is_err());
        assert!(parse_cpu_list("0-100000").is_err());
        assert!(parse_cpu_list(&format!("0-{MAX_CPU_ORDINAL}")).is_ok());
    }

    #[test]
    fn handles_a_large_but_realistic_machine() {
        // 256 logical processors: a real dual-socket server.
        let parsed = parse("0-255");
        assert_eq!(parsed.len(), 256);
        assert_eq!(parsed[0], 0);
        assert_eq!(parsed[255], 255);
    }
}
