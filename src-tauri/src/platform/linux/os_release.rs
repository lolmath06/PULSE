//! Reads the distribution name from `/etc/os-release`.
//!
//! Standardised by systemd and therefore present on Fedora and every other
//! mainstream distribution. Absence is not an error: PULSE simply reports no
//! OS version.

use std::fs;

const OS_RELEASE_PATHS: [&str; 2] = ["/etc/os-release", "/usr/lib/os-release"];

/// Returns `PRETTY_NAME` from os-release, e.g. `Fedora Linux 39 (Workstation Edition)`.
pub fn pretty_name() -> Option<String> {
    OS_RELEASE_PATHS
        .iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .as_deref()
        .and_then(parse_pretty_name)
}

/// Extracts `PRETTY_NAME` from os-release content, unquoting the value.
fn parse_pretty_name(content: &str) -> Option<String> {
    content
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "PRETTY_NAME")
        .map(|(_, value)| {
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_fedora_os_release() {
        let content = "NAME=\"Fedora Linux\"\nVERSION=\"39 (Workstation Edition)\"\n\
                       PRETTY_NAME=\"Fedora Linux 39 (Workstation Edition)\"\nID=fedora\n";

        assert_eq!(
            parse_pretty_name(content).as_deref(),
            Some("Fedora Linux 39 (Workstation Edition)")
        );
    }

    #[test]
    fn handles_unquoted_values() {
        assert_eq!(
            parse_pretty_name("ID=arch\nPRETTY_NAME=Arch Linux\n").as_deref(),
            Some("Arch Linux")
        );
    }

    #[test]
    fn returns_none_when_absent_or_empty() {
        assert_eq!(parse_pretty_name("ID=fedora\nNAME=Fedora\n"), None);
        assert_eq!(parse_pretty_name("PRETTY_NAME=\"\"\n"), None);
        assert_eq!(parse_pretty_name(""), None);
    }
}
