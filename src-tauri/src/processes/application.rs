//! Grouping processes into applications.
//!
//! # Why a name is not enough
//!
//! Modern programs are process swarms. Firefox runs a parent plus one content
//! process per few tabs, a GPU process, a socket process and several utility
//! processes; Chrome, Electron, VS Code and Steam do the same. A process list
//! that shows thirteen `firefox` rows answers "what is running" but not "what
//! is using my machine".
//!
//! But grouping by the displayed name alone is wrong in the other direction:
//!
//! ```text
//! /usr/bin/python3        a backup script
//! /opt/tool/venv/bin/python3   something else entirely
//! ```
//!
//! `python`, `node`, `java`, `sh` and `electron` are the names of *runtimes*,
//! not of applications. Folding two unrelated programs together because they
//! share an interpreter would produce a row whose CPU total means nothing.
//!
//! So the grouping key is, in order of preference:
//!
//! 1. the **executable identity** — the resolved path, which two different
//!    programs cannot share;
//! 2. failing that, the **process name**, with the grouping marked as
//!    lower-confidence so the interface can say so.
//!
//! The key is an internal, deterministic value. It is **not** a [`SourceId`]
//! and is not stored in any dashboard: it contains a filesystem path, which
//! may include a user's home directory and therefore their name, and PULSE
//! does not put that in an identifier that outlives the window.
//!
//! [`SourceId`]: crate::metrics::model::SourceId

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::metrics::model::Availability;

use super::field::Field;
use super::snapshot::ProcessEntry;
use super::state::ProcessClass;

/// How confident the grouping is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplicationIdentity {
    /// Grouped on the executable's resolved path. Two different programs
    /// cannot collide.
    Executable,
    /// Grouped on the process name because no path was readable. Two
    /// unrelated `python3` processes *can* collide here, and the interface
    /// says as much.
    Name,
}

/// One application: the processes sharing an executable identity, summed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationEntry {
    /// Deterministic internal key, `exe:<path>` or `name:<name>`.
    pub key: String,
    /// The executable's own name, cleaned of its path and `.exe` suffix.
    pub display_name: String,
    pub identity: ApplicationIdentity,
    pub classification: ProcessClass,
    pub process_count: u32,
    pub thread_count: Field<f64>,
    pub cpu_percent: Field<f64>,
    pub resident_memory_bytes: Field<f64>,
    pub read_bytes_per_second: Field<f64>,
    pub write_bytes_per_second: Field<f64>,
}

/// Whether a path uses Windows conventions, and is therefore case-insensitive.
///
/// Decided from the path's own shape rather than from the compilation target,
/// so the grouping logic is one function tested against both platforms' paths
/// on either host.
fn is_windows_path(path: &str) -> bool {
    path.contains('\\')
        || path
            .as_bytes()
            .get(1)
            .is_some_and(|byte| *byte == b':' && path.as_bytes()[0].is_ascii_alphabetic())
}

/// The canonical form of an executable path for grouping.
///
/// Windows paths are lowercased because `C:\Program Files\App\app.exe` and
/// `c:\program files\app\APP.EXE` name the same file. POSIX paths are left
/// exactly as they are, because there they name two different files.
fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if is_windows_path(trimmed) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// Whether a file name is a version string rather than a program name.
///
/// Some programs install each release in its own directory and name the
/// binary after the version: `~/.local/share/thing/versions/2.1.278`. Taking
/// the file name there produces an application called `2.1.278`, which names
/// nothing. Digits and dots only is a narrow, unambiguous test — no real
/// program is called `2.1.278` — and it is the *only* name-shape rule PULSE
/// applies.
fn looks_like_a_version(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_digit() || c == '.')
        && name.chars().any(|c| c.is_ascii_digit())
}

