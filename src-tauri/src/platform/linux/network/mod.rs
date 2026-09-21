//! The Fedora network provider.
//!
//! **One provider owns every network metric on the machine**, whatever mix of
//! adapters, tunnels and container plumbing it holds:
//!
//! ```text
//! linux.network
//!  ├── inventory   rtnetlink RTM_GETLINK    interfaces, identity, MTU, state
//!  ├── traffic     rtnetlink IFLA_STATS64   the same dump, same instant
//!  ├── addresses   rtnetlink RTM_GETADDR    local addresses, for display
//!  ├── link speed  /sys/class/net/*/speed   Ethernet only
//!  └── Wi-Fi       nl80211                  which are wireless, and their link
//! ```
//!
//! Wi-Fi is deliberately **not** a `linux.wifi` provider of its own. A Wi-Fi
//! adapter is one interface with one identity, and two providers publishing
//! about it would claim the same `SourceId` — which the engine rejects by
//! design, exactly as it would for NVML beside the GPU inventory or an NVMe
//! health backend beside the disk inventory.
//!
//! So on a machine with networking:
//!
//! ```text
//! Providers = 5   linux.cpu, linux.memory, linux.gpu, linux.storage, linux.network
//! ```
//!
//! # Degradation
//!
//! Every layer is optional and fails alone. No `nl80211` leaves every generic
//! metric working and simply means no interface is classified as Wi-Fi. No
//! address dump leaves the counters intact. A single interface that vanishes
//! between the inventory and the sample costs that interface's values and
//! nothing else. No interfaces at all leaves `network.interface.count`
//! reporting zero, which is a fact rather than a failure.
//!
//! # What a refresh costs
//!
//! ```text
//! 1   netlink transaction   RTM_GETLINK   every interface, every counter
//! 1   netlink transaction   RTM_GETADDR   every local address
//! E   file reads            speed         one per *Ethernet* interface
//! 1   netlink transaction   nl80211 station dump, per Wi-Fi interface
//! ```
//!
//! On the development machine — thirteen interfaces, one of them Wi-Fi, one
//! Ethernet — that is **three netlink round trips and one file read**, for
//! thirteen interfaces' worth of metrics. The alternative shape, one
//! `/sys/class/net/<iface>/statistics/<counter>` read per metric, would be 104
//! file opens at 104 slightly different instants.
//!
//! # What is cached
//!
//! Identity, name, kind and permanent address are read **once** when the
//! provider is built. They cannot change while an interface exists. A refresh
//! re-reads the state, the addresses, the counters and the Wi-Fi link.
//!
//! An interface that appears afterwards — a USB adapter, a VPN connecting —
//! is therefore picked up on the next launch rather than the next refresh.
//! Rebuilding the inventory dynamically is a later phase's work; nothing here
//! breaks in the meantime, and a vanished interface degrades to
//! `temporarilyUnavailable` rather than a panic.

pub mod addresses;
pub mod netlink;
pub mod nl80211;
pub mod rtnetlink;
pub mod socket;
pub mod speed;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricRef, MetricSample, ProviderId, SourceId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::network::{
    self as wellknown, format_mac, interface_identity, LinkState, NetworkCapabilities,
    NetworkInterfaceDescriptor, NetworkInterfaceKind, NetworkInterfaceState, NetworkIoTracker,
    NetworkSnapshot, NetworkTelemetry, NetworkTraffic, WifiCapabilities, WifiLinkInfo,
};

use nl80211::Nl80211;
use rtnetlink::LinkEntry;

/// Identifier of the Linux network provider.
pub const PROVIDER_ID: &str = "linux.network";

/// One interface, with what is needed to read it again.
#[derive(Debug, Clone)]
struct Interface {
    descriptor: NetworkInterfaceDescriptor,
    /// The kernel's index, used to match this interface in a later dump and
    /// to address `nl80211`. **A handle, never an identity** — the kernel
    /// reuses it after an interface is destroyed.
    index: i32,
}

/// What the provider discovered when it was built.
struct NetworkInventory {
    interfaces: Vec<Interface>,
    /// The `nl80211` handle, resolved once. `None` on a machine with no
    /// wireless hardware, which is a normal answer rather than a failure.
    nl80211: Option<Nl80211>,
}

impl std::fmt::Debug for NetworkInventory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkInventory")
            .field("interfaces", &self.interfaces.len())
            .field("nl80211", &self.nl80211.is_some())
            .finish()
    }
}

