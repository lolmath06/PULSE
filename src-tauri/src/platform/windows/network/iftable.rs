//! `GetIfTable2` and `MIB_IF_ROW2` — the Windows interface inventory and its
//! counters, in one call.
//!
//! PULSE runs no `PowerShell`, `netsh`, `wmic` or `ipconfig`, and does not go
//! through PDH. `GetIfTable2` returns every interface's identity, description,
//! MTU, type, media state, link speeds **and** its counters in a single
//! structure — one call, one instant, everything, exactly as `RTM_GETLINK`
//! does on Fedora.
//!
//! # Why the structure is declared here
//!
//! `MIB_IF_ROW2` is a stable, documented binary layout. Declaring it in PULSE
//! rather than depending on a binding crate's spelling has one concrete
//! benefit, the same one that applied to the storage descriptors in Phase 6:
//! **the field reading is pure, so it compiles and is tested on Fedora.**
//!
//! The layout is `#[repr(C)]` and the compiler computes the offsets, so there
//! are no hand-counted byte positions to get wrong; a compile-time assertion
//! pins the total size at the documented 1352 bytes on a 64-bit target, which
//! is what catches a field added in the wrong place.
//!
//! # Packets are the sum of two counters
//!
//! Windows splits packet counts into unicast and non-unicast:
//!
//! ```text
//! RX packets = InUcastPkts  + InNUcastPkts
//! TX packets = OutUcastPkts + OutNUcastPkts
//! ```
//!
//! Publishing only the unicast half — the obvious reading, since it is the
//! field whose name looks like "packets" — silently drops every broadcast and
//! multicast frame, which on a normal network is a large and variable share of
//! the total. The addition is checked, because two near-maximum counters must
//! not wrap into a small number.

use crate::metrics::wellknown::network::{LinkState, NetworkCounters, NetworkInterfaceKind};

/// `IF_MAX_STRING_SIZE + 1`, the length of the alias and description arrays.
pub const IF_MAX_STRING: usize = 257;
/// `IF_MAX_PHYS_ADDRESS_LENGTH`.
pub const IF_MAX_PHYS_ADDRESS: usize = 32;

/// A Windows `GUID`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl Guid {
    /// The canonical brace-and-dash form Windows uses everywhere.
    pub fn to_text(self) -> String {
        format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7],
        )
    }

    /// Whether this GUID is all zeros, which Windows uses for "none".
    pub fn is_zero(self) -> bool {
        self.data1 == 0 && self.data2 == 0 && self.data3 == 0 && self.data4 == [0; 8]
    }
}

/// `MIB_IF_ROW2`, as `netioapi.h` declares it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MibIfRow2 {
    pub interface_luid: u64,
    pub interface_index: u32,
    pub interface_guid: Guid,
    pub alias: [u16; IF_MAX_STRING],
    pub description: [u16; IF_MAX_STRING],
    pub physical_address_length: u32,
    pub physical_address: [u8; IF_MAX_PHYS_ADDRESS],
    pub permanent_physical_address: [u8; IF_MAX_PHYS_ADDRESS],
    pub mtu: u32,
    /// `IFTYPE` — an IANA ifType value.
    pub if_type: u32,
    pub tunnel_type: u32,
    /// `NDIS_MEDIUM`.
    pub media_type: u32,
    /// `NDIS_PHYSICAL_MEDIUM`.
    pub physical_medium_type: u32,
    pub access_type: u32,
    pub direction_type: u32,
    /// A packed set of `BOOLEAN` bitfields.
    pub interface_and_oper_status_flags: u8,
    /// Explicit padding to the next four-byte boundary.
    ///
    /// Public because the compiler would insert it anyway and a caller
    /// building one of these with struct-update syntax needs to name every
    /// field. Never read.
    pub padding: [u8; 3],
    /// `IF_OPER_STATUS`.
    pub oper_status: u32,
    /// `NET_IF_ADMIN_STATUS`.
    pub admin_status: u32,
    /// `NET_IF_MEDIA_CONNECT_STATE`.
    pub media_connect_state: u32,
    pub network_guid: Guid,
    pub connection_type: u32,
    /// Explicit padding to the next eight-byte boundary, before the `u64`
    /// counters. Never read.
    pub padding2: [u8; 4],
    /// Bits per second, already.
    pub transmit_link_speed: u64,
    pub receive_link_speed: u64,
    pub in_octets: u64,
    pub in_ucast_pkts: u64,
    pub in_nucast_pkts: u64,
    pub in_discards: u64,
    pub in_errors: u64,
    pub in_unknown_protos: u64,
    pub in_ucast_octets: u64,
    pub in_multicast_octets: u64,
    pub in_broadcast_octets: u64,
    pub out_octets: u64,
    pub out_ucast_pkts: u64,
    pub out_nucast_pkts: u64,
    pub out_discards: u64,
    pub out_errors: u64,
    pub out_ucast_octets: u64,
    pub out_multicast_octets: u64,
    pub out_broadcast_octets: u64,
    pub out_qlen: u64,
}

