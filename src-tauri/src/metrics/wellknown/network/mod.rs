//! The `network.*` metrics: declarations shared by every platform.
//!
//! This module owns every network metric key, unit, kind and user-facing
//! string PULSE ships. Platform backends supply **only raw counters and
//! descriptors**; they never choose a key or a unit, which is what keeps
//! `network.receive.bytes_per_second@network:mac-9009df3e97f2` meaning the
//! same thing whether it came from `rtnetlink` on Fedora or `GetIfTable2` on
//! Windows.
//!
//! # The catalog is sized by the machine
//!
//! For `I` published interfaces of which `W` are Wi-Fi:
//!
//! ```text
//! 2     machine-wide   network.interface.count, network.interface.up_count
//! 11I   per interface  eight traffic rates, two link speeds, the MTU
//! 4W    per Wi-Fi      quality, RSSI, receive rate, transmit rate
//! ```
//!
//! `I` counts the interfaces PULSE **publishes**, which excludes loopback —
//! see [`descriptor::NetworkInterfaceKind::is_published`]. Nothing hardcodes
//! `I` or `W`.
//!
//! # Wi-Fi metrics exist only on Wi-Fi interfaces
//!
//! Unlike the storage health metrics, which are declared on every device and
//! marked `unsupported` where they cannot be read, the four `network.wifi.*`
//! keys are declared **only for interfaces whose kind is Wi-Fi**.
//!
//! The distinction is between *a capability a device might have and does not*
//! and *a concept that does not apply at all*. An NVMe health log is something
//! a SATA drive could conceivably report, so keeping the definition tells the
//! user something. An RSSI on an Ethernet port is not a missing measurement —
//! there is no radio — and declaring four permanently-unsupported metrics on
//! every wired adapter, bridge and `veth` on the machine would add noise to
//! the catalog in exchange for nothing. A machine with seven bridges and three
//! `veth` pairs would carry forty dead definitions.
//!
//! # Receive and transmit, from the machine's point of view
//!
//! `receive` is what arrives at this machine and `transmit` is what leaves it.
//! The interface renders them as **Download** and **Upload** respectively.
//! Getting this backwards is easy and produces a monitor that is confidently
//! wrong, so the direction is stated in every description.

pub mod descriptor;
pub mod traffic;
pub mod wifi;

use crate::metrics::model::{
    Availability, MetricCategory, MetricDefinition, MetricDefinitionBuilder, MetricKey, MetricKind,
    MetricRef, MetricUnit, ProviderId, SourceId,
};

pub use descriptor::{
    format_mac, interface_identity, is_predictable_name, is_usable_mac, mac_source_id,
    name_source_id, normalize_identifier, system_source_id, IdentityStability, LinkState,
    NetworkCapabilities, NetworkInterfaceDescriptor, NetworkInterfaceIdentity,
    NetworkInterfaceKind, NetworkInterfaceState, WifiCapabilities, MAC_LEN,
};
pub use traffic::{
    per_second, NeedsAnotherSample, NetworkCounters, NetworkIoTracker, NetworkRates,
    NetworkSnapshot, NetworkTraffic,
};
pub use wifi::{not_connected, quality_unsupported, WifiLink, WifiLinkInfo};

// --- machine-wide keys ----------------------------------------------------

/// `network.interface.count` — how many interfaces PULSE monitors.
///
/// **Excludes loopback.** See the module documentation.
pub const INTERFACE_COUNT: &str = "network.interface.count";
/// `network.interface.up_count` — how many of those currently carry a link.
pub const INTERFACE_UP_COUNT: &str = "network.interface.up_count";

// --- per-interface keys ---------------------------------------------------

/// `network.receive.bytes_per_second` — bytes arriving at this machine.
/// Rendered as **Download**.
pub const RECEIVE_BYTES: &str = "network.receive.bytes_per_second";
/// `network.transmit.bytes_per_second` — bytes leaving this machine.
/// Rendered as **Upload**.
pub const TRANSMIT_BYTES: &str = "network.transmit.bytes_per_second";
/// `network.receive.packets_per_second` — packets arriving per second.
pub const RECEIVE_PACKETS: &str = "network.receive.packets_per_second";
/// `network.transmit.packets_per_second` — packets leaving per second.
pub const TRANSMIT_PACKETS: &str = "network.transmit.packets_per_second";
/// `network.receive.errors_per_second` — malformed frames rejected per second.
pub const RECEIVE_ERRORS: &str = "network.receive.errors_per_second";
/// `network.transmit.errors_per_second` — frames that failed to transmit.
pub const TRANSMIT_ERRORS: &str = "network.transmit.errors_per_second";
/// `network.receive.dropped_per_second` — intact frames discarded per second.
/// **Not Internet packet loss.**
pub const RECEIVE_DROPPED: &str = "network.receive.dropped_per_second";
/// `network.transmit.dropped_per_second` — outbound frames discarded.
pub const TRANSMIT_DROPPED: &str = "network.transmit.dropped_per_second";