impl NetworkInventory {
    fn discover() -> Self {
        // A machine with no wireless hardware has no `nl80211` family at all,
        // and every generic metric must keep working there.
        let nl80211 = Nl80211::resolve().ok();

        let wireless: BTreeSet<i32> = nl80211
            .and_then(|handle| handle.wireless_indices().ok())
            .unwrap_or_default()
            .into_iter()
            .collect();

        let entries = rtnetlink::dump().unwrap_or_default();

        Self {
            interfaces: describe_all(&entries, &wireless),
            nl80211,
        }
    }

    /// Reads every interface's counters in one transaction.
    fn snapshot(&self, at: &Instant, origin: &Instant) -> NetworkSnapshot {
        let mut snapshot = NetworkSnapshot::new(at.duration_since(*origin).as_millis() as u64);

        let Ok(entries) = rtnetlink::dump() else {
            return snapshot;
        };

        let by_index: BTreeMap<i32, &LinkEntry> =
            entries.iter().map(|entry| (entry.index, entry)).collect();

        for interface in &self.interfaces {
            // Matched on the kernel index *and* the name: an index alone can
            // be reused by a different interface after the original is
            // destroyed, and publishing the newcomer's counters under the old
            // one's identity would be a silent lie.
            if let Some(entry) = by_index.get(&interface.index) {
                if entry.name == interface.descriptor.os_name {
                    if let Some(counters) = entry.counters {
                        snapshot.insert(interface.descriptor.source_id.clone(), counters);
                    }
                }
            }
        }

        snapshot
    }
}

/// Turns a dump into descriptors, in a deterministic order.
fn describe_all(entries: &[LinkEntry], wireless: &BTreeSet<i32>) -> Vec<Interface> {
    let mut interfaces: Vec<Interface> = entries
        .iter()
        .filter_map(|entry| describe(entry, wireless.contains(&entry.index)))
        .collect();

    // Deduplicate on identity: two entries resolving to one `SourceId` would
    // make the engine reject the provider outright. It should not happen —
    // permanent addresses are unique — but a driver reporting the same one
    // twice must not take down every network metric on the machine.
    interfaces.sort_by(|left, right| {
        left.descriptor
            .source_id
            .cmp(&right.descriptor.source_id)
            .then_with(|| left.index.cmp(&right.index))
    });
    interfaces.dedup_by(|left, right| left.descriptor.source_id == right.descriptor.source_id);

    interfaces
}

/// Turns one dump entry into a descriptor.
///
/// Pure, so the whole mapping — identity choice, kind, capabilities — is
/// tested against synthetic dumps rather than against whatever this machine
/// happens to have plugged in.
fn describe(entry: &LinkEntry, wireless: bool) -> Option<Interface> {
    let kind = entry.kind(wireless);

    let (source_id, identity) = interface_identity(
        entry.permanent_address.as_deref(),
        None,
        entry.address.as_deref(),
        &entry.name,
    )?;

    let mut capabilities = NetworkCapabilities::all_available();

    if entry.counters.is_none() {
        capabilities = capabilities.with_traffic(Availability::not_detected(format!(
            "the kernel keeps no statistics block for '{}'",
            entry.name
        )));
    }

    if entry.mtu.is_none() {
        capabilities.mtu =
            Availability::not_detected("the kernel reports no MTU for this interface");
    }

    // Only a wired interface has a fixed link rate to report. A Wi-Fi link's
    // negotiated rate is a different thing with a different unit of meaning,
    // and it is published under `network.wifi.link.*`.
    if !matches!(
        kind,
        NetworkInterfaceKind::Ethernet | NetworkInterfaceKind::Virtual
    ) {
        capabilities = capabilities.with_link_speed(link_speed_reason(kind));
    }

    Some(Interface {
        index: entry.index,
        descriptor: NetworkInterfaceDescriptor {
            source_id,
            display_name: entry.name.clone(),
            os_name: entry.name.clone(),
            identity,
            kind,
            permanent_mac: entry.permanent_address.as_deref().and_then(format_mac),
            current_mac: entry.address.as_deref().and_then(format_mac),
            backend: "rtnetlink",
            capabilities,
            wifi: kind.is_wireless().then(WifiCapabilities::all_available),
        },
    })
}

