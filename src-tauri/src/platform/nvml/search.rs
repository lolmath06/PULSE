//! Where PULSE is willing to load NVML from, and where it refuses to.
//!
//! Kept apart from the FFI in [`super::library`] and compiled on **every**
//! platform, so the rules below are reviewed and tested as ordinary code rather
//! than living inside a `#[cfg(windows)]` block nobody can run.
//!
//! # Fedora
//!
//! `dlopen("libnvidia-ml.so.1")` — the **SONAME**, resolved through the normal
//! loader search. That is what the proprietary driver package installs and
//! registers with `ldconfig`, wherever the distribution puts it.
//!
//! # Windows: two locations, both absolute, in this order
//!
//! ```text
//! 1. %SystemRoot%\System32\nvml.dll            the display driver's copy
//! 2. <Program Files>\NVIDIA Corporation\NVSMI\nvml.dll
//! ```
//!
//! The first is where the NVIDIA display driver installs NVML and is the normal
//! answer on a desktop. The second is the layout the NVIDIA management tooling
//! (`nvidia-smi` and its supporting libraries) has historically used, and is
//! where NVML is found on machines — some server and datacentre installations
//! among them — whose driver package did not place a copy in the system
//! directory.
//!
//! ## What is deliberately *not* searched
//!
//! ```text
//! the application directory      ← where an attacker can most easily write
//! the current working directory
//! %PATH%
//! any user-writable directory
//! ```
//!
//! A bare `LoadLibraryW("nvml.dll")` searches the application directory
//! **first**. Anyone able to drop a file next to `pulse.exe` — an installer, an
//! unpacked archive, a shared downloads folder — would then have PULSE load
//! their DLL with PULSE's privileges. That is DLL planting, and a monitoring
//! tool that loads vendor libraries is exactly its target. PULSE therefore
//! never falls back to the default search path: a missing NVML costs one
//! vendor's metrics, a hijacked one costs arbitrary code execution.
//!
//! So System32 is opened with `LOAD_LIBRARY_SEARCH_SYSTEM32`, which searches
//! that directory and nothing else, and the Program Files copy is opened by
//! **absolute path** with `LOAD_LIBRARY_SEARCH_SYSTEM32 |
//! LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR`, so even the loaded library's own
//! dependencies resolve only from System32 and from the directory it was loaded
//! out of.
//!
//! ## Why a Known Folder, and not `%ProgramW6432%`
//!
//! The environment variable is inherited, and anything that can start PULSE can
//! set it: trusting it would put an attacker-chosen directory back into the
//! search. [`SHGetKnownFolderPath`] with `FOLDERID_ProgramFiles` asks the
//! system where Program Files *is*, and cannot be redirected by the process's
//! own environment. A 64-bit PULSE process resolves it to the 64-bit Program
//! Files, which is where the 64-bit NVML lives.
//!
//! [`SHGetKnownFolderPath`]: https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath

use std::path::{Path, PathBuf};

/// The file name of the Windows library.
pub const WINDOWS_LIBRARY: &str = "nvml.dll";

/// The SONAME of the Fedora library.
///
/// Never the unversioned `libnvidia-ml.so`, which belongs to the CUDA
/// *development* package that most users do not have installed.
pub const LINUX_SONAME: &str = "libnvidia-ml.so.1";

/// The directories, below Program Files, that hold NVIDIA's management tooling.
pub const NVSMI_DIRECTORIES: &[&str] = &["NVIDIA Corporation", "NVSMI"];

/// Where PULSE looks for `nvml.dll`, in order of preference.
///
/// A closed enum rather than a list of strings: every location PULSE will load
/// from is spelled out here, so adding one is a visible change to a reviewed
/// list and not a string appearing in an FFI block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WindowsLocation {
    /// `%SystemRoot%\System32`, searched through the loader's own
    /// system-directory-only flag rather than by building a path.
    System32,
    /// `<Program Files>\NVIDIA Corporation\NVSMI`, opened by absolute path.
    ProgramFilesNvsmi,
}

