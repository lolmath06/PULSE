//! `RTM_GETLINK` — the whole interface inventory and every counter, in one
//! transaction.
//!
//! # Why one dump rather than a file per counter
//!
//! `/sys/class/net/<iface>/statistics/` holds one file per counter. Reading
//! the eight PULSE needs, for the thirteen interfaces on the development
//! machine, would be **104 file opens per refresh** — and worse, 104 separate
//! instants, so two interfaces' rates would describe two slightly different
//! windows.
//!
//! One `RTM_GETLINK` dump returns every interface's identity, name, MTU,
//! operational state, hardware addresses *and* its `rtnl_link_stats64` block,
//! captured together. One syscall pair, one instant, everything.
//!
//! # What is read
//!
//! ```text
//! IFLA_IFNAME        the kernel name
//! IFLA_MTU           the MTU
//! IFLA_OPERSTATE     RFC 2863 operational state
//! IFLA_ADDRESS       the address currently in use
//! IFLA_PERM_ADDRESS  the address burned into the adapter, when it has one
//! IFLA_STATS64       the 64-bit counter block
//! IFLA_LINKINFO      nested; IFLA_INFO_KIND names the virtual device type
//! ```
//!
//! `IFLA_PERM_ADDRESS` is the one that matters most: it is what makes an
//! identity survive MAC randomisation. It was added in Linux 5.6; on an older
//! kernel it is simply absent and PULSE falls through to the current address,
//! recording the weaker guarantee rather than pretending.

use crate::metrics::wellknown::network::{LinkState, NetworkCounters, NetworkInterfaceKind};

use super::netlink::{Attributes, Messages};

/// `RTM_GETLINK`.
pub const RTM_GETLINK: u16 = 18;
/// `RTM_NEWLINK` — the message type each dump entry carries.
pub const RTM_NEWLINK: u16 = 16;

/// The size of `struct ifinfomsg`, which precedes the attributes.
pub const IFINFOMSG_LEN: usize = 16;

// --- IFLA attribute types --------------------------------------------------

const IFLA_ADDRESS: u16 = 1;
const IFLA_IFNAME: u16 = 3;
const IFLA_MTU: u16 = 4;
const IFLA_LINKINFO: u16 = 18;
const IFLA_STATS64: u16 = 23;
const IFLA_OPERSTATE: u16 = 16;
const IFLA_PERM_ADDRESS: u16 = 54;

/// `IFLA_INFO_KIND`, nested inside `IFLA_LINKINFO`.
const IFLA_INFO_KIND: u16 = 1;

// --- interface flags -------------------------------------------------------

/// `IFF_UP` — administratively enabled.
const IFF_UP: u32 = 0x1;
/// `IFF_LOOPBACK`.
const IFF_LOOPBACK: u32 = 0x8;

// --- operational states ----------------------------------------------------
//
// RFC 2863, as `IF_OPER_*` in `linux/if.h`.

const IF_OPER_DOWN: u8 = 2;
const IF_OPER_LOWERLAYERDOWN: u8 = 3;
const IF_OPER_DORMANT: u8 = 5;
const IF_OPER_UP: u8 = 6;

/// `ARPHRD_ETHER` — the link type nearly every interface reports, Wi-Fi
/// included.
///
/// Worth stating because it is a common source of wrong guesses: a Wi-Fi
/// station interface reports `ARPHRD_ETHER`, exactly like a wired NIC. The
/// link type cannot tell them apart, which is why PULSE asks `nl80211` which
/// interfaces are wireless instead of guessing from this field or from the
/// name.
pub const ARPHRD_ETHER: u16 = 1;
/// `ARPHRD_LOOPBACK`.
pub const ARPHRD_LOOPBACK: u16 = 772;
/// `ARPHRD_NONE` — what a TUN interface and WireGuard report.
pub const ARPHRD_NONE: u16 = 0xFFFE;

