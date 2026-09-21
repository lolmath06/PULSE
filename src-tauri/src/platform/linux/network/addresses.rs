//! `RTM_GETADDR` — the local addresses assigned to each interface.
//!
//! **For display only.** An IP address is not an identity and never reaches a
//! `SourceId`: it is assigned by DHCP, it changes with every network the
//! machine joins, several can exist on one interface at once, and two machines
//! on different networks routinely hold the same one. It is shown because a
//! user recognises `192.168.1.41` as "my laptop on my network", and for
//! nothing else.
//!
//! One dump returns every address on every interface, so this costs one
//! transaction per refresh rather than one per interface.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::netlink::{Attributes, Messages};

/// `RTM_GETADDR`.
pub const RTM_GETADDR: u16 = 22;
/// `RTM_NEWADDR` — the message type each dump entry carries.
pub const RTM_NEWADDR: u16 = 20;

/// The size of `struct ifaddrmsg`.
pub const IFADDRMSG_LEN: usize = 8;

/// `IFA_ADDRESS` — the peer address on a point-to-point link, and the local
/// one everywhere else.
const IFA_ADDRESS: u16 = 1;
/// `IFA_LOCAL` — the local address. Present on IPv4, absent on IPv6.
const IFA_LOCAL: u16 = 2;

/// `AF_INET`.
pub const AF_INET: u8 = 2;
/// `AF_INET6`.
pub const AF_INET6: u8 = 10;

/// `RT_SCOPE_LINK` — a link-local address.
const RT_SCOPE_LINK: u8 = 253;
/// `RT_SCOPE_HOST` — a loopback address.
const RT_SCOPE_HOST: u8 = 254;

/// One address, as `RTM_GETADDR` described it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressEntry {
    /// The interface index it belongs to.
    pub index: i32,
    pub family: u8,
    pub prefix_len: u8,
    pub scope: u8,
    /// The address in its canonical textual form.
    pub address: String,
}

impl AddressEntry {
    /// Whether this address is worth showing a user.
    ///
    /// Link-local and host-scoped addresses are filtered: `fe80::…` exists on
    /// every IPv6-capable interface and tells the user nothing about which
    /// network they are on, and a host-scoped address is loopback. A machine
    /// with three bridges would otherwise show four `fe80::` addresses that
    /// all look alike.
    pub const fn is_user_facing(&self) -> bool {
        self.scope != RT_SCOPE_LINK && self.scope != RT_SCOPE_HOST
    }
}

/// Formats four bytes as dotted-quad.
pub fn format_ipv4(bytes: &[u8]) -> Option<String> {
    let octets: [u8; 4] = bytes.get(..4)?.try_into().ok()?;

    Some(format!(
        "{}.{}.{}.{}",
        octets[0], octets[1], octets[2], octets[3]
    ))
}

/// Formats sixteen bytes as a compressed IPv6 address.
///
/// Implements the RFC 5952 rules PULSE needs: lowercase hex, leading zeros in
/// each group dropped, and the **longest** run of two or more zero groups
/// replaced by `::`. Without the compression a perfectly ordinary address
/// renders as `2a02:842a:869f:b301:0000:0000:0000:0001`, which overflows the
/// card's line and is not what any other tool shows.
pub fn format_ipv6(bytes: &[u8]) -> Option<String> {
    let raw: [u8; 16] = bytes.get(..16)?.try_into().ok()?;

    let groups: [u16; 8] =
        std::array::from_fn(|index| u16::from_be_bytes([raw[index * 2], raw[index * 2 + 1]]));

    // Find the longest run of zero groups, preferring the leftmost on a tie,
    // as the RFC requires.
    let mut best_start = 0_usize;
    let mut best_len = 0_usize;
    let mut index = 0_usize;

    while index < groups.len() {
        if groups[index] != 0 {
            index += 1;
            continue;
        }

        let start = index;
        while index < groups.len() && groups[index] == 0 {
            index += 1;
        }

        if index - start > best_len {
            best_start = start;
            best_len = index - start;
        }
    }

    // A single zero group is written out; `::` is only for runs of two or more.
    if best_len < 2 {
        best_len = 0;
    }

    let mut out = String::with_capacity(39);
    let mut group = 0_usize;

    while group < groups.len() {
        if best_len > 0 && group == best_start {
            out.push_str("::");
            group += best_len;
            continue;
        }

        if !out.is_empty() && !out.ends_with(':') {
            out.push(':');
        }
        let _ = write!(out, "{:x}", groups[group]);
        group += 1;
    }

    Some(out)
}

