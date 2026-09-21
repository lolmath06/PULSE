//! The Windows network provider.
//!
//! **One provider owns every network metric on the machine**, exactly as on
//! Fedora:
//!
//! ```text
//! windows.network
//!  ├── inventory   GetIfTable2 / MIB_IF_ROW2   interfaces, identity, MTU, state
//!  ├── traffic     the same table              counters, same instant
//!  ├── link speed  the same table              ReceiveLinkSpeed, TransmitLinkSpeed
//!  └── Wi-Fi       WLAN API                    realtime connection quality
//! ```
//!
//! Wi-Fi is a capability, never a provider of its own and never a
//! precondition: no wireless hardware, a stopped WLAN service or a Windows
//! build without the realtime-quality opcode all leave the full generic
//! catalog publishing normally.
//!
//! # Not PDH
//!
//! `\Network Interface(*)\Bytes Received/sec` is the obvious source and is not
//! used, for the same reason `\PhysicalDisk` was not used for storage: its
//! instance names are presentation strings derived from the adapter
//! description, they are localised, they are mangled — parentheses and slashes
//! are rewritten — and correlating one back to a `MIB_IF_ROW2` means matching
//! munged text. `GetIfTable2` is asked of the stack directly and returns the
//! identity alongside the counters, so there is nothing to correlate.
//!
//! # Not executed
//!
//! This code compiles and type checks for `x86_64-pc-windows-msvc` through
//! `tools/windows-check`, and every pure part of it — the `MIB_IF_ROW2`
//! layout, the kind and state mapping, the counter arithmetic, the WLAN
//! response parsing, the identity rules — is unit-tested on Fedora against
//! synthetic structures. **It has not been run on a Windows machine.** See
//! `docs/platforms/windows.md`.

pub mod iftable;
pub mod wlan;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricRef, MetricSample, ProviderId, SourceId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::network::{
    self as wellknown, format_mac, interface_identity, LinkState, NetworkCapabilities,
    NetworkInterfaceDescriptor, NetworkInterfaceState, NetworkIoTracker, NetworkSnapshot,
    NetworkTelemetry, NetworkTraffic, WifiCapabilities, WifiLinkInfo,
};

use iftable::MibIfRow2;

/// Identifier of the Windows network provider.
pub const PROVIDER_ID: &str = "windows.network";

/// One interface, with what is needed to find it again.
#[derive(Debug, Clone)]
pub struct Interface {
    pub descriptor: NetworkInterfaceDescriptor,
    /// The interface LUID. Windows documents it as stable for the lifetime of
    /// the interface and, unlike the index, not reused while it exists — so it
    /// is what a later table is matched on. **Still not an identity**: it does
    /// not survive a reinstall and means nothing on another OS.
    pub luid: u64,
    /// The interface GUID, in text, used to find this adapter's WLAN entry.
    pub guid: String,
}

/// Turns one table row into a descriptor.
///
/// Pure, so the whole mapping — identity choice, kind, capabilities — is
/// tested on Fedora against synthetic rows rather than on a Windows machine
/// with that hardware in it.
pub fn describe(row: &MibIfRow2) -> Option<Interface> {
    let kind = row.kind();
    let description = row.description_text();
    let alias = row.alias_text();

    // The GUID is the fallback identity for a virtual adapter with no
    // permanent address — a VPN tunnel, a Hyper-V switch — because Windows
    // keeps it across reboots and renames while the interface exists.
    let guid = row.interface_guid;
    let guid_text = (!guid.is_zero()).then(|| guid.to_text());

    let name = if description.is_empty() {
        alias.clone()
    } else {
        description.clone()
    };

    let (source_id, identity) = interface_identity(
        row.permanent_mac(),
        guid_text.as_deref(),
        row.current_mac(),
        // The alias is the better fallback name: it is what the user sees in
        // Windows, where the description names the silicon.
        if alias.is_empty() { &name } else { &alias },
    )?;

    let mut capabilities = NetworkCapabilities::all_available();

    if row.counters().is_none() {
        capabilities = capabilities.with_traffic(Availability::provider_error(
            crate::metrics::model::MetricError::new(
                crate::metrics::model::MetricErrorCode::Parse,
                "this adapter's packet counters overflowed when combined",
            ),
        ));
    }

    if row.mtu_bytes().is_none() {
        capabilities.mtu = Availability::not_detected("this adapter reports no usable MTU");
    }

    Some(Interface {
        luid: row.interface_luid,
        guid: guid_text.unwrap_or_default(),
        descriptor: NetworkInterfaceDescriptor {
            source_id,
            // The description names the hardware — `Intel(R) Wi-Fi 6E AX211` —
            // where the alias is whatever the user renamed the connection to.
            // The hardware name is the more useful label and the more stable
            // one.
            display_name: name,
            os_name: alias,
            identity,
            kind,
            permanent_mac: row.permanent_mac().and_then(format_mac),
            current_mac: row.current_mac().and_then(format_mac),
            backend: "iftable2",
            capabilities,
            wifi: kind.is_wireless().then(WifiCapabilities::all_available),
        },
    })
}