/// One interface, as `RTM_GETLINK` described it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkEntry {
    /// The kernel's index. **Never an identity** — it is reused after an
    /// interface is destroyed.
    pub index: i32,
    /// `ARPHRD_*`.
    pub link_type: u16,
    /// `IFF_*`.
    pub flags: u32,
    pub name: String,
    pub mtu: Option<u32>,
    /// The address currently in use. On Wi-Fi this is frequently randomised.
    pub address: Option<Vec<u8>>,
    /// The address burned into the adapter, when the kernel reports one.
    pub permanent_address: Option<Vec<u8>>,
    /// `IFLA_INFO_KIND` — `bridge`, `veth`, `wireguard`, `tun`, `vlan`…
    /// Absent for a physical adapter, which is itself informative.
    pub info_kind: Option<String>,
    /// The RFC 2863 operational state.
    pub oper_state: Option<u8>,
    pub counters: Option<NetworkCounters>,
}

impl LinkEntry {
    /// Whether the kernel flagged this interface as loopback.
    pub const fn is_loopback(&self) -> bool {
        self.flags & IFF_LOOPBACK != 0 || self.link_type == ARPHRD_LOOPBACK
    }

    /// Whether the interface is administratively enabled.
    pub const fn is_administratively_up(&self) -> bool {
        self.flags & IFF_UP != 0
    }

    /// The link state, from the operational state and the flags together.
    ///
    /// The two answer different questions — "has the administrator enabled
    /// it" and "is there a link" — and the distinction is what separates a
    /// disabled adapter from one with an unplugged cable.
    pub fn link_state(&self) -> LinkState {
        if !self.is_administratively_up() {
            return LinkState::Down;
        }

        match self.oper_state {
            Some(IF_OPER_UP) => LinkState::Connected,
            // `LOWERLAYERDOWN` is an unplugged cable; `DORMANT` is a radio
            // waiting to associate. Both are "up, but nothing to talk to".
            Some(IF_OPER_DOWN) | Some(IF_OPER_LOWERLAYERDOWN) | Some(IF_OPER_DORMANT) => {
                LinkState::Disconnected
            }
            // `IF_OPER_UNKNOWN` is what a TUN device reports while perfectly
            // functional, so it must not be read as a failure.
            _ => LinkState::Unknown,
        }
    }

    /// The interface kind, from what the kernel actually reports.
    ///
    /// `wireless` comes from `nl80211`'s own list of wireless interfaces, not
    /// from the name: `wlan0` is a convention, a bridge can be called `eth0`,
    /// and every Wi-Fi station reports `ARPHRD_ETHER` like a wired NIC.
    pub fn kind(&self, wireless: bool) -> NetworkInterfaceKind {
        if self.is_loopback() {
            return NetworkInterfaceKind::Loopback;
        }
        if wireless {
            return NetworkInterfaceKind::Wifi;
        }

        match self.info_kind.as_deref() {
            Some("bridge") => NetworkInterfaceKind::Bridge,
            // WireGuard, TUN/TAP, and every VPN client that uses one. They are
            // grouped rather than split into "tunnel" and "VPN" because
            // nothing the kernel reports distinguishes a WireGuard link used
            // for a VPN from one used for anything else.
            Some("wireguard") | Some("tun") | Some("tap") | Some("ipip") | Some("sit")
            | Some("gre") | Some("gretap") | Some("vti") | Some("wwan") => {
                NetworkInterfaceKind::Tunnel
            }
            Some("veth") | Some("macvlan") | Some("ipvlan") | Some("vlan") | Some("vxlan")
            | Some("dummy") | Some("bond") | Some("team") => NetworkInterfaceKind::Virtual,
            // A kind PULSE has no mapping for is still virtual — the kernel
            // only reports `IFLA_INFO_KIND` for software devices.
            Some(_) => NetworkInterfaceKind::Virtual,
            None => match self.link_type {
                ARPHRD_ETHER => NetworkInterfaceKind::Ethernet,
                // A TUN device created outside a named link type.
                ARPHRD_NONE => NetworkInterfaceKind::Tunnel,
                _ => NetworkInterfaceKind::Other,
            },
        }
    }
}

