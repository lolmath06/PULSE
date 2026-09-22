//! Parsing `/proc/<pid>/stat`.
//!
//! # Why this file exists at all
//!
//! `/proc/<pid>/stat` is a single space-separated line, which invites exactly
//! one implementation and punishes it:
//!
//! ```text
//! 1234 (firefox) S 1 1234 1234 0 -1 4194560 …
//! ```
//!
//! Field 2 is `comm`, the first 15 bytes of the executable's name, wrapped in
//! parentheses — and the kernel does **not** escape it. Real names on a
//! running Fedora desktop include:
//!
//! ```text
//! 42 (Web Content) S …            a space
//! 43 (foo (bar)) S …              nested parentheses
//! 44 (kworker/3:1H-events) S …    slashes and colons
//! 45 (a b) c (d) S …              both, in the worst order
//! ```
//!
//! A `split_whitespace()` over the whole line shifts every subsequent field by
//! the number of spaces in the name. The consequence is not a parse error — it
//! is a *successful* parse of the wrong columns: `starttime` becomes garbage,
//! so the process identity changes every refresh, so no CPU baseline ever
//! matches, so the CPU column is permanently blank for exactly those processes
//! the user most wants to see. Silent, plausible, and wrong.
//!
//! So the parser splits on the **last** `)` in the line, never the first, and
//! is tested against every shape above.
//!
//! # Which fields, and why so few
//!
//! One read of this file yields the state, the parent, the thread count, the
//! CPU time, the resident set and the start token — six of the eight things a
//! row needs. Only the I/O counters and the executable path come from
//! elsewhere. Reading `/proc/<pid>/statm` and `/proc/<pid>/status` as well
//! would add two opens per process, several hundred per refresh, for numbers
//! this file already carries.
//!
//! Fields are numbered as `proc(5)` numbers them, starting at 1.

/// The `PF_KTHREAD` flag in field 9.
///
/// The kernel's own marker for "this is a kernel thread". Using it rather than
/// guessing from a bracketed name or a zero resident set is the difference
/// between a reliable classification and a fragile one — a user program is
/// perfectly entitled to be named `[something]` or to have been swapped out.
const PF_KTHREAD: u64 = 0x0020_0000;

/// The fields PULSE reads out of one `/proc/<pid>/stat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcStat {
    /// Field 1. Read back from the file rather than trusted from the directory
    /// name, so a mismatch can be detected.
    pub pid: u32,
    /// Field 2, with its parentheses removed and its contents untouched.
    pub comm: String,
    /// Field 3, a single letter. See [`ProcessState`].
    ///
    /// [`ProcessState`]: crate::processes::ProcessState
    pub state: char,
    /// Field 4.
    pub parent_pid: u32,
    /// Field 9, tested for [`PF_KTHREAD`].
    pub flags: u64,
    /// Field 14 + field 15: user and system CPU time, in clock ticks.
    ///
    /// `cutime`/`cstime` (fields 16 and 17) are deliberately **excluded**:
    /// they accumulate the CPU time of *reaped children*, so a shell that has
    /// run a long build would appear to be burning CPU it never used, and the
    /// same work would be counted twice across the table.
    pub cpu_ticks: u64,
    /// Field 20.
    pub num_threads: u32,
    /// Field 22 — clock ticks after boot at which this process started.
    ///
    /// The Linux half of [`ProcessInstanceId`]. Never interpreted as a time;
    /// only ever compared.
    ///
    /// [`ProcessInstanceId`]: crate::processes::ProcessInstanceId
    pub start_ticks: u64,
    /// Field 24 — resident set size, **in pages**.
    ///
    /// Signed in the kernel's own printout and occasionally negative for a
    /// process being torn down, which is why it is parsed as `i64` and clamped
    /// rather than rejected.
    pub rss_pages: i64,
}

impl ProcStat {
    /// Whether this is a kernel thread rather than a program.
    pub const fn is_kernel_thread(&self) -> bool {
        self.flags & PF_KTHREAD != 0
    }

    /// Resident memory in bytes, for a given page size.
    ///
    /// A negative page count — which the kernel does occasionally print for a
    /// dying process — yields `0`, not a wrapped enormous number.
    pub fn resident_bytes(&self, page_size: u64) -> u64 {
        u64::try_from(self.rss_pages)
            .unwrap_or(0)
            .saturating_mul(page_size)
    }

    /// Total CPU time in nanoseconds, for a given clock tick rate.
    ///
    /// Returns `None` for a nonsensical tick rate rather than dividing by
    /// zero.
    pub fn cpu_time_nanos(&self, ticks_per_second: u64) -> Option<u64> {
        if ticks_per_second == 0 {
            return None;
        }

        const NANOS_PER_SECOND: u128 = 1_000_000_000;
        let nanos = (u128::from(self.cpu_ticks) * NANOS_PER_SECOND) / u128::from(ticks_per_second);

        u64::try_from(nanos).ok()
    }
}

