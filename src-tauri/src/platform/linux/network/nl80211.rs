//! `nl80211` — which interfaces are wireless, and what their link is doing.
//!
//! PULSE runs no `iw`, `iwconfig` or `nmcli`. Each of those opens the same
//! generic netlink family and sends the same two commands; spawning one per
//! refresh would add a runtime dependency on a package Fedora does not always
//! install, a human-readable output format to parse, and a process.
//!
//! # Two questions, two commands
//!
//! ```text
//! NL80211_CMD_GET_INTERFACE  which interface indices are wireless at all
//! NL80211_CMD_GET_STATION    signal and negotiated rates for one of them
//! ```
//!
//! The first matters more than it looks. **A Wi-Fi interface is not
//! identifiable from its name or its link type**: `wlan0` is a convention that
//! nothing enforces, a bridge can be called `eth0`, and every Wi-Fi station
//! reports `ARPHRD_ETHER` exactly like a wired NIC. Asking `nl80211` for its
//! own list of interfaces is the only way to know, and it costs one dump.
//!
//! # Generic netlink needs a family lookup first
//!
//! `nl80211` has no fixed protocol number. Its family id is assigned when the
//! module loads, so it must be resolved through the controller family — which
//! *does* have a fixed id — before anything else. A machine with no wireless
//! hardware has no `nl80211` family at all, and the lookup failing there is
//! the normal answer rather than an error worth surfacing.
//!
//! # Bitrates are in units of 100 kbit/s
//!
//! `NL80211_RATE_INFO_BITRATE32` reports `1755` for a 175.5 Mbit/s link. The
//! obvious misreadings — treating it as Mbit/s, or as bit/s — are out by a
//! factor of 100 000 in one direction or 10 in the other, and both produce a
//! number that looks superficially plausible. The conversion happens once,
//! here, and is tested.

use crate::metrics::model::MetricError;
use crate::metrics::wellknown::network::{WifiLink, WifiLinkInfo};

use super::netlink::{Attributes, Messages};

/// The size of `struct genlmsghdr`.
pub const GENLMSGHDR_LEN: usize = 4;

/// `GENL_ID_CTRL` — the controller family, the only one with a fixed id.
pub const GENL_ID_CTRL: u16 = 0x10;

// The constants and builders below are used only by the Linux implementation
// at the bottom of this file. They are compiled everywhere so the Windows
// cross-check type checks them, which makes them dead there and nowhere else.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// `CTRL_CMD_GETFAMILY`.
const CTRL_CMD_GETFAMILY: u8 = 3;
/// `CTRL_ATTR_FAMILY_ID`.
const CTRL_ATTR_FAMILY_ID: u16 = 1;
/// `CTRL_ATTR_FAMILY_NAME`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const CTRL_ATTR_FAMILY_NAME: u16 = 2;

/// The family PULSE resolves.
pub const NL80211_FAMILY: &str = "nl80211";

/// `NL80211_CMD_GET_INTERFACE`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const NL80211_CMD_GET_INTERFACE: u8 = 5;
/// `NL80211_CMD_GET_STATION`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const NL80211_CMD_GET_STATION: u8 = 17;

/// `NL80211_ATTR_IFINDEX`.
const NL80211_ATTR_IFINDEX: u16 = 3;
/// `NL80211_ATTR_STA_INFO`.
const NL80211_ATTR_STA_INFO: u16 = 21;

/// `NL80211_STA_INFO_SIGNAL` — the current signal, in dBm, as a signed byte.
const NL80211_STA_INFO_SIGNAL: u16 = 7;
/// `NL80211_STA_INFO_TX_BITRATE` — nested.
const NL80211_STA_INFO_TX_BITRATE: u16 = 8;
/// `NL80211_STA_INFO_SIGNAL_AVG` — the driver's smoothed average, in dBm.
const NL80211_STA_INFO_SIGNAL_AVG: u16 = 13;
/// `NL80211_STA_INFO_RX_BITRATE` — nested.
const NL80211_STA_INFO_RX_BITRATE: u16 = 14;