/// Turns a whole table into descriptors, in a deterministic order.
pub fn describe_all(rows: &[MibIfRow2]) -> Vec<Interface> {
    let mut interfaces: Vec<Interface> = rows.iter().filter_map(describe).collect();

    // Two rows resolving to one `SourceId` would make the engine reject the
    // provider outright.
    interfaces.sort_by(|left, right| {
        left.descriptor
            .source_id
            .cmp(&right.descriptor.source_id)
            .then_with(|| left.luid.cmp(&right.luid))
    });
    interfaces.dedup_by(|left, right| left.descriptor.source_id == right.descriptor.source_id);

    interfaces
}

/// Reads one row's dynamic state.
pub fn state_of(row: &MibIfRow2) -> NetworkInterfaceState {
    NetworkInterfaceState {
        link: Some(row.link_state()),
        mtu: row.mtu_bytes(),
        receive_link_bps: row.receive_link_bps(),
        transmit_link_bps: row.transmit_link_bps(),
        // Local addresses would come from `GetAdaptersAddresses`, which this
        // phase does not call: the Windows card shows the interface's state
        // and traffic without them rather than adding a second enumeration
        // for a display-only field.
        addresses: Vec::new(),
    }
}

/// What the provider discovered when it was built.
#[derive(Debug)]
pub struct NetworkInventory {
    pub interfaces: Vec<Interface>,
}

impl NetworkInventory {
    /// Builds the inventory from raw rows.
    ///
    /// Pure, and therefore the seam the tests use: the FFI produces rows, this
    /// turns them into the catalog, and the two are checked separately.
    pub fn from_rows(rows: &[MibIfRow2]) -> Self {
        Self {
            interfaces: describe_all(rows),
        }
    }

    fn discover() -> Self {
        Self::from_rows(&iftable::table().unwrap_or_default())
    }

    /// Builds a counter snapshot from a table.
    fn snapshot(&self, rows: &[MibIfRow2], at_ms: u64) -> NetworkSnapshot {
        let mut snapshot = NetworkSnapshot::new(at_ms);

        let by_luid: BTreeMap<u64, &MibIfRow2> =
            rows.iter().map(|row| (row.interface_luid, row)).collect();

        for interface in &self.interfaces {
            if let Some(row) = by_luid.get(&interface.luid) {
                if let Some(counters) = row.counters() {
                    snapshot.insert(interface.descriptor.source_id.clone(), counters);
                }
            }
        }

        snapshot
    }
}

/// Publishes every network metric on Windows.
pub struct WindowsNetworkProvider {
    id: ProviderId,
    inventory: NetworkInventory,
    traffic: NetworkIoTracker,
    origin: Instant,
}

impl std::fmt::Debug for WindowsNetworkProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsNetworkProvider")
            .field("id", &self.id)
            .field("interfaces", &self.inventory.interfaces.len())
            .finish()
    }
}

impl WindowsNetworkProvider {
    pub fn new() -> Self {
        let origin = Instant::now();
        let inventory = NetworkInventory::discover();
        let traffic = NetworkIoTracker::new();

        // The same baseline-at-startup as Fedora, for the same reason: the
        // user's first Refresh should produce a real interval, and the first
        // request should say it is waiting rather than report 0 B/s.
        let rows = iftable::table().unwrap_or_default();
        traffic.prime(&inventory.snapshot(&rows, elapsed_ms(&origin)));

        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
            inventory,
            traffic,
            origin,
        }
    }

    pub fn interfaces(&self) -> Vec<NetworkInterfaceDescriptor> {
        self.inventory
            .interfaces
            .iter()
            .map(|interface| interface.descriptor.clone())
            .collect()
    }

    fn interface_for(&self, source: &str) -> Option<&Interface> {
        self.inventory
            .interfaces
            .iter()
            .find(|interface| interface.descriptor.source_id.as_str() == source)
    }

    /// How many interfaces PULSE publishes — loopback excluded.
    fn published_count(&self) -> usize {
        self.inventory
            .interfaces
            .iter()
            .filter(|interface| interface.descriptor.kind.is_published())
            .count()
    }
}

