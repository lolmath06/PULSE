//! What PULSE knows about one network interface, and how it identifies it.
//!
//! # An interface name is not an identity
//!
//! Every obvious candidate is wrong, as it was for GPUs and disks:
//!
//! | Candidate | Why it must not be an identity |
//! |---|---|
//! | `eth0`, `wlan0` | Kernel enumeration order. A second adapter renames the first |
//! | `Ethernet`, `Wi-Fi` | Windows *display* names, and the user can rename them |
//! | `InterfaceIndex` | Documented as unstable; changes when adapters are added or removed |
//! | `ifindex` | The same, on Linux — and reused after an interface is destroyed |
//! | An IP address | Assigned by DHCP, changes per network, and several may exist at once |
//! | A current MAC | Randomised on Wi-Fi by default on both platforms — see below |
//!
//! `enp58s0` and `wlp59s0f0` are better than `eth0` — systemd derives them from
//! the PCI path, so they survive a reboot — but they still describe the *slot*
//! rather than the hardware, and they say nothing on Windows. They are a
//! documented fallback, not a first choice.
//!
//! # The MAC randomisation trap
//!
//! A Wi-Fi interface's **current** hardware address is frequently not the one
//! burned into the adapter. Both NetworkManager and Windows randomise it per
//! network by default, as an anti-tracking measure, and it changes when the
//! machine joins a different SSID. Building an identity on it would give a
//! user's Wi-Fi widget a new identity every time they moved between home and
//! the office.
//!
//! Both platforms expose the real one separately — `IFLA_PERM_ADDRESS` on
//! Linux, `PermanentPhysicalAddress` on Windows — and that is what PULSE
//! prefers. Where only the current address exists, PULSE uses it and
//! **records that it did**, so the weaker guarantee is inspectable rather than
//! assumed.
//!
//! # Why this yields one identifier across operating systems
//!
//! A permanent MAC is burned into the adapter, so Fedora and Windows read the
//! *same six bytes* off the same card. Normalised the same way, they produce
//! the same `SourceId`, and a dashboard widget bound to a laptop's Wi-Fi card
//! survives a dual boot. A contract test asserts exactly that.

use std::fmt;

use crate::metrics::model::{Availability, SourceId};

/// What kind of interface this is.
///
/// **Never inferred from the name.** `wlan0` is a convention, not a guarantee;
/// a bridge can be called `eth0`; and a Windows adapter named `Wi-Fi` may be a
/// virtual one. Both platforms report the real answer — a `phy80211` link and
/// the `IFLA_LINKINFO` kind on Linux, `Type` and `PhysicalMediumType` on
/// Windows — and PULSE uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetworkInterfaceKind {
    /// Wired Ethernet, including USB Ethernet adapters and Thunderbolt docks.
    Ethernet,
    /// An 802.11 station interface.
    Wifi,
    /// A software bridge — `docker0`, a Docker Compose network, a Hyper-V
    /// virtual switch.
    Bridge,
    /// A point-to-point tunnel: TUN/TAP, WireGuard, Tailscale, a VPN client's
    /// adapter.
    Tunnel,
    /// A virtual Ethernet pair — the container end of a `veth` link. Real, and
    /// usually numerous enough to drown out everything else.
    Virtual,
    /// The loopback interface. Discovered, and deliberately not published.
    Loopback,
    /// Something PULSE has no name for. Shown, not hidden.
    Other,
}

impl NetworkInterfaceKind {
    /// The short label shown next to an interface.
    pub const fn label(self) -> &'static str {
        match self {
            NetworkInterfaceKind::Ethernet => "Ethernet",
            NetworkInterfaceKind::Wifi => "Wi-Fi",
            NetworkInterfaceKind::Bridge => "Bridge",
            NetworkInterfaceKind::Tunnel => "Tunnel",
            NetworkInterfaceKind::Virtual => "Virtual",
            NetworkInterfaceKind::Loopback => "Loopback",
            NetworkInterfaceKind::Other => "Network",
        }
    }

    /// Whether PULSE publishes metrics for an interface of this kind.
    ///
    /// Loopback is the only exclusion, and it is deliberate: `lo` always
    /// exists, always reports traffic that never left the machine, and adding
    /// it to the dashboard would make "how much is this machine downloading"
    /// answerable only after mentally subtracting a number. It is still
    /// discovered, so the inventory is complete internally.
    pub const fn is_published(self) -> bool {
        !matches!(self, NetworkInterfaceKind::Loopback)
    }

    /// Whether this kind is hardware a user thinks of as "my network
    /// connection".
    ///
    /// Drives presentation only: the UI lists these first and groups the rest
    /// under a collapsed section, because a machine running containers can
    /// have thirty `veth` interfaces and two real ones. Nothing is hidden —
    /// see `docs/metrics/network.md`.
    pub const fn is_primary(self) -> bool {
        matches!(
            self,
            NetworkInterfaceKind::Ethernet | NetworkInterfaceKind::Wifi
        )
    }

    /// Whether an interface of this kind can carry Wi-Fi link metrics.
    pub const fn is_wireless(self) -> bool {
        matches!(self, NetworkInterfaceKind::Wifi)
    }

    pub const ALL: &'static [NetworkInterfaceKind] = &[
        NetworkInterfaceKind::Ethernet,
        NetworkInterfaceKind::Wifi,
        NetworkInterfaceKind::Bridge,
        NetworkInterfaceKind::Tunnel,
        NetworkInterfaceKind::Virtual,
        NetworkInterfaceKind::Loopback,
        NetworkInterfaceKind::Other,
    ];
}

