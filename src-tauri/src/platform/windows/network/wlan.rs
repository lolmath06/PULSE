//! Windows Wi-Fi link quality, through the WLAN API.
//!
//! # Quality without identity
//!
//! The obvious source for a signal strength is
//! `wlan_intf_opcode_current_connection`, which returns
//! `WLAN_CONNECTION_ATTRIBUTES` — signal quality, receive rate, transmit rate,
//! **and the SSID and BSSID of the network**. On recent Windows those last two
//! make the call subject to the machine's location permission, because a BSSID
//! is a geolocation primitive: given one, a lookup service will place the
//! device to within a few metres.
//!
//! PULSE does not want that. It wants **how good is this link**, not **which
//! network is this and where**. So it asks
//! `wlan_intf_opcode_realtime_connection_quality`, which reports link quality,
//! per-link RSSI and negotiated rates and carries no network identity at all.
//! Nothing here requests, reads, stores or displays an SSID or a BSSID.
//!
//! # And no fallback to the location-gated call
//!
//! That opcode is recent. On a Windows version that does not implement it the
//! query fails, and PULSE reports the four Wi-Fi metrics as `unsupported` with
//! that reason — it does **not** quietly fall back to
//! `wlan_intf_opcode_current_connection`. Falling back would work, and would
//! mean a monitoring tool silently reaching for a location-gated API behind
//! the user's back to obtain a number it already said it could not get. The
//! generic interface metrics are unaffected either way, which is the point of
//! keeping the capability separate.
//!
//! # Nothing here can fail the provider
//!
//! No WLAN service, no wireless hardware, an older Windows, a refused query —
//! every one of them leaves `windows.network` publishing its full generic
//! catalog. Wi-Fi is a capability of the provider, never a precondition.

use crate::metrics::wellknown::network::{WifiLink, WifiLinkInfo};

use super::iftable::Guid;

/// `wlan_intf_opcode_realtime_connection_quality`.
///
/// Reports link quality, negotiated rates and per-link RSSI, and carries **no**
/// SSID or BSSID — which is the entire reason PULSE uses it. A Windows build
/// that does not implement it answers `ERROR_INVALID_PARAMETER`, which is
/// handled as "unsupported" rather than as a failure.
pub const WLAN_INTF_OPCODE_REALTIME_CONNECTION_QUALITY: u32 = 19;

/// The client version PULSE requests from `WlanOpenHandle`.
///
/// Version 2 is the Vista-era interface, supported by every Windows PULSE
/// targets. A newer one buys nothing here.
pub const WLAN_API_VERSION: u32 = 2;

/// The fixed part of `WLAN_REALTIME_CONNECTION_QUALITY`, in bytes.
///
/// ```text
///  0  ULONG ulVersion
///  4  ULONG dot11PhyType
///  8  ULONG ulLinkQuality          0-100, the whole connection
/// 12  ULONG ulRxRate               kilobits per second
/// 16  ULONG ulTxRate               kilobits per second
/// 20  BOOL  bIsMLOConnection
/// 24  ULONG ulNumLinks
/// 28  WLAN_REALTIME_CONNECTION_QUALITY_LINK_INFO linksInfo[]
/// ```
pub const QUALITY_HEADER_LEN: usize = 28;

/// The size of one `WLAN_REALTIME_CONNECTION_QUALITY_LINK_INFO`.
///
/// ```text
///  0  ULONG ulLinkID
///  4  ULONG ulChannelCenterFrequencyMhz
///  8  ULONG ulBandwidth
/// 12  LONG  lRssi                  dBm, signed
/// 16  ULONG ulLinkQuality          0-100, this link
/// 20  ULONG ulRxRate               kilobits per second
/// 24  ULONG ulTxRate               kilobits per second
/// ```
pub const LINK_INFO_LEN: usize = 28;

/// Offset of `ulLinkQuality` in the header.
const OFFSET_LINK_QUALITY: usize = 8;
/// Offset of `ulRxRate` in the header.
const OFFSET_RX_RATE: usize = 12;
/// Offset of `ulTxRate` in the header.
const OFFSET_TX_RATE: usize = 16;
/// Offset of `ulNumLinks` in the header.
const OFFSET_NUM_LINKS: usize = 24;

