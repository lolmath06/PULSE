//! Telling a Windows system process from a user's application.
//!
//! Pure, and compiled everywhere, so the rules are testable without Windows.
//!
//! # Only signals that are actually reliable
//!
//! Windows has no per-process "is this the user's program" flag. What it does
//! have are three facts PULSE will trust:
//!
//! 1. **PID 0 and PID 4.** The Idle Process and the System process. Fixed by
//!    the operating system, on every Windows version.
//! 2. **The executable lives under `%SystemRoot%`.** `C:\Windows\System32\…`
//!    is where the operating system keeps itself. A user's program can be
//!    installed there, but essentially never is.
//! 3. **No path at all.** A protected process refused the handle, so PULSE
//!    knows nothing and says [`ProcessClass::Unknown`] rather than guessing.
//!
//! Notably absent: guessing from the name. `svchost`, `explorer` and `csrss`
//! are recognisable to someone who knows Windows, and a list of them would be
//! wrong on the next release and trivially spoofable by any program that
//! renames itself. PULSE classifies from where a file *is*, not what it is
//! called.

use crate::processes::ProcessClass;

/// The Idle Process — not a real process at all, but it appears in the table.
pub const IDLE_PROCESS_PID: u32 = 0;
/// The System process, which hosts the kernel's own threads.
pub const SYSTEM_PROCESS_PID: u32 = 4;

/// Whether a path lies inside the Windows directory.
///
/// Case-insensitive, because NTFS paths are, and boundary-aware: a program
/// installed at `C:\WindowsApps\thing.exe` is **not** inside `C:\Windows`, and
/// a prefix test alone would say it was.
pub fn is_system_path(path: &str, system_root: &str) -> bool {
    let root = system_root.trim_end_matches(['\\', '/']);
    if root.is_empty() {
        return false;
    }

    let path = path.to_ascii_lowercase().replace('/', "\\");
    let root = root.to_ascii_lowercase().replace('/', "\\");

    path.strip_prefix(&root)
        .is_some_and(|rest| rest.starts_with('\\'))
}

/// Classifies one Windows process.
pub fn classify(
    pid: u32,
    executable_path: Option<&str>,
    system_root: Option<&str>,
) -> ProcessClass {
    if pid == IDLE_PROCESS_PID || pid == SYSTEM_PROCESS_PID {
        return ProcessClass::SystemProcess;
    }

    let Some(path) = executable_path else {
        // The handle was refused, or the image name could not be read.
        return ProcessClass::Unknown;
    };

    match system_root {
        Some(root) if is_system_path(path, root) => ProcessClass::SystemProcess,
        Some(_) => ProcessClass::UserApplication,
        // Without a system root there is no boundary to test against, and
        // inventing one from a hardcoded `C:\Windows` would be wrong on a
        // machine that boots from another drive.
        None => ProcessClass::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = r"C:\Windows";

    #[test]
    fn the_idle_and_system_processes_are_always_system_processes() {
        // Even though neither of them has a readable executable path.
        assert_eq!(classify(0, None, None), ProcessClass::SystemProcess);
        assert_eq!(classify(4, None, Some(ROOT)), ProcessClass::SystemProcess);
    }

    #[test]
    fn an_executable_under_the_windows_directory_is_a_system_process() {
        for path in [
            r"C:\Windows\System32\svchost.exe",
            r"C:\Windows\explorer.exe",
            r"C:\Windows\System32\drivers\something.exe",
        ] {
            assert_eq!(
                classify(1234, Some(path), Some(ROOT)),
                ProcessClass::SystemProcess,
                "for {path}"
            );
        }
    }

    #[test]
    fn an_executable_anywhere_else_is_a_user_application() {
        for path in [
            r"C:\Program Files\Mozilla Firefox\firefox.exe",
            r"C:\Users\someone\AppData\Local\app\app.exe",
            r"D:\Games\game.exe",
        ] {
            assert_eq!(
                classify(1234, Some(path), Some(ROOT)),
                ProcessClass::UserApplication,
                "for {path}"
            );
        }
    }

    #[test]
    fn a_neighbouring_directory_is_not_inside_the_windows_one() {
        // The prefix test that a naive `starts_with` would get wrong.
        assert!(!is_system_path(r"C:\WindowsApps\store.exe", ROOT));
        assert!(!is_system_path(r"C:\Windows.old\System32\x.exe", ROOT));
        assert_eq!(
            classify(1234, Some(r"C:\WindowsApps\store.exe"), Some(ROOT)),
            ProcessClass::UserApplication
        );
    }

    #[test]
    fn the_comparison_ignores_case_and_slash_direction() {
        assert!(is_system_path(r"c:\windows\system32\lsass.exe", ROOT));
        assert!(is_system_path(r"C:\WINDOWS\System32\LSASS.EXE", ROOT));
        assert!(is_system_path("C:/Windows/System32/lsass.exe", ROOT));
        assert!(is_system_path(r"C:\Windows\System32\x.exe", r"c:\windows\"));
    }

    #[test]
    fn a_process_whose_path_was_refused_stays_unknown() {
        // A protected process is still a row; it is just not claimed to be
        // one thing or the other.
        assert_eq!(classify(1234, None, Some(ROOT)), ProcessClass::Unknown);
    }

    #[test]
    fn no_system_root_means_no_classification_rather_than_a_hardcoded_guess() {
        assert_eq!(
            classify(1234, Some(r"C:\Windows\System32\svchost.exe"), None),
            ProcessClass::Unknown
        );
        assert!(!is_system_path(r"C:\Windows\x.exe", ""));
    }

    #[test]
    fn a_boot_drive_other_than_c_is_handled() {
        assert!(is_system_path(r"E:\Windows\System32\x.exe", r"E:\Windows"));
        assert!(!is_system_path(r"C:\Windows\System32\x.exe", r"E:\Windows"));
    }
}