/// Milliseconds since an origin, on the monotonic clock.
fn elapsed_ms(origin: &Instant) -> u64 {
    Instant::now().duration_since(*origin).as_millis() as u64
}

impl Default for WindowsNetworkProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for WindowsNetworkProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        Ok(wellknown::definitions(&self.id, &self.interfaces()))
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        // One table serves every interface in this request, and captures them
        // all at one instant.
        let rows = iftable::table().unwrap_or_default();
        let snapshot = self.inventory.snapshot(&rows, elapsed_ms(&self.origin));
        let rates = self.traffic.update(&snapshot)?;

        let by_luid: BTreeMap<u64, &MibIfRow2> =
            rows.iter().map(|row| (row.interface_luid, row)).collect();

        let states: BTreeMap<SourceId, NetworkInterfaceState> = self
            .inventory
            .interfaces
            .iter()
            .map(|interface| {
                let state = by_luid
                    .get(&interface.luid)
                    .map(|row| state_of(row))
                    .unwrap_or_default();

                (interface.descriptor.source_id.clone(), state)
            })
            .collect();

        // The WLAN service is asked once per refresh, and only when the
        // request touches a wireless interface.
        let wants_wifi = requested.iter().any(|reference| {
            self.interface_for(reference.source_id.as_str())
                .is_some_and(|interface| interface.descriptor.is_wireless())
        });
        let links = if wants_wifi {
            wlan::links()
        } else {
            BTreeMap::new()
        };

        let mut telemetry: BTreeMap<&str, NetworkTelemetry> = BTreeMap::new();

        for reference in requested {
            let source = reference.source_id.as_str();

            if let Some(interface) = self.interface_for(source) {
                telemetry.entry(source).or_insert_with(|| NetworkTelemetry {
                    traffic: match rates.get(&interface.descriptor.source_id) {
                        Some(NetworkTraffic::Ready(rates)) => Some(*rates),
                        _ => None,
                    },
                    state: states
                        .get(&interface.descriptor.source_id)
                        .cloned()
                        .unwrap_or_default(),
                    wifi: interface.descriptor.is_wireless().then(|| {
                        links
                            .get(&interface.guid)
                            .cloned()
                            .unwrap_or_else(WifiLinkInfo::disconnected)
                    }),
                });
            }
        }

        Ok(requested
            .iter()
            .map(|reference| self.sample_one(reference, &telemetry, &rates, &states))
            .collect())
    }
}

impl WindowsNetworkProvider {
    fn sample_one(
        &self,
        reference: &MetricRef,
        telemetry: &BTreeMap<&str, NetworkTelemetry>,
        rates: &BTreeMap<SourceId, NetworkTraffic>,
        states: &BTreeMap<SourceId, NetworkInterfaceState>,
    ) -> MetricSample {
        let key = reference.key.as_str();

        match key {
            wellknown::INTERFACE_COUNT => {
                return MetricSample::number(reference.clone(), self.published_count() as f64)
            }
            wellknown::INTERFACE_UP_COUNT => {
                let connected = self
                    .inventory
                    .interfaces
                    .iter()
                    .filter(|interface| interface.descriptor.kind.is_published())
                    .filter(|interface| {
                        states
                            .get(&interface.descriptor.source_id)
                            .and_then(|state| state.link)
                            .is_some_and(LinkState::is_connected)
                    })
                    .count();

                return MetricSample::number(reference.clone(), connected as f64);
            }
            _ => {}
        }

        let source = reference.source_id.as_str();

        let Some(interface) = self.interface_for(source) else {
            return MetricSample::unavailable(
                reference.clone(),
                Availability::not_registered(format!(
                    "'{reference}' does not name an interface this provider inventoried"
                )),
            );
        };

        let Some(telemetry) = telemetry.get(source) else {
            return MetricSample::unavailable(
                reference.clone(),
                Availability::temporarily_unavailable(
                    "this interface was not read during this sample",
                ),
            );
        };

        match telemetry.value_for(key) {
            Some(value) => MetricSample::number(reference.clone(), value),
            None => MetricSample::unavailable(
                reference.clone(),
                match interface_reason(interface, key, telemetry, rates) {
                    // A declared-available capability with no value at this
                    // instant would otherwise produce a sample claiming
                    // success with nothing in it.
                    Availability::Available => Availability::temporarily_unavailable(
                        "this interface reported no value for this metric during the sample",
                    ),
                    reason => reason,
                },
            ),
        }
    }
}