/// `network.link.receive_speed` — the link's negotiated inbound capacity.
pub const LINK_RECEIVE_SPEED: &str = "network.link.receive_speed";
/// `network.link.transmit_speed` — the link's negotiated outbound capacity.
pub const LINK_TRANSMIT_SPEED: &str = "network.link.transmit_speed";
/// `network.mtu` — the largest payload this interface carries in one frame.
pub const MTU: &str = "network.mtu";

// --- Wi-Fi keys -----------------------------------------------------------

/// `network.wifi.signal.quality` — the platform's own 0–100 quality figure.
/// **Never derived from the RSSI.**
pub const WIFI_SIGNAL_QUALITY: &str = "network.wifi.signal.quality";
/// `network.wifi.signal.rssi` — received signal strength, in dBm.
pub const WIFI_SIGNAL_RSSI: &str = "network.wifi.signal.rssi";
/// `network.wifi.link.receive_rate` — negotiated inbound rate, in bits/s.
pub const WIFI_LINK_RECEIVE_RATE: &str = "network.wifi.link.receive_rate";
/// `network.wifi.link.transmit_rate` — negotiated outbound rate, in bits/s.
pub const WIFI_LINK_TRANSMIT_RATE: &str = "network.wifi.link.transmit_rate";

/// The per-interface keys describing **traffic**, all of which need two
/// samples.
///
/// Grouped because they are absent together before a baseline exists, so the
/// interface says so once rather than eight times.
pub const TRAFFIC_KEYS: &[&str] = &[
    RECEIVE_BYTES,
    TRANSMIT_BYTES,
    RECEIVE_PACKETS,
    TRANSMIT_PACKETS,
    RECEIVE_ERRORS,
    TRANSMIT_ERRORS,
    RECEIVE_DROPPED,
    TRANSMIT_DROPPED,
];

/// The per-interface keys describing the **link** itself, which are read
/// directly rather than differenced.
pub const LINK_KEYS: &[&str] = &[LINK_RECEIVE_SPEED, LINK_TRANSMIT_SPEED, MTU];

/// Every per-interface key, in declaration order.
pub const PER_INTERFACE_KEYS: &[&str] = &[
    RECEIVE_BYTES,
    TRANSMIT_BYTES,
    RECEIVE_PACKETS,
    TRANSMIT_PACKETS,
    RECEIVE_ERRORS,
    TRANSMIT_ERRORS,
    RECEIVE_DROPPED,
    TRANSMIT_DROPPED,
    LINK_RECEIVE_SPEED,
    LINK_TRANSMIT_SPEED,
    MTU,
];

/// Every Wi-Fi key, declared only on Wi-Fi interfaces.
pub const PER_WIFI_KEYS: &[&str] = &[
    WIFI_SIGNAL_QUALITY,
    WIFI_SIGNAL_RSSI,
    WIFI_LINK_RECEIVE_RATE,
    WIFI_LINK_TRANSMIT_RATE,
];

// --- sources --------------------------------------------------------------

/// The machine-wide network source.
///
/// A *logical* identifier meaning "this machine's networking taken together" —
/// stable by construction and identical on every platform, like `cpu:system`,
/// `gpu:system` and `storage:system`.
pub const SOURCE: &str = "network:system";

/// The user-facing label for `network:system`.
const SYSTEM_LABEL: &str = "Network";

fn key(name: &str) -> MetricKey {
    MetricKey::new(name).expect("well-known network key must be valid")
}

fn system_source() -> SourceId {
    SourceId::new(SOURCE).expect("well-known network source must be valid")
}

/// Builds the `network.interface.count` reference.
pub fn interface_count_ref() -> MetricRef {
    MetricRef::new(key(INTERFACE_COUNT), system_source())
}

/// Builds the `network.interface.up_count` reference.
pub fn interface_up_count_ref() -> MetricRef {
    MetricRef::new(key(INTERFACE_UP_COUNT), system_source())
}

/// Builds a reference for any per-interface or Wi-Fi key.
pub fn network_ref(name: &str, source: &SourceId) -> MetricRef {
    MetricRef::new(key(name), source.clone())
}

// --- telemetry ------------------------------------------------------------

/// Everything one interface's backends produced for one refresh.
///
/// Assembled by a platform provider and resolved here, so both operating
/// systems publish the same number for the same key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetworkTelemetry {
    /// The rates derived from this refresh's counters, or `None` when the
    /// interface needs another sample.
    pub traffic: Option<NetworkRates>,
    /// The state read this refresh: link, MTU, link speeds.
    pub state: NetworkInterfaceState,
    /// The Wi-Fi association, for a Wi-Fi interface.
    pub wifi: Option<WifiLinkInfo>,
}

