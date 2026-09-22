//! [`ProcessInstanceId`] — the identity of one *incarnation* of a process.
//!
//! # A PID is not an identity
//!
//! Operating systems recycle process identifiers. On Fedora the kernel wraps
//! at `/proc/sys/kernel/pid_max` — 4 194 304 by default, but a busy machine
//! building software churns through PIDs in minutes — and Windows reuses them
//! aggressively and without any ordering guarantee.
//!
//! So this sequence is entirely ordinary:
//!
//! ```text
//! 10:00:00   PID 1234 = firefox      CPU baseline recorded: 812 s of CPU time
//! 10:00:03   firefox exits
//! 10:00:04   PID 1234 = cargo        CPU counter now reads 0.2 s
//! ```
//!
//! A tracker keyed on the PID alone would difference `0.2 s` against `812 s`,
//! get a negative delta, and either clamp it to zero (hiding real work) or
//! wrap it into an astronomical positive (a 10 000 % CPU spike). Both are the
//! kind of confidently-wrong number PULSE exists not to produce.
//!
//! # The fix: PID plus start token
//!
//! Every process carries a creation timestamp that the kernel never rewrites:
//!
//! | Platform | Source | Meaning |
//! |---|---|---|
//! | Linux | field 22 of `/proc/<pid>/stat` | clock ticks after boot |
//! | Windows | `GetProcessTimes` → `ftCreationTime` | 100 ns ticks since 1601 |
//!
//! PULSE does not interpret either — it only needs a value that is *stable for
//! one incarnation and different for the next*. That value is the **start
//! token**, and `(pid, start_token)` is the identity every baseline is keyed
//! on. Two incarnations of PID 1234 are then two different keys, and the
//! second one starts from no baseline instead of inheriting the first's.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The identity of one running incarnation of a process.
///
/// Compared, hashed and ordered on **both** fields. Constructing one from a
/// PID alone is deliberately impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInstanceId {
    /// The operating system's process identifier. A handle, never an identity.
    pub pid: u32,
    /// An opaque, platform-defined creation token. See the module docs.
    pub start_token: u64,
}

impl ProcessInstanceId {
    pub const fn new(pid: u32, start_token: u64) -> Self {
        Self { pid, start_token }
    }

    /// The canonical `process:<pid>-<token>` rendering.
    ///
    /// Shaped to satisfy the [`SourceId`] grammar — kind `process`, a
    /// digit-led instance, `-` as the only punctuation — because a process
    /// *is* conceptually a measurable source. It is nevertheless **never
    /// registered in the metric catalog**: a catalog entry per PID would add
    /// hundreds of ephemeral definitions per refresh, and
    /// `docs/metrics/processes.md` explains why that is refused.
    ///
    /// [`SourceId`]: crate::metrics::model::SourceId
    pub fn canonical_string(&self) -> String {
        format!("process:{}-{}", self.pid, self.start_token)
    }

    /// Parses the canonical `process:<pid>-<token>` form back into an identity.
    ///
    /// The frontend hands this string back with every inspector and control
    /// request, so it is **input** and is validated strictly: the exact
    /// prefix, two unsigned decimal integers, nothing else. PID 0 is refused
    /// — it is the Windows Idle pseudo-process and, on Linux, not a process
    /// at all — so no request can ever address it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let rest = text
            .strip_prefix("process:")
            .ok_or_else(|| format!("'{text}' is not a process instance identifier"))?;
        let (pid, token) = rest
            .split_once('-')
            .ok_or_else(|| format!("'{text}' has no start token"))?;

        let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !digits(pid) || !digits(token) {
            return Err(format!("'{text}' is not of the form process:<pid>-<token>"));
        }

        let pid: u32 = pid
            .parse()
            .map_err(|_| format!("'{text}' carries a PID out of range"))?;
        let start_token: u64 = token
            .parse()
            .map_err(|_| format!("'{text}' carries a start token out of range"))?;

        if pid == 0 {
            return Err("PID 0 is not a process PULSE can address".to_string());
        }

        Ok(Self::new(pid, start_token))
    }
}

impl fmt::Display for ProcessInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::SourceId;

    #[test]
    fn the_same_pid_with_a_different_start_token_is_a_different_process() {
        let firefox = ProcessInstanceId::new(1234, 9_001);
        let cargo = ProcessInstanceId::new(1234, 9_777);

        assert_ne!(firefox, cargo);
        assert_ne!(firefox.canonical_string(), cargo.canonical_string());
    }

    #[test]
    fn the_same_incarnation_is_stable_across_snapshots() {
        assert_eq!(
            ProcessInstanceId::new(1234, 9_001),
            ProcessInstanceId::new(1234, 9_001)
        );
    }

    #[test]
    fn different_pids_sharing_a_start_token_stay_distinct() {
        // Two processes spawned in the same clock tick share a Linux
        // `starttime`. Only the pair is unique.
        assert_ne!(
            ProcessInstanceId::new(1234, 9_001),
            ProcessInstanceId::new(1235, 9_001)
        );
    }

    #[test]
    fn the_canonical_form_obeys_the_source_id_grammar() {
        for id in [
            ProcessInstanceId::new(1, 0),
            ProcessInstanceId::new(1234, 9_001),
            ProcessInstanceId::new(4_194_303, u64::MAX),
        ] {
            let rendered = id.canonical_string();
            let source = SourceId::new(rendered.clone())
                .unwrap_or_else(|error| panic!("'{rendered}' must be a valid SourceId: {error}"));
            assert_eq!(source.kind(), "process");
            assert!(source.has_canonical_kind());
        }
    }

    #[test]
    fn parses_its_own_canonical_form() {
        for id in [
            ProcessInstanceId::new(1, 0),
            ProcessInstanceId::new(1234, 9_001),
            ProcessInstanceId::new(u32::MAX, u64::MAX),
        ] {
            assert_eq!(ProcessInstanceId::parse(&id.canonical_string()), Ok(id));
        }
    }

    #[test]
    fn rejects_anything_that_is_not_exactly_the_canonical_form() {
        for bad in [
            "",
            "1234",
            "process:",
            "process:1234",
            "process:1234-",
            "process:-9001",
            "process:12a4-9001",
            "process:1234-9001-7",
            "process:+1234-9001",
            "process: 1234-9001",
            "cpu:1234-9001",
            "process:0-9001",
            "process:4294967296-1",
            "process:1-18446744073709551616",
            "process:1234-9001; rm -rf /",
        ] {
            assert!(ProcessInstanceId::parse(bad).is_err(), "accepted '{bad}'");
        }
    }

    #[test]
    fn renders_as_its_canonical_form() {
        assert_eq!(
            ProcessInstanceId::new(1234, 9_001).to_string(),
            "process:1234-9001"
        );
    }
}