/// Explains why one interface could not answer one metric.
///
/// The same rules as Fedora's, so a user moving between the two sees the same
/// distinction between "waiting for a second sample", "nothing is connected"
/// and "this cannot be measured here".
fn interface_reason(
    interface: &Interface,
    key: &str,
    telemetry: &NetworkTelemetry,
    rates: &BTreeMap<SourceId, NetworkTraffic>,
) -> Availability {
    if wellknown::TRAFFIC_KEYS.contains(&key) {
        return match rates.get(&interface.descriptor.source_id) {
            Some(NetworkTraffic::NeedsAnotherSample(reason)) => {
                Availability::temporarily_unavailable(reason.reason())
            }
            Some(NetworkTraffic::Ready(_)) => Availability::temporarily_unavailable(
                "this counter was not reported during the interval",
            ),
            None => interface.descriptor.capabilities.receive_bytes.clone(),
        };
    }

    if wellknown::LINK_KEYS.contains(&key) {
        return match telemetry.state.link {
            Some(state) if !state.is_connected() => Availability::temporarily_unavailable(
                "this interface negotiates no link speed while it has no carrier",
            ),
            _ => Availability::unsupported("this adapter reports no link speed to Windows"),
        };
    }

    if wellknown::PER_WIFI_KEYS.contains(&key) {
        return match &telemetry.wifi {
            Some(info) if info.is_connected() => Availability::unsupported(
                "this adapter's driver reports no such figure for the current association",
            ),
            Some(_) => wifi_unavailable(),
            None => Availability::not_detected("this interface has no wireless backend"),
        };
    }

    capability_for(&interface.descriptor.capabilities, key)
}

/// Why a Wi-Fi figure is missing on Windows.
///
/// Deliberately says both possible reasons, because PULSE genuinely cannot
/// tell them apart from here: the radio may be unassociated, or this Windows
/// build may not implement the realtime-quality opcode. What it does **not**
/// do is fall back to the location-gated connection API to find out — see
/// [`wlan`].
fn wifi_unavailable() -> Availability {
    Availability::temporarily_unavailable(
        "this adapter is not associated with a network, or this version of Windows does not \
         provide the realtime connection quality interface. PULSE does not fall back to the \
         location-gated connection API to obtain a signal reading.",
    )
}

/// The declared availability of one metric on one interface.
fn capability_for(capabilities: &NetworkCapabilities, key: &str) -> Availability {
    match key {
        wellknown::RECEIVE_BYTES => capabilities.receive_bytes.clone(),
        wellknown::TRANSMIT_BYTES => capabilities.transmit_bytes.clone(),
        wellknown::RECEIVE_PACKETS => capabilities.receive_packets.clone(),
        wellknown::TRANSMIT_PACKETS => capabilities.transmit_packets.clone(),
        wellknown::RECEIVE_ERRORS => capabilities.receive_errors.clone(),
        wellknown::TRANSMIT_ERRORS => capabilities.transmit_errors.clone(),
        wellknown::RECEIVE_DROPPED => capabilities.receive_dropped.clone(),
        wellknown::TRANSMIT_DROPPED => capabilities.transmit_dropped.clone(),
        wellknown::LINK_RECEIVE_SPEED => capabilities.link_receive_speed.clone(),
        wellknown::LINK_TRANSMIT_SPEED => capabilities.link_transmit_speed.clone(),
        wellknown::MTU => capabilities.mtu.clone(),
        other => Availability::not_registered(format!("'{other}' is not a network metric")),
    }
}

/// Builds the Windows network provider.
#[cfg(target_os = "windows")]
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(WindowsNetworkProvider::new())
}