/// Whether the interface is administratively up, and whether a link is present.
///
/// Two separate questions, kept separate. An Ethernet port that is enabled but
/// has no cable in it is `Disconnected`, not `Down`: the difference is between
/// "you turned it off" and "nothing is plugged in", and the user can act on
/// the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// Administratively up and carrying a link.
    Connected,
    /// Administratively up, no link. An unplugged cable, or Wi-Fi not
    /// associated with any network.
    Disconnected,
    /// Administratively down.
    Down,
    /// The platform reported a state PULSE cannot map — `unknown`, `testing`,
    /// `dormant`. Reported as itself rather than guessed into one of the
    /// above.
    Unknown,
}

impl LinkState {
    pub const fn label(self) -> &'static str {
        match self {
            LinkState::Connected => "Connected",
            LinkState::Disconnected => "Disconnected",
            LinkState::Down => "Down",
            LinkState::Unknown => "Unknown",
        }
    }

    /// Whether traffic can currently flow.
    pub const fn is_connected(self) -> bool {
        matches!(self, LinkState::Connected)
    }
}

/// How stable a derived identity actually is.
///
/// Recorded rather than assumed, so a user can be told when a saved widget
/// rests on something weaker than a burned-in hardware address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityStability {
    /// Derived from the permanent hardware address the adapter carries.
    /// Survives reboots, driver updates, renames and MAC randomisation, and
    /// is the *same* on every operating system.
    Hardware,
    /// Derived from an identifier the operating system assigns to this
    /// interface and keeps — a Windows interface GUID. Survives reboots and
    /// renames; meaningless on another OS.
    SystemAssigned,
    /// Derived from the current hardware address, which on Wi-Fi may be
    /// randomised and change when the machine joins a different network.
    CurrentAddress,
    /// Derived from a predictable kernel name such as `enp58s0`. Survives a
    /// reboot because it encodes the PCI path, but describes the slot rather
    /// than the hardware and does not exist on Windows.
    PredictableName,
    /// Derived from a name valid only while the interface exists.
    Session,
}

impl IdentityStability {
    /// Whether this identity is expected to mean the same thing after a
    /// reboot.
    pub const fn survives_reboot(self) -> bool {
        !matches!(self, IdentityStability::Session)
    }

    /// Whether it is expected to mean the same thing on another operating
    /// system running on the same hardware.
    pub const fn survives_os_change(self) -> bool {
        matches!(self, IdentityStability::Hardware)
    }

    /// A short explanation, surfaced in documentation and diagnostics.
    pub const fn explanation(self) -> &'static str {
        match self {
            IdentityStability::Hardware => {
                "derived from the permanent hardware address burned into the adapter"
            }
            IdentityStability::SystemAssigned => {
                "derived from an identifier the operating system assigns to this interface"
            }
            IdentityStability::CurrentAddress => {
                "derived from the current hardware address, which may be randomised on Wi-Fi"
            }
            IdentityStability::PredictableName => {
                "derived from the kernel's predictable name for the slot this adapter occupies"
            }
            IdentityStability::Session => {
                "derived from a name that is only valid while this interface exists"
            }
        }
    }
}

/// Which identifier an interface's `SourceId` was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkInterfaceIdentity {
    /// The permanent hardware address, as the adapter reports it.
    PermanentMac(String),
    /// A stable identifier the OS assigns — currently a Windows interface
    /// GUID.
    SystemId(String),
    /// The current hardware address. Used only when no permanent one exists,
    /// and flagged as weaker because Wi-Fi randomises it.
    CurrentMac(String),
    /// A predictable kernel name: `enp58s0`, `wlp59s0f0`.
    PredictableName(String),
    /// Any other interface name.
    Name(String),
}

