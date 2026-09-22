//! Parsing the ownership lines of `/proc/<pid>/status`.
//!
//! `stat(/proc/<pid>)` gives the *effective* owner of the directory, which is
//! what the snapshot classifies by. The inspector wants the real story:
//!
//! ```text
//! Uid:    1000    1000    1000    1000
//!         real    effective saved  filesystem
//! ```
//!
//! The **real** UID is who launched the process and is what the inspector
//! shows as the owner. Real and saved UIDs are also what the kernel compares
//! against when deciding whether PULSE may signal the process, so both are
//! kept.
//!
//! Pure, and tested on every host.

/// The four UIDs from one `Uid:` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uids {
    pub real: u32,
    pub effective: u32,
    pub saved: u32,
}

/// Extracts the `Uid:` line from a whole `status` file.
pub fn parse_uids(status: &str) -> Option<Uids> {
    let line = status.lines().find_map(|line| line.strip_prefix("Uid:"))?;
    let mut fields = line.split_whitespace().map(|field| field.parse::<u32>());

    Some(Uids {
        real: fields.next()?.ok()?,
        effective: fields.next()?.ok()?,
        saved: fields.next()?.ok()?,
    })
}

/// Whether a caller with effective UID `caller` may signal, renice or
/// re-pin a process owned by `target`, under ordinary Unix rules.
///
/// `kill(2)`: the sender's real or effective UID must equal the target's real
/// or saved UID. PULSE only knows its effective UID cheaply, which is the one
/// that matters for a normal desktop session. Root may do anything. This is a
/// *prediction* shown in the interface; the kernel's own answer at action
/// time is authoritative and is reported as-is.
pub fn may_control(caller: u32, target: &Uids) -> bool {
    caller == 0 || caller == target.real || caller == target.saved
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "Name:\tbash\nUmask:\t0022\nState:\tS (sleeping)\n\
        Tgid:\t4242\nPid:\t4242\nPPid:\t4000\n\
        Uid:\t1000\t1001\t1002\t1003\nGid:\t1000\t1000\t1000\t1000\n";

    #[test]
    fn reads_real_effective_and_saved() {
        assert_eq!(
            parse_uids(STATUS),
            Some(Uids {
                real: 1000,
                effective: 1001,
                saved: 1002
            })
        );
    }

    #[test]
    fn tolerates_spaces_instead_of_tabs() {
        assert_eq!(
            parse_uids("Uid:    0    0    0    0\n").map(|uids| uids.real),
            Some(0)
        );
    }

    #[test]
    fn rejects_a_missing_or_malformed_line() {
        assert_eq!(parse_uids("Name:\tbash\n"), None);
        assert_eq!(parse_uids("Uid:\t1000\n"), None);
        assert_eq!(parse_uids("Uid:\tabc\t1\t1\t1\n"), None);
    }

    #[test]
    fn ordinary_permission_rules() {
        let mine = Uids {
            real: 1000,
            effective: 1000,
            saved: 1000,
        };
        let roots = Uids {
            real: 0,
            effective: 0,
            saved: 0,
        };

        assert!(may_control(1000, &mine));
        assert!(!may_control(1000, &roots));
        assert!(may_control(0, &roots));
        assert!(may_control(0, &mine));
    }
}