/// The documented size of `MIB_IF_ROW2` on a 64-bit target.
///
/// Asserted at compile time rather than trusted: a field declared at the wrong
/// width or in the wrong order shifts everything after it, and every counter
/// PULSE reads lives at the end of the structure.
pub const MIB_IF_ROW2_SIZE: usize = 1352;

const _: () = assert!(std::mem::size_of::<MibIfRow2>() == MIB_IF_ROW2_SIZE);

impl Default for MibIfRow2 {
    fn default() -> Self {
        // `[u16; 257]` has no `Default`, so the whole structure is built from
        // zeroes — which is also exactly what a caller must pass to
        // `GetIfEntry2`.
        Self {
            interface_luid: 0,
            interface_index: 0,
            interface_guid: Guid::default(),
            alias: [0; IF_MAX_STRING],
            description: [0; IF_MAX_STRING],
            physical_address_length: 0,
            physical_address: [0; IF_MAX_PHYS_ADDRESS],
            permanent_physical_address: [0; IF_MAX_PHYS_ADDRESS],
            mtu: 0,
            if_type: 0,
            tunnel_type: 0,
            media_type: 0,
            physical_medium_type: 0,
            access_type: 0,
            direction_type: 0,
            interface_and_oper_status_flags: 0,
            padding: [0; 3],
            oper_status: 0,
            admin_status: 0,
            media_connect_state: 0,
            network_guid: Guid::default(),
            connection_type: 0,
            padding2: [0; 4],
            transmit_link_speed: 0,
            receive_link_speed: 0,
            in_octets: 0,
            in_ucast_pkts: 0,
            in_nucast_pkts: 0,
            in_discards: 0,
            in_errors: 0,
            in_unknown_protos: 0,
            in_ucast_octets: 0,
            in_multicast_octets: 0,
            in_broadcast_octets: 0,
            out_octets: 0,
            out_ucast_pkts: 0,
            out_nucast_pkts: 0,
            out_discards: 0,
            out_errors: 0,
            out_ucast_octets: 0,
            out_multicast_octets: 0,
            out_broadcast_octets: 0,
            out_qlen: 0,
        }
    }
}

impl std::fmt::Debug for MibIfRow2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The two 514-byte string arrays would drown everything else.
        f.debug_struct("MibIfRow2")
            .field("index", &self.interface_index)
            .field("alias", &self.alias_text())
            .field("description", &self.description_text())
            .field("type", &self.if_type)
            .finish()
    }
}

// --- IANA interface types --------------------------------------------------

/// `IF_TYPE_ETHERNET_CSMACD` — what Windows reports for wired *and*, on many
/// drivers, wireless adapters.
pub const IF_TYPE_ETHERNET: u32 = 6;
/// `IF_TYPE_SOFTWARE_LOOPBACK`.
pub const IF_TYPE_LOOPBACK: u32 = 24;
/// `IF_TYPE_TUNNEL`.
pub const IF_TYPE_TUNNEL: u32 = 131;
/// `IF_TYPE_IEEE80211`.
pub const IF_TYPE_IEEE80211: u32 = 71;
/// `IF_TYPE_PPP`.
pub const IF_TYPE_PPP: u32 = 23;

// --- NDIS physical media ---------------------------------------------------

/// `NdisPhysicalMediumNative802_11` — the reliable Wi-Fi signal.
///
/// It matters because `if_type` is **not** reliable here: plenty of wireless
/// drivers report `IF_TYPE_ETHERNET` for compatibility, exactly as a Linux
/// Wi-Fi interface reports `ARPHRD_ETHER`. The physical medium is what
/// actually distinguishes them.
pub const NDIS_PHYSICAL_MEDIUM_NATIVE_802_11: u32 = 9;
/// `NdisPhysicalMediumWirelessLan` — the older spelling.
pub const NDIS_PHYSICAL_MEDIUM_WIRELESS_LAN: u32 = 1;
/// `NdisPhysicalMediumBluetooth`.
pub const NDIS_PHYSICAL_MEDIUM_BLUETOOTH: u32 = 10;

// --- states ----------------------------------------------------------------

/// `IfOperStatusUp`.
pub const IF_OPER_STATUS_UP: u32 = 1;
/// `IfOperStatusDown`.
pub const IF_OPER_STATUS_DOWN: u32 = 2;

/// `MediaConnectStateConnected`.
pub const MEDIA_CONNECT_STATE_CONNECTED: u32 = 1;
/// `MediaConnectStateDisconnected`.
pub const MEDIA_CONNECT_STATE_DISCONNECTED: u32 = 2;

/// Bit 0 of `InterfaceAndOperStatusFlags` — `HardwareInterface`.
const FLAG_HARDWARE_INTERFACE: u8 = 0x01;
/// Bit 2 — `ConnectorPresent`, i.e. a physical connector exists.
const FLAG_CONNECTOR_PRESENT: u8 = 0x04;

/// Reads a NUL-terminated UTF-16 array into a string.
pub fn utf16_to_string(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());

    String::from_utf16_lossy(&buffer[..end])
}

impl MibIfRow2 {
    /// The adapter's description — `Intel(R) Wi-Fi 6E AX211 160MHz`.
    ///
    /// **Presentation only**, and the right thing to show: it names the
    /// hardware, where the alias is whatever the user renamed the connection
    /// to.
    pub fn description_text(&self) -> String {
        utf16_to_string(&self.description)
    }