/// The application display name for a path or a process name.
///
/// `/usr/lib64/firefox/firefox` → `firefox`.
/// `C:\Program Files\Mozilla Firefox\firefox.exe` → `firefox`.
/// `~/.local/share/app/versions/2.1.278` → the process name, not `2.1.278`.
///
/// Deliberately minimal: no icon database, no `.desktop` lookup, no
/// capitalisation invented for a program PULSE knows nothing about. The
/// executable's own name is what the user's own system calls it.
pub fn display_name_for(path: Option<&str>, process_name: &str) -> String {
    let from_path = path
        .and_then(|path| {
            let trimmed = path.trim().trim_end_matches(['/', '\\']);
            trimmed
                .rsplit(['/', '\\'])
                .next()
                .map(str::to_string)
                .filter(|name| !name.is_empty())
        })
        .filter(|name| !looks_like_a_version(name));

    let name = from_path.unwrap_or_else(|| process_name.trim().to_string());
    let stripped = strip_executable_suffix(&name);

    if stripped.is_empty() {
        process_name.trim().to_string()
    } else {
        stripped
    }
}

/// Removes a trailing `.exe`, whatever its casing. Nothing else is stripped:
/// `node.js` is a real program name and `plasma-shell` keeps its hyphen.
fn strip_executable_suffix(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.strip_suffix(".exe") {
        Some(_) => name[..name.len() - 4].to_string(),
        None => name.to_string(),
    }
}

/// The grouping key and how confident it is.
pub fn application_key(path: Option<&str>, process_name: &str) -> (String, ApplicationIdentity) {
    match path.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => (
            format!("exe:{}", normalize_path(path)),
            ApplicationIdentity::Executable,
        ),
        // A process whose executable could not be read — a protected Windows
        // process, a Linux process owned by another user — still groups, just
        // less confidently.
        None => (
            format!("name:{}", process_name.trim().to_ascii_lowercase()),
            ApplicationIdentity::Name,
        ),
    }
}

/// Sums a set of per-process fields into one application field.
///
/// **A sum of what was measured, never of what was assumed.** If three of an
/// application's five processes report CPU and two are refused, the total is
/// the three — an understatement, but a true one. If *nothing* was measured,
/// the reason is carried up rather than replaced by `0`, so an application
/// whose processes are all still waiting for a baseline says "waiting" rather
/// than "idle".
fn sum(fields: &[&Field<f64>]) -> Field<f64> {
    let mut total = 0.0_f64;
    let mut measured = false;

    for field in fields {
        if let Some(value) = field.value {
            total += value;
            measured = true;
        }
    }

    if measured && total.is_finite() && total >= 0.0 {
        return Field::available(total);
    }

    // Nothing usable. Report the most actionable reason among the members:
    // "waiting" first because it resolves itself, then a refusal the user
    // could act on, then whatever the first member said.
    let pick = |status: &str| {
        fields
            .iter()
            .find(|field| field.availability.status_str() == status)
            .map(|field| field.availability.clone())
    };

    let availability = pick("temporarilyUnavailable")
        .or_else(|| pick("permissionDenied"))
        .or_else(|| fields.first().map(|field| field.availability.clone()))
        .unwrap_or_else(|| Availability::not_detected("this application reported no processes"));

    Field::missing(availability)
}

/// The class of an application, from the classes of its processes.
fn classify(processes: &[&ProcessEntry]) -> ProcessClass {
    if processes
        .iter()
        .any(|process| process.classification == ProcessClass::UserApplication)
    {
        ProcessClass::UserApplication
    } else if !processes.is_empty()
        && processes
            .iter()
            .all(|process| process.classification == ProcessClass::KernelThread)
    {
        ProcessClass::KernelThread
    } else if !processes.is_empty()
        && processes.iter().all(|process| {
            matches!(
                process.classification,
                ProcessClass::SystemProcess | ProcessClass::KernelThread
            )
        })
    {
        ProcessClass::SystemProcess
    } else {
        ProcessClass::Unknown
    }
}