/// `NL80211_RATE_INFO_BITRATE` — 16-bit, in units of 100 kbit/s.
///
/// Saturates at 6.5 Gbit/s, which modern links exceed, so the 32-bit form is
/// preferred whenever the driver reports it.
const NL80211_RATE_INFO_BITRATE: u16 = 1;
/// `NL80211_RATE_INFO_BITRATE32` — 32-bit, in units of 100 kbit/s.
const NL80211_RATE_INFO_BITRATE32: u16 = 5;

/// One unit of `NL80211_RATE_INFO_BITRATE*`, in bits per second.
pub const RATE_UNIT_BPS: u64 = 100_000;

/// Converts a `nl80211` rate figure into bits per second.
///
/// The attribute counts **hundreds of kilobits per second**: `1755` is
/// 175.5 Mbit/s. Returns `None` for zero, which the kernel uses where it has
/// no rate to report.
pub fn rate_to_bps(units: u32) -> Option<u64> {
    (units > 0).then(|| u64::from(units) * RATE_UNIT_BPS)
}

/// Parses a nested `NL80211_STA_INFO_*_BITRATE` attribute.
///
/// Prefers the 32-bit field: the 16-bit one saturates at 6.5 Gbit/s, which a
/// Wi-Fi 6E or 7 link exceeds, and a driver reporting both fills them
/// consistently.
pub fn parse_bitrate(payload: &[u8]) -> Option<u64> {
    let attributes = || Attributes::new(payload);

    let units = attributes()
        .find(NL80211_RATE_INFO_BITRATE32)
        .and_then(|attribute| attribute.as_u32())
        .or_else(|| {
            attributes()
                .find(NL80211_RATE_INFO_BITRATE)
                .and_then(|attribute| attribute.as_u16())
                .map(u32::from)
        })?;

    rate_to_bps(units)
}

/// Parses one `NL80211_ATTR_STA_INFO` payload into a link.
pub fn parse_station_info(payload: &[u8]) -> WifiLink {
    let attributes = || Attributes::new(payload);

    WifiLink {
        rssi_dbm: attributes()
            .find(NL80211_STA_INFO_SIGNAL)
            .and_then(|attribute| attribute.as_i8())
            .map(i32::from),
        rssi_average_dbm: attributes()
            .find(NL80211_STA_INFO_SIGNAL_AVG)
            .and_then(|attribute| attribute.as_i8())
            .map(i32::from),
        receive_bps: attributes()
            .find(NL80211_STA_INFO_RX_BITRATE)
            .and_then(|attribute| parse_bitrate(attribute.payload)),
        transmit_bps: attributes()
            .find(NL80211_STA_INFO_TX_BITRATE)
            .and_then(|attribute| parse_bitrate(attribute.payload)),
    }
}

/// Parses a `NL80211_CMD_GET_STATION` dump into link information.
///
/// # One station per link
///
/// A station interface in managed mode is associated with one access point,
/// so the dump normally returns one station. A radio using **multi-link
/// operation** reports one per link, and PULSE keeps them all — the shared
/// `WifiLinkInfo` then publishes the strongest, because dBm values cannot be
/// averaged. Taking the first station would report whichever link the driver
/// happened to list first.
///
/// `quality_percent` is always `None` here: `cfg80211` reports dBm and nothing
/// else, and PULSE does not invent a percentage from it.
pub fn parse_stations(buffer: &[u8]) -> WifiLinkInfo {
    let links: Vec<WifiLink> = Messages::new(buffer)
        .filter_map(|message| {
            let payload = message.payload.get(GENLMSGHDR_LEN..)?;
            let station = Attributes::new(payload).find(NL80211_ATTR_STA_INFO)?;

            Some(parse_station_info(station.payload))
        })
        .filter(|link| !link.is_empty())
        .collect();

    WifiLinkInfo {
        links,
        quality_percent: None,
    }
}