impl NetworkInterfaceIdentity {
    pub const fn stability(&self) -> IdentityStability {
        match self {
            NetworkInterfaceIdentity::PermanentMac(_) => IdentityStability::Hardware,
            NetworkInterfaceIdentity::SystemId(_) => IdentityStability::SystemAssigned,
            NetworkInterfaceIdentity::CurrentMac(_) => IdentityStability::CurrentAddress,
            NetworkInterfaceIdentity::PredictableName(_) => IdentityStability::PredictableName,
            NetworkInterfaceIdentity::Name(_) => IdentityStability::Session,
        }
    }

    /// A short machine-readable tag, for diagnostics and the report.
    pub const fn mechanism(&self) -> &'static str {
        match self {
            NetworkInterfaceIdentity::PermanentMac(_) => "permanent-mac",
            NetworkInterfaceIdentity::SystemId(_) => "system-id",
            NetworkInterfaceIdentity::CurrentMac(_) => "current-mac",
            NetworkInterfaceIdentity::PredictableName(_) => "predictable-name",
            NetworkInterfaceIdentity::Name(_) => "interface-name",
        }
    }
}

// --- hardware addresses ----------------------------------------------------

/// The length of an Ethernet/Wi-Fi hardware address, in bytes.
pub const MAC_LEN: usize = 6;