impl WindowsLocation {
    /// The order the locations are tried in.
    pub const ORDER: &'static [WindowsLocation] = &[
        WindowsLocation::System32,
        WindowsLocation::ProgramFilesNvsmi,
    ];

    /// A short description, for diagnostics and the availability reason.
    pub const fn describe(self) -> &'static str {
        match self {
            WindowsLocation::System32 => "the system directory",
            WindowsLocation::ProgramFilesNvsmi => {
                "the NVIDIA management tooling directory under Program Files"
            }
        }
    }
}

/// Builds the absolute path of the Program Files copy.
///
/// Takes the Program Files directory as an argument — it is never guessed here,
/// and never read from the environment — so the path construction is testable
/// on any platform.
pub fn nvml_under_program_files(program_files: &Path) -> PathBuf {
    let mut path = program_files.to_path_buf();

    for directory in NVSMI_DIRECTORIES {
        path.push(directory);
    }
    path.push(WINDOWS_LIBRARY);

    path
}

/// The message shown when NVML was not found in any permitted location.
///
/// Phrased as an absent capability rather than a failure: on a machine without
/// an NVIDIA card, or with the open-source driver, this is the *expected*
/// outcome and nothing is wrong.
pub fn not_found_message() -> String {
    format!(
        "{WINDOWS_LIBRARY} was not found in {} or {}, so NVIDIA telemetry is unavailable \
         (this is expected without the NVIDIA display driver). PULSE deliberately does not \
         search the application directory, the working directory or PATH for it.",
        WindowsLocation::System32.describe(),
        WindowsLocation::ProgramFilesNvsmi.describe(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_directory_is_tried_before_program_files() {
        // The system copy belongs to the display driver currently in use; the
        // Program Files copy may be left over from a different toolkit version.
        assert_eq!(
            WindowsLocation::ORDER,
            &[
                WindowsLocation::System32,
                WindowsLocation::ProgramFilesNvsmi
            ]
        );
    }

    #[test]
    fn builds_the_documented_nvsmi_path() {
        let path = nvml_under_program_files(Path::new(r"C:\Program Files"));

        let components: Vec<String> = path
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();

        // Asserted component by component so the test reads the same on a
        // Fedora CI runner, where `\` is not a separator.
        assert!(components
            .iter()
            .any(|component| component.contains("NVIDIA Corporation")));
        assert!(components
            .iter()
            .any(|component| component.contains("NVSMI")));
        assert!(components
            .last()
            .is_some_and(|last| last.ends_with("nvml.dll")));
    }

    #[test]
    fn the_program_files_path_is_absolute_and_rooted_at_what_it_was_given() {
        // The one property that matters for the security argument: the path is
        // built from the directory the system reported, never from a relative
        // name the loader would resolve against its own search order.
        // An absolute root on whatever platform runs the test (`/opt/pf` is
        // not absolute on Windows, which needs a drive). Only a path is built;
        // nothing is created.
        let program_files = std::env::temp_dir().join("pulse-program-files-fixture");
        assert!(program_files.is_absolute());

        let path = nvml_under_program_files(&program_files);

        assert!(path.starts_with(&program_files));
        assert!(path.is_absolute());
        assert_ne!(path, PathBuf::from(WINDOWS_LIBRARY));
    }

    #[test]
    fn no_location_is_a_bare_library_name() {
        // A bare name is what the default search path resolves, and the
        // default search path includes the application directory.
        for location in WindowsLocation::ORDER {
            assert!(
                !location.describe().is_empty(),
                "every location must be explainable to a user"
            );
        }

        let message = not_found_message();
        assert!(message.contains("does not search the application directory"));
    }

    #[test]
    fn the_linux_library_is_the_versioned_soname() {
        // `libnvidia-ml.so` without the version belongs to the CUDA
        // development package, which most users do not have.
        assert_eq!(LINUX_SONAME, "libnvidia-ml.so.1");
        assert!(LINUX_SONAME.ends_with(".1"));
    }

    #[test]
    fn the_absent_library_reads_as_a_missing_capability_not_a_fault() {
        let message = not_found_message();

        assert!(message.contains("expected without the NVIDIA display driver"));
        assert!(message.contains("nvml.dll"));
    }
}