/// Parses one `/proc/<pid>/stat` line.
///
/// Returns `None` for anything malformed. A malformed line is **not** an error
/// worth surfacing: the overwhelmingly likely cause is that the process exited
/// while the file was being read, and the honest response is to leave that one
/// process out of this snapshot.
pub fn parse(content: &str) -> Option<ProcStat> {
    let line = content.trim_end_matches('\n');

    let open = line.find('(')?;
    // The LAST closing parenthesis, because `comm` may contain its own.
    let close = line.rfind(')')?;
    if close < open {
        return None;
    }

    let pid: u32 = line[..open].trim().parse().ok()?;
    let comm = line[open + 1..close].to_string();

    // Everything after `)` is plain whitespace-separated, and `after[n]` is
    // `proc(5)` field `n + 3`.
    let after: Vec<&str> = line[close + 1..].split_whitespace().collect();

    let field = |number: usize| after.get(number - 3).copied();
    let parse_u64 = |number: usize| field(number).and_then(|value| value.parse::<u64>().ok());

    let state = field(3)?.chars().next()?;
    let parent_pid = u32::try_from(parse_u64(4)?).ok()?;
    let flags = parse_u64(9)?;
    let utime = parse_u64(14)?;
    let stime = parse_u64(15)?;
    let num_threads = u32::try_from(parse_u64(20)?).ok()?;
    let start_ticks = parse_u64(22)?;
    let rss_pages: i64 = field(24)?.parse().ok()?;

    Some(ProcStat {
        pid,
        comm,
        state,
        parent_pid,
        flags,
        cpu_ticks: utime.saturating_add(stime),
        num_threads,
        start_ticks,
        rss_pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic line, with every field after 24 elided — the parser must
    /// not depend on them.
    fn line(pid: u32, comm: &str, state: char, extras: &str) -> String {
        format!(
            "{pid} ({comm}) {state} 1 1234 1234 0 -1 4194560 \
             5551 0 21 0 \
             1200 340 10 20 \
             20 0 \
             57 0 \
             8675309 \
             123456789 \
             4096 \
             {extras}"
        )
    }

    fn ordinary() -> String {
        line(
            1234,
            "firefox",
            'S',
            "18446744073709551615 94000000 94000000",
        )
    }

    #[test]
    fn parses_an_ordinary_process() {
        let stat = parse(&ordinary()).expect("parses");

        assert_eq!(stat.pid, 1234);
        assert_eq!(stat.comm, "firefox");
        assert_eq!(stat.state, 'S');
        assert_eq!(stat.parent_pid, 1);
        assert_eq!(stat.cpu_ticks, 1200 + 340);
        assert_eq!(stat.num_threads, 57);
        assert_eq!(stat.start_ticks, 8_675_309);
        assert_eq!(stat.rss_pages, 4096);
    }

    #[test]
    fn children_cpu_time_is_excluded_from_the_total() {
        // Fields 16 and 17 are 10 and 20 in the fixture; counting them would
        // attribute a reaped build's CPU time to the shell that launched it.
        let stat = parse(&ordinary()).expect("parses");
        assert_eq!(stat.cpu_ticks, 1540);
        assert_ne!(stat.cpu_ticks, 1540 + 30);
    }

    // --- the comm field, which is where naive parsers die ------------------

    #[test]
    fn a_name_containing_spaces_does_not_shift_every_later_field() {
        // `Web Content` is what every Firefox tab process is called.
        let stat = parse(&line(42, "Web Content", 'S', "")).expect("parses");

        assert_eq!(stat.comm, "Web Content");
        assert_eq!(stat.state, 'S');
        assert_eq!(stat.num_threads, 57);
        assert_eq!(stat.start_ticks, 8_675_309);
    }

    #[test]
    fn a_name_containing_parentheses_splits_on_the_last_one() {
        let stat = parse(&line(43, "foo (bar)", 'R', "")).expect("parses");

        assert_eq!(stat.comm, "foo (bar)");
        assert_eq!(stat.state, 'R');
        assert_eq!(stat.start_ticks, 8_675_309);
    }

    #[test]
    fn a_name_containing_both_spaces_and_parentheses_still_parses() {
        let stat = parse(&line(45, "a b) c (d", 'S', "")).expect("parses");

        assert_eq!(stat.comm, "a b) c (d");
        assert_eq!(stat.num_threads, 57);
        assert_eq!(stat.rss_pages, 4096);
    }

    #[test]
    fn kernel_worker_names_with_slashes_and_colons_parse() {
        let stat = parse(&line(44, "kworker/3:1H-events", 'I', "")).expect("parses");

        assert_eq!(stat.comm, "kworker/3:1H-events");
        assert_eq!(stat.state, 'I');
    }

    #[test]
    fn an_empty_name_is_still_a_name() {
        let stat = parse(&line(46, "", 'S', "")).expect("parses");
        assert_eq!(stat.comm, "");
    }

    #[test]
    fn a_naive_whitespace_split_would_have_been_wrong_here() {
        // The regression this parser exists to prevent, stated as a test: the
        // naive approach reads field 22 from the wrong column and produces a
        // different start token, hence a different identity, every refresh.
        let content = line(42, "Web Content", 'S', "");
        let naive: Vec<&str> = content.split_whitespace().collect();
        let naive_start: u64 = naive[21].parse().unwrap_or(0);

        assert_ne!(
            naive_start,
            parse(&content).expect("parses").start_ticks,
            "the naive split must genuinely disagree, or this test proves nothing"
        );
    }

    // --- states and classification ----------------------------------------

    #[test]
    fn a_zombie_parses_like_any_other_process() {
        let stat = parse(&line(47, "defunct", 'Z', "")).expect("parses");
        assert_eq!(stat.state, 'Z');
        assert_eq!(stat.num_threads, 57);
    }

    #[test]
    fn kernel_threads_are_recognised_from_the_kernels_own_flag() {
        let kernel = line(2, "kthreadd", 'S', "").replace(" 4194560 ", " 2129984 ");
        assert!(parse(&kernel).expect("parses").is_kernel_thread());
        assert!(!parse(&ordinary()).expect("parses").is_kernel_thread());
    }

    // --- extreme values ----------------------------------------------------

    #[test]
    fn a_large_pid_parses() {
        let stat = parse(&line(4_194_303, "late", 'S', "")).expect("parses");
        assert_eq!(stat.pid, 4_194_303);
    }

    #[test]
    fn a_process_with_many_threads_parses() {
        let content = ordinary().replace(" 57 0 ", " 4096 0 ");
        assert_eq!(parse(&content).expect("parses").num_threads, 4096);
    }

    #[test]
    fn enormous_counters_do_not_overflow() {
        let huge = u64::MAX;
        let content = ordinary().replace(" 1200 340 ", &format!(" {huge} {huge} "));
        let stat = parse(&content).expect("parses");

        // Saturating rather than wrapping: a wrapped total would become a
        // small number and look like a plausible reading.
        assert_eq!(stat.cpu_ticks, u64::MAX);
        assert_eq!(stat.cpu_time_nanos(100), None, "no honest nanosecond value");
    }

    #[test]
    fn a_negative_resident_set_becomes_zero_not_an_enormous_number() {
        let content = ordinary().replace(" 4096 ", " -1 ");
        let stat = parse(&content).expect("parses");

        assert_eq!(stat.rss_pages, -1);
        assert_eq!(stat.resident_bytes(4096), 0);
    }

    #[test]
    fn resident_bytes_multiplies_pages_by_the_page_size() {
        let stat = parse(&ordinary()).expect("parses");
        assert_eq!(stat.resident_bytes(4096), 4096 * 4096);
        assert_eq!(stat.resident_bytes(16384), 4096 * 16384);
    }

    #[test]
    fn cpu_time_converts_ticks_to_nanoseconds() {
        let stat = parse(&ordinary()).expect("parses");

        // 1540 ticks at 100 Hz is 15.4 s.
        assert_eq!(stat.cpu_time_nanos(100), Some(15_400_000_000));
        // The same ticks at 1000 Hz are ten times less time.
        assert_eq!(stat.cpu_time_nanos(1000), Some(1_540_000_000));
        assert_eq!(stat.cpu_time_nanos(0), None);
    }

    // --- malformed input ---------------------------------------------------

    #[test]
    fn malformed_lines_are_skipped_rather_than_fatal() {
        for malformed in [
            "",
            "\n",
            "not a stat line at all",
            "1234 firefox S 1",            // no parentheses
            "1234 (firefox",               // unterminated
            "1234 (firefox) S",            // truncated after the state
            ") 1234 (firefox S",           // parentheses in the wrong order
            "abc (firefox) S 1 2 3 4 5 6", // non-numeric pid
            &ordinary().replace(" 57 0 ", " notanumber 0 "),
            &ordinary().replace("8675309", "-5"),
        ] {
            assert!(
                parse(malformed).is_none(),
                "expected '{malformed}' to be rejected"
            );
        }
    }

    #[test]
    fn a_trailing_newline_is_tolerated() {
        let content = format!("{}\n", ordinary());
        assert!(parse(&content).is_some());
    }

    #[test]
    fn an_extremely_long_name_parses() {
        // `comm` is capped at 15 bytes by the kernel, but the parser must not
        // depend on that.
        let long = "x".repeat(4096);
        let stat = parse(&line(48, &long, 'S', "")).expect("parses");
        assert_eq!(stat.comm.len(), 4096);
    }
}