/// Offset of `lRssi` within one link.
const LINK_OFFSET_RSSI: usize = 12;
/// Offset of `ulRxRate` within one link.
const LINK_OFFSET_RX_RATE: usize = 20;
/// Offset of `ulTxRate` within one link.
const LINK_OFFSET_TX_RATE: usize = 24;

/// The WLAN API reports rates in **kilobits per second**.
///
/// `866700` is an 866.7 Mbit/s link. Reading it as bits per second would
/// report a gigabit link as under a megabit; as megabits, as 866 terabits.
pub const RATE_UNIT_BPS: u64 = 1_000;

/// The largest number of links PULSE will read from one response.
///
/// Wi-Fi 7 defines at most three. The bound exists so that a malformed
/// `ulNumLinks` cannot drive a long loop, and it is checked against the actual
/// buffer length as well.
const MAX_LINKS: usize = 8;

fn read_u32(buffer: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = buffer.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn read_i32(buffer: &[u8], offset: usize) -> Option<i32> {
    let bytes: [u8; 4] = buffer.get(offset..offset + 4)?.try_into().ok()?;
    Some(i32::from_le_bytes(bytes))
}

/// Converts a WLAN rate figure into bits per second.
///
/// Returns `None` for zero, which the API uses where it has no rate.
pub fn rate_to_bps(kilobits: u32) -> Option<u64> {
    (kilobits > 0).then(|| u64::from(kilobits) * RATE_UNIT_BPS)
}

/// Parses a `WLAN_REALTIME_CONNECTION_QUALITY` response.
///
/// Every read is bounded by the buffer Windows actually returned, and
/// `ulNumLinks` is checked against it rather than trusted: a response claiming
/// more links than it carries would otherwise read past the end.
///
/// Returns `None` only when the buffer is too short for the fixed header,
/// which is a truncated response rather than a disconnected radio.
pub fn parse_quality(buffer: &[u8]) -> Option<WifiLinkInfo> {
    if buffer.len() < QUALITY_HEADER_LEN {
        return None;
    }

    let quality = read_u32(buffer, OFFSET_LINK_QUALITY)?;
    let declared_links = read_u32(buffer, OFFSET_NUM_LINKS)? as usize;

    // How many links the buffer can actually hold, whatever it claims.
    let available = buffer.len().saturating_sub(QUALITY_HEADER_LEN) / LINK_INFO_LEN;
    let count = declared_links.min(available).min(MAX_LINKS);

    let mut links: Vec<WifiLink> = (0..count)
        .map(|index| {
            let base = QUALITY_HEADER_LEN + index * LINK_INFO_LEN;

            WifiLink {
                rssi_dbm: read_i32(buffer, base + LINK_OFFSET_RSSI),
                rssi_average_dbm: None,
                receive_bps: read_u32(buffer, base + LINK_OFFSET_RX_RATE).and_then(rate_to_bps),
                transmit_bps: read_u32(buffer, base + LINK_OFFSET_TX_RATE).and_then(rate_to_bps),
            }
        })
        .filter(|link| !link.is_empty())
        .collect();

    // A single-link radio on a driver that fills only the header still has a
    // usable connection, so the header's own rates become one link. Without
    // this, such an adapter would report a quality and no rates at all.
    if links.is_empty() {
        let receive = read_u32(buffer, OFFSET_RX_RATE).and_then(rate_to_bps);
        let transmit = read_u32(buffer, OFFSET_TX_RATE).and_then(rate_to_bps);

        if receive.is_some() || transmit.is_some() {
            links.push(WifiLink {
                rssi_dbm: None,
                rssi_average_dbm: None,
                receive_bps: receive,
                transmit_bps: transmit,
            });
        }
    }

    Some(WifiLinkInfo {
        links,
        // Windows computes this itself, so PULSE publishes it. It is the one
        // platform that does — see `metrics::wellknown::network::wifi`.
        quality_percent: quality_percent(quality),
    })
}

/// Validates the API's 0–100 quality figure.
///
/// A value outside the documented range is refused rather than clamped:
/// clamping would hide a driver reporting nonsense behind a plausible number.
pub fn quality_percent(raw: u32) -> Option<f64> {
    (raw <= 100).then(|| f64::from(raw))
}

/// One wireless interface the WLAN service knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WlanInterface {
    pub guid: Guid,
    pub description: String,
    /// `WLAN_INTERFACE_STATE`.
    pub state: u32,
}