/// Why an interface of this kind reports no fixed link speed.
fn link_speed_reason(kind: NetworkInterfaceKind) -> Availability {
    match kind {
        NetworkInterfaceKind::Wifi => Availability::unsupported(
            "a Wi-Fi link has no fixed rate; the rate negotiated with the access point is \
             published as network.wifi.link.receive_rate and .transmit_rate instead",
        ),
        NetworkInterfaceKind::Tunnel => Availability::not_detected(
            "a tunnel has no physical link, so it negotiates no rate of its own",
        ),
        NetworkInterfaceKind::Bridge => {
            Availability::not_detected("a bridge has no physical link; its members do")
        }
        NetworkInterfaceKind::Loopback => {
            Availability::not_detected("loopback has no link to negotiate")
        }
        _ => Availability::unsupported("this interface reports no link speed to the kernel"),
    }
}

/// Publishes every network metric on Fedora.
///
/// Requires no elevated privileges: `AF_NETLINK` is open to any process,
/// `RTM_GETLINK` and `RTM_GETADDR` are unprivileged dumps, `nl80211`'s station
/// query works for any user, and `/sys/class/net` is world-readable.
pub struct LinuxNetworkProvider {
    id: ProviderId,
    inventory: NetworkInventory,
    traffic: NetworkIoTracker,
    /// The origin of the monotonic clock rates are measured against.
    ///
    /// Monotonic rather than wall-clock: an NTP correction between two
    /// refreshes must not become a throughput spike or a negative interval.
    origin: Instant,
}

impl std::fmt::Debug for LinuxNetworkProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxNetworkProvider")
            .field("id", &self.id)
            .field("inventory", &self.inventory)
            .finish()
    }
}

impl LinuxNetworkProvider {
    pub fn new() -> Self {
        let origin = Instant::now();
        let inventory = NetworkInventory::discover();
        let traffic = NetworkIoTracker::new();

        // Prime the baseline at startup so the *second* request — typically
        // the user's first Refresh — already has a real interval. The first
        // request still reports "waiting for another sample", honestly,
        // rather than a fabricated 0 B/s.
        traffic.prime(&inventory.snapshot(&Instant::now(), &origin));

        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
            inventory,
            traffic,
            origin,
        }
    }

    /// The interfaces this provider published its catalog from.
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

    /// Reads the state of every interface this request touches.
    ///
    /// The link state and MTU come from the same dump the counters do, so all
    /// three describe one instant.
    fn states(&self, entries: &[LinkEntry]) -> BTreeMap<SourceId, NetworkInterfaceState> {
        let addresses = addresses::dump().unwrap_or_default();

        let by_index: BTreeMap<i32, &LinkEntry> =
            entries.iter().map(|entry| (entry.index, entry)).collect();

        self.inventory
            .interfaces
            .iter()
            .map(|interface| {
                let entry = by_index
                    .get(&interface.index)
                    .filter(|entry| entry.name == interface.descriptor.os_name);

                let state = match entry {
                    Some(entry) => {
                        // Only an interface with a fixed link rate is asked;
                        // a Wi-Fi adapter's `speed` file fails with EINVAL and
                        // a bridge has no rate to report.
                        let link_bps = interface
                            .descriptor
                            .capabilities
                            .link_receive_speed
                            .is_available()
                            .then(|| speed::read(&entry.name))
                            .flatten();

                        NetworkInterfaceState {
                            link: Some(entry.link_state()),
                            mtu: entry.mtu,
                            receive_link_bps: link_bps,
                            transmit_link_bps: link_bps,
                            addresses: addresses.get(&interface.index).cloned().unwrap_or_default(),
                        }
                    }
                    // The interface existed when the provider was built and
                    // does not now.
                    None => NetworkInterfaceState::default(),
                };

                (interface.descriptor.source_id.clone(), state)
            })
            .collect()
    }

    /// Reads one Wi-Fi interface's link, when a backend can.
    fn wifi_for(&self, interface: &Interface) -> Option<WifiLinkInfo> {
        if !interface.descriptor.is_wireless() {
            return None;
        }

        let nl80211 = self.inventory.nl80211?;

        // A query that fails — the interface vanished, the driver refused —
        // is reported as a disconnected link rather than as a crash. The
        // availability the sample carries then explains it.
        Some(
            nl80211
                .station(interface.index)
                .unwrap_or_else(|_| WifiLinkInfo::disconnected()),
        )
    }
}