/// Parses one `RTM_NEWADDR` message payload.
pub fn parse_address(payload: &[u8]) -> Option<AddressEntry> {
    let header = payload.get(..IFADDRMSG_LEN)?;

    let family = header[0];
    let prefix_len = header[1];
    let scope = header[3];
    let index = i32::from_ne_bytes(header[4..8].try_into().ok()?);

    let attributes = Attributes::new(&payload[IFADDRMSG_LEN..]);

    // IPv4 carries the local address in `IFA_LOCAL`; IPv6 uses `IFA_ADDRESS`.
    // Preferring `IFA_LOCAL` matters on a point-to-point link, where
    // `IFA_ADDRESS` is the *peer's* address — showing a VPN's far end as this
    // machine's own address would be actively misleading.
    let raw = Attributes::new(&payload[IFADDRMSG_LEN..])
        .find(IFA_LOCAL)
        .or_else(|| attributes.find(IFA_ADDRESS))?;

    let address = match family {
        AF_INET => format_ipv4(raw.payload)?,
        AF_INET6 => format_ipv6(raw.payload)?,
        // A family PULSE does not render.
        _ => return None,
    };

    Some(AddressEntry {
        index,
        family,
        prefix_len,
        scope,
        address,
    })
}

/// Parses a whole `RTM_GETADDR` dump, grouped by interface index.
///
/// Only user-facing addresses are kept, IPv4 first — which is what a user
/// looks for — and then in the order the kernel listed them.
pub fn parse_dump(buffer: &[u8]) -> BTreeMap<i32, Vec<String>> {
    let mut entries: Vec<AddressEntry> = Messages::new(buffer)
        .filter(|message| message.header.message_type == RTM_NEWADDR)
        .filter_map(|message| parse_address(message.payload))
        .filter(AddressEntry::is_user_facing)
        .collect();

    // Stable sort: IPv4 before IPv6, otherwise the kernel's order.
    entries.sort_by_key(|entry| u8::from(entry.family != AF_INET));

    let mut grouped: BTreeMap<i32, Vec<String>> = BTreeMap::new();
    for entry in entries {
        let addresses = grouped.entry(entry.index).or_default();
        if !addresses.contains(&entry.address) {
            addresses.push(entry.address);
        }
    }

    grouped
}

/// Builds the `ifaddrmsg` payload of a dump request. All zeros asks for every
/// family on every interface.
pub fn dump_request() -> Vec<u8> {
    vec![0_u8; IFADDRMSG_LEN]
}

/// Reads every local address from the kernel, in one transaction.
#[cfg(target_os = "linux")]
pub fn dump() -> Result<BTreeMap<i32, Vec<String>>, crate::metrics::model::MetricError> {
    use super::netlink::NLM_F_DUMP;
    use super::socket::{NetlinkSocket, NETLINK_ROUTE};

    let mut socket = NetlinkSocket::open(NETLINK_ROUTE)?;
    let reply = socket.request(
        RTM_GETADDR,
        NLM_F_DUMP,
        &dump_request(),
        "the network address dump",
    )?;

    Ok(parse_dump(&reply))
}