/// `wlan_interface_state_connected`.
pub const WLAN_INTERFACE_STATE_CONNECTED: u32 = 1;

impl WlanInterface {
    /// Whether this interface is associated with a network.
    pub const fn is_connected(&self) -> bool {
        self.state == WLAN_INTERFACE_STATE_CONNECTED
    }
}

/// The size of one `WLAN_INTERFACE_INFO`: a GUID, a 256-unit UTF-16
/// description, and a state.
pub const WLAN_INTERFACE_INFO_LEN: usize = 16 + 512 + 4;

/// The header of `WLAN_INTERFACE_INFO_LIST`: `dwNumberOfItems`, `dwIndex`.
pub const WLAN_INTERFACE_INFO_LIST_HEADER: usize = 8;

/// Parses a `WLAN_INTERFACE_INFO_LIST`.
///
/// Bounded the same way as the quality response: the declared count is checked
/// against what the buffer can hold.
pub fn parse_interface_list(buffer: &[u8]) -> Vec<WlanInterface> {
    if buffer.len() < WLAN_INTERFACE_INFO_LIST_HEADER {
        return Vec::new();
    }

    let Some(declared) = read_u32(buffer, 0) else {
        return Vec::new();
    };

    let available =
        buffer.len().saturating_sub(WLAN_INTERFACE_INFO_LIST_HEADER) / WLAN_INTERFACE_INFO_LEN;
    let count = (declared as usize).min(available);

    (0..count)
        .filter_map(|index| {
            let base = WLAN_INTERFACE_INFO_LIST_HEADER + index * WLAN_INTERFACE_INFO_LEN;

            let guid = Guid {
                data1: read_u32(buffer, base)?,
                data2: u16::from_le_bytes(buffer.get(base + 4..base + 6)?.try_into().ok()?),
                data3: u16::from_le_bytes(buffer.get(base + 6..base + 8)?.try_into().ok()?),
                data4: buffer.get(base + 8..base + 16)?.try_into().ok()?,
            };

            let description: Vec<u16> = buffer
                .get(base + 16..base + 16 + 512)?
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();

            Some(WlanInterface {
                guid,
                description: super::iftable::utf16_to_string(&description),
                state: read_u32(buffer, base + 16 + 512)?,
            })
        })
        .collect()
}

