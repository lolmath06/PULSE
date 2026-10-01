//! Prints the showcase machine's metric catalog and source-reference map as
//! JSON, using PULSE's real metric declarations. See `Cargo.toml`.

#![allow(dead_code)]

#[path = "../../../../src-tauri/src/history/mod.rs"]
pub mod history;
#[path = "../../../../src-tauri/src/live/mod.rs"]
pub mod live;
#[path = "../../../../src-tauri/src/metrics/mod.rs"]
pub mod metrics;
#[path = "../../../../src-tauri/src/overlay/mod.rs"]
pub mod overlay;
#[path = "../../../../src-tauri/src/platform/mod.rs"]
pub mod platform;
#[path = "../../../../src-tauri/src/processes/mod.rs"]
pub mod processes;
#[path = "../../../../src-tauri/src/services/mod.rs"]
pub mod services;
#[path = "../../../../src-tauri/src/ui_config/mod.rs"]
pub mod ui_config;

use std::collections::BTreeMap;

use metrics::wellknown::{cpu, gpu, memory, network, process, storage};
use metrics::{Availability, MetricDefinition, ProviderId};

fn provider(id: &str) -> ProviderId {
    ProviderId::new(id).expect("valid provider id")
}

/// 8 cores / 16 threads, one package with a temperature sensor.
fn cpu_topology() -> cpu::CpuTopology {
    cpu::CpuTopology::new(
        (0..16)
            .map(|ordinal| cpu::LogicalProcessor::available(cpu::LogicalId::new(ordinal)))
            .collect(),
        Some(8),
        Some(1),
    )
    .with_packages(vec![cpu::CpuPackage::available(cpu::PackageId::new(0))])
}

fn gpus() -> Vec<gpu::GpuDescriptor> {
    // A made-up NVML UUID: no real board carries it.
    let uuid = "GPU-5ec0a5e0-0000-4000-8000-000000000001";
    vec![gpu::GpuDescriptor {
        source_id: gpu::nvml_source_id(uuid).expect("valid uuid"),
        display_name: "NVIDIA GeForce RTX 4070".to_string(),
        vendor: gpu::GpuVendor::Nvidia,
        identity: gpu::GpuIdentity::NvmlUuid(uuid.to_string()),
        pci: Some(gpu::PciAddress::new(0, 1, 0, 0)),
        backend: "nvml",
        capabilities: gpu::GpuCapabilities::all_available(),
    }]
}

fn storage() -> (
    Vec<storage::StorageDeviceDescriptor>,
    Vec<storage::StorageVolumeDescriptor>,
) {
    let device =
        |eui: &str, model: &str, os_name: &str, bus, capacity| storage::StorageDeviceDescriptor {
            source_id: storage::wwid_source_id(eui).expect("valid wwid"),
            display_name: model.to_string(),
            os_name: os_name.to_string(),
            identity: storage::StorageIdentity::Wwid(eui.to_string()),
            bus,
            capacity_bytes: Some(capacity),
            rotational: Some(false),
            removable: Some(false),
            backend: "showcase",
            capabilities: storage::StorageCapabilities::all_available(),
        };
    let system = device(
        "eui.5ec0a5e000000001",
        "PULSE Demo NVMe 2TB",
        "nvme0n1",
        storage::StorageBus::Nvme,
        2_000_398_934_016,
    );
    let mut data = device(
        "eui.5ec0a5e000000002",
        "PULSE Demo SATA SSD 1TB",
        "sda",
        storage::StorageBus::Ata,
        1_000_204_886_016,
    );
    // SATA drives expose no NVMe health log.
    let no_nvme = Availability::unsupported("NVMe health is only defined for NVMe devices");
    data.capabilities.health_percentage_used = no_nvme.clone();
    data.capabilities.health_available_spare = no_nvme.clone();
    data.capabilities.health_unsafe_shutdowns = no_nvme.clone();
    data.capabilities.health_media_errors = no_nvme;

    let volume = |device: &storage::StorageDeviceDescriptor, partition, mount: &str, fs: &str| {
        storage::StorageVolumeDescriptor {
            source_id: storage::partition_volume_source_id(&device.source_id, partition)
                .expect("valid volume"),
            display_name: mount.to_string(),
            identity: storage::VolumeIdentity::Partition {
                device: device.source_id.clone(),
                partition,
            },
            filesystem: Some(fs.to_string()),
            mount_points: vec![mount.to_string()],
            device: Some(device.source_id.clone()),
            read_only: false,
            backend: "showcase",
            capabilities: storage::VolumeCapabilities::all_available(),
        }
    };
    let volumes = vec![
        volume(&system, 3, "/", "btrfs"),
        volume(&data, 1, "/mnt/data", "ext4"),
    ];
    (vec![system, data], volumes)
}

fn interfaces() -> Vec<network::NetworkInterfaceDescriptor> {
    // Locally administered addresses (02:…): never assigned to real hardware.
    let interface = |mac: [u8; 6], name: &str, kind, wifi| {
        let (source_id, identity) =
            network::interface_identity(Some(&mac), None, Some(&mac), name).expect("identified");
        network::NetworkInterfaceDescriptor {
            source_id,
            display_name: name.to_string(),
            os_name: name.to_string(),
            identity,
            kind,
            permanent_mac: network::format_mac(&mac),
            current_mac: network::format_mac(&mac),
            backend: "showcase",
            capabilities: network::NetworkCapabilities::all_available(),
            wifi,
        }
    };
    vec![
        interface(
            [0x02, 0x5e, 0xc0, 0xa5, 0xe0, 0x01],
            "enp6s0",
            network::NetworkInterfaceKind::Ethernet,
            None,
        ),
        interface(
            [0x02, 0x5e, 0xc0, 0xa5, 0xe0, 0x02],
            "wlp4s0",
            network::NetworkInterfaceKind::Wifi,
            Some(network::WifiCapabilities::all_available()),
        ),
    ]
}

fn main() {
    let (devices, volumes) = storage();
    let mut catalog: Vec<MetricDefinition> = Vec::new();
    catalog.extend(cpu::definitions(&provider("linux.cpu"), &cpu_topology()));
    catalog.extend(memory::definitions(&provider("linux.memory")));
    catalog.extend(gpu::definitions(&provider("linux.gpu"), &gpus()));
    catalog.extend(storage::definitions(
        &provider("linux.storage"),
        &devices,
        &volumes,
    ));
    catalog.extend(network::definitions(
        &provider("linux.network"),
        &interfaces(),
    ));
    catalog.extend(process::definitions(
        &provider("linux.processes"),
        Availability::Available,
    ));

    let source_refs: BTreeMap<String, String> = catalog
        .iter()
        .map(|definition| {
            (
                definition.metric.source_id.as_str().to_string(),
                services::metrics::persistable_source_ref(&definition.metric.source_id),
            )
        })
        .collect();

    let output = serde_json::json!({
        "generatedBy": "scripts/showcase/catalog (PULSE metric declarations, fictional hardware)",
        "catalog": catalog,
        "sourceRefs": source_refs,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("serialisable")
    );
}