/// Parses `struct rtnl_link_stats64`.
///
/// The layout is a sequence of 64-bit fields in a documented order. PULSE
/// reads the first eight, which are exactly the ones the shared contract
/// needs:
///
/// ```text
/// 0  rx_packets      4  rx_errors
/// 1  tx_packets      5  tx_errors
/// 2  rx_bytes        6  rx_dropped
/// 3  tx_bytes        7  tx_dropped
/// ```
///
/// A shorter attribute is refused rather than zero-filled: a partial stats
/// block would publish a throughput derived from whichever fields happened to
/// arrive.
pub fn parse_stats64(payload: &[u8]) -> Option<NetworkCounters> {
    const FIELD: usize = 8;
    const REQUIRED: usize = FIELD * 8;

    if payload.len() < REQUIRED {
        return None;
    }

    let field = |index: usize| -> Option<u64> {
        let start = index * FIELD;
        let bytes: [u8; FIELD] = payload.get(start..start + FIELD)?.try_into().ok()?;
        Some(u64::from_ne_bytes(bytes))
    };

    Some(NetworkCounters {
        receive_packets: field(0)?,
        transmit_packets: field(1)?,
        receive_bytes: field(2)?,
        transmit_bytes: field(3)?,
        receive_errors: field(4)?,
        transmit_errors: field(5)?,
        receive_dropped: field(6)?,
        transmit_dropped: field(7)?,
    })
}

/// Parses one `RTM_NEWLINK` message payload.
///
/// Returns `None` when the `ifinfomsg` header is truncated.
pub fn parse_link(payload: &[u8]) -> Option<LinkEntry> {
    let header = payload.get(..IFINFOMSG_LEN)?;

    let mut entry = LinkEntry {
        link_type: u16::from_ne_bytes(header[2..4].try_into().ok()?),
        index: i32::from_ne_bytes(header[4..8].try_into().ok()?),
        flags: u32::from_ne_bytes(header[8..12].try_into().ok()?),
        ..LinkEntry::default()
    };

    for attribute in Attributes::new(&payload[IFINFOMSG_LEN..]) {
        match attribute.kind {
            IFLA_IFNAME => {
                if let Some(name) = attribute.as_str() {
                    entry.name = name.to_string();
                }
            }
            IFLA_MTU => entry.mtu = attribute.as_u32(),
            IFLA_OPERSTATE => entry.oper_state = attribute.as_u8(),
            IFLA_ADDRESS => entry.address = Some(attribute.payload.to_vec()),
            IFLA_PERM_ADDRESS => entry.permanent_address = Some(attribute.payload.to_vec()),
            IFLA_STATS64 => entry.counters = parse_stats64(attribute.payload),
            IFLA_LINKINFO => {
                entry.info_kind = attribute
                    .nested()
                    .find(IFLA_INFO_KIND)
                    .and_then(|kind| kind.as_str())
                    .map(str::to_string);
            }
            _ => {}
        }
    }

    // An interface with no name is not one PULSE can do anything with.
    (!entry.name.is_empty()).then_some(entry)
}

/// Parses a whole `RTM_GETLINK` dump.
///
/// Messages that are not link entries — the terminating `NLMSG_DONE`, any
/// acknowledgement — are skipped, and a single unparseable entry does not cost
/// the others.
pub fn parse_dump(buffer: &[u8]) -> Vec<LinkEntry> {
    Messages::new(buffer)
        .filter(|message| message.header.message_type == RTM_NEWLINK)
        .filter_map(|message| parse_link(message.payload))
        .collect()
}

/// Builds the `ifinfomsg` payload of a `RTM_GETLINK` dump request.
///
/// All zeros: family `AF_UNSPEC`, no index, no filter — which is what asks the
/// kernel for every interface.
pub fn dump_request() -> Vec<u8> {
    vec![0_u8; IFINFOMSG_LEN]
}

/// Reads every interface from the kernel, in one transaction.
#[cfg(target_os = "linux")]
pub fn dump() -> Result<Vec<LinkEntry>, crate::metrics::model::MetricError> {
    use super::netlink::NLM_F_DUMP;
    use super::socket::{NetlinkSocket, NETLINK_ROUTE};

    let mut socket = NetlinkSocket::open(NETLINK_ROUTE)?;
    let reply = socket.request(
        RTM_GETLINK,
        NLM_F_DUMP,
        &dump_request(),
        "the network interface dump",
    )?;

    Ok(parse_dump(&reply))
}