impl NetworkTelemetry {
    /// The value for one metric key, or `None` when it was not measured.
    pub fn value_for(&self, name: &str) -> Option<f64> {
        match name {
            RECEIVE_BYTES => self.traffic.map(|rates| rates.receive_bytes_per_second),
            TRANSMIT_BYTES => self.traffic.map(|rates| rates.transmit_bytes_per_second),
            RECEIVE_PACKETS => self.traffic.map(|rates| rates.receive_packets_per_second),
            TRANSMIT_PACKETS => self.traffic.map(|rates| rates.transmit_packets_per_second),
            RECEIVE_ERRORS => self.traffic.map(|rates| rates.receive_errors_per_second),
            TRANSMIT_ERRORS => self.traffic.map(|rates| rates.transmit_errors_per_second),
            RECEIVE_DROPPED => self.traffic.map(|rates| rates.receive_dropped_per_second),
            TRANSMIT_DROPPED => self.traffic.map(|rates| rates.transmit_dropped_per_second),

            LINK_RECEIVE_SPEED => self.state.receive_link_bps.map(|bps| bps as f64),
            LINK_TRANSMIT_SPEED => self.state.transmit_link_bps.map(|bps| bps as f64),
            MTU => self.state.mtu.map(f64::from),

            WIFI_SIGNAL_QUALITY => self.wifi.as_ref().and_then(WifiLinkInfo::quality_percent),
            WIFI_SIGNAL_RSSI => self.wifi.as_ref().and_then(WifiLinkInfo::rssi_dbm),
            WIFI_LINK_RECEIVE_RATE => self.wifi.as_ref().and_then(WifiLinkInfo::receive_bps),
            WIFI_LINK_TRANSMIT_RATE => self.wifi.as_ref().and_then(WifiLinkInfo::transmit_bps),

            _ => None,
        }
    }
}

// --- declarations ---------------------------------------------------------

/// Declares every network metric PULSE ships for a discovered inventory.
///
/// Both the Linux and the Windows network provider call this, so their
/// declarations are identical apart from `providerId` and the availabilities
/// each platform genuinely discovered.
///
/// Interfaces whose kind is not published — loopback — are filtered here, so
/// neither platform has to remember to do it and the two cannot disagree.
///
/// Sorted by metric reference, matching the order the engine's catalog holds,
/// so the output is deterministic regardless of enumeration order.
pub fn definitions(
    provider: &ProviderId,
    interfaces: &[NetworkInterfaceDescriptor],
) -> Vec<MetricDefinition> {
    let published: Vec<&NetworkInterfaceDescriptor> = interfaces
        .iter()
        .filter(|interface| interface.kind.is_published())
        .collect();

    let mut definitions = Vec::with_capacity(2 + PER_INTERFACE_KEYS.len() * published.len());

    definitions.push(
        MetricDefinitionBuilder::new(
            interface_count_ref(),
            provider.clone(),
            MetricCategory::Network,
            MetricUnit::Count,
            // A discrete fact about the machine, not a reading that rises and
            // falls. `State` is not directly averageable, so history can never
            // produce "2.4 interfaces".
            MetricKind::State,
        )
        .source_label(SYSTEM_LABEL)
        .display_name("Interfaces")
        .description(
            "Number of network interfaces PULSE monitors. The loopback interface is \
             deliberately excluded: it always exists and only ever carries traffic that \
             never left this machine.",
        )
        .build(),
    );

    definitions.push(
        MetricDefinitionBuilder::new(
            interface_up_count_ref(),
            provider.clone(),
            MetricCategory::Network,
            MetricUnit::Count,
            // This one genuinely rises and falls as cables and networks come
            // and go, so averaging it over time is meaningful.
            MetricKind::Gauge,
        )
        .source_label(SYSTEM_LABEL)
        .display_name("Connected")
        .description(
            "How many of the monitored interfaces currently carry a link. An adapter that \
             is enabled but has no cable, or a radio not associated with a network, is not \
             counted.",
        )
        .build(),
    );

    for interface in published {
        definitions.extend(per_interface_definitions(provider, interface));

        if interface.wifi.is_some() {
            definitions.extend(per_wifi_definitions(provider, interface));
        }
    }

    definitions.sort_by(|left, right| left.metric.cmp(&right.metric));
    definitions
}

/// A label that names the interface and what kind it is.
fn label_for(interface: &NetworkInterfaceDescriptor) -> String {
    format!("{} · {}", interface.display_name, interface.kind.label())
}