    /// The connection's alias — `Wi-Fi`, `Ethernet 2`.
    ///
    /// The user can rename this freely, which is exactly why it is neither an
    /// identity nor the primary label.
    pub fn alias_text(&self) -> String {
        utf16_to_string(&self.alias)
    }

    /// Whether this row describes real hardware.
    pub const fn is_hardware(&self) -> bool {
        self.interface_and_oper_status_flags & FLAG_HARDWARE_INTERFACE != 0
    }

    /// Whether the adapter has a physical connector.
    pub const fn has_connector(&self) -> bool {
        self.interface_and_oper_status_flags & FLAG_CONNECTOR_PRESENT != 0
    }

    /// The current hardware address, when the row reports a usable one.
    ///
    /// `PhysicalAddressLength` bounds the read: the array is a fixed 32 bytes
    /// and a driver reporting an 8-byte address must not have the 24 bytes of
    /// padding after it read as part of the address.
    pub fn current_mac(&self) -> Option<&[u8]> {
        let length = (self.physical_address_length as usize).min(IF_MAX_PHYS_ADDRESS);

        (length >= 6).then(|| &self.physical_address[..6])
    }

    /// The permanent hardware address, when the row reports a usable one.
    ///
    /// This is what survives MAC randomisation, and what makes an identity
    /// match Fedora's for the same card.
    pub fn permanent_mac(&self) -> Option<&[u8]> {
        let length = (self.physical_address_length as usize).min(IF_MAX_PHYS_ADDRESS);

        (length >= 6).then(|| &self.permanent_physical_address[..6])
    }

    /// Whether this interface is wireless.
    ///
    /// Decided from the **physical medium**, not from `if_type`: many wireless
    /// drivers report `IF_TYPE_ETHERNET` for compatibility, so trusting the
    /// type would classify a Wi-Fi adapter as wired on a large share of real
    /// machines. `IF_TYPE_IEEE80211` is accepted as well, for the drivers that
    /// do report it.
    pub fn is_wireless(&self) -> bool {
        matches!(
            self.physical_medium_type,
            NDIS_PHYSICAL_MEDIUM_NATIVE_802_11 | NDIS_PHYSICAL_MEDIUM_WIRELESS_LAN
        ) || self.if_type == IF_TYPE_IEEE80211
    }

    /// The interface kind.
    pub fn kind(&self) -> NetworkInterfaceKind {
        if self.if_type == IF_TYPE_LOOPBACK {
            return NetworkInterfaceKind::Loopback;
        }
        if self.is_wireless() {
            return NetworkInterfaceKind::Wifi;
        }
        if matches!(self.if_type, IF_TYPE_TUNNEL | IF_TYPE_PPP) {
            return NetworkInterfaceKind::Tunnel;
        }
        if self.physical_medium_type == NDIS_PHYSICAL_MEDIUM_BLUETOOTH {
            return NetworkInterfaceKind::Other;
        }

        if self.if_type == IF_TYPE_ETHERNET {
            // A Hyper-V virtual switch, a VPN's virtual adapter and a loopback
            // adapter all report Ethernet without being hardware. The flag is
            // what tells them apart.
            return if self.is_hardware() {
                NetworkInterfaceKind::Ethernet
            } else {
                NetworkInterfaceKind::Virtual
            };
        }

        NetworkInterfaceKind::Other
    }

    /// The link state, from the operational status and the media state.
    ///
    /// Two separate questions, exactly as on Fedora: `OperStatus` says whether
    /// the adapter is enabled, and `MediaConnectState` whether a cable or a
    /// network is present. An Ethernet port that is enabled with nothing
    /// plugged in is `Disconnected`, not `Down`.
    pub fn link_state(&self) -> LinkState {
        if self.oper_status == IF_OPER_STATUS_DOWN {
            return LinkState::Down;
        }

        match self.media_connect_state {
            MEDIA_CONNECT_STATE_CONNECTED if self.oper_status == IF_OPER_STATUS_UP => {
                LinkState::Connected
            }
            MEDIA_CONNECT_STATE_DISCONNECTED => LinkState::Disconnected,
            _ if self.oper_status == IF_OPER_STATUS_UP => LinkState::Connected,
            _ => LinkState::Unknown,
        }
    }

    /// The link speed Windows reports, in bits per second.
    ///
    /// # Zero and `u64::MAX` are both "unknown"
    ///
    /// Windows uses `0` for an adapter that has negotiated nothing — an
    /// unplugged port — and some virtual adapters report `u64::MAX`, which is
    /// not an exabit-per-second link. Neither is published: a link speed of
    /// zero would claim a connection with no capacity, and the maximum would
    /// render as a nonsense figure.
    pub const fn link_speed(raw: u64) -> Option<u64> {
        // 10 Tbit/s: far beyond any interface, and below `u64::MAX`.
        const MAX_PLAUSIBLE: u64 = 10_000_000_000_000;

        if raw == 0 || raw > MAX_PLAUSIBLE {
            None
        } else {
            Some(raw)
        }
    }

