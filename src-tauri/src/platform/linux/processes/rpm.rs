//! Fedora package provenance: which RPM, if any, owns an executable.
//!
//! # Why a subprocess here, and only here
//!
//! PULSE otherwise never runs a program to learn something (see the collector
//! docs). The RPM database is the exception: its on-disk format is a private
//! SQLite schema managed by `librpm`, and linking `librpm` would tie PULSE's
//! build to the exact RPM version of the build host. One targeted query to the
//! system's own `rpm` binary is the proportionate answer, under strict rules:
//!
//! * only when the inspector is opened on one process — never on Refresh,
//!   never for the whole table, never as a metric provider;
//! * `Command` with separate arguments — **no shell**, no string that a
//!   shell could reinterpret;
//! * the path is passed after `--`, so a file literally named `-e` is still a
//!   path;
//! * an empty environment plus `LC_ALL=C`, so the output does not depend on
//!   the user's locale;
//! * a bounded timeout, after which the child is killed;
//! * no `rpm` binary means provenance is *unavailable*, not an error.
//!
//! # Owned by a package is not the same as safe
//!
//! It means the file sits where an installed package put it. It does not mean
//! the file is unmodified (`rpm -V` would be a separate, slower question) and
//! it is not a security verdict. See `docs/processes/provenance.md`.

use crate::processes::inspector::{LocationHint, PackageRef};

/// Where Fedora installs `rpm`. PATH is not consulted: provenance must not
/// depend on whatever a user's shell put first.
pub const RPM_CANDIDATES: [&str; 2] = ["/usr/bin/rpm", "/bin/rpm"];

/// How long one query may take before it is abandoned.
pub const RPM_TIMEOUT_MS: u64 = 5_000;

/// `rpm`'s own query format: one tab-separated line per owning package.
///
/// The backslash escapes are expanded by `rpm`, not by a shell — there is no
/// shell.
pub const QUERY_FORMAT: &str = "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{ARCH}\\n";

/// The arguments for one ownership query, in order.
pub fn arguments(path: &str) -> [&str; 5] {
    ["-qf", "--qf", QUERY_FORMAT, "--", path]
}

/// Parses the output of [`arguments`]'s query.
///
/// Lines that are not exactly three tab-separated, non-empty fields — such as
/// `file /x is not owned by any package` — are ignored.
pub fn parse_packages(stdout: &str) -> Vec<PackageRef> {
    let mut packages: Vec<PackageRef> = Vec::new();

    for line in stdout.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if let [name, version, arch] = fields[..] {
            if name.is_empty() || version.is_empty() || arch.is_empty() {
                continue;
            }
            let package = PackageRef {
                name: name.to_string(),
                version: version.to_string(),
                arch: arch.to_string(),
            };
            if !packages.contains(&package) {
                packages.push(package);
            }
        }
    }

    packages
}

/// A hint about where an unpackaged executable lives.
pub fn location_hint(path: &str, home: Option<&str>) -> LocationHint {
    let under = |root: &str| {
        path.strip_prefix(root.trim_end_matches('/'))
            .is_some_and(|rest| rest.starts_with('/'))
    };

    if home.is_some_and(|home| home.len() > 1 && under(home)) {
        LocationHint::UserHome
    } else if under("/usr/local") || under("/opt") {
        LocationHint::LocalInstall
    } else {
        LocationHint::Other
    }
}

#[cfg(target_os = "linux")]
pub use sys::query;

#[cfg(target_os = "linux")]
mod sys {
    use std::io::Read;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use crate::processes::inspector::Provenance;

    use super::{arguments, location_hint, parse_packages, RPM_CANDIDATES, RPM_TIMEOUT_MS};

    /// Asks `rpm` which package owns `path`.
    pub fn query(path: &str) -> Provenance {
        let Some(rpm) = RPM_CANDIDATES
            .iter()
            .find(|candidate| Path::new(candidate).is_file())
        else {
            return Provenance::Unavailable {
                reason: "The rpm tool is not installed, so package ownership cannot be checked."
                    .to_string(),
            };
        };

        let mut child = match Command::new(rpm)
            .args(arguments(path))
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return Provenance::Unavailable {
                    reason: format!("rpm could not be started: {error}"),
                }
            }
        };

        // Drain stdout on its own thread so a large answer can never fill
        // the pipe and deadlock the wait below.
        let mut stdout = child.stdout.take();
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(stdout) = stdout.as_mut() {
                let _ = stdout.take(64 * 1024).read_to_string(&mut text);
            }
            text
        });

        let deadline = Instant::now() + Duration::from_millis(RPM_TIMEOUT_MS);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };

        let output = reader.join().unwrap_or_default();

        let Some(status) = status else {
            return Provenance::Unavailable {
                reason: "rpm did not answer in time.".to_string(),
            };
        };

        let packages = parse_packages(&output);
        if !packages.is_empty() {
            return Provenance::RpmPackage { packages };
        }

        // `rpm -qf` exits 1 for "not owned by any package".
        if status.code() == Some(1) {
            let home = std::env::var("HOME").ok();
            return Provenance::NotPackaged {
                location: location_hint(path, home.as_deref()),
            };
        }

        Provenance::Unavailable {
            reason: "rpm could not answer for this file.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_an_argument_after_the_end_of_options() {
        let arguments = arguments("-e; rm -rf ~");
        assert_eq!(arguments[3], "--");
        assert_eq!(
            arguments[4], "-e; rm -rf ~",
            "passed verbatim, never parsed"
        );
    }

    #[test]
    fn parses_one_owning_package() {
        assert_eq!(
            parse_packages("bash\t5.2.26-1.fc39\tx86_64\n"),
            vec![PackageRef {
                name: "bash".into(),
                version: "5.2.26-1.fc39".into(),
                arch: "x86_64".into(),
            }]
        );
    }

    #[test]
    fn parses_several_owners_and_deduplicates() {
        let packages = parse_packages(
            "glibc\t2.38-18.fc39\tx86_64\nglibc\t2.38-18.fc39\ti686\nglibc\t2.38-18.fc39\tx86_64\n",
        );
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[1].arch, "i686");
    }

    #[test]
    fn ignores_the_not_owned_message_and_noise() {
        assert!(parse_packages("file /home/a/tool is not owned by any package\n").is_empty());
        assert!(parse_packages("").is_empty());
        assert!(parse_packages("a\tb\n\t\t\nx\ty\tz\tw\n").is_empty());
    }

    #[test]
    fn hints_where_an_unpackaged_file_lives() {
        let home = Some("/home/alice");
        assert_eq!(
            location_hint("/home/alice/.local/bin/tool", home),
            LocationHint::UserHome
        );
        assert_eq!(
            location_hint("/home/alicebob/tool", home),
            LocationHint::Other,
            "a prefix is not a parent directory"
        );
        assert_eq!(
            location_hint("/usr/local/bin/tool", home),
            LocationHint::LocalInstall
        );
        assert_eq!(
            location_hint("/opt/app/app", home),
            LocationHint::LocalInstall
        );
        assert_eq!(location_hint("/usr/libexec/x", home), LocationHint::Other);
        assert_eq!(location_hint("/tool", Some("/")), LocationHint::Other);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_real_query_answers_without_failing() {
        use crate::processes::inspector::Provenance;

        // Whatever the host: a package, not-packaged, or unavailable (no rpm)
        // — never a panic, never a hang.
        match query("/proc/self/exe") {
            Provenance::RpmPackage { .. }
            | Provenance::NotPackaged { .. }
            | Provenance::Unavailable { .. } => {}
            other => panic!("unexpected provenance {other:?}"),
        }
    }
}