/// Compiled on non-Linux hosts so the Windows cross-check type checks this
/// module. Never reached: only the Linux provider calls it.
#[cfg(not(target_os = "linux"))]
pub fn dump() -> Result<Vec<LinkEntry>, crate::metrics::model::MetricError> {
    use crate::metrics::model::{MetricError, MetricErrorCode};

    Err(MetricError::new(
        MetricErrorCode::Unsupported,
        "rtnetlink is a Linux interface",
    ))
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::platform::linux::network::netlink::fixtures as nl;
    use crate::platform::linux::network::netlink::{NLA_F_NESTED, NLM_F_MULTI};

    /// Builds a `rtnl_link_stats64` payload from the eight counters PULSE
    /// reads, followed by the tail fields it ignores.
    #[allow(clippy::too_many_arguments)]
    pub fn stats64(
        rx_packets: u64,
        tx_packets: u64,
        rx_bytes: u64,
        tx_bytes: u64,
        rx_errors: u64,
        tx_errors: u64,
        rx_dropped: u64,
        tx_dropped: u64,
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        for value in [
            rx_packets, tx_packets, rx_bytes, tx_bytes, rx_errors, tx_errors, rx_dropped,
            tx_dropped,
        ] {
            payload.extend_from_slice(&value.to_ne_bytes());
        }
        // The real structure carries a dozen more fields — multicast,
        // collisions, per-direction detail — which PULSE does not read.
        payload.extend_from_slice(&[0_u8; 8 * 12]);
        payload
    }

    /// Builds one `RTM_NEWLINK` message.
    pub struct Link {
        pub index: i32,
        pub link_type: u16,
        pub flags: u32,
        pub name: &'static str,
        pub mtu: Option<u32>,
        pub address: Option<Vec<u8>>,
        pub permanent_address: Option<Vec<u8>>,
        pub info_kind: Option<&'static str>,
        pub oper_state: Option<u8>,
        pub stats: Option<Vec<u8>>,
    }

    impl Default for Link {
        fn default() -> Self {
            Self {
                index: 1,
                link_type: ARPHRD_ETHER,
                flags: IFF_UP,
                name: "eth0",
                mtu: Some(1500),
                address: None,
                permanent_address: None,
                info_kind: None,
                oper_state: Some(IF_OPER_UP),
                stats: None,
            }
        }
    }

    impl Link {
        /// Encodes this link as a `RTM_NEWLINK` message.
        pub fn encode(&self, sequence: u32) -> Vec<u8> {
            let mut payload = Vec::new();
            payload.push(0); // ifi_family
            payload.push(0); // padding
            payload.extend_from_slice(&self.link_type.to_ne_bytes());
            payload.extend_from_slice(&self.index.to_ne_bytes());
            payload.extend_from_slice(&self.flags.to_ne_bytes());
            payload.extend_from_slice(&0_u32.to_ne_bytes()); // ifi_change

            payload.extend(nl::string_attribute(IFLA_IFNAME, self.name));
            if let Some(mtu) = self.mtu {
                payload.extend(nl::u32_attribute(IFLA_MTU, mtu));
            }
            if let Some(state) = self.oper_state {
                payload.extend(nl::u8_attribute(IFLA_OPERSTATE, state));
            }
            if let Some(address) = &self.address {
                payload.extend(nl::attribute(IFLA_ADDRESS, address));
            }
            if let Some(address) = &self.permanent_address {
                payload.extend(nl::attribute(IFLA_PERM_ADDRESS, address));
            }
            if let Some(kind) = self.info_kind {
                payload.extend(nl::attribute(
                    IFLA_LINKINFO | NLA_F_NESTED,
                    &nl::string_attribute(IFLA_INFO_KIND, kind),
                ));
            }
            if let Some(stats) = &self.stats {
                payload.extend(nl::attribute(IFLA_STATS64, stats));
            }

            nl::message(RTM_NEWLINK, NLM_F_MULTI, sequence, &payload)
        }
    }

    /// Builds a whole dump from several links, terminated properly.
    pub fn encode_dump(links: &[Link]) -> Vec<u8> {
        let mut buffer = Vec::new();
        for link in links {
            buffer.extend(link.encode(1));
        }
        buffer.extend(nl::done(1));
        buffer
    }

    /// The Wi-Fi adapter on the development machine.
    pub fn wifi() -> Link {
        Link {
            index: 3,
            name: "wlp59s0f0",
            flags: IFF_UP,
            oper_state: Some(IF_OPER_UP),
            address: Some(vec![0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2]),
            permanent_address: Some(vec![0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2]),
            stats: Some(stats64(
                3_591_166,
                1_267_925,
                3_210_730_391,
                868_552_645,
                0,
                0,
                337,
                0,
            )),
            ..Link::default()
        }
    }

    /// The unplugged Ethernet port on the development machine.
    pub fn ethernet_unplugged() -> Link {
        Link {
            index: 2,
            name: "enp58s0",
            flags: IFF_UP,
            oper_state: Some(IF_OPER_DOWN),
            address: Some(vec![0xd8, 0x43, 0xae, 0x32, 0x8a, 0xc9]),
            permanent_address: Some(vec![0xd8, 0x43, 0xae, 0x32, 0x8a, 0xc9]),
            stats: Some(stats64(0, 0, 0, 0, 0, 0, 0, 0)),
            ..Link::default()
        }
    }

    /// The loopback interface.
    pub fn loopback() -> Link {
        Link {
            index: 1,
            name: "lo",
            link_type: ARPHRD_LOOPBACK,
            flags: IFF_UP | IFF_LOOPBACK,
            oper_state: Some(0), // IF_OPER_UNKNOWN
            address: Some(vec![0; 6]),
            mtu: Some(65536),
            stats: Some(stats64(100, 100, 10_000, 10_000, 0, 0, 0, 0)),
            ..Link::default()
        }
    }

    /// A locally-administered address derived from a name.
    ///
    /// Real bridges and `veth` pairs each carry their own, so fixtures must
    /// too: giving several the same address would make them collapse onto one
    /// identity and hide the very deduplication being tested elsewhere.
    fn derived_address(prefix: u8, name: &str) -> Vec<u8> {
        let mut hash: u32 = 2_166_136_261;
        for byte in name.as_bytes() {
            hash ^= u32::from(*byte);
            hash = hash.wrapping_mul(16_777_619);
        }
        let bytes = hash.to_be_bytes();

        vec![0x02, prefix, bytes[0], bytes[1], bytes[2], bytes[3]]
    }

    /// A Docker bridge.
    pub fn bridge(name: &'static str, up: bool) -> Link {
        Link {
            index: 100 + (name.len() as i32),
            name,
            flags: IFF_UP,
            oper_state: Some(if up { IF_OPER_UP } else { IF_OPER_DOWN }),
            address: Some(derived_address(0x42, name)),
            info_kind: Some("bridge"),
            stats: Some(stats64(0, 0, 0, 0, 0, 0, 0, 0)),
            ..Link::default()
        }
    }

    /// One end of a `veth` pair.
    pub fn veth(name: &'static str) -> Link {
        Link {
            index: 200 + (name.len() as i32),
            name,
            flags: IFF_UP,
            oper_state: Some(IF_OPER_UP),
            address: Some(derived_address(0x4a, name)),
            info_kind: Some("veth"),
            stats: Some(stats64(10, 20, 1000, 2000, 0, 0, 0, 0)),
            ..Link::default()
        }
    }

    /// A WireGuard tunnel, which reports no hardware address at all.
    pub fn wireguard(name: &'static str) -> Link {
        Link {
            index: 6,
            name,
            link_type: ARPHRD_NONE,
            flags: IFF_UP,
            oper_state: Some(0), // IF_OPER_UNKNOWN, and perfectly functional
            address: None,
            permanent_address: None,
            info_kind: Some("wireguard"),
            mtu: Some(1420),
            stats: Some(stats64(500, 400, 60_000, 50_000, 0, 0, 0, 0)),
        }
    }

    /// A USB Ethernet adapter, with no `IFLA_PERM_ADDRESS` — an older kernel,
    /// or a driver that does not report one.
    pub fn usb_ethernet() -> Link {
        Link {
            index: 7,
            name: "enp0s20u1",
            flags: IFF_UP,
            oper_state: Some(IF_OPER_UP),
            address: Some(vec![0x00, 0xe0, 0x4c, 0x68, 0x01, 0x02]),
            permanent_address: None,
            stats: Some(stats64(1, 1, 100, 100, 0, 0, 0, 0)),
            ..Link::default()
        }
    }

    /// A Wi-Fi adapter whose current address has been randomised.
    pub fn wifi_randomised() -> Link {
        Link {
            address: Some(vec![0x02, 0xab, 0xcd, 0xef, 0x12, 0x34]),
            permanent_address: Some(vec![0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2]),
            ..wifi()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    // --- the stats block ---------------------------------------------------

    #[test]
    fn reads_the_eight_counters_in_their_documented_order() {
        // Getting the order wrong publishes packet counts as byte counts,
        // which looks plausible and is out by three orders of magnitude.
        let payload = stats64(
            3_591_166,
            1_267_925,
            3_210_730_391,
            868_552_645,
            1,
            2,
            337,
            4,
        );
        let counters = parse_stats64(&payload).expect("valid");

        assert_eq!(counters.receive_packets, 3_591_166);
        assert_eq!(counters.transmit_packets, 1_267_925);
        assert_eq!(counters.receive_bytes, 3_210_730_391);
        assert_eq!(counters.transmit_bytes, 868_552_645);
        assert_eq!(counters.receive_errors, 1);
        assert_eq!(counters.transmit_errors, 2);
        assert_eq!(counters.receive_dropped, 337);
        assert_eq!(counters.transmit_dropped, 4);
    }

    #[test]
    fn errors_and_drops_are_read_from_different_fields() {
        let payload = stats64(0, 0, 0, 0, 11, 22, 33, 44);
        let counters = parse_stats64(&payload).expect("valid");

        assert_eq!(counters.receive_errors, 11);
        assert_eq!(counters.receive_dropped, 33);
        assert_ne!(counters.receive_errors, counters.receive_dropped);
    }

    #[test]
    fn a_truncated_stats_block_is_refused_rather_than_zero_filled() {
        // A partial block would publish a throughput derived from whichever
        // fields happened to arrive.
        assert_eq!(parse_stats64(&[0_u8; 32]), None);
        assert_eq!(parse_stats64(&[]), None);
        assert!(parse_stats64(&[0_u8; 64]).is_some());
    }

    #[test]
    fn a_longer_stats_block_is_accepted_at_the_documented_offsets() {
        // The kernel has added fields over time and will add more.
        let mut payload = stats64(1, 2, 3, 4, 5, 6, 7, 8);
        payload.extend_from_slice(&[0xAB; 128]);

        let counters = parse_stats64(&payload).expect("valid");
        assert_eq!(counters.receive_packets, 1);
        assert_eq!(counters.transmit_dropped, 8);
    }

    // --- one link ----------------------------------------------------------

    #[test]
    fn parses_the_real_wifi_adapter() {
        let buffer = wifi().encode(1);
        let message = Messages::new(&buffer).next().expect("one message");
        let entry = parse_link(message.payload).expect("parsed");

        assert_eq!(entry.name, "wlp59s0f0");
        assert_eq!(entry.index, 3);
        assert_eq!(entry.mtu, Some(1500));
        assert_eq!(entry.link_state(), LinkState::Connected);
        assert_eq!(
            entry.permanent_address.as_deref(),
            Some(&[0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2][..])
        );
        assert_eq!(entry.counters.expect("stats").receive_bytes, 3_210_730_391);
        // Wi-Fi reports `ARPHRD_ETHER` exactly like a wired NIC, which is why
        // the kind cannot be derived from it.
        assert_eq!(entry.link_type, ARPHRD_ETHER);
        assert_eq!(entry.kind(false), NetworkInterfaceKind::Ethernet);
        assert_eq!(entry.kind(true), NetworkInterfaceKind::Wifi);
    }

    #[test]
    fn an_interface_with_no_name_is_not_an_interface() {
        let mut payload = vec![0_u8; IFINFOMSG_LEN];
        payload.extend(
            crate::platform::linux::network::netlink::fixtures::u32_attribute(IFLA_MTU, 1500),
        );

        assert_eq!(parse_link(&payload), None);
    }

    #[test]
    fn a_truncated_header_is_refused() {
        assert_eq!(parse_link(&[]), None);
        assert_eq!(parse_link(&[0_u8; 8]), None);
    }

    #[test]
    fn a_link_with_no_attributes_at_all_is_refused_without_panicking() {
        assert_eq!(parse_link(&[0_u8; IFINFOMSG_LEN]), None);
    }

    // --- link state --------------------------------------------------------

    #[test]
    fn an_unplugged_cable_is_disconnected_not_down() {
        let entry = parse_link(
            Messages::new(&ethernet_unplugged().encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");

        // Administratively up, no carrier.
        assert!(entry.is_administratively_up());
        assert_eq!(entry.link_state(), LinkState::Disconnected);
    }

    #[test]
    fn a_disabled_adapter_is_down() {
        let link = Link {
            flags: 0,
            oper_state: Some(IF_OPER_DOWN),
            ..ethernet_unplugged()
        };
        let entry =
            parse_link(Messages::new(&link.encode(1)).next().unwrap().payload).expect("parsed");

        assert!(!entry.is_administratively_up());
        assert_eq!(entry.link_state(), LinkState::Down);
    }

    #[test]
    fn a_lower_layer_down_state_is_disconnected() {
        let link = Link {
            oper_state: Some(IF_OPER_LOWERLAYERDOWN),
            ..ethernet_unplugged()
        };
        let entry =
            parse_link(Messages::new(&link.encode(1)).next().unwrap().payload).expect("parsed");

        assert_eq!(entry.link_state(), LinkState::Disconnected);
    }

    #[test]
    fn an_unknown_operational_state_is_not_read_as_a_failure() {
        // A TUN device reports `IF_OPER_UNKNOWN` while perfectly functional.
        let entry = parse_link(
            Messages::new(&wireguard("wg0").encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");

        assert_eq!(entry.link_state(), LinkState::Unknown);
        assert_ne!(entry.link_state(), LinkState::Down);
    }

    // --- kinds -------------------------------------------------------------

    #[test]
    fn a_bridge_is_recognised_from_its_link_info_kind() {
        let entry = parse_link(
            Messages::new(&bridge("docker0", false).encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");

        assert_eq!(entry.info_kind.as_deref(), Some("bridge"));
        assert_eq!(entry.kind(false), NetworkInterfaceKind::Bridge);
        assert!(!entry.kind(false).is_primary());
    }

    #[test]
    fn a_veth_is_recognised_as_virtual() {
        let entry = parse_link(
            Messages::new(&veth("vethe0ffe82").encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");

        assert_eq!(entry.kind(false), NetworkInterfaceKind::Virtual);
    }

    #[test]
    fn wireguard_and_tun_are_both_tunnels() {
        let entry = parse_link(
            Messages::new(&wireguard("wg0").encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");
        assert_eq!(entry.kind(false), NetworkInterfaceKind::Tunnel);

        let tun = Link {
            info_kind: Some("tun"),
            ..wireguard("tun0")
        };

        let entry =
            parse_link(Messages::new(&tun.encode(1)).next().unwrap().payload).expect("parsed");
        assert_eq!(entry.kind(false), NetworkInterfaceKind::Tunnel);
    }

    #[test]
    fn a_link_info_kind_pulse_has_no_mapping_for_is_still_virtual() {
        // The kernel reports `IFLA_INFO_KIND` only for software devices, so an
        // unrecognised one is certainly not a physical adapter.
        let link = Link {
            info_kind: Some("some_future_virtual_device"),
            ..Link::default()
        };
        let entry =
            parse_link(Messages::new(&link.encode(1)).next().unwrap().payload).expect("parsed");

        assert_eq!(entry.kind(false), NetworkInterfaceKind::Virtual);
    }

    #[test]
    fn a_physical_adapter_reports_no_link_info_kind_and_is_ethernet() {
        let entry = parse_link(
            Messages::new(&ethernet_unplugged().encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");

        assert_eq!(entry.info_kind, None);
        assert_eq!(entry.kind(false), NetworkInterfaceKind::Ethernet);
    }

    #[test]
    fn loopback_is_recognised_from_the_flag_and_from_the_link_type() {
        let entry = parse_link(Messages::new(&loopback().encode(1)).next().unwrap().payload)
            .expect("parsed");

        assert!(entry.is_loopback());
        assert_eq!(entry.kind(false), NetworkInterfaceKind::Loopback);
        // …and being told it is wireless does not change that.
        assert_eq!(entry.kind(true), NetworkInterfaceKind::Loopback);
    }

    // --- addresses ---------------------------------------------------------

    #[test]
    fn a_randomised_current_address_is_kept_apart_from_the_permanent_one() {
        let entry = parse_link(
            Messages::new(&wifi_randomised().encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");

        assert_eq!(
            entry.permanent_address.as_deref(),
            Some(&[0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2][..])
        );
        assert_eq!(
            entry.address.as_deref(),
            Some(&[0x02, 0xab, 0xcd, 0xef, 0x12, 0x34][..])
        );
        assert_ne!(entry.address, entry.permanent_address);
    }

    #[test]
    fn an_interface_with_no_permanent_address_reports_none() {
        // `IFLA_PERM_ADDRESS` arrived in Linux 5.6; before that, and on some
        // drivers since, it is simply absent.
        let entry = parse_link(
            Messages::new(&usb_ethernet().encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");

        assert_eq!(entry.permanent_address, None);
        assert!(entry.address.is_some());
    }

    #[test]
    fn a_tunnel_reports_no_address_at_all() {
        let entry = parse_link(
            Messages::new(&wireguard("wg0").encode(1))
                .next()
                .unwrap()
                .payload,
        )
        .expect("parsed");

        assert_eq!(entry.address, None);
        assert_eq!(entry.permanent_address, None);
        assert_eq!(entry.mtu, Some(1420));
    }

    // --- the whole dump ----------------------------------------------------

    #[test]
    fn parses_a_dump_resembling_the_development_machine() {
        let links = vec![
            loopback(),
            ethernet_unplugged(),
            wifi(),
            bridge("docker0", false),
            bridge("br-a4c5d1b67a43", true),
            veth("vethe0ffe82"),
            wireguard("wg0"),
        ];
        let buffer = encode_dump(&links);

        let entries = parse_dump(&buffer);

        assert_eq!(entries.len(), 7);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "lo",
                "enp58s0",
                "wlp59s0f0",
                "docker0",
                "br-a4c5d1b67a43",
                "vethe0ffe82",
                "wg0"
            ]
        );
    }

    #[test]
    fn the_terminating_message_is_not_parsed_as_an_interface() {
        let entries = parse_dump(&encode_dump(&[wifi()]));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "wlp59s0f0");
    }

    #[test]
    fn one_unparseable_entry_does_not_cost_the_others() {
        let mut buffer = wifi().encode(1);
        // A link message whose `ifinfomsg` is truncated.
        buffer.extend(crate::platform::linux::network::netlink::fixtures::message(
            RTM_NEWLINK,
            crate::platform::linux::network::netlink::NLM_F_MULTI,
            1,
            &[0_u8; 4],
        ));
        buffer.extend(ethernet_unplugged().encode(1));
        buffer.extend(crate::platform::linux::network::netlink::fixtures::done(1));

        let entries = parse_dump(&buffer);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "wlp59s0f0");
        assert_eq!(entries[1].name, "enp58s0");
    }

    #[test]
    fn an_empty_dump_yields_nothing_rather_than_failing() {
        assert!(parse_dump(&[]).is_empty());
        assert!(
            parse_dump(&crate::platform::linux::network::netlink::fixtures::done(1)).is_empty()
        );
    }

    #[test]
    fn the_dump_request_asks_for_every_interface() {
        // All zeros: `AF_UNSPEC`, no index, no filter.
        let request = dump_request();

        assert_eq!(request.len(), IFINFOMSG_LEN);
        assert!(request.iter().all(|&byte| byte == 0));
    }

    // --- against the real kernel -------------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_kernel_answers_a_dump_on_this_machine() {
        // `RTM_GETLINK` needs no privilege, so this works for any user.
        let entries = super::dump().expect("the kernel answers");

        assert!(!entries.is_empty(), "every machine has at least loopback");
        assert!(
            entries.iter().any(LinkEntry::is_loopback),
            "loopback is always present"
        );
        // Every entry the parser accepted has a name and a state.
        for entry in &entries {
            assert!(!entry.name.is_empty());
            assert!(entry.mtu.is_some(), "{} has no MTU", entry.name);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_kernel_reports_counters_for_its_interfaces() {
        let entries = super::dump().expect("the kernel answers");

        assert!(
            entries.iter().any(|entry| entry.counters.is_some()),
            "at least one interface reports stats64"
        );
    }
}