    /// The receive link speed, when Windows reports a usable one.
    pub const fn receive_link_bps(&self) -> Option<u64> {
        Self::link_speed(self.receive_link_speed)
    }

    /// The transmit link speed, when Windows reports a usable one.
    pub const fn transmit_link_bps(&self) -> Option<u64> {
        Self::link_speed(self.transmit_link_speed)
    }

    /// The MTU, when it is plausible.
    ///
    /// Some virtual adapters report `0` or `u32::MAX`; neither is a frame
    /// size.
    pub const fn mtu_bytes(&self) -> Option<u32> {
        // 64 KiB is the largest any interface reports; loopback uses it.
        if self.mtu == 0 || self.mtu > 65_536 {
            None
        } else {
            Some(self.mtu)
        }
    }

    /// The counters, mapped onto the shared contract.
    ///
    /// Returns `None` when a packet total would overflow — two near-maximum
    /// counters must not wrap into a small number and publish a throughput
    /// derived from it.
    pub fn counters(&self) -> Option<NetworkCounters> {
        Some(NetworkCounters {
            receive_bytes: self.in_octets,
            transmit_bytes: self.out_octets,
            // Unicast *and* non-unicast: publishing only the first silently
            // drops every broadcast and multicast frame.
            receive_packets: self.in_ucast_pkts.checked_add(self.in_nucast_pkts)?,
            transmit_packets: self.out_ucast_pkts.checked_add(self.out_nucast_pkts)?,
            receive_errors: self.in_errors,
            transmit_errors: self.out_errors,
            // Windows calls a dropped frame a "discard". Same counter, and
            // still not the same thing as an error.
            receive_dropped: self.in_discards,
            transmit_dropped: self.out_discards,
        })
    }
}

/// The header of `MIB_IF_TABLE2`: a count, then the rows.
///
/// The rows are 8-byte aligned because `MIB_IF_ROW2` contains `u64` fields, so
/// the header occupies eight bytes rather than four.
pub const MIB_IF_TABLE2_HEADER: usize = 8;

// --- talking to Windows ----------------------------------------------------

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use crate::metrics::model::{MetricError, MetricErrorCode};
    use windows_sys::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2};

    /// An owned `MIB_IF_TABLE2`, freed when it goes out of scope.
    ///
    /// A guard rather than a bare pointer: `GetIfTable2` allocates, and every
    /// early return in a function that forgot `FreeMibTable` would leak a
    /// kilobyte per interface per refresh.
    struct IfTable {
        raw: *mut core::ffi::c_void,
        rows: usize,
    }

    impl IfTable {
        /// Asks Windows for the whole interface table.
        fn get() -> Result<Self, MetricError> {
            let mut raw: *mut core::ffi::c_void = std::ptr::null_mut();

            // SAFETY: `raw` is a null pointer Windows overwrites with its own
            // allocation, which the guard below owns and frees.
            let status = unsafe { GetIfTable2(std::ptr::addr_of_mut!(raw) as *mut _) };

            if status != 0 {
                return Err(from_win32(status, "the network interface table"));
            }
            if raw.is_null() {
                return Err(MetricError::new(
                    MetricErrorCode::Io,
                    "GetIfTable2 reported success and returned no table",
                ));
            }

            // SAFETY: on success Windows guarantees at least the four-byte
            // entry count at the start of the allocation.
            let rows = unsafe { std::ptr::read_unaligned(raw as *const u32) } as usize;

            Ok(Self { raw, rows })
        }

        /// The rows, as a slice.
        ///
        /// # Safety
        ///
        /// The returned slice borrows the table and must not outlive it.
        fn rows(&self) -> &[MibIfRow2] {
            if self.rows == 0 {
                return &[];
            }

            // SAFETY: Windows laid out `self.rows` contiguous `MIB_IF_ROW2`
            // values after the eight-byte header, and the compile-time
            // assertion above pins the element size to the documented one.
            unsafe {
                std::slice::from_raw_parts(
                    (self.raw as *const u8).add(MIB_IF_TABLE2_HEADER) as *const MibIfRow2,
                    self.rows,
                )
            }
        }
    }

    impl Drop for IfTable {
        fn drop(&mut self) {
            if !self.raw.is_null() {
                // SAFETY: `self.raw` came from `GetIfTable2` and has not been
                // freed; `FreeMibTable` is its documented deallocator.
                unsafe { FreeMibTable(self.raw as *const _) };
            }
        }
    }

    /// Reads every interface from Windows, in one call.
    ///
    /// The rows are **copied out** before the table is freed, so no raw
    /// pointer ever leaves this function and nothing above it can hold a
    /// dangling reference.
    pub fn table() -> Result<Vec<MibIfRow2>, MetricError> {
        let table = IfTable::get()?;

        Ok(table.rows().to_vec())
    }
}

#[cfg(target_os = "windows")]
pub use imp::table;