/// Groups processes into applications and sums each group.
///
/// Ordered CPU descending, then display name, then key — the same tie-break
/// the process table uses, so neither view reshuffles rows that are equal.
pub fn aggregate(processes: &[ProcessEntry]) -> Vec<ApplicationEntry> {
    let mut groups: BTreeMap<&str, Vec<&ProcessEntry>> = BTreeMap::new();

    for process in processes {
        groups
            .entry(process.application_key.as_str())
            .or_default()
            .push(process);
    }

    let mut applications: Vec<ApplicationEntry> = groups
        .into_iter()
        .map(|(key, members)| {
            let identity = if key.starts_with("exe:") {
                ApplicationIdentity::Executable
            } else {
                ApplicationIdentity::Name
            };

            let display_name = members
                .first()
                .map(|process| {
                    display_name_for(process.executable_path.value.as_deref(), &process.name)
                })
                .unwrap_or_default();

            let field = |pick: fn(&ProcessEntry) -> &Field<f64>| {
                sum(&members.iter().map(|m| pick(m)).collect::<Vec<_>>())
            };

            ApplicationEntry {
                key: key.to_string(),
                display_name,
                identity,
                classification: classify(&members),
                process_count: members.len() as u32,
                thread_count: field(|process| &process.thread_count),
                cpu_percent: field(|process| &process.cpu_percent),
                resident_memory_bytes: field(|process| &process.resident_memory_bytes),
                read_bytes_per_second: field(|process| &process.read_bytes_per_second),
                write_bytes_per_second: field(|process| &process.write_bytes_per_second),
            }
        })
        .collect();

    applications.sort_by(|left, right| {
        right
            .cpu_percent
            .value
            .unwrap_or(-1.0)
            .partial_cmp(&left.cpu_percent.value.unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.key.cmp(&right.key))
    });

    applications
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::Availability;
    use crate::processes::state::ProcessState;

    fn process(pid: u32, name: &str, path: Option<&str>, cpu: Option<f64>) -> ProcessEntry {
        let (key, _) = application_key(path, name);

        ProcessEntry {
            instance_id: format!("process:{pid}-1"),
            pid,
            parent_pid: Some(1),
            name: name.to_string(),
            executable_path: match path {
                Some(path) => Field::available(path.to_string()),
                None => Field::missing(Availability::permission_denied("unreadable")),
            },
            state: ProcessState::SleepingOrWaiting,
            state_availability: Availability::Available,
            classification: ProcessClass::UserApplication,
            cpu_percent: match cpu {
                Some(cpu) => Field::available(cpu),
                None => Field::waiting_for_another_sample(),
            },
            resident_memory_bytes: Field::available(100.0),
            memory_percent: Field::available(1.0),
            thread_count: Field::available(4.0),
            read_bytes_per_second: Field::available(10.0),
            write_bytes_per_second: Field::available(20.0),
            application_key: key,
        }
    }

    #[test]
    fn three_firefox_processes_become_one_application() {
        let path = "/usr/lib64/firefox/firefox";
        let applications = aggregate(&[
            process(10, "firefox", Some(path), Some(1.0)),
            process(11, "Web Content", Some(path), Some(2.0)),
            process(12, "WebExtensions", Some(path), Some(0.5)),
        ]);

        assert_eq!(applications.len(), 1);
        let firefox = &applications[0];
        assert_eq!(firefox.display_name, "firefox");
        assert_eq!(firefox.process_count, 3);
        assert_eq!(firefox.identity, ApplicationIdentity::Executable);
    }

    #[test]
    fn same_name_different_executables_stay_two_applications() {
        // The scenario name-only grouping gets wrong.
        let applications = aggregate(&[
            process(10, "python3", Some("/usr/bin/python3"), Some(1.0)),
            process(11, "python3", Some("/opt/tool/venv/bin/python3"), Some(2.0)),
        ]);

        assert_eq!(applications.len(), 2);
        assert!(applications
            .iter()
            .all(|app| app.identity == ApplicationIdentity::Executable));
        assert!(applications.iter().all(|app| app.process_count == 1));
    }

    #[test]
    fn processes_without_a_path_fall_back_to_the_name_with_lower_confidence() {
        let applications = aggregate(&[
            process(10, "systemd-udevd", None, Some(0.1)),
            process(11, "systemd-udevd", None, Some(0.2)),
        ]);

        assert_eq!(applications.len(), 1);
        assert_eq!(applications[0].identity, ApplicationIdentity::Name);
        assert_eq!(applications[0].process_count, 2);
        assert_eq!(applications[0].key, "name:systemd-udevd");
    }

    #[test]
    fn a_path_and_a_missing_path_do_not_merge() {
        // Honest: PULSE cannot prove the unreadable one is the same program.
        let applications = aggregate(&[
            process(10, "sshd", Some("/usr/sbin/sshd"), Some(0.1)),
            process(11, "sshd", None, Some(0.2)),
        ]);

        assert_eq!(applications.len(), 2);
    }

    #[test]
    fn sums_cpu_memory_threads_and_io() {
        let path = "/usr/lib64/firefox/firefox";
        let applications = aggregate(&[
            process(10, "firefox", Some(path), Some(1.25)),
            process(11, "firefox", Some(path), Some(2.5)),
            process(12, "firefox", Some(path), Some(0.25)),
        ]);

        let firefox = &applications[0];
        assert_eq!(firefox.cpu_percent.value, Some(4.0));
        assert_eq!(firefox.resident_memory_bytes.value, Some(300.0));
        assert_eq!(firefox.thread_count.value, Some(12.0));
        assert_eq!(firefox.read_bytes_per_second.value, Some(30.0));
        assert_eq!(firefox.write_bytes_per_second.value, Some(60.0));
    }

    #[test]
    fn a_sum_counts_what_was_measured_and_says_so_when_nothing_was() {
        let path = "/opt/app/app";
        let applications = aggregate(&[
            process(10, "app", Some(path), Some(3.0)),
            process(11, "app", Some(path), None),
        ]);
        assert_eq!(
            applications[0].cpu_percent.value,
            Some(3.0),
            "an understatement is still a measurement; a fabricated total is not"
        );

        let waiting = aggregate(&[
            process(10, "app", Some(path), None),
            process(11, "app", Some(path), None),
        ]);
        assert_eq!(waiting[0].cpu_percent.value, None);
        assert_eq!(
            waiting[0].cpu_percent.availability.status_str(),
            "temporarilyUnavailable",
            "an application with no baseline yet must not read as idle"
        );
    }

    #[test]
    fn a_refusal_is_carried_up_rather_than_shown_as_zero() {
        let mut denied = process(10, "guarded", Some("/opt/guarded"), Some(0.0));
        denied.read_bytes_per_second =
            Field::missing(Availability::permission_denied("/proc/10/io refused"));

        let applications = aggregate(&[denied]);
        assert_eq!(applications[0].read_bytes_per_second.value, None);
        assert_eq!(
            applications[0]
                .read_bytes_per_second
                .availability
                .status_str(),
            "permissionDenied"
        );
    }

    #[test]
    fn ordering_is_cpu_descending_then_name_then_key() {
        let applications = aggregate(&[
            process(10, "zeta", Some("/bin/zeta"), Some(1.0)),
            process(11, "alpha", Some("/bin/alpha"), Some(1.0)),
            process(12, "busy", Some("/bin/busy"), Some(9.0)),
            process(13, "idle", Some("/bin/idle"), None),
        ]);

        let names: Vec<&str> = applications
            .iter()
            .map(|app| app.display_name.as_str())
            .collect();
        assert_eq!(names, ["busy", "alpha", "zeta", "idle"]);
    }

    #[test]
    fn ordering_is_stable_across_identical_snapshots() {
        let build = || {
            aggregate(&[
                process(10, "b", Some("/bin/b"), Some(1.0)),
                process(11, "a", Some("/bin/a"), Some(1.0)),
                process(12, "c", Some("/bin/c"), Some(1.0)),
            ])
        };

        assert_eq!(build(), build(), "the same input must not reshuffle");
    }

    #[test]
    fn display_names_drop_the_path_and_the_exe_suffix() {
        assert_eq!(
            display_name_for(Some("/usr/lib64/firefox/firefox"), "firefox"),
            "firefox"
        );
        assert_eq!(
            display_name_for(
                Some(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
                "firefox.exe"
            ),
            "firefox"
        );
        assert_eq!(display_name_for(Some("/usr/bin/node"), "node"), "node");
        assert_eq!(display_name_for(None, "kworker/3:1H"), "kworker/3:1H");
        assert_eq!(display_name_for(Some(""), "sshd"), "sshd");
        // A name that is only a suffix keeps something to show.
        assert_eq!(display_name_for(Some(r"C:\x\.exe"), ".exe"), ".exe");
    }

    #[test]
    fn a_binary_named_after_its_version_falls_back_to_the_process_name() {
        // Real shape, from a program that installs each release in its own
        // directory: an application called `2.1.278` names nothing.
        assert_eq!(
            display_name_for(
                Some("/home/someone/.local/share/thing/versions/2.1.278"),
                "thing"
            ),
            "thing"
        );
        assert_eq!(display_name_for(Some("/opt/app/1.0"), "app"), "app");

        // The rule is narrow on purpose: anything with a letter is a name.
        assert!(!looks_like_a_version("v2.1.278"));
        assert!(!looks_like_a_version("python3"));
        assert!(!looks_like_a_version("..."));
        assert!(!looks_like_a_version(""));
        assert!(looks_like_a_version("2.1.278"));
        assert!(looks_like_a_version("7"));
        assert_eq!(display_name_for(Some("/usr/bin/7z"), "7z"), "7z");
    }

    #[test]
    fn windows_paths_group_case_insensitively_and_posix_paths_do_not() {
        let (upper, _) = application_key(Some(r"C:\Program Files\App\APP.EXE"), "APP.EXE");
        let (lower, _) = application_key(Some(r"c:\program files\app\app.exe"), "app.exe");
        assert_eq!(upper, lower);

        let (one, _) = application_key(Some("/opt/App/App"), "App");
        let (two, _) = application_key(Some("/opt/app/app"), "app");
        assert_ne!(one, two, "POSIX paths are case-sensitive; so is the key");
    }

    #[test]
    fn classification_prefers_user_application_and_falls_back_to_unknown() {
        let path = "/opt/mixed/mixed";
        let mut system = process(10, "mixed", Some(path), Some(1.0));
        system.classification = ProcessClass::SystemProcess;
        let user = process(11, "mixed", Some(path), Some(1.0));

        assert_eq!(
            aggregate(&[system.clone(), user])[0].classification,
            ProcessClass::UserApplication
        );
        assert_eq!(
            aggregate(&[system.clone()])[0].classification,
            ProcessClass::SystemProcess
        );

        let mut unknown = process(12, "mixed", Some(path), Some(1.0));
        unknown.classification = ProcessClass::Unknown;
        assert_eq!(
            aggregate(&[system, unknown])[0].classification,
            ProcessClass::Unknown
        );
    }

    #[test]
    fn no_processes_yield_no_applications() {
        assert!(aggregate(&[]).is_empty());
    }

    #[test]
    fn a_single_process_is_still_an_application() {
        let applications = aggregate(&[process(10, "alone", Some("/bin/alone"), Some(0.0))]);
        assert_eq!(applications.len(), 1);
        assert_eq!(applications[0].process_count, 1);
        assert_eq!(applications[0].cpu_percent.value, Some(0.0));
    }
}