/// The eleven generic metrics of one interface.
fn per_interface_definitions(
    provider: &ProviderId,
    interface: &NetworkInterfaceDescriptor,
) -> Vec<MetricDefinition> {
    let label = label_for(interface);
    let capabilities = &interface.capabilities;
    let source = &interface.source_id;

    let build = |name: &str,
                 unit: MetricUnit,
                 kind: MetricKind,
                 display: &str,
                 description: &str,
                 availability: &Availability| {
        MetricDefinitionBuilder::new(
            network_ref(name, source),
            provider.clone(),
            MetricCategory::Network,
            unit,
            kind,
        )
        .source_label(&label)
        .display_name(display)
        .description(description)
        .availability(availability.clone())
        .build()
    };

    vec![
        build(
            RECEIVE_BYTES,
            MetricUnit::BytesPerSecond,
            MetricKind::Gauge,
            "Download",
            "Bytes arriving at this machine per second, derived from the operating \
             system's cumulative counters between two samples. Receive means inbound, \
             from this machine's point of view.",
            &capabilities.receive_bytes,
        ),
        build(
            TRANSMIT_BYTES,
            MetricUnit::BytesPerSecond,
            MetricKind::Gauge,
            "Upload",
            "Bytes leaving this machine per second, derived from the operating system's \
             cumulative counters between two samples. Transmit means outbound.",
            &capabilities.transmit_bytes,
        ),
        build(
            RECEIVE_PACKETS,
            MetricUnit::PacketsPerSecond,
            MetricKind::Gauge,
            "RX packets",
            "Packets arriving per second, unicast and non-unicast together. Zero is a \
             real measurement: it means nothing arrived during the interval.",
            &capabilities.receive_packets,
        ),
        build(
            TRANSMIT_PACKETS,
            MetricUnit::PacketsPerSecond,
            MetricKind::Gauge,
            "TX packets",
            "Packets leaving per second, unicast and non-unicast together.",
            &capabilities.transmit_packets,
        ),
        build(
            RECEIVE_ERRORS,
            MetricUnit::PacketsPerSecond,
            MetricKind::Gauge,
            "RX errors",
            "Inbound frames the interface rejected because something was wrong with \
             them — a bad checksum, a framing error. A different counter from dropped \
             frames, and a persistent non-zero rate usually means a cable or a driver \
             problem.",
            &capabilities.receive_errors,
        ),
        build(
            TRANSMIT_ERRORS,
            MetricUnit::PacketsPerSecond,
            MetricKind::Gauge,
            "TX errors",
            "Outbound frames the interface failed to transmit.",
            &capabilities.transmit_errors,
        ),
        build(
            RECEIVE_DROPPED,
            MetricUnit::PacketsPerSecond,
            MetricKind::Gauge,
            "RX dropped",
            "Inbound frames that arrived intact and were discarded anyway — a full \
             buffer, or no protocol handler wanting them. This is a local counter and \
             is not Internet packet loss, which PULSE does not measure.",
            &capabilities.receive_dropped,
        ),
        build(
            TRANSMIT_DROPPED,
            MetricUnit::PacketsPerSecond,
            MetricKind::Gauge,
            "TX dropped",
            "Outbound frames discarded before transmission, usually because a queue was \
             full.",
            &capabilities.transmit_dropped,
        ),
        build(
            LINK_RECEIVE_SPEED,
            MetricUnit::BitsPerSecond,
            MetricKind::Gauge,
            "Link RX",
            "The inbound capacity of the link, as the driver negotiated it — not how \
             much is flowing. A gauge rather than a fixed value because Wi-Fi and \
             Ethernet both renegotiate.",
            &capabilities.link_receive_speed,
        ),
        build(
            LINK_TRANSMIT_SPEED,
            MetricUnit::BitsPerSecond,
            MetricKind::Gauge,
            "Link TX",
            "The outbound capacity of the link. Equal to the inbound figure on Ethernet; \
             frequently different on Wi-Fi.",
            &capabilities.link_transmit_speed,
        ),
        build(
            MTU,
            MetricUnit::Bytes,
            MetricKind::State,
            "MTU",
            "The largest payload this interface carries in a single frame. A \
             configuration fact rather than a reading, so it is a state.",
            &capabilities.mtu,
        ),
    ]
}