/// Formats six bytes as the canonical lowercase colon form, `90:09:df:3e:97:f2`.
///
/// **Presentation and diagnostics only.** The `SourceId` uses the dashed form
/// below, because `SourceId` instances forbid `:`.
pub fn format_mac(bytes: &[u8]) -> Option<String> {
    let mac: &[u8; MAC_LEN] = bytes.get(..MAC_LEN)?.try_into().ok()?;

    Some(
        mac.iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// Whether six bytes are a usable hardware address.
///
/// Refuses the two values every platform uses to mean "there isn't one":
///
/// - **all zeros** — what `IFLA_PERM_ADDRESS` and `PermanentPhysicalAddress`
///   report for an interface that has no permanent address, such as a tunnel;
/// - **all ones** — the broadcast address, which no adapter owns.
///
/// Also refuses a **multicast** address (the low bit of the first byte set),
/// because an interface cannot be assigned one, so a value with that bit set
/// is a misread rather than a card.
///
/// It does *not* refuse locally-administered addresses (the second-lowest bit
/// of the first byte): a Docker bridge's `02:42:…` address is locally
/// administered, genuinely belongs to that interface, and is stable for as
/// long as the bridge exists.
pub fn is_usable_mac(bytes: &[u8]) -> bool {
    let Some(mac) = bytes.get(..MAC_LEN) else {
        return false;
    };

    if mac.iter().all(|&byte| byte == 0) || mac.iter().all(|&byte| byte == 0xFF) {
        return false;
    }

    // Bit 0 of the first octet is the individual/group bit.
    mac[0] & 0x01 == 0
}

/// Whether a kernel name is one systemd's predictable naming scheme produced.
///
/// `enp58s0`, `wlp59s0f0`, `enx001122334455`, `ens1`, `eno1`, `wls3`. These
/// encode the firmware index, the PCI path or the MAC, so they survive a
/// reboot — unlike `eth0` and `wlan0`, which are assigned in probe order.
pub fn is_predictable_name(name: &str) -> bool {
    // Two-letter prefix (`en`, `wl`, `ww`, `sl`) plus a scheme letter.
    let Some(rest) = name
        .strip_prefix("en")
        .or_else(|| name.strip_prefix("wl"))
        .or_else(|| name.strip_prefix("ww"))
    else {
        return false;
    };

    let Some(scheme) = rest.chars().next() else {
        return false;
    };

    // o = onboard index, s = hotplug slot, p = PCI path, x = MAC address.
    matches!(scheme, 'o' | 's' | 'p' | 'x') && rest.len() > 1
}

// --- source identifiers ----------------------------------------------------

/// Reduces an arbitrary identifier to something a `SourceId` instance accepts.
///
/// `SourceId` allows lowercase letters, digits, `-`, `_` and `.`, and forbids
/// a leading `-`. Every disallowed run collapses to a single `-`, so the
/// mapping is deterministic. Returns `None` when nothing usable survives.
pub fn normalize_identifier(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut pending_separator = false;

    for character in raw.trim().chars() {
        let lowered = character.to_ascii_lowercase();

        if lowered.is_ascii_lowercase() || lowered.is_ascii_digit() {
            if pending_separator && !out.is_empty() {
                out.push('-');
            }
            pending_separator = false;
            out.push(lowered);
        } else {
            pending_separator = true;
        }
    }

    let trimmed = out.trim_matches('-');
    (!trimmed.is_empty() && trimmed.chars().any(|c| c.is_ascii_alphanumeric()))
        .then(|| trimmed.to_string())
}

/// The longest instance fragment that still fits inside `SourceId`'s limit.
const MAX_INSTANCE_FRAGMENT: usize = 96;

fn truncate(fragment: String) -> String {
    if fragment.len() <= MAX_INSTANCE_FRAGMENT {
        return fragment;
    }

    let mut cut = MAX_INSTANCE_FRAGMENT;
    while cut > 0 && !fragment.is_char_boundary(cut) {
        cut -= 1;
    }

    fragment[..cut].trim_end_matches('-').to_string()
}

/// Builds the `SourceId` of an interface identified by a hardware address.
///
/// The six bytes become `network:mac-9009df3e97f2` — no separators, because a
/// colon terminates the `SourceId` kind and a dash would be noise. **The same
/// card produces the same string on Fedora and on Windows.**
pub fn mac_source_id(bytes: &[u8]) -> Option<SourceId> {
    if !is_usable_mac(bytes) {
        return None;
    }

    let mac = bytes.get(..MAC_LEN)?;
    let instance: String = mac.iter().map(|byte| format!("{byte:02x}")).collect();

    SourceId::new(format!("network:mac-{instance}")).ok()
}

/// Builds the `SourceId` of an interface identified by an OS-assigned
/// identifier, such as a Windows interface GUID.
pub fn system_source_id(system_id: &str) -> Option<SourceId> {
    let normalized = truncate(normalize_identifier(system_id)?);
    SourceId::new(format!("network:sys-{normalized}")).ok()
}

/// Builds the `SourceId` of an interface identified only by its name.
///
/// The `if-` prefix makes the weakness visible in the identifier itself.
pub fn name_source_id(name: &str) -> Option<SourceId> {
    let normalized = truncate(normalize_identifier(name)?);
    SourceId::new(format!("network:if-{normalized}")).ok()
}

/// Picks the strongest identity available and builds its `SourceId`.
///
/// The order is the whole point, and it is shared by both platforms so that
/// the *same* adapter yields the *same* identifier whichever operating system
/// enumerated it:
///
/// 1. the **permanent** hardware address — burned in, identical across
///    operating systems, unaffected by MAC randomisation;
/// 2. an **OS-assigned** stable identifier, for virtual interfaces that have
///    no permanent address but that the OS tracks properly;
/// 3. the **current** hardware address — real, but randomised on Wi-Fi;
/// 4. a **predictable kernel name** — describes the slot, survives reboots;
/// 5. the plain **name**, session-scoped, with the weakness recorded.
pub fn interface_identity(
    permanent_mac: Option<&[u8]>,
    system_id: Option<&str>,
    current_mac: Option<&[u8]>,
    name: &str,
) -> Option<(SourceId, NetworkInterfaceIdentity)> {
    if let Some(bytes) = permanent_mac {
        if let (Some(source), Some(text)) = (mac_source_id(bytes), format_mac(bytes)) {
            return Some((source, NetworkInterfaceIdentity::PermanentMac(text)));
        }
    }

    if let Some(system_id) = system_id {
        if let Some(source) = system_source_id(system_id) {
            return Some((
                source,
                NetworkInterfaceIdentity::SystemId(system_id.trim().to_string()),
            ));
        }
    }

    if let Some(bytes) = current_mac {
        if let (Some(source), Some(text)) = (mac_source_id(bytes), format_mac(bytes)) {
            return Some((source, NetworkInterfaceIdentity::CurrentMac(text)));
        }
    }

    let source = name_source_id(name)?;

    Some((
        source,
        if is_predictable_name(name) {
            NetworkInterfaceIdentity::PredictableName(name.to_string())
        } else {
            NetworkInterfaceIdentity::Name(name.to_string())
        },
    ))
}

// --- capabilities ----------------------------------------------------------

/// Whether each generic per-interface metric can be sampled here.
///
/// Carried **per metric**, never per interface: a tunnel has perfectly good
/// byte counters and no link speed, and a bridge reports no errors because it
/// has no physical layer to have them on. One flag per interface is what would
/// make PULSE hide a whole adapter over one missing counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkCapabilities {
    pub receive_bytes: Availability,
    pub transmit_bytes: Availability,
    pub receive_packets: Availability,
    pub transmit_packets: Availability,
    pub receive_errors: Availability,
    pub transmit_errors: Availability,
    pub receive_dropped: Availability,
    pub transmit_dropped: Availability,
    pub link_receive_speed: Availability,
    pub link_transmit_speed: Availability,
    pub mtu: Availability,
}

impl NetworkCapabilities {
    /// Everything readable.
    pub fn all_available() -> Self {
        Self {
            receive_bytes: Availability::Available,
            transmit_bytes: Availability::Available,
            receive_packets: Availability::Available,
            transmit_packets: Availability::Available,
            receive_errors: Availability::Available,
            transmit_errors: Availability::Available,
            receive_dropped: Availability::Available,
            transmit_dropped: Availability::Available,
            link_receive_speed: Availability::Available,
            link_transmit_speed: Availability::Available,
            mtu: Availability::Available,
        }
    }

    /// Replaces every traffic-counter capability with one reason.
    pub fn with_traffic(mut self, reason: Availability) -> Self {
        self.receive_bytes = reason.clone();
        self.transmit_bytes = reason.clone();
        self.receive_packets = reason.clone();
        self.transmit_packets = reason.clone();
        self.receive_errors = reason.clone();
        self.transmit_errors = reason.clone();
        self.receive_dropped = reason.clone();
        self.transmit_dropped = reason;
        self
    }

    /// Replaces both link-speed capabilities with one reason.
    pub fn with_link_speed(mut self, reason: Availability) -> Self {
        self.link_receive_speed = reason.clone();
        self.link_transmit_speed = reason;
        self
    }
}

/// Whether each Wi-Fi metric can be sampled on this interface.
///
/// Only ever carried by a descriptor whose kind is
/// [`NetworkInterfaceKind::Wifi`]. An Ethernet adapter does not declare Wi-Fi
/// metrics at all — see the module documentation of
/// [`super`](super#wi-fi-metrics-exist-only-on-wi-fi-interfaces).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiCapabilities {
    pub signal_quality: Availability,
    pub signal_rssi: Availability,
    pub receive_rate: Availability,
    pub transmit_rate: Availability,
}

