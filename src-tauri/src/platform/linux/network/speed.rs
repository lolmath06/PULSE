//! Ethernet link speed, from `/sys/class/net/<iface>/speed`.
//!
//! # Why sysfs and not ethtool netlink
//!
//! The modern, structured answer is `ETHTOOL_MSG_LINKMODES_GET` over its own
//! generic netlink family. It carries far more than PULSE needs — every
//! supported and advertised link mode, autonegotiation state, lane counts —
//! and it would mean resolving a third netlink family and writing a third
//! attribute mapping, for **one number**.
//!
//! The kernel exposes that same number, already in megabits per second, as a
//! one-line file. Reading it costs one `open`/`read`/`close` per *Ethernet*
//! interface per refresh — on the development machine, one — against a round
//! trip and a parser for the alternative. The trade is worth stating because
//! Phase 6 made the opposite call for disk counters: there, one file per
//! counter per device meant a hundred opens and a hundred different instants,
//! and here it is one file per interface for a value that is not a rate and
//! does not need to be captured at the same instant as anything else.
//!
//! If a later phase needs advertised link modes or autonegotiation state, the
//! ethtool family becomes worth its cost and this module is where it lands.
//!
//! # `-1` and `EINVAL` are not speeds
//!
//! The file reports `-1` when the driver has no answer — an unplugged cable,
//! typically — and reading it fails outright with `EINVAL` on an interface
//! that has no fixed link rate at all, which is what a Wi-Fi adapter does.
//! **Neither is a link running at zero bits per second.** Publishing `0` would
//! claim a link with no capacity; PULSE publishes nothing and says why.
//!
//! A `veth` pair, meanwhile, reports a perfectly real `10000` — 10 Gbit/s,
//! being a software device with no physical limit. That is what the kernel
//! says, so it is what PULSE reports.

use std::path::{Path, PathBuf};

/// Where the kernel exposes each interface's attributes.
pub const NET_ROOT: &str = "/sys/class/net";

/// Megabits per second, the unit the `speed` file uses.
const MEGABIT: u64 = 1_000_000;

/// Converts the file's contents into bits per second.
///
/// Returns `None` for the values that mean "no answer":
///
/// - **`-1`** — the driver has no speed to report, typically because there is
///   no carrier;
/// - **`0`** — an unnegotiated link, which is not a link running at zero;
/// - anything unparseable.
///
/// An implausibly large figure is also refused: above 10 Tbit/s the file has
/// been misread, since no interface the kernel describes this way runs that
/// fast.
pub fn parse_speed(contents: &str) -> Option<u64> {
    const MAX_MEGABITS: i64 = 10_000_000;

    let megabits: i64 = contents.trim().parse().ok()?;

    (megabits > 0 && megabits <= MAX_MEGABITS).then(|| megabits as u64 * MEGABIT)
}

/// The path of one interface's `speed` file.
pub fn speed_path(root: &Path, name: &str) -> PathBuf {
    root.join(name).join("speed")
}

/// Reads one interface's link speed, in bits per second.
///
/// Every failure — the file missing, the read returning `EINVAL`, the contents
/// being `-1` — collapses to `None`, because they all mean the same thing to
/// the caller: this interface reports no link speed.
pub fn read_in(root: &Path, name: &str) -> Option<u64> {
    let contents = std::fs::read_to_string(speed_path(root, name)).ok()?;

    parse_speed(&contents)
}