/// The four Wi-Fi metrics of one wireless interface.
fn per_wifi_definitions(
    provider: &ProviderId,
    interface: &NetworkInterfaceDescriptor,
) -> Vec<MetricDefinition> {
    let label = label_for(interface);
    let source = &interface.source_id;
    let capabilities = interface
        .wifi
        .as_ref()
        .expect("only called for a Wi-Fi interface");

    let build = |name: &str,
                 unit: MetricUnit,
                 display: &str,
                 description: &str,
                 availability: &Availability| {
        MetricDefinitionBuilder::new(
            network_ref(name, source),
            provider.clone(),
            MetricCategory::Network,
            unit,
            MetricKind::Gauge,
        )
        .source_label(&label)
        .display_name(display)
        .description(description)
        .availability(availability.clone())
        .build()
    };

    vec![
        build(
            WIFI_SIGNAL_QUALITY,
            MetricUnit::Percent,
            "Quality",
            "A 0–100 link quality figure, published only where the operating system \
             computes one itself. It is never derived from the signal strength: every \
             common dBm-to-percentage formula is arbitrary, and a number produced that \
             way would look like a measurement while being a guess.",
            &capabilities.signal_quality,
        ),
        build(
            WIFI_SIGNAL_RSSI,
            MetricUnit::DecibelMilliwatts,
            "Signal",
            "Received signal strength in dBm, as the radio reports it — a logarithmic \
             scale where −40 is strong and −90 is barely usable. On a radio associated \
             over several links, this is the strongest active link's reading; dBm values \
             are never averaged together.",
            &capabilities.signal_rssi,
        ),
        build(
            WIFI_LINK_RECEIVE_RATE,
            MetricUnit::BitsPerSecond,
            "Wi-Fi RX rate",
            "The inbound rate negotiated with the access point. A capacity, not traffic: \
             a link negotiated at 400 Mbit/s carrying nothing still reports 400 Mbit/s.",
            &capabilities.receive_rate,
        ),
        build(
            WIFI_LINK_TRANSMIT_RATE,
            MetricUnit::BitsPerSecond,
            "Wi-Fi TX rate",
            "The outbound rate negotiated with the access point. Frequently different \
             from the inbound rate.",
            &capabilities.transmit_rate,
        ),
    ]
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// A Wi-Fi interface identified by its permanent address.
    pub fn wifi(mac: [u8; 6], name: &str) -> NetworkInterfaceDescriptor {
        let (source_id, identity) =
            interface_identity(Some(&mac), None, Some(&mac), name).expect("identified");

        NetworkInterfaceDescriptor {
            source_id,
            display_name: name.to_string(),
            os_name: name.to_string(),
            identity,
            kind: NetworkInterfaceKind::Wifi,
            permanent_mac: format_mac(&mac),
            current_mac: format_mac(&mac),
            backend: "test",
            capabilities: NetworkCapabilities::all_available(),
            wifi: Some(WifiCapabilities::all_available()),
        }
    }

    /// An Ethernet interface identified by its permanent address.
    pub fn ethernet(mac: [u8; 6], name: &str) -> NetworkInterfaceDescriptor {
        let (source_id, identity) =
            interface_identity(Some(&mac), None, Some(&mac), name).expect("identified");

        NetworkInterfaceDescriptor {
            source_id,
            display_name: name.to_string(),
            os_name: name.to_string(),
            identity,
            kind: NetworkInterfaceKind::Ethernet,
            permanent_mac: format_mac(&mac),
            current_mac: format_mac(&mac),
            backend: "test",
            capabilities: NetworkCapabilities::all_available(),
            wifi: None,
        }
    }

    /// An interface of an arbitrary kind, identified by name only.
    pub fn named(name: &str, kind: NetworkInterfaceKind) -> NetworkInterfaceDescriptor {
        let (source_id, identity) = interface_identity(None, None, None, name).expect("identified");

        NetworkInterfaceDescriptor {
            source_id,
            display_name: name.to_string(),
            os_name: name.to_string(),
            identity,
            kind,
            permanent_mac: None,
            current_mac: None,
            backend: "test",
            capabilities: NetworkCapabilities::all_available(),
            wifi: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::metrics::model::MetricValueType;

    const WIFI_MAC: [u8; 6] = [0x90, 0x09, 0xdf, 0x3e, 0x97, 0xf2];
    const ETH_MAC: [u8; 6] = [0xd8, 0x43, 0xae, 0x32, 0x8a, 0xc9];

    fn provider() -> ProviderId {
        ProviderId::new("linux.network").expect("valid")
    }

    // --- references --------------------------------------------------------

    #[test]
    fn the_machine_wide_references_are_valid_and_stable() {
        assert_eq!(
            interface_count_ref().to_string(),
            "network.interface.count@network:system"
        );
        assert_eq!(
            interface_up_count_ref().to_string(),
            "network.interface.up_count@network:system"
        );
    }

    #[test]
    fn per_interface_references_carry_the_interfaces_own_source() {
        let interface = wifi(WIFI_MAC, "wlp59s0f0");

        assert_eq!(
            network_ref(RECEIVE_BYTES, &interface.source_id).to_string(),
            "network.receive.bytes_per_second@network:mac-9009df3e97f2"
        );
        assert_eq!(
            network_ref(WIFI_SIGNAL_RSSI, &interface.source_id).to_string(),
            "network.wifi.signal.rssi@network:mac-9009df3e97f2"
        );
    }

    #[test]
    fn every_declared_key_is_a_valid_metric_key() {
        for name in PER_INTERFACE_KEYS
            .iter()
            .chain(PER_WIFI_KEYS)
            .chain(&[INTERFACE_COUNT, INTERFACE_UP_COUNT])
        {
            assert!(MetricKey::new(*name).is_ok(), "'{name}' must be valid");
        }
    }

    #[test]
    fn the_key_groups_partition_the_per_interface_set() {
        let mut grouped: Vec<&str> = TRAFFIC_KEYS.iter().chain(LINK_KEYS).copied().collect();
        let mut all: Vec<&str> = PER_INTERFACE_KEYS.to_vec();

        grouped.sort_unstable();
        all.sort_unstable();

        assert_eq!(grouped, all);
        assert_eq!(PER_INTERFACE_KEYS.len(), 11);
        assert_eq!(PER_WIFI_KEYS.len(), 4);
    }

    // --- catalog shape -----------------------------------------------------

    #[test]
    fn the_catalog_grows_with_the_machine() {
        for count in 0..=4_usize {
            for wireless in 0..=2_usize {
                let mut interfaces: Vec<NetworkInterfaceDescriptor> = (0..count)
                    .map(|index| ethernet([0x00, 0x11, 0x22, 0x33, 0x44, index as u8], "eth"))
                    .collect();
                for index in 0..wireless {
                    interfaces.push(wifi([0x90, 0x09, 0xdf, 0x3e, 0x97, index as u8], "wlan"));
                }

                assert_eq!(
                    definitions(&provider(), &interfaces).len(),
                    2 + PER_INTERFACE_KEYS.len() * (count + wireless)
                        + PER_WIFI_KEYS.len() * wireless,
                    "wrong catalog size for {count} wired and {wireless} wireless"
                );
            }
        }
    }

    #[test]
    fn loopback_is_excluded_from_the_catalog_entirely() {
        // Not declared-and-unavailable: absent. It always exists and only ever
        // carries traffic that never left the machine.
        let interfaces = vec![
            ethernet(ETH_MAC, "enp58s0"),
            named("lo", NetworkInterfaceKind::Loopback),
        ];

        let declared = definitions(&provider(), &interfaces);

        assert_eq!(declared.len(), 2 + PER_INTERFACE_KEYS.len());
        assert!(
            declared
                .iter()
                .all(|definition| !definition.source_label.contains("Loopback")),
            "no loopback metric may reach the catalog"
        );
    }

    #[test]
    fn a_machine_with_no_interfaces_still_declares_both_counts() {
        let declared = definitions(&provider(), &[]);

        assert_eq!(declared.len(), 2);
        // Zero interfaces is a known fact, not a failure to detect.
        assert!(declared.iter().all(|d| d.availability.is_available()));
    }

    #[test]
    fn the_declaration_order_is_deterministic() {
        let interfaces = vec![
            wifi([0x90, 0x09, 0xdf, 0x3e, 0x97, 0xbb], "wlan1"),
            ethernet([0x00, 0x11, 0x22, 0x33, 0x44, 0xaa], "eth0"),
        ];

        let forward: Vec<String> = definitions(&provider(), &interfaces)
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();

        let mut sorted = forward.clone();
        sorted.sort();
        assert_eq!(forward, sorted);

        let mut reversed = interfaces.clone();
        reversed.reverse();
        let backward: Vec<String> = definitions(&provider(), &reversed)
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();
        assert_eq!(forward, backward, "enumeration order must not matter");
    }

    // --- Wi-Fi keys are Wi-Fi only -----------------------------------------

    #[test]
    fn an_ethernet_interface_declares_no_wifi_metrics_at_all() {
        // Not four permanently-unsupported ones: an RSSI on a wired port is
        // not a missing measurement, there is no radio.
        let declared = definitions(&provider(), &[ethernet(ETH_MAC, "enp58s0")]);

        for name in PER_WIFI_KEYS {
            assert!(
                declared
                    .iter()
                    .all(|definition| definition.metric.key.as_str() != *name),
                "'{name}' must not be declared on an Ethernet interface"
            );
        }
        assert_eq!(declared.len(), 2 + PER_INTERFACE_KEYS.len());
    }

    #[test]
    fn a_machine_full_of_bridges_carries_no_dead_wifi_definitions() {
        // Seven bridges and three veth pairs would otherwise add forty
        // permanently-unsupported definitions to the catalog.
        let mut interfaces = vec![wifi(WIFI_MAC, "wlp59s0f0")];
        for index in 0..7 {
            interfaces.push(named(&format!("br-{index}"), NetworkInterfaceKind::Bridge));
        }
        for index in 0..3 {
            interfaces.push(named(
                &format!("veth{index}"),
                NetworkInterfaceKind::Virtual,
            ));
        }

        let declared = definitions(&provider(), &interfaces);
        let wifi_definitions = declared
            .iter()
            .filter(|definition| PER_WIFI_KEYS.contains(&definition.metric.key.as_str()))
            .count();

        assert_eq!(
            wifi_definitions,
            PER_WIFI_KEYS.len(),
            "one radio, four keys"
        );
        assert_eq!(
            declared.len(),
            2 + PER_INTERFACE_KEYS.len() * 11 + PER_WIFI_KEYS.len()
        );
    }

    #[test]
    fn a_wifi_interface_keeps_its_definitions_when_it_is_not_connected() {
        // A radio between networks is still a radio. The definitions stay and
        // carry a transient reason.
        let mut interface = wifi(WIFI_MAC, "wlp59s0f0");
        interface.wifi = Some(WifiCapabilities::none(&not_connected()));

        let declared = definitions(&provider(), &[interface]);

        for name in PER_WIFI_KEYS {
            let definition = declared
                .iter()
                .find(|d| d.metric.key.as_str() == *name)
                .unwrap_or_else(|| panic!("'{name}' declared"));

            assert_eq!(
                definition.availability.status_str(),
                "temporarilyUnavailable"
            );
            assert!(definition.availability.is_transient());
        }
    }

    // --- types -------------------------------------------------------------

    #[test]
    fn the_types_match_the_documented_contract() {
        let declared = definitions(&provider(), &[wifi(WIFI_MAC, "wlp59s0f0")]);

        let typed = |name: &str| {
            declared
                .iter()
                .find(|d| d.metric.key.as_str() == name)
                .map(|d| (d.unit, d.kind, d.value_type))
                .unwrap_or_else(|| panic!("'{name}' declared"))
        };

        assert_eq!(
            typed(INTERFACE_COUNT),
            (
                MetricUnit::Count,
                MetricKind::State,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(INTERFACE_UP_COUNT),
            (
                MetricUnit::Count,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(RECEIVE_BYTES),
            (
                MetricUnit::BytesPerSecond,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(RECEIVE_PACKETS),
            (
                MetricUnit::PacketsPerSecond,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(RECEIVE_ERRORS),
            (
                MetricUnit::PacketsPerSecond,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(RECEIVE_DROPPED),
            (
                MetricUnit::PacketsPerSecond,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(LINK_RECEIVE_SPEED),
            (
                MetricUnit::BitsPerSecond,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(MTU),
            (
                MetricUnit::Bytes,
                MetricKind::State,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(WIFI_SIGNAL_RSSI),
            (
                MetricUnit::DecibelMilliwatts,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(WIFI_SIGNAL_QUALITY),
            (
                MetricUnit::Percent,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
        assert_eq!(
            typed(WIFI_LINK_RECEIVE_RATE),
            (
                MetricUnit::BitsPerSecond,
                MetricKind::Gauge,
                MetricValueType::Number
            )
        );
    }

    #[test]
    fn traffic_is_measured_in_bytes_and_capacity_in_bits() {
        // The factor-of-eight trap. A 1 Gbit/s link carrying 12 MiB/s is two
        // different units, and mixing them looks entirely plausible on screen.
        let declared = definitions(&provider(), &[wifi(WIFI_MAC, "wlp59s0f0")]);

        let unit_of = |name: &str| {
            declared
                .iter()
                .find(|d| d.metric.key.as_str() == name)
                .expect("declared")
                .unit
        };

        assert_eq!(unit_of(RECEIVE_BYTES), MetricUnit::BytesPerSecond);
        assert_eq!(unit_of(TRANSMIT_BYTES), MetricUnit::BytesPerSecond);
        assert_eq!(unit_of(LINK_RECEIVE_SPEED), MetricUnit::BitsPerSecond);
        assert_eq!(unit_of(WIFI_LINK_RECEIVE_RATE), MetricUnit::BitsPerSecond);
    }

    #[test]
    fn a_packet_rate_is_never_declared_as_a_disk_operation_rate() {
        let declared = definitions(&provider(), &[ethernet(ETH_MAC, "enp58s0")]);

        for definition in &declared {
            if TRAFFIC_KEYS.contains(&definition.metric.key.as_str())
                && definition.metric.key.as_str().contains("packets")
            {
                assert_eq!(definition.unit, MetricUnit::PacketsPerSecond);
                assert_ne!(definition.unit, MetricUnit::OperationsPerSecond);
            }
        }
    }

    #[test]
    fn the_source_label_names_the_interface_and_its_kind() {
        let declared = definitions(&provider(), &[wifi(WIFI_MAC, "wlp59s0f0")]);

        let label = &declared
            .iter()
            .find(|d| d.metric.key.as_str() == MTU)
            .expect("declared")
            .source_label;

        assert_eq!(label, "wlp59s0f0 · Wi-Fi");
    }

    #[test]
    fn the_descriptions_state_the_direction_and_the_traps() {
        let declared = definitions(&provider(), &[wifi(WIFI_MAC, "wlp59s0f0")]);

        let description = |name: &str| {
            declared
                .iter()
                .find(|d| d.metric.key.as_str() == name)
                .expect("declared")
                .description
                .clone()
        };

        // Direction, which is easy to get backwards.
        assert!(description(RECEIVE_BYTES).contains("arriving at this machine"));
        assert!(description(TRANSMIT_BYTES).contains("leaving this machine"));
        // A local drop counter is not Internet packet loss.
        assert!(description(RECEIVE_DROPPED).contains("not Internet packet loss"));
        // Errors and drops are different counters.
        assert!(description(RECEIVE_ERRORS).contains("different counter"));
        // Link capacity is not traffic.
        assert!(description(LINK_RECEIVE_SPEED).contains("not how"));
        // Quality is never derived from dBm.
        assert!(description(WIFI_SIGNAL_QUALITY).contains("never derived"));
        // dBm is not averaged across links.
        assert!(description(WIFI_SIGNAL_RSSI).contains("never averaged"));
    }

    #[test]
    fn no_metric_is_a_network_verdict() {
        // PULSE publishes measurements. There is deliberately no score.
        for name in PER_INTERFACE_KEYS.iter().chain(PER_WIFI_KEYS) {
            assert!(!name.contains("score"), "'{name}' looks like a verdict");
            assert!(!name.contains("health"), "'{name}' looks like a verdict");
        }
    }

    // --- telemetry resolution ---------------------------------------------

    fn full_telemetry() -> NetworkTelemetry {
        NetworkTelemetry {
            traffic: Some(NetworkRates {
                receive_bytes_per_second: 13_002_342.0,
                transmit_bytes_per_second: 1_258_291.0,
                receive_packets_per_second: 8400.0,
                transmit_packets_per_second: 2100.0,
                receive_errors_per_second: 0.0,
                transmit_errors_per_second: 0.0,
                receive_dropped_per_second: 0.0,
                transmit_dropped_per_second: 0.0,
            }),
            state: NetworkInterfaceState {
                link: Some(LinkState::Connected),
                mtu: Some(1500),
                receive_link_bps: Some(1_000_000_000),
                transmit_link_bps: Some(1_000_000_000),
                addresses: vec!["192.168.1.41".to_string()],
            },
            wifi: Some(WifiLinkInfo {
                links: vec![WifiLink {
                    rssi_dbm: Some(-68),
                    rssi_average_dbm: Some(-67),
                    receive_bps: Some(175_500_000),
                    transmit_bps: Some(390_000_000),
                }],
                quality_percent: None,
            }),
        }
    }

    #[test]
    fn telemetry_resolves_every_key() {
        let telemetry = full_telemetry();

        assert_eq!(telemetry.value_for(RECEIVE_BYTES), Some(13_002_342.0));
        assert_eq!(telemetry.value_for(TRANSMIT_BYTES), Some(1_258_291.0));
        assert_eq!(telemetry.value_for(RECEIVE_PACKETS), Some(8400.0));
        assert_eq!(telemetry.value_for(TRANSMIT_PACKETS), Some(2100.0));
        assert_eq!(telemetry.value_for(RECEIVE_ERRORS), Some(0.0));
        assert_eq!(telemetry.value_for(TRANSMIT_DROPPED), Some(0.0));
        assert_eq!(telemetry.value_for(LINK_RECEIVE_SPEED), Some(1e9));
        assert_eq!(telemetry.value_for(MTU), Some(1500.0));
        assert_eq!(telemetry.value_for(WIFI_SIGNAL_RSSI), Some(-68.0));
        assert_eq!(
            telemetry.value_for(WIFI_LINK_RECEIVE_RATE),
            Some(175_500_000.0)
        );
        assert_eq!(
            telemetry.value_for(WIFI_LINK_TRANSMIT_RATE),
            Some(390_000_000.0)
        );

        // Fedora reports no quality percentage, and none is invented.
        assert_eq!(telemetry.value_for(WIFI_SIGNAL_QUALITY), None);
        // Not one of ours.
        assert_eq!(telemetry.value_for("network.nonsense"), None);
    }

    #[test]
    fn an_interface_awaiting_its_baseline_reports_no_traffic_at_all() {
        let telemetry = NetworkTelemetry {
            traffic: None,
            state: NetworkInterfaceState {
                link: Some(LinkState::Connected),
                mtu: Some(1500),
                ..NetworkInterfaceState::default()
            },
            wifi: None,
        };

        for name in TRAFFIC_KEYS {
            assert_eq!(
                telemetry.value_for(name),
                None,
                "'{name}' must not fabricate a value before a baseline exists"
            );
        }
        // The static facts are unaffected.
        assert_eq!(telemetry.value_for(MTU), Some(1500.0));
    }

    #[test]
    fn an_idle_interval_publishes_zero_on_every_traffic_rate() {
        let telemetry = NetworkTelemetry {
            traffic: Some(NetworkRates {
                receive_bytes_per_second: 0.0,
                transmit_bytes_per_second: 0.0,
                receive_packets_per_second: 0.0,
                transmit_packets_per_second: 0.0,
                receive_errors_per_second: 0.0,
                transmit_errors_per_second: 0.0,
                receive_dropped_per_second: 0.0,
                transmit_dropped_per_second: 0.0,
            }),
            state: NetworkInterfaceState::default(),
            wifi: None,
        };

        for name in TRAFFIC_KEYS {
            assert_eq!(
                telemetry.value_for(name),
                Some(0.0),
                "'{name}' over a real interval is a measurement"
            );
        }
    }

    #[test]
    fn a_disconnected_wifi_interface_reports_no_link_figures() {
        let telemetry = NetworkTelemetry {
            traffic: None,
            state: NetworkInterfaceState {
                link: Some(LinkState::Disconnected),
                mtu: Some(1500),
                ..NetworkInterfaceState::default()
            },
            wifi: Some(WifiLinkInfo::disconnected()),
        };

        for name in PER_WIFI_KEYS {
            assert_eq!(telemetry.value_for(name), None, "'{name}'");
        }
        assert_eq!(telemetry.value_for(MTU), Some(1500.0));
    }

    #[test]
    fn an_unknown_link_speed_is_absent_rather_than_zero() {
        // Zero is what several platforms report for "I do not know", and
        // publishing it would claim a link with no capacity at all.
        let telemetry = NetworkTelemetry {
            traffic: None,
            state: NetworkInterfaceState {
                link: Some(LinkState::Connected),
                mtu: Some(1500),
                receive_link_bps: None,
                transmit_link_bps: None,
                addresses: Vec::new(),
            },
            wifi: None,
        };

        assert_eq!(telemetry.value_for(LINK_RECEIVE_SPEED), None);
        assert_eq!(telemetry.value_for(LINK_TRANSMIT_SPEED), None);
    }
}