impl WifiCapabilities {
    pub fn all_available() -> Self {
        Self {
            signal_quality: Availability::Available,
            signal_rssi: Availability::Available,
            receive_rate: Availability::Available,
            transmit_rate: Availability::Available,
        }
    }

    /// Nothing readable, all for the same stated reason.
    pub fn none(reason: &Availability) -> Self {
        Self {
            signal_quality: reason.clone(),
            signal_rssi: reason.clone(),
            receive_rate: reason.clone(),
            transmit_rate: reason.clone(),
        }
    }
}

// --- the descriptor --------------------------------------------------------

/// One network interface, as PULSE understands it.
///
/// Identity, name, kind and permanent address are **static for the life of the
/// process** and built once. A refresh re-reads the state, the addresses and
/// the counters; it does not re-enumerate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInterfaceDescriptor {
    /// The stable identifier a dashboard stores.
    pub source_id: SourceId,
    /// What the interface is called in the interface — a Windows adapter
    /// description, or the kernel name on Linux. **Presentation only.**
    pub display_name: String,
    /// The name the operating system uses: `wlp59s0f0`, or a Windows alias.
    /// Kept for diagnostics; never an identity unless nothing better existed,
    /// in which case `identity` says so.
    pub os_name: String,
    /// Which identifier `source_id` was derived from.
    pub identity: NetworkInterfaceIdentity,
    pub kind: NetworkInterfaceKind,
    /// The permanent hardware address, when the platform reports one.
    /// Diagnostics only — never rendered in the card's main text.
    pub permanent_mac: Option<String>,
    /// The address currently in use. Differs from `permanent_mac` when the
    /// platform is randomising it.
    pub current_mac: Option<String>,
    /// Which backend inventoried it, for diagnostics: `rtnetlink`, `iftable2`.
    pub backend: &'static str,
    pub capabilities: NetworkCapabilities,
    /// Present only for a Wi-Fi interface.
    pub wifi: Option<WifiCapabilities>,
}

impl NetworkInterfaceDescriptor {
    /// Whether this interface's `SourceId` is derived from something the
    /// hardware itself carries.
    pub fn has_hardware_identity(&self) -> bool {
        self.identity.stability() == IdentityStability::Hardware
    }

    /// Whether PULSE publishes Wi-Fi metrics for this interface.
    pub fn is_wireless(&self) -> bool {
        self.wifi.is_some()
    }

    /// Whether the current address differs from the permanent one — i.e. the
    /// platform is randomising it.
    ///
    /// Surfaced in diagnostics because it explains why an interface's identity
    /// and its visible address disagree.
    pub fn is_address_randomised(&self) -> bool {
        match (&self.permanent_mac, &self.current_mac) {
            (Some(permanent), Some(current)) => permanent != current,
            _ => false,
        }
    }
}