// Referenced only by the Windows-gated `provider()`; kept so the module's
// imports do not depend on the target.
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
fn _arc_is_used(provider: WindowsNetworkProvider) -> Arc<dyn MetricProvider> {
    Arc::new(provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::wellknown::network::NetworkInterfaceKind;
    use iftable::fixtures as row;

    const WIFI_MAC: [u8; 6] = [0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2];
    const ETH_MAC: [u8; 6] = [0xd8, 0x43, 0xae, 0x32, 0x8a, 0xc9];

    /// A plausible Windows machine.
    fn real_machine() -> Vec<MibIfRow2> {
        vec![
            row::loopback(),
            row::ethernet_unplugged(ETH_MAC),
            row::wifi(WIFI_MAC, WIFI_MAC),
            row::vpn("WireGuard Tunnel"),
            row::virtual_switch(),
        ]
    }

    // --- inventory ---------------------------------------------------------

    #[test]
    fn every_interface_including_loopback_is_inventoried() {
        let inventory = NetworkInventory::from_rows(&real_machine());

        assert_eq!(inventory.interfaces.len(), 5);
        assert!(inventory
            .interfaces
            .iter()
            .any(|interface| interface.descriptor.kind == NetworkInterfaceKind::Loopback));
    }

    #[test]
    fn loopback_never_reaches_the_catalog() {
        let inventory = NetworkInventory::from_rows(&real_machine());
        let descriptors: Vec<NetworkInterfaceDescriptor> = inventory
            .interfaces
            .iter()
            .map(|interface| interface.descriptor.clone())
            .collect();

        let provider = ProviderId::new(PROVIDER_ID).expect("valid");
        let declared = wellknown::definitions(&provider, &descriptors);

        // Four published interfaces, one of them Wi-Fi.
        assert_eq!(
            declared.len(),
            2 + wellknown::PER_INTERFACE_KEYS.len() * 4 + wellknown::PER_WIFI_KEYS.len()
        );
    }

    #[test]
    fn a_permanent_address_outranks_the_guid() {
        let inventory = NetworkInventory::from_rows(&[row::wifi(WIFI_MAC, WIFI_MAC)]);

        assert_eq!(
            inventory.interfaces[0].descriptor.source_id.as_str(),
            "network:mac-9009df3e97f2"
        );
        assert!(inventory.interfaces[0].descriptor.has_hardware_identity());
    }

    #[test]
    fn a_randomised_current_address_never_becomes_the_identity() {
        // Windows randomises the Wi-Fi MAC per network by default, exactly as
        // NetworkManager does.
        let randomised = [0x02, 0xab, 0xcd, 0xef, 0x12, 0x34];
        let inventory = NetworkInventory::from_rows(&[row::wifi(WIFI_MAC, randomised)]);
        let descriptor = &inventory.interfaces[0].descriptor;

        assert_eq!(descriptor.source_id.as_str(), "network:mac-9009df3e97f2");
        assert!(descriptor.is_address_randomised());
        assert_eq!(descriptor.current_mac.as_deref(), Some("02:ab:cd:ef:12:34"));
    }

    #[test]
    fn a_virtual_adapter_with_no_permanent_address_falls_back_to_its_guid() {
        let inventory = NetworkInventory::from_rows(&[row::vpn("WireGuard")]);
        let descriptor = &inventory.interfaces[0].descriptor;

        assert_eq!(descriptor.identity.mechanism(), "system-id");
        assert!(descriptor.source_id.as_str().starts_with("network:sys-"));
        // Survives reboots and renames; means nothing on another OS.
        assert!(descriptor.identity.stability().survives_reboot());
        assert!(!descriptor.identity.stability().survives_os_change());
    }

    #[test]
    fn the_interface_index_is_never_part_of_an_identity() {
        // It changes when adapters are added or removed.
        let first = NetworkInventory::from_rows(&[MibIfRow2 {
            interface_index: 12,
            ..row::wifi(WIFI_MAC, WIFI_MAC)
        }]);
        let second = NetworkInventory::from_rows(&[MibIfRow2 {
            interface_index: 47,
            ..row::wifi(WIFI_MAC, WIFI_MAC)
        }]);

        assert_eq!(
            first.interfaces[0].descriptor.source_id,
            second.interfaces[0].descriptor.source_id
        );
    }

    #[test]
    fn two_identical_adapter_models_stay_two_interfaces() {
        let inventory = NetworkInventory::from_rows(&[
            row::ethernet_unplugged([0x00, 0x11, 0x22, 0x33, 0x44, 0x01]),
            row::ethernet_unplugged([0x00, 0x11, 0x22, 0x33, 0x44, 0x02]),
        ]);

        assert_eq!(inventory.interfaces.len(), 2);
        assert_ne!(
            inventory.interfaces[0].descriptor.source_id,
            inventory.interfaces[1].descriptor.source_id
        );
        assert_eq!(
            inventory.interfaces[0].descriptor.display_name,
            inventory.interfaces[1].descriptor.display_name
        );
    }

    #[test]
    fn the_hardware_description_is_the_label_and_the_alias_is_kept_beside_it() {
        // The description names the silicon; the alias is whatever the user
        // renamed the connection to.
        let inventory = NetworkInventory::from_rows(&[row::wifi(WIFI_MAC, WIFI_MAC)]);
        let descriptor = &inventory.interfaces[0].descriptor;

        assert_eq!(descriptor.display_name, "Intel(R) Wi-Fi 6E AX211 160MHz");
        assert_eq!(descriptor.os_name, "Wi-Fi");
    }

    #[test]
    fn the_inventory_order_does_not_depend_on_the_tables() {
        let forward = NetworkInventory::from_rows(&real_machine());

        let mut reversed = real_machine();
        reversed.reverse();
        let backward = NetworkInventory::from_rows(&reversed);

        let sources = |inventory: &NetworkInventory| -> Vec<String> {
            inventory
                .interfaces
                .iter()
                .map(|interface| interface.descriptor.source_id.as_str().to_string())
                .collect()
        };

        assert_eq!(sources(&forward), sources(&backward));
    }

    #[test]
    fn a_machine_with_no_interfaces_yields_an_empty_inventory() {
        assert!(NetworkInventory::from_rows(&[]).interfaces.is_empty());
    }

    // --- Wi-Fi -------------------------------------------------------------

    #[test]
    fn only_a_wireless_adapter_declares_wifi_metrics() {
        let inventory = NetworkInventory::from_rows(&real_machine());

        let wireless: Vec<&Interface> = inventory
            .interfaces
            .iter()
            .filter(|interface| interface.descriptor.is_wireless())
            .collect();

        assert_eq!(wireless.len(), 1);
        assert_eq!(wireless[0].descriptor.kind, NetworkInterfaceKind::Wifi);
    }

    #[test]
    fn the_wifi_unavailable_reason_names_both_possibilities_and_the_refused_fallback() {
        let reason = wifi_unavailable();

        let Availability::TemporarilyUnavailable { reason } = reason else {
            panic!("expected temporarilyUnavailable");
        };
        assert!(reason.contains("not associated"), "{reason}");
        assert!(reason.contains("version of Windows"), "{reason}");
        assert!(reason.contains("location-gated"), "{reason}");
    }

    // --- state -------------------------------------------------------------

    #[test]
    fn a_connected_adapters_state_carries_its_link_speed_and_mtu() {
        let state = state_of(&row::wifi(WIFI_MAC, WIFI_MAC));

        assert_eq!(state.link, Some(LinkState::Connected));
        assert_eq!(state.mtu, Some(1500));
        assert_eq!(state.receive_link_bps, Some(866_700_000));
    }

    #[test]
    fn an_unplugged_ports_state_reports_no_link_speed() {
        let state = state_of(&row::ethernet_unplugged(ETH_MAC));

        assert_eq!(state.link, Some(LinkState::Down));
        assert_eq!(state.receive_link_bps, None, "zero is unknown, not a link");
        assert_eq!(state.mtu, Some(1500), "the MTU is still known");
    }

    #[test]
    fn a_virtual_adapter_reporting_the_maximum_speed_publishes_none() {
        let state = state_of(&row::vpn("WireGuard"));

        assert_eq!(state.receive_link_bps, None);
        assert_eq!(state.transmit_link_bps, None);
    }

    // --- the catalog -------------------------------------------------------

    #[test]
    fn the_catalog_size_follows_the_machine() {
        let inventory = NetworkInventory::from_rows(&real_machine());
        let descriptors: Vec<NetworkInterfaceDescriptor> = inventory
            .interfaces
            .iter()
            .map(|interface| interface.descriptor.clone())
            .collect();

        let provider = ProviderId::new(PROVIDER_ID).expect("valid");
        let declared = wellknown::definitions(&provider, &descriptors);

        let published = descriptors
            .iter()
            .filter(|descriptor| descriptor.kind.is_published())
            .count();
        let wireless = descriptors
            .iter()
            .filter(|descriptor| descriptor.is_wireless())
            .count();

        assert_eq!(
            declared.len(),
            2 + wellknown::PER_INTERFACE_KEYS.len() * published
                + wellknown::PER_WIFI_KEYS.len() * wireless
        );
    }
}