/// Parses a `NL80211_CMD_GET_INTERFACE` dump into the set of wireless
/// interface indices.
pub fn parse_wireless_indices(buffer: &[u8]) -> Vec<i32> {
    let mut indices: Vec<i32> = Messages::new(buffer)
        .filter_map(|message| {
            let payload = message.payload.get(GENLMSGHDR_LEN..)?;

            Attributes::new(payload)
                .find(NL80211_ATTR_IFINDEX)
                .and_then(|attribute| attribute.as_i32())
        })
        .collect();

    indices.sort_unstable();
    indices.dedup();
    indices
}

/// Parses the controller's reply to a family lookup.
pub fn parse_family_id(buffer: &[u8]) -> Option<u16> {
    Messages::new(buffer).find_map(|message| {
        let payload = message.payload.get(GENLMSGHDR_LEN..)?;

        Attributes::new(payload)
            .find(CTRL_ATTR_FAMILY_ID)
            .and_then(|attribute| attribute.as_u16())
    })
}

/// Builds a generic netlink payload: the 4-byte header plus attributes.
pub fn genl_payload(command: u8, version: u8, attributes: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(GENLMSGHDR_LEN + attributes.len());

    payload.push(command);
    payload.push(version);
    payload.extend_from_slice(&0_u16.to_ne_bytes());
    payload.extend_from_slice(attributes);

    payload
}

/// Builds the attribute asking the controller for a family by name.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn family_name_attribute(name: &str) -> Vec<u8> {
    let mut value = name.as_bytes().to_vec();
    value.push(0);

    let length = super::netlink::ATTR_HEADER_LEN + value.len();
    let mut attribute = Vec::with_capacity(super::netlink::align(length));
    attribute.extend_from_slice(&(length as u16).to_ne_bytes());
    attribute.extend_from_slice(&CTRL_ATTR_FAMILY_NAME.to_ne_bytes());
    attribute.extend_from_slice(&value);
    attribute.resize(super::netlink::align(length), 0);

    attribute
}

/// Builds the attribute naming one interface index.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn ifindex_attribute(index: i32) -> Vec<u8> {
    let length = super::netlink::ATTR_HEADER_LEN + 4;
    let mut attribute = Vec::with_capacity(length);

    attribute.extend_from_slice(&(length as u16).to_ne_bytes());
    attribute.extend_from_slice(&NL80211_ATTR_IFINDEX.to_ne_bytes());
    attribute.extend_from_slice(&index.to_ne_bytes());

    attribute
}

// --- talking to the kernel -------------------------------------------------

/// What one machine's `nl80211` can answer.
///
/// Resolving the family is the expensive part — one round trip — so the
/// handle is built once per refresh and reused for every wireless interface.
#[derive(Debug, Clone, Copy)]
pub struct Nl80211 {
    family_id: u16,
}