/// The parts of an interface that change while the machine runs.
///
/// Read on every refresh, unlike the descriptor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkInterfaceState {
    pub link: Option<LinkState>,
    /// MTU in bytes.
    pub mtu: Option<u32>,
    /// Link capacity in **bits per second**, as the platform reports it.
    /// `None` where the platform reports no usable figure — see
    /// `docs/metrics/network.md` on the zero-means-unknown trap.
    pub receive_link_bps: Option<u64>,
    pub transmit_link_bps: Option<u64>,
    /// Local addresses, for display. **Never an identity.**
    pub addresses: Vec<String>,
}

impl fmt::Display for NetworkInterfaceDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.display_name, self.source_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIFI_MAC: [u8; 6] = [0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2];
    const ETH_MAC: [u8; 6] = [0xd8, 0x43, 0xae, 0x32, 0x8a, 0xc9];
    const DOCKER_MAC: [u8; 6] = [0x02, 0x42, 0x70, 0x57, 0x01, 0x5e];

    // --- hardware addresses -----------------------------------------------

    #[test]
    fn formats_a_mac_in_the_canonical_lowercase_form() {
        assert_eq!(format_mac(&WIFI_MAC).as_deref(), Some("90:09:df:3e:97:f2"));
        assert_eq!(format_mac(&ETH_MAC).as_deref(), Some("d8:43:ae:32:8a:c9"));
    }

    #[test]
    fn refuses_a_short_buffer_rather_than_padding_it() {
        assert_eq!(format_mac(&[0x90, 0x09]), None);
        assert_eq!(format_mac(&[]), None);
        assert!(!is_usable_mac(&[0x90, 0x09, 0xdf]));
    }

    #[test]
    fn accepts_a_real_hardware_address() {
        assert!(is_usable_mac(&WIFI_MAC));
        assert!(is_usable_mac(&ETH_MAC));
    }

    #[test]
    fn refuses_the_values_that_mean_there_is_no_address() {
        // A tunnel reports all-zeros for its permanent address; the broadcast
        // address belongs to no adapter.
        assert!(!is_usable_mac(&[0; 6]));
        assert!(!is_usable_mac(&[0xFF; 6]));
        assert_eq!(mac_source_id(&[0; 6]), None);
        assert_eq!(mac_source_id(&[0xFF; 6]), None);
    }

    #[test]
    fn refuses_a_multicast_address() {
        // An interface cannot be assigned one, so the low bit being set means
        // the bytes were misread.
        assert!(!is_usable_mac(&[0x01, 0x00, 0x5e, 0x00, 0x00, 0x01]));
    }

    #[test]
    fn accepts_a_locally_administered_address() {
        // A Docker bridge's `02:42:…` is locally administered, genuinely
        // belongs to that bridge, and is stable for as long as it exists.
        assert!(is_usable_mac(&DOCKER_MAC));
        assert_eq!(
            mac_source_id(&DOCKER_MAC).map(|id| id.as_str().to_string()),
            Some("network:mac-02427057015e".to_string())
        );
    }

    #[test]
    fn a_mac_source_id_has_no_separators() {
        // `SourceId` forbids `:`, and dashes between every byte would be noise.
        assert_eq!(
            mac_source_id(&WIFI_MAC).map(|id| id.as_str().to_string()),
            Some("network:mac-9009df3e97f2".to_string())
        );
    }

    // --- predictable names -------------------------------------------------

    #[test]
    fn recognises_systemds_predictable_names() {
        for name in [
            "enp58s0",
            "wlp59s0f0",
            "eno1",
            "ens1",
            "enx001122334455",
            "wlp3s0",
        ] {
            assert!(is_predictable_name(name), "'{name}' should be predictable");
        }
    }

    #[test]
    fn does_not_mistake_a_probe_order_name_for_a_predictable_one() {
        // `eth0` and `wlan0` are assigned in probe order: a second adapter
        // renames the first.
        for name in [
            "eth0",
            "wlan0",
            "docker0",
            "veth18ddbc5",
            "br-4ccd0f14dd3d",
            "lo",
            "",
            "en",
        ] {
            assert!(!is_predictable_name(name), "'{name}' is not predictable");
        }
    }

    // --- identity ----------------------------------------------------------

    #[test]
    fn identity_prefers_the_permanent_address_over_everything() {
        let (source, identity) = interface_identity(
            Some(&WIFI_MAC),
            Some("{6C5A1B2D-0000-0000-0000-000000000000}"),
            Some(&[0x02, 0x11, 0x22, 0x33, 0x44, 0x55]),
            "wlp59s0f0",
        )
        .expect("identified");

        assert_eq!(source.as_str(), "network:mac-9009df3e97f2");
        assert_eq!(identity.mechanism(), "permanent-mac");
        assert_eq!(identity.stability(), IdentityStability::Hardware);
        assert!(identity.stability().survives_os_change());
    }

    #[test]
    fn a_randomised_wifi_address_never_becomes_the_identity() {
        // The trap this whole module exists for: NetworkManager and Windows
        // both randomise the current Wi-Fi MAC per network by default, so an
        // identity built on it changes when the user moves between home and
        // the office.
        let randomised = [0x02, 0xab, 0xcd, 0xef, 0x12, 0x34];

        let home =
            interface_identity(Some(&WIFI_MAC), None, Some(&randomised), "wlp59s0f0").unwrap();
        let office = interface_identity(
            Some(&WIFI_MAC),
            None,
            Some(&[0x02, 0x99, 0x88, 0x77, 0x66, 0x55]),
            "wlp59s0f0",
        )
        .unwrap();

        assert_eq!(home.0, office.0, "the card is the card, wherever it is");
        assert_eq!(home.0.as_str(), "network:mac-9009df3e97f2");
    }

    #[test]
    fn identity_falls_through_each_candidate_in_order() {
        let system = interface_identity(
            None,
            Some("{6C5A1B2D-1111-2222-3333-444444444444}"),
            None,
            "Ethernet",
        )
        .expect("identified");
        assert_eq!(system.1.mechanism(), "system-id");
        assert_eq!(system.1.stability(), IdentityStability::SystemAssigned);

        let current =
            interface_identity(None, None, Some(&DOCKER_MAC), "docker0").expect("identified");
        assert_eq!(current.1.mechanism(), "current-mac");
        assert_eq!(current.1.stability(), IdentityStability::CurrentAddress);

        let predictable = interface_identity(None, None, None, "enp58s0").expect("identified");
        assert_eq!(predictable.0.as_str(), "network:if-enp58s0");
        assert_eq!(
            predictable.1.stability(),
            IdentityStability::PredictableName
        );
        assert!(predictable.1.stability().survives_reboot());
        assert!(!predictable.1.stability().survives_os_change());

        let session = interface_identity(None, None, None, "tun0").expect("identified");
        assert_eq!(session.1.stability(), IdentityStability::Session);
        assert!(!session.1.stability().survives_reboot());
    }

    #[test]
    fn an_all_zero_permanent_address_falls_through_rather_than_colliding() {
        // Tunnels report all-zeros. Without this, every WireGuard, TUN and VPN
        // interface on the machine would collapse onto `network:mac-000000000000`.
        let first = interface_identity(Some(&[0; 6]), None, None, "wg0").expect("identified");
        let second =
            interface_identity(Some(&[0; 6]), None, None, "tailscale0").expect("identified");

        assert_ne!(first.0, second.0);
        assert_eq!(first.0.as_str(), "network:if-wg0");
        assert_eq!(second.0.as_str(), "network:if-tailscale0");
    }

    #[test]
    fn two_identical_adapter_models_stay_two_interfaces() {
        // The failure this module exists to prevent: identical NICs share a
        // description and differ only in their address.
        let first = interface_identity(
            Some(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x01]),
            None,
            None,
            "eth0",
        )
        .expect("identified");
        let second = interface_identity(
            Some(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x02]),
            None,
            None,
            "eth1",
        )
        .expect("identified");

        assert_ne!(first.0, second.0);
    }

    #[test]
    fn an_interface_with_nothing_at_all_still_gets_an_identifier() {
        let (source, identity) =
            interface_identity(None, None, None, "veth18ddbc5").expect("identified");

        assert_eq!(source.as_str(), "network:if-veth18ddbc5");
        assert_eq!(identity.mechanism(), "interface-name");
    }

    #[test]
    fn a_windows_guid_normalises_to_a_valid_instance() {
        let source = system_source_id("{6C5A1B2D-1111-2222-3333-444444444444}").expect("valid");

        assert_eq!(
            source.as_str(),
            "network:sys-6c5a1b2d-1111-2222-3333-444444444444"
        );
        assert_eq!(source.kind(), "network");
    }

    #[test]
    fn normalisation_refuses_a_string_with_nothing_in_it() {
        assert_eq!(normalize_identifier(""), None);
        assert_eq!(normalize_identifier("   "), None);
        assert_eq!(normalize_identifier("{}"), None);
        assert!(system_source_id("---").is_none());
    }

    #[test]
    fn an_over_long_name_is_truncated_to_a_valid_source() {
        let long = "a".repeat(400);
        let source = name_source_id(&long).expect("still valid");

        assert!(source.as_str().len() <= 128);
        assert!(source.as_str().starts_with("network:if-a"));
    }

    // --- kinds -------------------------------------------------------------

    #[test]
    fn loopback_is_the_only_kind_that_is_not_published() {
        for kind in NetworkInterfaceKind::ALL {
            assert_eq!(
                kind.is_published(),
                *kind != NetworkInterfaceKind::Loopback,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn only_real_adapters_are_primary_for_presentation() {
        assert!(NetworkInterfaceKind::Ethernet.is_primary());
        assert!(NetworkInterfaceKind::Wifi.is_primary());

        // Real, useful, and grouped separately so thirty of them cannot drown
        // out the two that matter. Not hidden.
        assert!(!NetworkInterfaceKind::Bridge.is_primary());
        assert!(!NetworkInterfaceKind::Tunnel.is_primary());
        assert!(!NetworkInterfaceKind::Virtual.is_primary());
        assert!(!NetworkInterfaceKind::Other.is_primary());

        for kind in NetworkInterfaceKind::ALL {
            assert!(!kind.label().is_empty());
        }
    }

    #[test]
    fn only_wifi_is_wireless() {
        assert!(NetworkInterfaceKind::Wifi.is_wireless());
        for kind in NetworkInterfaceKind::ALL {
            if *kind != NetworkInterfaceKind::Wifi {
                assert!(!kind.is_wireless(), "{kind:?}");
            }
        }
    }

    // --- link state --------------------------------------------------------

    #[test]
    fn an_unplugged_cable_is_not_the_same_as_a_disabled_adapter() {
        // "You turned it off" and "nothing is plugged in" are different, and
        // the user can act on the second.
        assert_ne!(LinkState::Disconnected, LinkState::Down);
        assert!(!LinkState::Disconnected.is_connected());
        assert!(!LinkState::Down.is_connected());
        assert!(LinkState::Connected.is_connected());
        assert!(!LinkState::Unknown.is_connected());

        for state in [
            LinkState::Connected,
            LinkState::Disconnected,
            LinkState::Down,
            LinkState::Unknown,
        ] {
            assert!(!state.label().is_empty());
        }
    }

    // --- capabilities ------------------------------------------------------

    #[test]
    fn capabilities_fail_one_at_a_time() {
        let capabilities = NetworkCapabilities::all_available().with_link_speed(
            Availability::unsupported("this interface reports no link speed"),
        );

        // The link speed is gone; the interface is still fully measured.
        assert!(!capabilities.link_receive_speed.is_available());
        assert!(!capabilities.link_transmit_speed.is_available());
        assert!(capabilities.receive_bytes.is_available());
        assert!(capabilities.transmit_packets.is_available());
        assert!(capabilities.mtu.is_available());
    }

    #[test]
    fn traffic_and_link_speed_are_independent() {
        let capabilities = NetworkCapabilities::all_available()
            .with_traffic(Availability::not_detected("no counters"));

        assert!(!capabilities.receive_bytes.is_available());
        assert!(capabilities.link_receive_speed.is_available());
        assert!(capabilities.mtu.is_available());
    }

    // --- the descriptor ----------------------------------------------------

    fn wifi_descriptor() -> NetworkInterfaceDescriptor {
        let (source_id, identity) =
            interface_identity(Some(&WIFI_MAC), None, Some(&WIFI_MAC), "wlp59s0f0").unwrap();

        NetworkInterfaceDescriptor {
            source_id,
            display_name: "wlp59s0f0".to_string(),
            os_name: "wlp59s0f0".to_string(),
            identity,
            kind: NetworkInterfaceKind::Wifi,
            permanent_mac: format_mac(&WIFI_MAC),
            current_mac: format_mac(&WIFI_MAC),
            backend: "test",
            capabilities: NetworkCapabilities::all_available(),
            wifi: Some(WifiCapabilities::all_available()),
        }
    }

    #[test]
    fn a_wifi_descriptor_carries_wifi_capabilities_and_an_ethernet_one_does_not() {
        let wifi = wifi_descriptor();
        assert!(wifi.is_wireless());
        assert!(wifi.has_hardware_identity());

        let mut ethernet = wifi_descriptor();
        ethernet.kind = NetworkInterfaceKind::Ethernet;
        ethernet.wifi = None;
        assert!(!ethernet.is_wireless());
    }

    #[test]
    fn randomisation_is_detected_by_comparing_the_two_addresses() {
        let mut descriptor = wifi_descriptor();
        assert!(!descriptor.is_address_randomised());

        descriptor.current_mac = format_mac(&[0x02, 0xab, 0xcd, 0xef, 0x12, 0x34]);
        assert!(descriptor.is_address_randomised());

        // …and with only one of the two known, nothing is claimed.
        descriptor.permanent_mac = None;
        assert!(!descriptor.is_address_randomised());
    }
}