/// Compiled on non-Linux hosts so the Windows cross-check type checks this
/// module.
#[cfg(not(target_os = "linux"))]
pub fn dump() -> Result<BTreeMap<i32, Vec<String>>, crate::metrics::model::MetricError> {
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
    use crate::platform::linux::network::netlink::NLM_F_MULTI;

    /// `RT_SCOPE_UNIVERSE` — a globally routable address.
    pub const SCOPE_UNIVERSE: u8 = 0;
    /// `RT_SCOPE_LINK`.
    pub const SCOPE_LINK: u8 = 253;
    /// `RT_SCOPE_HOST`.
    pub const SCOPE_HOST: u8 = 254;

    /// Builds one `RTM_NEWADDR` message.
    pub fn address(index: i32, family: u8, scope: u8, raw: &[u8], use_local: bool) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.push(family);
        payload.push(if family == AF_INET { 24 } else { 64 });
        payload.push(0); // flags
        payload.push(scope);
        payload.extend_from_slice(&index.to_ne_bytes());

        if use_local {
            payload.extend(nl::attribute(IFA_LOCAL, raw));
        }
        payload.extend(nl::attribute(IFA_ADDRESS, raw));

        nl::message(RTM_NEWADDR, NLM_F_MULTI, 1, &payload)
    }

    /// The development machine's Wi-Fi addresses.
    pub fn real_dump() -> Vec<u8> {
        let mut buffer = Vec::new();

        // 192.168.1.41 on index 3.
        buffer.extend(address(
            3,
            AF_INET,
            SCOPE_UNIVERSE,
            &[192, 168, 1, 41],
            true,
        ));
        // A global IPv6 on the same interface.
        buffer.extend(address(
            3,
            AF_INET6,
            SCOPE_UNIVERSE,
            &[
                0x2a, 0x02, 0x84, 0x2a, 0x86, 0x9f, 0xb3, 0x01, 0xfd, 0x27, 0x96, 0xa7, 0xb8, 0x24,
                0xf1, 0x6f,
            ],
            false,
        ));
        // A link-local IPv6, which is filtered.
        buffer.extend(address(
            3,
            AF_INET6,
            SCOPE_LINK,
            &[
                0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0xe3, 0xdb, 0xf3, 0x68, 0x4d, 0x20, 0xa1, 0x5e,
            ],
            false,
        ));
        // Loopback, which is filtered by scope.
        buffer.extend(address(1, AF_INET, SCOPE_HOST, &[127, 0, 0, 1], true));
        // A docker bridge.
        buffer.extend(address(4, AF_INET, SCOPE_UNIVERSE, &[172, 17, 0, 1], true));

        buffer.extend(nl::done(1));
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    // --- formatting --------------------------------------------------------

    #[test]
    fn formats_a_dotted_quad() {
        assert_eq!(
            format_ipv4(&[192, 168, 1, 41]).as_deref(),
            Some("192.168.1.41")
        );
        assert_eq!(format_ipv4(&[10, 0, 0, 1]).as_deref(), Some("10.0.0.1"));
        assert_eq!(
            format_ipv4(&[255, 255, 255, 255]).as_deref(),
            Some("255.255.255.255")
        );
    }

    #[test]
    fn refuses_a_short_ipv4_buffer() {
        assert_eq!(format_ipv4(&[192, 168]), None);
        assert_eq!(format_ipv4(&[]), None);
    }

    #[test]
    fn formats_an_ipv6_address_the_way_every_other_tool_does() {
        let bytes = [
            0x2a, 0x02, 0x84, 0x2a, 0x86, 0x9f, 0xb3, 0x01, 0xfd, 0x27, 0x96, 0xa7, 0xb8, 0x24,
            0xf1, 0x6f,
        ];

        assert_eq!(
            format_ipv6(&bytes).as_deref(),
            Some("2a02:842a:869f:b301:fd27:96a7:b824:f16f")
        );
    }

    #[test]
    fn compresses_the_longest_run_of_zero_groups() {
        // Without this, an ordinary address overflows the card's line.
        let mut bytes = [0_u8; 16];
        bytes[0] = 0x2a;
        bytes[1] = 0x02;
        bytes[15] = 0x01;
        assert_eq!(format_ipv6(&bytes).as_deref(), Some("2a02::1"));

        assert_eq!(format_ipv6(&[0_u8; 16]).as_deref(), Some("::"));

        let mut loopback = [0_u8; 16];
        loopback[15] = 1;
        assert_eq!(format_ipv6(&loopback).as_deref(), Some("::1"));
    }

    #[test]
    fn a_single_zero_group_is_written_out_rather_than_compressed() {
        // RFC 5952: `::` is only for runs of two or more.
        let bytes = [
            0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01,
            0x00, 0x01,
        ];

        assert_eq!(format_ipv6(&bytes).as_deref(), Some("2001:db8:0:1:1:1:1:1"));
    }

    #[test]
    fn the_leftmost_of_two_equal_runs_is_compressed() {
        let bytes = [
            0x20, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x01,
        ];

        assert_eq!(format_ipv6(&bytes).as_deref(), Some("2001::1:0:0:1:1"));
    }

    #[test]
    fn a_link_local_address_formats_correctly() {
        let bytes = [
            0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0xe3, 0xdb, 0xf3, 0x68, 0x4d, 0x20, 0xa1, 0x5e,
        ];

        assert_eq!(
            format_ipv6(&bytes).as_deref(),
            Some("fe80::e3db:f368:4d20:a15e")
        );
    }

    #[test]
    fn refuses_a_short_ipv6_buffer() {
        assert_eq!(format_ipv6(&[0x2a, 0x02]), None);
        assert_eq!(format_ipv6(&[]), None);
    }

    // --- parsing -----------------------------------------------------------

    #[test]
    fn parses_an_ipv4_address_from_its_local_attribute() {
        let buffer = address(3, AF_INET, SCOPE_UNIVERSE, &[192, 168, 1, 41], true);
        let message = Messages::new(&buffer).next().expect("one");
        let entry = parse_address(message.payload).expect("parsed");

        assert_eq!(entry.index, 3);
        assert_eq!(entry.family, AF_INET);
        assert_eq!(entry.address, "192.168.1.41");
        assert_eq!(entry.prefix_len, 24);
        assert!(entry.is_user_facing());
    }

    #[test]
    fn a_point_to_point_link_reports_its_own_address_not_its_peers() {
        // On a VPN, `IFA_ADDRESS` is the far end. Showing it as this machine's
        // address would be actively misleading, so `IFA_LOCAL` wins.
        let mut payload = vec![AF_INET, 32, 0, SCOPE_UNIVERSE];
        payload.extend_from_slice(&6_i32.to_ne_bytes());
        payload.extend(
            crate::platform::linux::network::netlink::fixtures::attribute(
                IFA_LOCAL,
                &[10, 8, 0, 2],
            ),
        );
        payload.extend(
            crate::platform::linux::network::netlink::fixtures::attribute(
                IFA_ADDRESS,
                &[10, 8, 0, 1],
            ),
        );

        let entry = parse_address(&payload).expect("parsed");
        assert_eq!(entry.address, "10.8.0.2", "the local end, not the peer");
    }

    #[test]
    fn an_ipv6_address_falls_back_to_the_address_attribute() {
        let buffer = address(
            3,
            AF_INET6,
            SCOPE_UNIVERSE,
            &[
                0x2a, 0x02, 0x84, 0x2a, 0x86, 0x9f, 0xb3, 0x01, 0xfd, 0x27, 0x96, 0xa7, 0xb8, 0x24,
                0xf1, 0x6f,
            ],
            false,
        );
        let message = Messages::new(&buffer).next().expect("one");
        let entry = parse_address(message.payload).expect("parsed");

        assert_eq!(entry.address, "2a02:842a:869f:b301:fd27:96a7:b824:f16f");
        assert_eq!(entry.prefix_len, 64);
    }

    #[test]
    fn a_truncated_header_is_refused() {
        assert_eq!(parse_address(&[]), None);
        assert_eq!(parse_address(&[AF_INET, 24]), None);
    }

    #[test]
    fn an_address_with_no_attributes_is_refused() {
        assert_eq!(parse_address(&[AF_INET, 24, 0, 0, 1, 0, 0, 0]), None);
    }

    #[test]
    fn an_unrecognised_address_family_is_skipped() {
        let mut payload = vec![99, 24, 0, SCOPE_UNIVERSE];
        payload.extend_from_slice(&3_i32.to_ne_bytes());
        payload.extend(
            crate::platform::linux::network::netlink::fixtures::attribute(IFA_LOCAL, &[1, 2, 3, 4]),
        );

        assert_eq!(parse_address(&payload), None);
    }

    // --- the dump ----------------------------------------------------------

    #[test]
    fn groups_the_real_dump_by_interface_and_filters_the_noise() {
        let grouped = parse_dump(&real_dump());

        // Wi-Fi keeps its IPv4 and its global IPv6; the `fe80::` is filtered.
        assert_eq!(
            grouped[&3],
            vec![
                "192.168.1.41".to_string(),
                "2a02:842a:869f:b301:fd27:96a7:b824:f16f".to_string()
            ]
        );
        // The docker bridge keeps its address.
        assert_eq!(grouped[&4], vec!["172.17.0.1".to_string()]);
        // Loopback is filtered by scope.
        assert!(!grouped.contains_key(&1));
    }

    #[test]
    fn ipv4_is_listed_before_ipv6() {
        // It is what a user looks for first.
        let grouped = parse_dump(&real_dump());
        let wifi = &grouped[&3];

        assert!(wifi[0].contains('.'), "IPv4 first: {:?}", wifi);
        assert!(wifi[1].contains(':'), "IPv6 second: {:?}", wifi);
    }

    #[test]
    fn a_link_local_address_is_never_shown() {
        // Every IPv6-capable interface has one, and it tells the user nothing
        // about which network they are on.
        let buffer = address(
            3,
            AF_INET6,
            SCOPE_LINK,
            &[0xfe, 0x80, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
            false,
        );
        let message = Messages::new(&buffer).next().expect("one");
        let entry = parse_address(message.payload).expect("parsed");

        assert!(!entry.is_user_facing());
        assert!(parse_dump(&buffer).is_empty());
    }

    #[test]
    fn duplicate_addresses_are_listed_once() {
        let mut buffer = address(3, AF_INET, SCOPE_UNIVERSE, &[192, 168, 1, 41], true);
        buffer.extend(address(
            3,
            AF_INET,
            SCOPE_UNIVERSE,
            &[192, 168, 1, 41],
            true,
        ));
        buffer.extend(crate::platform::linux::network::netlink::fixtures::done(1));

        assert_eq!(parse_dump(&buffer)[&3].len(), 1);
    }

    #[test]
    fn an_empty_dump_yields_nothing_rather_than_failing() {
        assert!(parse_dump(&[]).is_empty());
        assert!(
            parse_dump(&crate::platform::linux::network::netlink::fixtures::done(1)).is_empty()
        );
    }

    #[test]
    fn the_dump_request_asks_for_every_family() {
        let request = dump_request();

        assert_eq!(request.len(), IFADDRMSG_LEN);
        assert!(request.iter().all(|&byte| byte == 0));
    }

    // --- against the real kernel -------------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_kernel_answers_an_address_dump() {
        let grouped = dump().expect("the kernel answers");

        // Every address the parser accepted renders as something.
        for addresses in grouped.values() {
            for address in addresses {
                assert!(!address.is_empty());
                assert!(address.contains('.') || address.contains(':'));
            }
        }
    }
}