impl Nl80211 {
    /// The resolved family id.
    pub const fn family_id(self) -> u16 {
        self.family_id
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::platform::linux::network::netlink::NLM_F_DUMP;
    use crate::platform::linux::network::socket::{NetlinkSocket, NETLINK_GENERIC};

    impl Nl80211 {
        /// Resolves the `nl80211` family, if this kernel has one.
        ///
        /// A machine with no wireless hardware has no such family, and the
        /// lookup failing is the normal answer — the caller treats it as "no
        /// Wi-Fi here" rather than surfacing an error.
        pub fn resolve() -> Result<Self, MetricError> {
            let mut socket = NetlinkSocket::open(NETLINK_GENERIC)?;

            let reply = socket.request(
                GENL_ID_CTRL,
                0,
                &genl_payload(
                    CTRL_CMD_GETFAMILY,
                    1,
                    &family_name_attribute(NL80211_FAMILY),
                ),
                "the nl80211 family lookup",
            )?;

            parse_family_id(&reply)
                .map(|family_id| Self { family_id })
                .ok_or_else(|| {
                    MetricError::new(
                        crate::metrics::model::MetricErrorCode::NotDetected,
                        "this kernel has no nl80211 family, so it has no wireless interfaces",
                    )
                })
        }

        /// The interface indices `nl80211` considers wireless.
        pub fn wireless_indices(self) -> Result<Vec<i32>, MetricError> {
            let mut socket = NetlinkSocket::open(NETLINK_GENERIC)?;

            let reply = socket.request(
                self.family_id,
                NLM_F_DUMP,
                &genl_payload(NL80211_CMD_GET_INTERFACE, 0, &[]),
                "the wireless interface dump",
            )?;

            Ok(parse_wireless_indices(&reply))
        }

        /// One wireless interface's link information.
        pub fn station(self, index: i32) -> Result<WifiLinkInfo, MetricError> {
            let mut socket = NetlinkSocket::open(NETLINK_GENERIC)?;

            let reply = socket.request(
                self.family_id,
                NLM_F_DUMP,
                &genl_payload(NL80211_CMD_GET_STATION, 0, &ifindex_attribute(index)),
                "the wireless station dump",
            )?;

            Ok(parse_stations(&reply))
        }
    }
}

/// Compiled on non-Linux hosts so the Windows cross-check type checks this
/// module. Never reached: only the Linux provider calls it.
#[cfg(not(target_os = "linux"))]
impl Nl80211 {
    pub fn resolve() -> Result<Self, MetricError> {
        Err(MetricError::new(
            crate::metrics::model::MetricErrorCode::Unsupported,
            "nl80211 is a Linux interface",
        ))
    }

    pub fn wireless_indices(self) -> Result<Vec<i32>, MetricError> {
        Ok(Vec::new())
    }