/// Reads one interface's link speed on this machine.
pub fn read(name: &str) -> Option<u64> {
    read_in(Path::new(NET_ROOT), name)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A throwaway `/sys/class/net` tree.
    ///
    /// Nothing in the test suite writes to the real `/sys`.
    pub struct NetFixture {
        root: PathBuf,
    }

    impl NetFixture {
        pub fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "pulse-net-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("create fixture root");

            Self { root }
        }

        pub fn root(&self) -> &Path {
            &self.root
        }

        /// Creates an interface with a `speed` file.
        pub fn interface(&self, name: &str, speed: &str) -> &Self {
            let path = self.root.join(name);
            fs::create_dir_all(&path).expect("create interface");
            fs::write(path.join("speed"), format!("{speed}\n")).expect("write speed");
            self
        }

        /// Creates an interface with no `speed` file at all.
        pub fn interface_without_speed(&self, name: &str) -> &Self {
            fs::create_dir_all(self.root.join(name)).expect("create interface");
            self
        }
    }

    impl Drop for NetFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::NetFixture;
    use super::*;

    #[test]
    fn converts_megabits_to_bits_per_second() {
        assert_eq!(parse_speed("1000"), Some(1_000_000_000));
        assert_eq!(parse_speed("2500"), Some(2_500_000_000));
        assert_eq!(parse_speed("10000"), Some(10_000_000_000));
        assert_eq!(parse_speed("100"), Some(100_000_000));
        assert_eq!(parse_speed("10"), Some(10_000_000));
    }

    #[test]
    fn tolerates_the_trailing_newline_the_kernel_writes() {
        assert_eq!(parse_speed("1000\n"), Some(1_000_000_000));
        assert_eq!(parse_speed("  1000  \n"), Some(1_000_000_000));
    }

    #[test]
    fn minus_one_is_not_a_speed() {
        // What an unplugged Ethernet port reports. Publishing `0` from it
        // would claim a link with no capacity.
        assert_eq!(parse_speed("-1"), None);
        assert_eq!(parse_speed("-1\n"), None);
    }

    #[test]
    fn zero_is_not_a_speed_either() {
        // An unnegotiated link is not a link running at zero bits per second.
        assert_eq!(parse_speed("0"), None);
    }

    #[test]
    fn refuses_an_unparseable_or_implausible_value() {
        assert_eq!(parse_speed(""), None);
        assert_eq!(parse_speed("unknown"), None);
        assert_eq!(parse_speed("1000 Mbit"), None);
        // Above 10 Tbit/s the file has been misread.
        assert_eq!(parse_speed("99999999"), None);
    }

    #[test]
    fn reads_a_plausible_ethernet_speed_from_a_fixture_tree() {
        let fixture = NetFixture::new("ethernet");
        fixture.interface("enp58s0", "1000");

        assert_eq!(read_in(fixture.root(), "enp58s0"), Some(1_000_000_000));
    }

    #[test]
    fn an_unplugged_port_reports_no_speed() {
        // The real value on the development machine's unplugged Ethernet port.
        let fixture = NetFixture::new("unplugged");
        fixture.interface("enp58s0", "-1");

        assert_eq!(read_in(fixture.root(), "enp58s0"), None);
    }

    #[test]
    fn a_wifi_adapter_reports_no_speed_here() {
        // Reading the file fails with `EINVAL` on a real Wi-Fi interface,
        // because it has no fixed link rate. Its negotiated rate comes from
        // `nl80211` instead, under `network.wifi.link.*`.
        let fixture = NetFixture::new("wifi");
        fixture.interface_without_speed("wlp59s0f0");

        assert_eq!(read_in(fixture.root(), "wlp59s0f0"), None);
    }

    #[test]
    fn a_veth_pair_reports_the_speed_the_kernel_gives_it() {
        // 10 Gbit/s, being a software device with no physical limit. That is
        // what the kernel says, so it is what PULSE reports.
        let fixture = NetFixture::new("veth");
        fixture.interface("vethe0ffe82", "10000");

        assert_eq!(read_in(fixture.root(), "vethe0ffe82"), Some(10_000_000_000));
    }

    #[test]
    fn a_missing_interface_reports_no_speed_rather_than_failing() {
        let fixture = NetFixture::new("missing");

        assert_eq!(read_in(fixture.root(), "nonexistent"), None);
        assert_eq!(read_in(Path::new("/nonexistent/pulse/net"), "eth0"), None);
    }

    #[test]
    fn the_path_is_built_under_the_interfaces_own_directory() {
        assert_eq!(
            speed_path(Path::new("/sys/class/net"), "enp58s0"),
            Path::new("/sys/class/net/enp58s0/speed")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reading_a_real_interface_never_panics() {
        // Whatever this machine has, and whatever state it is in.
        for name in ["lo", "enp58s0", "wlp59s0f0", "docker0", "nonexistent"] {
            let speed = read(name);
            if let Some(bps) = speed {
                assert!(bps > 0, "{name} reported {bps}");
            }
        }
    }
}