/// Compiled on non-Windows hosts so everything above — the layout, the field
/// reading, the kind and state mapping — type checks and is unit-tested there.
/// Never reached: only the Windows provider calls it.
#[cfg(not(target_os = "windows"))]
pub fn table() -> Result<Vec<MibIfRow2>, crate::metrics::model::MetricError> {
    use crate::metrics::model::{MetricError, MetricErrorCode};

    Err(MetricError::new(
        MetricErrorCode::Unsupported,
        "GetIfTable2 is a Windows interface",
    ))
}

/// Maps a Win32 status onto the error code that describes it honestly.
pub fn from_win32(status: u32, what: &str) -> crate::metrics::model::MetricError {
    use crate::metrics::model::{MetricError, MetricErrorCode};

    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_NOT_SUPPORTED: u32 = 50;
    const ERROR_INVALID_PARAMETER: u32 = 87;
    const ERROR_NOT_FOUND: u32 = 1168;
    const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;

    let (code, message) = match status {
        ERROR_ACCESS_DENIED => (
            MetricErrorCode::PermissionDenied,
            format!("Windows refused {what} to this process"),
        ),
        ERROR_NOT_SUPPORTED | ERROR_INVALID_PARAMETER => (
            MetricErrorCode::Unsupported,
            format!("this version of Windows does not provide {what}"),
        ),
        ERROR_NOT_FOUND => (
            MetricErrorCode::NotDetected,
            format!("{what} found nothing to report"),
        ),
        ERROR_NOT_ENOUGH_MEMORY => (
            MetricErrorCode::Io,
            format!("Windows could not allocate {what}"),
        ),
        other => (
            MetricErrorCode::Io,
            format!("{what} failed with Windows error {other}"),
        ),
    };

    MetricError::new(code, message)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Writes a Rust string into a fixed UTF-16 array, NUL-terminated.
    pub fn utf16_array<const N: usize>(value: &str) -> [u16; N] {
        let mut out = [0_u16; N];
        for (slot, unit) in out.iter_mut().zip(value.encode_utf16()) {
            *slot = unit;
        }
        // Guarantee termination even for an over-long value.
        if let Some(last) = out.last_mut() {
            *last = 0;
        }
        out
    }

    fn mac(address: [u8; 6]) -> [u8; IF_MAX_PHYS_ADDRESS] {
        let mut out = [0_u8; IF_MAX_PHYS_ADDRESS];
        out[..6].copy_from_slice(&address);
        out
    }

    /// A connected Wi-Fi adapter, reporting Ethernet as its type — which is
    /// what a great many real drivers do.
    pub fn wifi(permanent: [u8; 6], current: [u8; 6]) -> MibIfRow2 {
        MibIfRow2 {
            interface_index: 12,
            interface_guid: Guid {
                data1: 0x6C5A_1B2D,
                data2: 0x1111,
                data3: 0x2222,
                data4: [0x33, 0x33, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44],
            },
            alias: utf16_array("Wi-Fi"),
            description: utf16_array("Intel(R) Wi-Fi 6E AX211 160MHz"),
            physical_address_length: 6,
            physical_address: mac(current),
            permanent_physical_address: mac(permanent),
            mtu: 1500,
            if_type: IF_TYPE_ETHERNET,
            physical_medium_type: NDIS_PHYSICAL_MEDIUM_NATIVE_802_11,
            interface_and_oper_status_flags: FLAG_HARDWARE_INTERFACE,
            oper_status: IF_OPER_STATUS_UP,
            media_connect_state: MEDIA_CONNECT_STATE_CONNECTED,
            transmit_link_speed: 866_700_000,
            receive_link_speed: 866_700_000,
            in_octets: 3_210_730_391,
            in_ucast_pkts: 3_500_000,
            in_nucast_pkts: 91_166,
            out_octets: 868_552_645,
            out_ucast_pkts: 1_200_000,
            out_nucast_pkts: 67_925,
            ..MibIfRow2::default()
        }
    }

    /// A wired Ethernet port with nothing plugged in.
    pub fn ethernet_unplugged(permanent: [u8; 6]) -> MibIfRow2 {
        MibIfRow2 {
            interface_index: 8,
            interface_guid: Guid {
                data1: 0xAAAA_BBBB,
                ..Guid::default()
            },
            alias: utf16_array("Ethernet"),
            description: utf16_array("Realtek Gaming 2.5GbE Family Controller"),
            physical_address_length: 6,
            physical_address: mac(permanent),
            permanent_physical_address: mac(permanent),
            mtu: 1500,
            if_type: IF_TYPE_ETHERNET,
            physical_medium_type: 14, // NdisPhysicalMedium802_3
            interface_and_oper_status_flags: FLAG_HARDWARE_INTERFACE | FLAG_CONNECTOR_PRESENT,
            oper_status: IF_OPER_STATUS_DOWN,
            media_connect_state: MEDIA_CONNECT_STATE_DISCONNECTED,
            // Windows reports zero for an unnegotiated link.
            transmit_link_speed: 0,
            receive_link_speed: 0,
            ..MibIfRow2::default()
        }
    }

    /// The Windows loopback pseudo-interface.
    pub fn loopback() -> MibIfRow2 {
        MibIfRow2 {
            interface_index: 1,
            alias: utf16_array("Loopback Pseudo-Interface 1"),
            description: utf16_array("Software Loopback Interface 1"),
            mtu: 65_536,
            if_type: IF_TYPE_LOOPBACK,
            oper_status: IF_OPER_STATUS_UP,
            media_connect_state: MEDIA_CONNECT_STATE_CONNECTED,
            transmit_link_speed: 1_073_741_824,
            receive_link_speed: 1_073_741_824,
            ..MibIfRow2::default()
        }
    }

    /// A VPN adapter — a tunnel with no permanent address.
    pub fn vpn(name: &str) -> MibIfRow2 {
        MibIfRow2 {
            interface_index: 24,
            interface_guid: Guid {
                data1: 0xCCCC_DDDD,
                ..Guid::default()
            },
            alias: utf16_array(name),
            description: utf16_array("WireGuard Tunnel"),
            physical_address_length: 0,
            mtu: 1420,
            if_type: IF_TYPE_TUNNEL,
            oper_status: IF_OPER_STATUS_UP,
            media_connect_state: MEDIA_CONNECT_STATE_CONNECTED,
            // Some virtual adapters report the maximum, which is not a link.
            transmit_link_speed: u64::MAX,
            receive_link_speed: u64::MAX,
            ..MibIfRow2::default()
        }
    }

    /// A Hyper-V virtual switch: Ethernet by type, not hardware.
    pub fn virtual_switch() -> MibIfRow2 {
        MibIfRow2 {
            interface_index: 30,
            interface_guid: Guid {
                data1: 0xEEEE_FFFF,
                ..Guid::default()
            },
            alias: utf16_array("vEthernet (Default Switch)"),
            description: utf16_array("Hyper-V Virtual Ethernet Adapter"),
            physical_address_length: 6,
            physical_address: mac([0x00, 0x15, 0x5d, 0x01, 0x02, 0x03]),
            permanent_physical_address: mac([0x00, 0x15, 0x5d, 0x01, 0x02, 0x03]),
            mtu: 1500,
            if_type: IF_TYPE_ETHERNET,
            physical_medium_type: 14,
            interface_and_oper_status_flags: 0, // not hardware
            oper_status: IF_OPER_STATUS_UP,
            media_connect_state: MEDIA_CONNECT_STATE_CONNECTED,
            transmit_link_speed: 10_000_000_000,
            receive_link_speed: 10_000_000_000,
            ..MibIfRow2::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::metrics::model::MetricErrorCode;

    const WIFI_MAC: [u8; 6] = [0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2];
    const ETH_MAC: [u8; 6] = [0xd8, 0x43, 0xae, 0x32, 0x8a, 0xc9];

    // --- layout ------------------------------------------------------------

    #[test]
    fn the_structure_matches_the_documented_size() {
        // Every counter PULSE reads lives at the end, so a field declared at
        // the wrong width shifts all of them.
        assert_eq!(std::mem::size_of::<MibIfRow2>(), 1352);
        assert_eq!(std::mem::align_of::<MibIfRow2>(), 8);
    }

    #[test]
    fn a_guid_renders_in_the_canonical_windows_form() {
        let guid = Guid {
            data1: 0x6C5A_1B2D,
            data2: 0x1111,
            data3: 0x2222,
            data4: [0x33, 0x33, 0x44, 0x44, 0x44, 0x44, 0x55, 0x55],
        };

        assert_eq!(guid.to_text(), "{6C5A1B2D-1111-2222-3333-444444445555}");
        assert!(!guid.is_zero());
        assert!(Guid::default().is_zero());
    }

    #[test]
    fn a_utf16_field_stops_at_its_terminator() {
        let row = wifi(WIFI_MAC, WIFI_MAC);

        assert_eq!(row.description_text(), "Intel(R) Wi-Fi 6E AX211 160MHz");
        assert_eq!(row.alias_text(), "Wi-Fi");
        // Not padded with the 200-odd NULs that follow.
        assert_eq!(row.alias_text().len(), 5);
    }

    #[test]
    fn an_over_long_description_stays_terminated() {
        let long = "A".repeat(500);
        let row = MibIfRow2 {
            description: utf16_array(&long),
            ..MibIfRow2::default()
        };

        assert!(row.description_text().len() < IF_MAX_STRING);
    }

    // --- addresses ---------------------------------------------------------

    #[test]
    fn reads_both_addresses_of_a_wifi_adapter() {
        let randomised = [0x02, 0xab, 0xcd, 0xef, 0x12, 0x34];
        let row = wifi(WIFI_MAC, randomised);

        assert_eq!(row.permanent_mac(), Some(&WIFI_MAC[..]));
        assert_eq!(row.current_mac(), Some(&randomised[..]));
    }

    #[test]
    fn an_address_shorter_than_six_bytes_is_refused() {
        // A driver reporting a 4-byte address must not have the padding after
        // it read as part of an address.
        let row = MibIfRow2 {
            physical_address_length: 4,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };

        assert_eq!(row.permanent_mac(), None);
        assert_eq!(row.current_mac(), None);
    }

    #[test]
    fn an_interface_with_no_address_reports_none() {
        let row = vpn("WireGuard");

        assert_eq!(row.physical_address_length, 0);
        assert_eq!(row.permanent_mac(), None);
    }

    #[test]
    fn an_address_length_past_the_array_is_clamped_rather_than_read_out_of_bounds() {
        let row = MibIfRow2 {
            physical_address_length: 9999,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };

        assert_eq!(row.permanent_mac(), Some(&WIFI_MAC[..]));
    }

    // --- kinds -------------------------------------------------------------

    #[test]
    fn a_wifi_adapter_reporting_ethernet_is_still_recognised_as_wireless() {
        // The trap: a great many wireless drivers report `IF_TYPE_ETHERNET`
        // for compatibility, exactly as Linux reports `ARPHRD_ETHER`. The
        // physical medium is what actually distinguishes them.
        let row = wifi(WIFI_MAC, WIFI_MAC);

        assert_eq!(row.if_type, IF_TYPE_ETHERNET);
        assert!(row.is_wireless());
        assert_eq!(row.kind(), NetworkInterfaceKind::Wifi);
    }

    #[test]
    fn a_driver_reporting_ieee80211_is_also_recognised() {
        let row = MibIfRow2 {
            if_type: IF_TYPE_IEEE80211,
            physical_medium_type: 0,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };

        assert!(row.is_wireless());
    }

    #[test]
    fn the_older_wireless_lan_medium_is_recognised_too() {
        let row = MibIfRow2 {
            physical_medium_type: NDIS_PHYSICAL_MEDIUM_WIRELESS_LAN,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };

        assert!(row.is_wireless());
    }

    #[test]
    fn a_wired_port_is_ethernet() {
        let row = ethernet_unplugged(ETH_MAC);

        assert!(!row.is_wireless());
        assert_eq!(row.kind(), NetworkInterfaceKind::Ethernet);
        assert!(row.has_connector());
    }

    #[test]
    fn a_hyper_v_switch_is_virtual_rather_than_ethernet() {
        // Ethernet by type, and not hardware — which is what tells them apart.
        let row = virtual_switch();

        assert_eq!(row.if_type, IF_TYPE_ETHERNET);
        assert!(!row.is_hardware());
        assert_eq!(row.kind(), NetworkInterfaceKind::Virtual);
        assert!(!row.kind().is_primary());
    }

    #[test]
    fn a_vpn_adapter_is_a_tunnel() {
        assert_eq!(vpn("WireGuard").kind(), NetworkInterfaceKind::Tunnel);
    }

    #[test]
    fn loopback_is_recognised_and_is_not_published() {
        let row = loopback();

        assert_eq!(row.kind(), NetworkInterfaceKind::Loopback);
        assert!(!row.kind().is_published());
    }

    #[test]
    fn a_bluetooth_adapter_is_not_mistaken_for_ethernet() {
        let row = MibIfRow2 {
            physical_medium_type: NDIS_PHYSICAL_MEDIUM_BLUETOOTH,
            ..ethernet_unplugged(ETH_MAC)
        };

        assert_eq!(row.kind(), NetworkInterfaceKind::Other);
    }

    // --- link state --------------------------------------------------------

    #[test]
    fn an_unplugged_port_is_disconnected_not_down() {
        // "You disabled it" and "nothing is plugged in" are different, and the
        // user can act on the second.
        let row = MibIfRow2 {
            oper_status: IF_OPER_STATUS_UP,
            media_connect_state: MEDIA_CONNECT_STATE_DISCONNECTED,
            ..ethernet_unplugged(ETH_MAC)
        };

        assert_eq!(row.link_state(), LinkState::Disconnected);
    }

    #[test]
    fn a_disabled_adapter_is_down() {
        assert_eq!(ethernet_unplugged(ETH_MAC).link_state(), LinkState::Down);
    }

    #[test]
    fn a_connected_adapter_is_connected() {
        assert_eq!(wifi(WIFI_MAC, WIFI_MAC).link_state(), LinkState::Connected);
    }

    #[test]
    fn an_unrecognised_media_state_on_an_up_adapter_is_treated_as_connected() {
        let row = MibIfRow2 {
            oper_status: IF_OPER_STATUS_UP,
            media_connect_state: 0, // MediaConnectStateUnknown
            ..wifi(WIFI_MAC, WIFI_MAC)
        };

        assert_eq!(row.link_state(), LinkState::Connected);
    }

    // --- link speed --------------------------------------------------------

    #[test]
    fn reads_a_plausible_link_speed() {
        let row = wifi(WIFI_MAC, WIFI_MAC);

        assert_eq!(row.receive_link_bps(), Some(866_700_000));
        assert_eq!(row.transmit_link_bps(), Some(866_700_000));
    }

    #[test]
    fn zero_is_unknown_rather_than_a_link_with_no_capacity() {
        // What Windows reports for an unplugged port.
        let row = ethernet_unplugged(ETH_MAC);

        assert_eq!(row.receive_link_speed, 0);
        assert_eq!(row.receive_link_bps(), None);
    }

    #[test]
    fn the_maximum_is_unknown_rather_than_an_exabit_link() {
        // What several virtual adapters report.
        let row = vpn("WireGuard");

        assert_eq!(row.receive_link_speed, u64::MAX);
        assert_eq!(row.receive_link_bps(), None);
    }

    #[test]
    fn asymmetric_link_speeds_are_kept_apart() {
        let row = MibIfRow2 {
            receive_link_speed: 1_200_000_000,
            transmit_link_speed: 600_000_000,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };

        assert_eq!(row.receive_link_bps(), Some(1_200_000_000));
        assert_eq!(row.transmit_link_bps(), Some(600_000_000));
    }

    #[test]
    fn common_ethernet_speeds_all_survive() {
        for bps in [
            10_000_000_u64,
            100_000_000,
            1_000_000_000,
            2_500_000_000,
            10_000_000_000,
        ] {
            assert_eq!(MibIfRow2::link_speed(bps), Some(bps), "{bps}");
        }
    }

    // --- MTU ---------------------------------------------------------------

    #[test]
    fn reads_a_plausible_mtu_and_refuses_the_rest() {
        assert_eq!(wifi(WIFI_MAC, WIFI_MAC).mtu_bytes(), Some(1500));
        assert_eq!(loopback().mtu_bytes(), Some(65_536));

        let broken = MibIfRow2 {
            mtu: u32::MAX,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };
        assert_eq!(broken.mtu_bytes(), None);

        let zero = MibIfRow2 {
            mtu: 0,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };
        assert_eq!(zero.mtu_bytes(), None);
    }

    // --- counters ----------------------------------------------------------

    #[test]
    fn packet_totals_add_unicast_and_non_unicast() {
        // Publishing only the unicast half silently drops every broadcast and
        // multicast frame — a large and variable share on a real network.
        let row = wifi(WIFI_MAC, WIFI_MAC);
        let counters = row.counters().expect("valid");

        assert_eq!(counters.receive_packets, 3_500_000 + 91_166);
        assert_eq!(counters.transmit_packets, 1_200_000 + 67_925);
        assert_ne!(counters.receive_packets, row.in_ucast_pkts);
    }

    #[test]
    fn bytes_come_from_the_octet_counters() {
        let counters = wifi(WIFI_MAC, WIFI_MAC).counters().expect("valid");

        assert_eq!(counters.receive_bytes, 3_210_730_391);
        assert_eq!(counters.transmit_bytes, 868_552_645);
    }

    #[test]
    fn discards_and_errors_are_read_from_different_fields() {
        let row = MibIfRow2 {
            in_errors: 11,
            out_errors: 22,
            in_discards: 33,
            out_discards: 44,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };
        let counters = row.counters().expect("valid");

        assert_eq!(counters.receive_errors, 11);
        assert_eq!(counters.receive_dropped, 33);
        assert_ne!(counters.receive_errors, counters.receive_dropped);
    }

    #[test]
    fn an_overflowing_packet_total_is_refused_rather_than_wrapped() {
        let row = MibIfRow2 {
            in_ucast_pkts: u64::MAX,
            in_nucast_pkts: 1,
            ..wifi(WIFI_MAC, WIFI_MAC)
        };

        assert_eq!(row.counters(), None);
    }

    #[test]
    fn windows_and_linux_counters_describe_the_same_thing() {
        // The point of one shared counter type: given the same activity, both
        // platforms hand the tracker the same numbers.
        let windows = MibIfRow2 {
            in_octets: 4096,
            out_octets: 2048,
            in_ucast_pkts: 30,
            in_nucast_pkts: 2,
            out_ucast_pkts: 16,
            out_nucast_pkts: 0,
            in_errors: 1,
            out_errors: 2,
            in_discards: 3,
            out_discards: 4,
            ..MibIfRow2::default()
        }
        .counters()
        .expect("valid");

        let linux = crate::platform::linux::network::rtnetlink::parse_stats64(
            &crate::platform::linux::network::rtnetlink::fixtures::stats64(
                32, 16, 4096, 2048, 1, 2, 3, 4,
            ),
        )
        .expect("valid");

        assert_eq!(windows, linux);
    }

    // --- error mapping -----------------------------------------------------

    #[test]
    fn access_denied_is_never_reported_as_unsupported() {
        let error = from_win32(5, "the network interface table");

        assert_eq!(error.code, MetricErrorCode::PermissionDenied);
        assert_eq!(
            crate::metrics::wellknown::availability_for(error).status_str(),
            "permissionDenied"
        );
    }

    #[test]
    fn an_unimplemented_call_is_unsupported() {
        for status in [50, 87] {
            assert_eq!(
                from_win32(status, "the WLAN quality query").code,
                MetricErrorCode::Unsupported,
                "status {status}"
            );
        }
    }

    #[test]
    fn an_unrecognised_status_stays_transient() {
        let error = from_win32(1117, "the network interface table");

        assert_eq!(error.code, MetricErrorCode::Io);
        assert!(crate::metrics::wellknown::availability_for(error).is_transient());
    }

    #[test]
    fn an_error_names_what_failed() {
        let message = from_win32(5, "the network interface table").message;
        assert!(message.contains("the network interface table"), "{message}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_stand_in_reports_the_interface_as_windows_only() {
        assert_eq!(
            table().expect_err("unsupported").code,
            MetricErrorCode::Unsupported
        );
    }
}