// --- talking to Windows ----------------------------------------------------

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use crate::metrics::model::MetricError;
    use std::collections::BTreeMap;
    use windows_sys::Win32::NetworkManagement::WiFi::{
        WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanOpenHandle, WlanQueryInterface,
    };

    /// An open WLAN service handle, closed when it goes out of scope.
    pub struct WlanHandle(windows_sys::Win32::Foundation::HANDLE);

    impl WlanHandle {
        /// Opens the WLAN service.
        ///
        /// Fails on a machine with no wireless hardware and on one where the
        /// WLAN AutoConfig service is stopped. Both are normal answers, and
        /// the caller treats them as "no Wi-Fi here".
        pub fn open() -> Result<Self, MetricError> {
            let mut negotiated: u32 = 0;
            let mut handle: windows_sys::Win32::Foundation::HANDLE = std::ptr::null_mut();

            // SAFETY: both out-parameters are owned locals that outlive the
            // call, and the reserved argument is null as documented.
            let status = unsafe {
                WlanOpenHandle(
                    WLAN_API_VERSION,
                    std::ptr::null_mut(),
                    std::ptr::addr_of_mut!(negotiated),
                    std::ptr::addr_of_mut!(handle),
                )
            };

            if status != 0 {
                return Err(super::super::iftable::from_win32(
                    status,
                    "the Wi-Fi service",
                ));
            }

            Ok(Self(handle))
        }

        /// Every wireless interface the service knows about.
        pub fn interfaces(&self) -> Result<Vec<WlanInterface>, MetricError> {
            let mut list: *mut core::ffi::c_void = std::ptr::null_mut();

            // SAFETY: `list` is a null pointer the call overwrites with its
            // own allocation, freed by the guard below.
            let status = unsafe {
                WlanEnumInterfaces(
                    self.0,
                    std::ptr::null_mut(),
                    std::ptr::addr_of_mut!(list) as *mut _,
                )
            };

            if status != 0 {
                return Err(super::super::iftable::from_win32(
                    status,
                    "the Wi-Fi interface list",
                ));
            }

            let _guard = WlanBuffer(list);
            if list.is_null() {
                return Ok(Vec::new());
            }

            // SAFETY: on success the allocation holds at least the two-word
            // header; the count it declares is checked against the length
            // derived from it before any element is read.
            let count = unsafe { std::ptr::read_unaligned(list as *const u32) } as usize;
            let length = WLAN_INTERFACE_INFO_LIST_HEADER + count * WLAN_INTERFACE_INFO_LEN;

            // SAFETY: `length` is exactly what the declared count implies, and
            // the parser bounds every read within it.
            let bytes = unsafe { std::slice::from_raw_parts(list as *const u8, length) };

            Ok(parse_interface_list(bytes))
        }

        /// One interface's realtime connection quality.
        pub fn quality(&self, guid: &Guid) -> Result<WifiLinkInfo, MetricError> {
            let mut size: u32 = 0;
            let mut data: *mut core::ffi::c_void = std::ptr::null_mut();

            // SAFETY: `guid` outlives the call; `size` and `data` are owned
            // out-parameters; the optional opcode-value-type argument is null
            // as documented. The buffer is freed by the guard below.
            let status = unsafe {
                WlanQueryInterface(
                    self.0,
                    guid as *const Guid as *const _,
                    // The binding types the opcode as a signed enum; the
                    // value is the documented one either way.
                    WLAN_INTF_OPCODE_REALTIME_CONNECTION_QUALITY as i32,
                    std::ptr::null_mut(),
                    std::ptr::addr_of_mut!(size),
                    std::ptr::addr_of_mut!(data),
                    std::ptr::null_mut(),
                )
            };

            if status != 0 {
                return Err(super::super::iftable::from_win32(
                    status,
                    "the Wi-Fi connection quality",
                ));
            }

            let _guard = WlanBuffer(data);
            if data.is_null() || size == 0 {
                return Ok(WifiLinkInfo::disconnected());
            }

            // SAFETY: Windows reported `size` bytes at `data`, and the parser
            // bounds every read within that slice.
            let bytes = unsafe { std::slice::from_raw_parts(data as *const u8, size as usize) };

            Ok(parse_quality(bytes).unwrap_or_else(WifiLinkInfo::disconnected))
        }
    }

    impl Drop for WlanHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the handle came from `WlanOpenHandle` and has not
                // been closed.
                unsafe { WlanCloseHandle(self.0, std::ptr::null_mut()) };
            }
        }
    }

    /// A buffer the WLAN API allocated, freed on every path out.
    struct WlanBuffer(*mut core::ffi::c_void);

    impl Drop for WlanBuffer {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the pointer came from a WLAN API call that allocates
                // with the matching allocator, and is not used afterwards.
                unsafe { WlanFreeMemory(self.0) };
            }
        }
    }

    /// Every connected wireless interface's link, keyed by interface GUID.
    ///
    /// Returns an empty map rather than an error on a machine with no
    /// wireless hardware: the generic interface metrics must keep working, and
    /// an adapter with no association is reported through its own
    /// availability rather than by the whole query failing.
    pub fn links() -> BTreeMap<String, WifiLinkInfo> {
        let Ok(handle) = WlanHandle::open() else {
            return BTreeMap::new();
        };
        let Ok(interfaces) = handle.interfaces() else {
            return BTreeMap::new();
        };

        interfaces
            .into_iter()
            .map(|interface| {
                let info = if interface.is_connected() {
                    handle
                        .quality(&interface.guid)
                        .unwrap_or_else(|_| WifiLinkInfo::disconnected())
                } else {
                    WifiLinkInfo::disconnected()
                };

                (interface.guid.to_text(), info)
            })
            .collect()
    }
}

#[cfg(target_os = "windows")]
pub use imp::links;