impl Default for LinuxNetworkProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for LinuxNetworkProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        Ok(wellknown::definitions(&self.id, &self.interfaces()))
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        // One dump serves every interface in this request, however many of
        // their metrics were asked for, and captures them all at one instant.
        let entries = rtnetlink::dump().unwrap_or_default();

        let snapshot = {
            let taken_at = Instant::now();
            let mut snapshot =
                NetworkSnapshot::new(taken_at.duration_since(self.origin).as_millis() as u64);

            let by_index: BTreeMap<i32, &LinkEntry> =
                entries.iter().map(|entry| (entry.index, entry)).collect();

            for interface in &self.inventory.interfaces {
                if let Some(entry) = by_index.get(&interface.index) {
                    if entry.name == interface.descriptor.os_name {
                        if let Some(counters) = entry.counters {
                            snapshot.insert(interface.descriptor.source_id.clone(), counters);
                        }
                    }
                }
            }

            snapshot
        };

        let rates = self.traffic.update(&snapshot)?;
        let states = self.states(&entries);

        // Only the interfaces this request actually touches: a Wi-Fi adapter
        // nobody asked about is never queried.
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
                    wifi: self.wifi_for(interface),
                });
            }
        }

        Ok(requested
            .iter()
            .map(|reference| self.sample_one(reference, &telemetry, &rates, &states))
            .collect())
    }
}

impl LinuxNetworkProvider {
    /// Answers one requested reference.
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
            // Never a fabricated zero. The reason is chosen from what actually
            // went wrong for *this* metric on *this* interface.
            None => MetricSample::unavailable(
                reference.clone(),
                // A declared-available capability with no value at this
                // instant would otherwise produce a sample claiming success
                // with nothing in it. The safety net keeps that impossible
                // however the reasons above evolve.
                match interface_reason(interface, key, telemetry, rates) {
                    Availability::Available => Availability::temporarily_unavailable(
                        "this interface reported no value for this metric during the sample",
                    ),
                    reason => reason,
                },
            ),
        }
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