    pub fn station(self, _index: i32) -> Result<WifiLinkInfo, MetricError> {
        Ok(WifiLinkInfo::disconnected())
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::platform::linux::network::netlink::fixtures as nl;
    use crate::platform::linux::network::netlink::{NLA_F_NESTED, NLM_F_MULTI};

    /// Builds a nested bitrate attribute using the 32-bit field.
    pub fn bitrate32(kind: u16, units: u32) -> Vec<u8> {
        nl::attribute(
            kind | NLA_F_NESTED,
            &nl::u32_attribute(NL80211_RATE_INFO_BITRATE32, units),
        )
    }

    /// Builds a nested bitrate attribute using only the legacy 16-bit field.
    pub fn bitrate16(kind: u16, units: u16) -> Vec<u8> {
        nl::attribute(
            kind | NLA_F_NESTED,
            &nl::attribute(NL80211_RATE_INFO_BITRATE, &units.to_ne_bytes()),
        )
    }

    /// Builds one station's `NL80211_ATTR_STA_INFO` payload.
    pub fn station_info(
        signal: Option<i8>,
        signal_avg: Option<i8>,
        rx_units: Option<u32>,
        tx_units: Option<u32>,
    ) -> Vec<u8> {
        let mut payload = Vec::new();

        if let Some(signal) = signal {
            payload.extend(nl::attribute(NL80211_STA_INFO_SIGNAL, &[signal as u8]));
        }
        if let Some(average) = signal_avg {
            payload.extend(nl::attribute(NL80211_STA_INFO_SIGNAL_AVG, &[average as u8]));
        }
        if let Some(units) = rx_units {
            payload.extend(bitrate32(NL80211_STA_INFO_RX_BITRATE, units));
        }
        if let Some(units) = tx_units {
            payload.extend(bitrate32(NL80211_STA_INFO_TX_BITRATE, units));
        }

        payload
    }

    /// Builds one `NL80211_CMD_NEW_STATION` message.
    pub fn station_message(info: &[u8]) -> Vec<u8> {
        let mut attributes = nl::u32_attribute(NL80211_ATTR_IFINDEX, 3);
        attributes.extend(nl::attribute(NL80211_ATTR_STA_INFO | NLA_F_NESTED, info));

        nl::message(
            // `NL80211_CMD_NEW_STATION`; the message type is the family id,
            // which the parser does not filter on.
            19,
            NLM_F_MULTI,
            1,
            &genl_payload(19, 0, &attributes),
        )
    }

    /// Builds a whole station dump.
    pub fn station_dump(stations: &[Vec<u8>]) -> Vec<u8> {
        let mut buffer = Vec::new();
        for info in stations {
            buffer.extend(station_message(info));
        }
        buffer.extend(nl::done(1));
        buffer
    }

    /// The development machine's real association.
    pub fn real_station() -> Vec<u8> {
        station_info(Some(-68), Some(-67), Some(1755), Some(3900))
    }

    /// Builds a `NL80211_CMD_NEW_INTERFACE` dump listing wireless indices.
    pub fn interface_dump(indices: &[i32]) -> Vec<u8> {
        let mut buffer = Vec::new();

        for &index in indices {
            let attributes = nl::u32_attribute(NL80211_ATTR_IFINDEX, index as u32);
            buffer.extend(nl::message(
                7, // NL80211_CMD_NEW_INTERFACE
                NLM_F_MULTI,
                1,
                &genl_payload(7, 0, &attributes),
            ));
        }

        buffer.extend(nl::done(1));
        buffer
    }

    /// Builds the controller's reply to a family lookup.
    pub fn family_reply(family_id: u16) -> Vec<u8> {
        let attributes = nl::attribute(CTRL_ATTR_FAMILY_ID, &family_id.to_ne_bytes());

        nl::message(GENL_ID_CTRL, 0, 1, &genl_payload(1, 2, &attributes))
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    // --- the rate unit -----------------------------------------------------

    #[test]
    fn a_bitrate_unit_is_a_hundred_kilobits_per_second() {
        // `1755` is 175.5 Mbit/s. Reading it as Mbit/s or as bit/s is out by
        // a factor of 100 000 or 10, and both look plausible on screen.
        assert_eq!(rate_to_bps(1755), Some(175_500_000));
        assert_eq!(rate_to_bps(3900), Some(390_000_000));
        assert_eq!(rate_to_bps(10), Some(1_000_000));
        assert_eq!(rate_to_bps(12_000), Some(1_200_000_000));
    }

    #[test]
    fn a_zero_rate_means_the_kernel_has_none_to_report() {
        assert_eq!(rate_to_bps(0), None);
    }

    #[test]
    fn a_very_large_rate_unit_does_not_overflow() {
        let bps = rate_to_bps(u32::MAX).expect("some");
        assert_eq!(bps, u64::from(u32::MAX) * RATE_UNIT_BPS);
    }

    // --- bitrate attributes ------------------------------------------------

    #[test]
    fn prefers_the_32_bit_bitrate_field() {
        // The 16-bit field saturates at 6.5 Gbit/s, which Wi-Fi 6E and 7
        // exceed.
        let buffer = bitrate32(NL80211_STA_INFO_RX_BITRATE, 1755);
        let attribute = Attributes::new(&buffer)
            .find(NL80211_STA_INFO_RX_BITRATE)
            .expect("found");

        assert_eq!(parse_bitrate(attribute.payload), Some(175_500_000));
    }

    #[test]
    fn falls_back_to_the_legacy_16_bit_field() {
        let buffer = bitrate16(NL80211_STA_INFO_TX_BITRATE, 650);
        let attribute = Attributes::new(&buffer)
            .find(NL80211_STA_INFO_TX_BITRATE)
            .expect("found");

        assert_eq!(parse_bitrate(attribute.payload), Some(65_000_000));
    }

    #[test]
    fn a_bitrate_attribute_with_neither_field_reports_none() {
        assert_eq!(parse_bitrate(&[]), None);
        // A nested block carrying only fields PULSE does not read.
        let other = crate::platform::linux::network::netlink::fixtures::u32_attribute(99, 7);
        assert_eq!(parse_bitrate(&other), None);
    }

    // --- station info ------------------------------------------------------

    #[test]
    fn parses_the_real_association() {
        let link = parse_station_info(&real_station());

        assert_eq!(link.rssi_dbm, Some(-68));
        assert_eq!(link.rssi_average_dbm, Some(-67));
        assert_eq!(link.receive_bps, Some(175_500_000));
        assert_eq!(link.transmit_bps, Some(390_000_000));
        assert!(!link.is_empty());
    }

    #[test]
    fn a_signal_is_read_as_a_signed_byte() {
        // The single easiest mistake here: 0xBC is −68 dBm, not 188.
        let link = parse_station_info(&station_info(Some(-68), None, None, None));
        assert_eq!(link.rssi_dbm, Some(-68));

        let weak = parse_station_info(&station_info(Some(-90), None, None, None));
        assert_eq!(weak.rssi_dbm, Some(-90));

        let strong = parse_station_info(&station_info(Some(-40), None, None, None));
        assert_eq!(strong.rssi_dbm, Some(-40));
    }

    #[test]
    fn a_station_with_no_bitrate_still_reports_its_signal() {
        let link = parse_station_info(&station_info(Some(-55), Some(-56), None, None));

        assert_eq!(link.rssi_dbm, Some(-55));
        assert_eq!(link.receive_bps, None);
        assert_eq!(link.transmit_bps, None);
        assert!(!link.is_empty());
    }

    #[test]
    fn an_empty_station_info_yields_an_empty_link() {
        let link = parse_station_info(&[]);

        assert!(link.is_empty());
        assert_eq!(link.rssi_dbm, None);
    }

    #[test]
    fn malformed_station_attributes_do_not_panic() {
        // A length below the attribute header, which a naive walker hangs on.
        for payload in [
            vec![0x00, 0x00, 0x07, 0x00],
            vec![0xFF, 0xFF, 0x07, 0x00, 0x01],
            vec![0x02, 0x00],
            vec![0xFF; 7],
        ] {
            let link = parse_station_info(&payload);
            assert!(link.is_empty() || link.rssi_dbm.is_some());
        }
    }

    // --- the station dump --------------------------------------------------

    #[test]
    fn parses_a_single_station_dump() {
        let info = parse_stations(&station_dump(&[real_station()]));

        assert!(info.is_connected());
        assert_eq!(info.link_count(), 1);
        assert_eq!(info.rssi_dbm(), Some(-68.0));
        assert_eq!(info.receive_bps(), Some(175_500_000.0));
        assert_eq!(info.transmit_bps(), Some(390_000_000.0));
    }

    #[test]
    fn a_disconnected_interface_yields_no_links() {
        // The dump returns only the terminating message.
        let buffer = crate::platform::linux::network::netlink::fixtures::done(1);
        let info = parse_stations(&buffer);

        assert!(!info.is_connected());
        assert_eq!(info.link_count(), 0);
        assert_eq!(info.rssi_dbm(), None);
    }

    #[test]
    fn several_stations_become_several_links_and_the_strongest_is_published() {
        // Multi-link operation. Taking the first would report −85 dBm for a
        // radio that also holds a −45 dBm link.
        let info = parse_stations(&station_dump(&[
            station_info(Some(-85), None, Some(1000), Some(1000)),
            station_info(Some(-45), None, Some(9000), Some(9000)),
        ]));

        assert_eq!(info.link_count(), 2);
        assert_eq!(info.rssi_dbm(), Some(-45.0));
        assert_eq!(info.receive_bps(), Some(900_000_000.0));
    }

    #[test]
    fn linux_never_reports_a_quality_percentage() {
        // `cfg80211` reports dBm and nothing else, and PULSE does not invent a
        // percentage from it.
        let info = parse_stations(&station_dump(&[real_station()]));

        assert_eq!(info.quality_percent, None);
        assert_eq!(info.quality_percent(), None);
    }

    #[test]
    fn a_station_message_too_short_for_its_generic_header_is_skipped() {
        let mut buffer = crate::platform::linux::network::netlink::fixtures::message(
            19,
            crate::platform::linux::network::netlink::NLM_F_MULTI,
            1,
            &[0x00, 0x01],
        );
        buffer.extend(station_dump(&[real_station()]));

        let info = parse_stations(&buffer);

        assert_eq!(info.link_count(), 1, "the intact station survives");
    }

    #[test]
    fn an_empty_buffer_yields_a_disconnected_interface() {
        let info = parse_stations(&[]);

        assert!(!info.is_connected());
        assert_eq!(info.rssi_dbm(), None);
    }

    // --- the wireless interface list ---------------------------------------

    #[test]
    fn parses_the_wireless_interface_indices() {
        let indices = parse_wireless_indices(&interface_dump(&[3, 7]));

        assert_eq!(indices, [3, 7]);
    }

    #[test]
    fn duplicate_indices_are_collapsed_and_the_order_is_deterministic() {
        // A radio with several virtual interfaces on one physical device can
        // report the same index more than once.
        let indices = parse_wireless_indices(&interface_dump(&[7, 3, 7, 3]));

        assert_eq!(indices, [3, 7]);
    }

    #[test]
    fn a_machine_with_no_wireless_hardware_reports_no_indices() {
        let buffer = crate::platform::linux::network::netlink::fixtures::done(1);

        assert!(parse_wireless_indices(&buffer).is_empty());
        assert!(parse_wireless_indices(&[]).is_empty());
    }

    // --- the family lookup -------------------------------------------------

    #[test]
    fn parses_the_family_id_from_the_controllers_reply() {
        assert_eq!(parse_family_id(&family_reply(28)), Some(28));
        assert_eq!(parse_family_id(&family_reply(0x1C)), Some(0x1C));
    }

    #[test]
    fn a_reply_with_no_family_id_yields_none() {
        assert_eq!(parse_family_id(&[]), None);
        assert_eq!(
            parse_family_id(&crate::platform::linux::network::netlink::fixtures::done(1)),
            None
        );
    }

    #[test]
    fn the_controller_family_id_is_fixed() {
        // It is the only one that is: `nl80211`'s is assigned at module load.
        assert_eq!(GENL_ID_CTRL, 0x10);
    }

    #[test]
    fn a_generic_payload_carries_its_command_and_version() {
        let payload = genl_payload(NL80211_CMD_GET_STATION, 0, &[1, 2, 3, 4]);

        assert_eq!(payload.len(), GENLMSGHDR_LEN + 4);
        assert_eq!(payload[0], NL80211_CMD_GET_STATION);
        assert_eq!(payload[1], 0);
        assert_eq!(&payload[GENLMSGHDR_LEN..], &[1, 2, 3, 4]);
    }

    #[test]
    fn the_family_name_attribute_is_nul_terminated() {
        // The controller matches on the terminated string.
        let attribute = family_name_attribute(NL80211_FAMILY);
        let parsed = Attributes::new(&attribute)
            .find(CTRL_ATTR_FAMILY_NAME)
            .expect("found");

        assert_eq!(parsed.as_str(), Some("nl80211"));
        assert_eq!(parsed.payload.last(), Some(&0));
    }

    #[test]
    fn the_ifindex_attribute_carries_the_index() {
        let attribute = ifindex_attribute(3);
        let parsed = Attributes::new(&attribute)
            .find(NL80211_ATTR_IFINDEX)
            .expect("found");

        assert_eq!(parsed.as_i32(), Some(3));
    }

    // --- against the real kernel -------------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_kernel_answers_a_wireless_query_or_says_it_has_none() {
        // Both outcomes are correct: a machine with no wireless hardware has
        // no `nl80211` family, and that is an answer rather than a failure.
        match Nl80211::resolve() {
            Ok(nl80211) => {
                assert!(nl80211.family_id() > GENL_ID_CTRL, "a real family id");

                let indices = nl80211.wireless_indices().expect("the dump answers");
                for index in indices {
                    // Every listed interface answers a station query without
                    // panicking, connected or not.
                    let info = nl80211.station(index).expect("the station query answers");
                    if let Some(rssi) = info.rssi_dbm() {
                        assert!(
                            (-120.0..=0.0).contains(&rssi),
                            "implausible RSSI {rssi} on index {index}"
                        );
                    }
                }
            }
            Err(error) => {
                assert!(!error.message.is_empty());
            }
        }
    }
}