/// Compiled on non-Windows hosts so everything above type checks and is
/// unit-tested there. Never reached: only the Windows provider calls it.
#[cfg(not(target_os = "windows"))]
pub fn links() -> std::collections::BTreeMap<String, WifiLinkInfo> {
    std::collections::BTreeMap::new()
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Builds a `WLAN_REALTIME_CONNECTION_QUALITY` response.
    pub fn quality(
        link_quality: u32,
        rx_kbps: u32,
        tx_kbps: u32,
        links: &[(i32, u32, u32)],
    ) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(QUALITY_HEADER_LEN + links.len() * LINK_INFO_LEN);

        buffer.extend_from_slice(&1_u32.to_le_bytes()); // ulVersion
        buffer.extend_from_slice(&8_u32.to_le_bytes()); // dot11PhyType
        buffer.extend_from_slice(&link_quality.to_le_bytes());
        buffer.extend_from_slice(&rx_kbps.to_le_bytes());
        buffer.extend_from_slice(&tx_kbps.to_le_bytes());
        buffer.extend_from_slice(&u32::from(links.len() > 1).to_le_bytes()); // bIsMLOConnection
        buffer.extend_from_slice(&(links.len() as u32).to_le_bytes());

        for (index, (rssi, rx, tx)) in links.iter().enumerate() {
            buffer.extend_from_slice(&(index as u32).to_le_bytes()); // ulLinkID
            buffer.extend_from_slice(&5500_u32.to_le_bytes()); // frequency
            buffer.extend_from_slice(&80_u32.to_le_bytes()); // bandwidth
            buffer.extend_from_slice(&rssi.to_le_bytes());
            buffer.extend_from_slice(&link_quality.to_le_bytes());
            buffer.extend_from_slice(&rx.to_le_bytes());
            buffer.extend_from_slice(&tx.to_le_bytes());
        }

        buffer
    }

    /// A plausible single-link association.
    pub fn connected() -> Vec<u8> {
        quality(92, 866_700, 866_700, &[(-54, 866_700, 866_700)])
    }

    /// Builds a `WLAN_INTERFACE_INFO_LIST`.
    pub fn interface_list(interfaces: &[(Guid, &str, u32)]) -> Vec<u8> {
        let mut buffer = Vec::new();

        buffer.extend_from_slice(&(interfaces.len() as u32).to_le_bytes());
        buffer.extend_from_slice(&0_u32.to_le_bytes()); // dwIndex

        for (guid, description, state) in interfaces {
            buffer.extend_from_slice(&guid.data1.to_le_bytes());
            buffer.extend_from_slice(&guid.data2.to_le_bytes());
            buffer.extend_from_slice(&guid.data3.to_le_bytes());
            buffer.extend_from_slice(&guid.data4);

            let mut units = [0_u16; 256];
            for (slot, unit) in units.iter_mut().zip(description.encode_utf16()) {
                *slot = unit;
            }
            for unit in units {
                buffer.extend_from_slice(&unit.to_le_bytes());
            }

            buffer.extend_from_slice(&state.to_le_bytes());
        }

        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    // --- the rate unit -----------------------------------------------------

    #[test]
    fn a_wlan_rate_is_in_kilobits_per_second() {
        // `866700` is 866.7 Mbit/s. As bit/s it would read as under a
        // megabit; as Mbit/s, as 866 terabits.
        assert_eq!(rate_to_bps(866_700), Some(866_700_000));
        assert_eq!(rate_to_bps(1_200_000), Some(1_200_000_000));
        assert_eq!(rate_to_bps(54_000), Some(54_000_000));
    }

    #[test]
    fn a_zero_rate_means_the_api_has_none() {
        assert_eq!(rate_to_bps(0), None);
    }

    // --- quality -----------------------------------------------------------

    #[test]
    fn parses_a_plausible_single_link_association() {
        let info = parse_quality(&connected()).expect("parsed");

        assert!(info.is_connected());
        assert_eq!(info.link_count(), 1);
        assert_eq!(info.rssi_dbm(), Some(-54.0));
        assert_eq!(info.quality_percent(), Some(92.0));
        assert_eq!(info.receive_bps(), Some(866_700_000.0));
        assert_eq!(info.transmit_bps(), Some(866_700_000.0));
    }

    #[test]
    fn windows_is_the_platform_that_does_report_a_quality() {
        // Unlike Fedora, where `cfg80211` reports dBm and nothing else.
        let info = parse_quality(&quality(0, 1000, 1000, &[(-90, 1000, 1000)])).expect("parsed");
        assert_eq!(info.quality_percent(), Some(0.0), "zero is a measurement");

        let full = parse_quality(&quality(100, 1000, 1000, &[(-40, 1000, 1000)])).expect("parsed");
        assert_eq!(full.quality_percent(), Some(100.0));
    }

    #[test]
    fn a_quality_outside_its_documented_range_is_refused_not_clamped() {
        assert_eq!(quality_percent(101), None);
        assert_eq!(quality_percent(u32::MAX), None);
        assert_eq!(quality_percent(0), Some(0.0));
        assert_eq!(quality_percent(100), Some(100.0));
    }

    #[test]
    fn an_rssi_is_read_as_signed() {
        // The single easiest mistake: −54 stored as a `LONG` is 0xFFFFFFCA.
        let info = parse_quality(&quality(50, 0, 0, &[(-54, 0, 0)])).expect("parsed");
        assert_eq!(info.rssi_dbm(), Some(-54.0));

        let weak = parse_quality(&quality(10, 0, 0, &[(-90, 0, 0)])).expect("parsed");
        assert_eq!(weak.rssi_dbm(), Some(-90.0));
    }

    #[test]
    fn an_implausible_rssi_is_refused_by_the_shared_contract() {
        let info = parse_quality(&quality(50, 0, 0, &[(-200, 0, 0)])).expect("parsed");

        assert_eq!(info.rssi_dbm(), None);
    }

    // --- multi-link operation ---------------------------------------------

    #[test]
    fn a_multi_link_connection_publishes_its_strongest_link() {
        // Taking `linksInfo[0]` would report −85 dBm for a radio that also
        // holds a −45 dBm link.
        let info = parse_quality(&quality(
            88,
            0,
            0,
            &[(-85, 100_000, 100_000), (-45, 900_000, 900_000)],
        ))
        .expect("parsed");

        assert_eq!(info.link_count(), 2);
        assert_eq!(info.rssi_dbm(), Some(-45.0));
        assert_eq!(info.receive_bps(), Some(900_000_000.0));
        // The whole-connection quality is the API's own, not a per-link one.
        assert_eq!(info.quality_percent(), Some(88.0));
    }

    #[test]
    fn three_links_are_all_kept() {
        let info = parse_quality(&quality(
            80,
            0,
            0,
            &[(-70, 1000, 1000), (-60, 2000, 2000), (-50, 3000, 3000)],
        ))
        .expect("parsed");

        assert_eq!(info.link_count(), 3);
        assert_eq!(info.rssi_dbm(), Some(-50.0));
    }

    // --- defensive parsing -------------------------------------------------

    #[test]
    fn a_response_claiming_more_links_than_it_carries_reads_only_what_is_there() {
        let mut buffer = quality(90, 0, 0, &[(-54, 1000, 1000)]);
        // Claim eight links while carrying one.
        buffer[OFFSET_NUM_LINKS..OFFSET_NUM_LINKS + 4].copy_from_slice(&8_u32.to_le_bytes());

        let info = parse_quality(&buffer).expect("parsed");

        assert_eq!(info.link_count(), 1);
        assert_eq!(info.rssi_dbm(), Some(-54.0));
    }

    #[test]
    fn an_absurd_link_count_is_bounded() {
        let mut buffer = quality(90, 0, 0, &[(-54, 1000, 1000)]);
        buffer[OFFSET_NUM_LINKS..OFFSET_NUM_LINKS + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        let info = parse_quality(&buffer).expect("parsed");
        assert!(info.link_count() <= MAX_LINKS);
    }

    #[test]
    fn a_truncated_response_is_refused_rather_than_parsed() {
        assert_eq!(parse_quality(&[]), None);
        assert_eq!(parse_quality(&[0_u8; 16]), None);
        assert!(parse_quality(&[0_u8; QUALITY_HEADER_LEN]).is_some());
    }

    #[test]
    fn a_truncated_link_block_is_dropped_rather_than_half_read() {
        let mut buffer = connected();
        buffer.truncate(buffer.len() - 8);

        let info = parse_quality(&buffer).expect("parsed");
        // The partial link is not read; the header's own rates stand in.
        assert_eq!(info.rssi_dbm(), None);
        assert_eq!(info.receive_bps(), Some(866_700_000.0));
    }

    #[test]
    fn a_driver_filling_only_the_header_still_reports_its_rates() {
        // Without the fallback, such an adapter would show a quality and no
        // rates at all.
        let info = parse_quality(&quality(75, 400_000, 200_000, &[])).expect("parsed");

        assert!(info.is_connected());
        assert_eq!(info.receive_bps(), Some(400_000_000.0));
        assert_eq!(info.transmit_bps(), Some(200_000_000.0));
        assert_eq!(info.rssi_dbm(), None);
        assert_eq!(info.quality_percent(), Some(75.0));
    }

    #[test]
    fn a_disconnected_radio_reports_nothing_rather_than_zero() {
        let info = parse_quality(&quality(0, 0, 0, &[])).expect("parsed");

        assert!(!info.is_connected());
        assert_eq!(info.rssi_dbm(), None);
        assert_eq!(info.receive_bps(), None);
    }

    // --- the interface list ------------------------------------------------

    fn guid(first: u32) -> Guid {
        Guid {
            data1: first,
            data2: 0x1111,
            data3: 0x2222,
            data4: [0x33, 0x33, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44],
        }
    }

    #[test]
    fn parses_one_wireless_interface() {
        let buffer = interface_list(&[(
            guid(0x6C5A_1B2D),
            "Intel(R) Wi-Fi 6E AX211 160MHz",
            WLAN_INTERFACE_STATE_CONNECTED,
        )]);

        let interfaces = parse_interface_list(&buffer);

        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].description, "Intel(R) Wi-Fi 6E AX211 160MHz");
        assert_eq!(interfaces[0].guid.data1, 0x6C5A_1B2D);
        assert!(interfaces[0].is_connected());
    }

    #[test]
    fn parses_several_wireless_interfaces() {
        let buffer = interface_list(&[
            (guid(1), "Adapter One", WLAN_INTERFACE_STATE_CONNECTED),
            (guid(2), "Adapter Two", 0),
        ]);

        let interfaces = parse_interface_list(&buffer);

        assert_eq!(interfaces.len(), 2);
        assert!(interfaces[0].is_connected());
        assert!(!interfaces[1].is_connected(), "not associated");
        assert_ne!(interfaces[0].guid, interfaces[1].guid);
    }

    #[test]
    fn a_machine_with_no_wireless_hardware_yields_an_empty_list() {
        assert!(parse_interface_list(&interface_list(&[])).is_empty());
        assert!(parse_interface_list(&[]).is_empty());
        assert!(parse_interface_list(&[0_u8; 4]).is_empty());
    }

    #[test]
    fn a_list_claiming_more_interfaces_than_it_carries_reads_only_what_is_there() {
        let mut buffer = interface_list(&[(guid(1), "One", WLAN_INTERFACE_STATE_CONNECTED)]);
        buffer[0..4].copy_from_slice(&99_u32.to_le_bytes());

        assert_eq!(parse_interface_list(&buffer).len(), 1);
    }

    #[test]
    fn a_truncated_interface_entry_is_dropped() {
        let mut buffer = interface_list(&[(guid(1), "One", WLAN_INTERFACE_STATE_CONNECTED)]);
        buffer.truncate(buffer.len() - 4);

        assert!(parse_interface_list(&buffer).is_empty());
    }

    // --- privacy -----------------------------------------------------------

    #[test]
    fn nothing_here_reaches_for_a_location_gated_interface() {
        // A guard against a later change quietly adding the fallback this
        // module deliberately refuses. The needles are assembled at runtime so
        // that this test's own source does not trip it.
        let source = include_str!("wlan.rs");

        let ssid = ["dot11", "Ssid"].concat();
        let bss_list = ["WlanGetNetwork", "BssList"].concat();
        let query_current = ["WLAN_INTF_OPCODE_CURRENT_", "CONNECTION"].concat();

        for needle in [&ssid, &bss_list, &query_current] {
            assert!(
                !source.contains(needle.as_str()),
                "'{needle}' must not appear: PULSE reads link quality, not network identity"
            );
        }

        assert_eq!(WLAN_INTF_OPCODE_REALTIME_CONNECTION_QUALITY, 19);
    }

    #[test]
    fn the_parsed_types_have_nowhere_to_put_a_network_identity() {
        // Structural, not textual: `WifiLink` and `WifiLinkInfo` carry a
        // signal, two rates and a quality. There is no field an SSID or a
        // BSSID could be stored in even by accident.
        let info = parse_quality(&connected()).expect("parsed");
        let debug = format!("{info:?}");

        assert!(debug.contains("rssi_dbm"));
        assert!(!debug.to_lowercase().contains("ssid"));
        assert!(!debug.to_lowercase().contains("bssid"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_stand_in_reports_no_wireless_links() {
        assert!(links().is_empty());
    }
}