/// Explains why one interface could not answer one metric.
fn interface_reason(
    interface: &Interface,
    key: &str,
    telemetry: &NetworkTelemetry,
    rates: &BTreeMap<SourceId, NetworkTraffic>,
) -> Availability {
    if wellknown::TRAFFIC_KEYS.contains(&key) {
        // An interface whose counters exist but has no interval yet is not an
        // interface with a problem, and the two say different things.
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
        // The capability says the *interface* reports a link speed; at this
        // instant it may still not have one, and the two are different
        // statements. An unplugged Ethernet port is the ordinary case.
        let declared = capability_for(&interface.descriptor.capabilities, key);

        if declared.is_available() {
            return match telemetry.state.link {
                Some(state) if !state.is_connected() => Availability::temporarily_unavailable(
                    "this interface negotiates no link speed while it has no carrier",
                ),
                _ => {
                    Availability::unsupported("the driver reports no link speed for this interface")
                }
            };
        }

        return declared;
    }

    if wellknown::PER_WIFI_KEYS.contains(&key) {
        return match &telemetry.wifi {
            // Associated, but this particular figure was not reported — a
            // driver that publishes a signal and no negotiated rate.
            Some(info) if info.is_connected() => Availability::unsupported(
                "this driver reports no such figure for the current association",
            ),
            // Present and not associated with any network.
            Some(_) => wellknown::not_connected(),
            None => Availability::not_detected("this interface has no wireless backend"),
        };
    }

    capability_for(&interface.descriptor.capabilities, key)
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

/// Builds the Fedora network provider.
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(LinuxNetworkProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtnetlink::fixtures as link;

    fn describe_dump(links: &[link::Link], wireless: &[i32]) -> Vec<Interface> {
        let buffer = link::encode_dump(links);
        let entries = rtnetlink::parse_dump(&buffer);
        let wireless: BTreeSet<i32> = wireless.iter().copied().collect();

        describe_all(&entries, &wireless)
    }

    /// The development machine's interface list, as fixtures.
    fn real_machine() -> Vec<link::Link> {
        vec![
            link::loopback(),
            link::ethernet_unplugged(),
            link::wifi(),
            link::bridge("docker0", false),
            link::bridge("br-a4c5d1b67a43", true),
            link::veth("vethe0ffe82"),
        ]
    }

    // --- inventory ---------------------------------------------------------

    #[test]
    fn every_interface_including_loopback_is_inventoried() {
        // Loopback is discovered internally and filtered when the catalog is
        // built, so the inventory itself is complete.
        let interfaces = describe_dump(&real_machine(), &[3]);

        assert_eq!(interfaces.len(), 6);
        assert!(interfaces
            .iter()
            .any(|interface| interface.descriptor.kind == NetworkInterfaceKind::Loopback));
    }

    #[test]
    fn loopback_never_reaches_the_catalog() {
        let interfaces = describe_dump(&real_machine(), &[3]);
        let descriptors: Vec<NetworkInterfaceDescriptor> = interfaces
            .iter()
            .map(|interface| interface.descriptor.clone())
            .collect();

        let provider = ProviderId::new(PROVIDER_ID).expect("valid");
        let declared = wellknown::definitions(&provider, &descriptors);

        // Five published interfaces, one of them Wi-Fi.
        assert_eq!(
            declared.len(),
            2 + wellknown::PER_INTERFACE_KEYS.len() * 5 + wellknown::PER_WIFI_KEYS.len()
        );
    }

    #[test]
    fn the_wifi_adapter_is_classified_from_nl80211_not_from_its_name() {
        // Told it is wireless, `wlp59s0f0` is Wi-Fi.
        let wireless = describe_dump(&[link::wifi()], &[3]);
        assert_eq!(wireless[0].descriptor.kind, NetworkInterfaceKind::Wifi);
        assert!(wireless[0].descriptor.wifi.is_some());

        // Not told, the very same entry is Ethernet — because a Wi-Fi station
        // reports `ARPHRD_ETHER` exactly like a wired NIC, and the name proves
        // nothing.
        let not_wireless = describe_dump(&[link::wifi()], &[]);
        assert_eq!(
            not_wireless[0].descriptor.kind,
            NetworkInterfaceKind::Ethernet
        );
        assert!(not_wireless[0].descriptor.wifi.is_none());
    }

    #[test]
    fn each_interface_gets_the_strongest_identity_available() {
        let interfaces = describe_dump(&real_machine(), &[3]);

        let by_name = |name: &str| {
            interfaces
                .iter()
                .find(|interface| interface.descriptor.os_name == name)
                .unwrap_or_else(|| panic!("'{name}' inventoried"))
        };

        // Permanent addresses where the kernel reports one.
        assert_eq!(
            by_name("wlp59s0f0").descriptor.source_id.as_str(),
            "network:mac-9009df3e97f2"
        );
        assert_eq!(
            by_name("enp58s0").descriptor.source_id.as_str(),
            "network:mac-d843ae328ac9"
        );
        assert!(by_name("wlp59s0f0").descriptor.has_hardware_identity());

        // A bridge has only a current address, and says so.
        assert_eq!(
            by_name("docker0").descriptor.identity.mechanism(),
            "current-mac"
        );
    }

    #[test]
    fn a_randomised_wifi_address_does_not_change_the_identity() {
        let randomised = describe_dump(&[link::wifi_randomised()], &[3]);

        assert_eq!(
            randomised[0].descriptor.source_id.as_str(),
            "network:mac-9009df3e97f2",
            "the permanent address, not the randomised one"
        );
        assert!(randomised[0].descriptor.is_address_randomised());
    }

    #[test]
    fn an_interface_with_no_address_at_all_falls_back_to_its_name() {
        let tunnel = describe_dump(&[link::wireguard("wg0")], &[]);

        assert_eq!(tunnel[0].descriptor.source_id.as_str(), "network:if-wg0");
        assert_eq!(tunnel[0].descriptor.kind, NetworkInterfaceKind::Tunnel);
        assert!(!tunnel[0].descriptor.has_hardware_identity());
    }

    #[test]
    fn the_inventory_order_does_not_depend_on_the_kernels() {
        let forward = describe_dump(&real_machine(), &[3]);

        let mut reversed = real_machine();
        reversed.reverse();
        let backward = describe_dump(&reversed, &[3]);

        let sources = |interfaces: &[Interface]| -> Vec<String> {
            interfaces
                .iter()
                .map(|interface| interface.descriptor.source_id.as_str().to_string())
                .collect()
        };

        assert_eq!(sources(&forward), sources(&backward));
    }

    #[test]
    fn two_entries_resolving_to_one_identity_are_collapsed() {
        // Should not happen, but a driver reporting a duplicate permanent
        // address must not make the engine reject the whole provider.
        let interfaces = describe_dump(&[link::wifi(), link::wifi()], &[3]);

        assert_eq!(interfaces.len(), 1);
    }

    #[test]
    fn a_machine_with_no_interfaces_yields_an_empty_inventory() {
        assert!(describe_dump(&[], &[]).is_empty());
    }

    // --- capabilities ------------------------------------------------------

    #[test]
    fn only_a_wired_interface_declares_a_link_speed() {
        let interfaces = describe_dump(&real_machine(), &[3]);

        let by_name = |name: &str| {
            interfaces
                .iter()
                .find(|interface| interface.descriptor.os_name == name)
                .unwrap_or_else(|| panic!("'{name}' inventoried"))
                .descriptor
                .capabilities
                .link_receive_speed
                .clone()
        };

        assert!(by_name("enp58s0").is_available());
        assert!(by_name("vethe0ffe82").is_available());

        // A Wi-Fi link has no fixed rate; its negotiated one is a Wi-Fi metric.
        assert_eq!(by_name("wlp59s0f0").status_str(), "unsupported");
        assert_eq!(by_name("docker0").status_str(), "notDetected");
    }

    #[test]
    fn the_wifi_link_speed_reason_points_at_the_metric_that_does_answer() {
        let reason = link_speed_reason(NetworkInterfaceKind::Wifi);

        let Availability::Unsupported { reason } = reason else {
            panic!("expected unsupported");
        };
        assert!(
            reason.contains("network.wifi.link.receive_rate"),
            "{reason}"
        );
    }

    #[test]
    fn an_interface_with_no_statistics_block_declares_its_traffic_undetected() {
        let no_stats = link::Link {
            stats: None,
            ..link::ethernet_unplugged()
        };
        let interfaces = describe_dump(&[no_stats], &[]);

        let capabilities = &interfaces[0].descriptor.capabilities;
        assert_eq!(capabilities.receive_bytes.status_str(), "notDetected");
        // …while the MTU it does report is unaffected.
        assert!(capabilities.mtu.is_available());
    }

    #[test]
    fn every_kind_has_its_own_link_speed_explanation() {
        let mut reasons: Vec<String> = NetworkInterfaceKind::ALL
            .iter()
            .map(|kind| format!("{:?}", link_speed_reason(*kind)))
            .collect();
        reasons.sort();
        reasons.dedup();

        // Ethernet, Virtual and Other share the generic sentence; the rest are
        // specific.
        assert!(
            reasons.len() >= 4,
            "a generic catch-all would collapse these"
        );
    }

    // --- against the real machine ------------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_provider_answers_every_metric_it_declares() {
        let provider = LinuxNetworkProvider::new();
        let definitions = provider.describe().expect("describes");

        let references: Vec<MetricRef> = definitions
            .iter()
            .map(|definition| definition.metric.clone())
            .collect();
        let samples = provider.sample(&references).expect("samples");

        assert_eq!(samples.len(), references.len());
        for (sample, reference) in samples.iter().zip(&references) {
            assert_eq!(&sample.metric, reference);
            // Every answer is either a value or a stated reason; never both
            // and never neither.
            assert_eq!(
                sample.has_value(),
                sample.availability.is_available(),
                "{reference} answered inconsistently"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_machine_reports_at_least_one_interface() {
        let provider = LinuxNetworkProvider::new();

        // Every machine has loopback, and this one has more.
        assert!(!provider.interfaces().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn no_loopback_interface_reaches_the_real_catalog() {
        let provider = LinuxNetworkProvider::new();
        let definitions = provider.describe().expect("describes");

        let loopback: Vec<SourceId> = provider
            .interfaces()
            .into_iter()
            .filter(|interface| interface.kind == NetworkInterfaceKind::Loopback)
            .map(|interface| interface.source_id)
            .collect();

        assert!(!loopback.is_empty(), "this machine has loopback");
        for source in loopback {
            assert!(
                definitions
                    .iter()
                    .all(|definition| definition.metric.source_id != source),
                "{source} must not be published"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_unknown_source_is_answered_rather_than_rejected() {
        let provider = LinuxNetworkProvider::new();
        let reference = MetricRef::parse(wellknown::MTU, "network:if-nonexistent").expect("valid");

        let samples = provider
            .sample(std::slice::from_ref(&reference))
            .expect("samples");

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].availability.status_str(), "notRegistered");
        assert!(!samples[0].has_value());
    }
}
