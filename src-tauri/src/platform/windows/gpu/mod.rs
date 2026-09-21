//! The Windows GPU provider.
//!
//! Same shape as its Fedora counterpart, and for the same reason: **one
//! provider owns every GPU metric on the machine**, because the generic
//! inventory and the vendor backend both see the same physical card and two
//! providers claiming one `MetricRef` would make the engine reject one of them.
//!
//! ```text
//! windows.gpu
//!  ├── generic inventory   DXGI                  every adapter, every vendor
//!  └── NVIDIA capability   NVML (runtime-loaded) usage, VRAM, clocks
//! ```
//!
//! ```text
//! Providers = 3        windows.cpu, windows.memory, windows.gpu
//! ```
//!
//! # NVIDIA is identified by NVML, then correlated
//!
//! An NVIDIA card is visible to both DXGI and NVML. PULSE publishes it **once**,
//! under NVML's hardware UUID rather than DXGI's model tuple, because the UUID
//! is a genuine hardware identity and the tuple is not. `platform::gpu::merge`
//! performs the correlation; on this platform it pairs by vendor and
//! enumeration order, since DXGI exposes no bus address to match on.
//!
//! Without NVML, an NVIDIA card still appears — inventoried by DXGI, with its
//! VRAM capacity and an honest `unsupported` on everything else.

pub mod d3dkmt;
pub mod dxgi;

use std::sync::Arc;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricRef, MetricSample, ProviderId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::gpu::{self, GpuDescriptor, GpuMemoryReading, GpuTelemetry};
use crate::platform::gpu::{self as shared, InventoriedGpu};
use crate::platform::nvml::NvmlBackend;

/// Identifier of the Windows GPU provider.
pub const PROVIDER_ID: &str = "windows.gpu";

/// Which backend serves one inventoried device.
#[derive(Debug, Clone)]
enum Telemetry {
    /// NVML, by its enumeration index for this session.
    Nvml(u32),
    /// DXGI only: a static VRAM capacity and nothing that moves.
    Capacity { dedicated_video_memory: u64 },
    /// Nothing can be measured.
    None,
}

/// One device, with the backend that measures it.
#[derive(Debug, Clone)]
struct Device {
    descriptor: GpuDescriptor,
    telemetry: Telemetry,
}

/// Builds the device list from a DXGI inventory and an optional NVML.
///
/// Free of every Windows type, so the composition — including the
/// deduplication that stops an RTX appearing twice — is unit-tested on Fedora.
fn compose(adapters: Vec<dxgi::DxgiAdapter>, nvml: Option<&dyn NvmlBackend>) -> Vec<Device> {
    let hardware = dxgi::hardware_adapters(adapters);
    let generic = dxgi::describe_all(&hardware);

    let vendor = nvml
        .and_then(|nvml| shared::nvml_descriptors(nvml).ok())
        .unwrap_or_default();

    shared::merge(generic, vendor)
        .into_iter()
        .map(|entry| attach(entry, &hardware))
        .collect()
}

/// Decides which backend measures a merged device.
fn attach(entry: InventoriedGpu, adapters: &[dxgi::DxgiAdapter]) -> Device {
    if let Some(index) = entry.nvml_index {
        return Device {
            descriptor: entry.descriptor,
            telemetry: Telemetry::Nvml(index),
        };
    }

    // A DXGI-only adapter: its capacity is static, so it is captured here
    // rather than re-read on every refresh.
    //
    // Matched on the bus address when `D3DKMT` supplied one, and only then on
    // the description — which is not an identity and collapses two identical
    // cards onto one entry. Since this device was not claimed by a vendor
    // backend, its display name is still DXGI's own, so the fallback is exact
    // whenever the machine holds no two adapters of the same model.
    let capacity = adapters
        .iter()
        .find(|adapter| match (adapter.pci, entry.descriptor.pci) {
            (Some(left), Some(right)) => left == right,
            _ => adapter.description == entry.descriptor.display_name,
        })
        .map(|adapter| adapter.dedicated_video_memory)
        .unwrap_or(0);

    Device {
        descriptor: entry.descriptor,
        telemetry: if capacity > 0 {
            Telemetry::Capacity {
                dedicated_video_memory: capacity,
            }
        } else {
            Telemetry::None
        },
    }
}

/// Reads one device's live telemetry.
fn telemetry_for(device: &Device, nvml: Option<&dyn NvmlBackend>) -> GpuTelemetry {
    match &device.telemetry {
        Telemetry::Nvml(index) => match nvml {
            Some(nvml) => shared::nvml_telemetry(nvml, *index),
            None => GpuTelemetry::default(),
        },
        Telemetry::Capacity {
            dedicated_video_memory,
        } => GpuTelemetry {
            // The capacity only. `used` and `free` stay absent: DXGI's
            // `QueryVideoMemoryInfo` reports this process's own consumption,
            // not the system's, so publishing it would be confidently wrong.
            memory: GpuMemoryReading::from_total_and_used(*dedicated_video_memory, 0).ok(),
            ..GpuTelemetry::default()
        },
        Telemetry::None => GpuTelemetry::default(),
    }
}

/// The value for one key, honouring what this backend may publish.
fn value_for(device: &Device, telemetry: &GpuTelemetry, key: &str) -> Option<f64> {
    // A capacity-only device publishes the total and nothing else derived
    // from it — a `used` of zero would be an invention, and so would the
    // `free` and percentage that follow from it.
    if matches!(device.telemetry, Telemetry::Capacity { .. }) && key != gpu::MEMORY_TOTAL {
        return None;
    }

    telemetry.value_for(key)
}

/// What the provider discovered at startup.
struct GpuInventory {
    devices: Vec<Device>,
    /// Held for the life of the provider so its device handles stay valid.
    nvml: Option<Arc<dyn NvmlBackend>>,
}

impl std::fmt::Debug for GpuInventory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuInventory")
            .field("devices", &self.devices.len())
            .field("nvml", &self.nvml.is_some())
            .finish()
    }
}

impl GpuInventory {
    fn discover() -> Self {
        #[cfg(target_os = "windows")]
        let adapters = dxgi::imp::enumerate().unwrap_or_default();
        #[cfg(not(target_os = "windows"))]
        let adapters: Vec<dxgi::DxgiAdapter> = Vec::new();

        let nvml = crate::platform::nvml::library::NvmlLibrary::load()
            .ok()
            .map(|library| Arc::new(library) as Arc<dyn NvmlBackend>);

        Self {
            devices: compose(adapters, nvml.as_deref()),
            nvml,
        }
    }
}

/// Publishes every GPU metric on Windows.
///
/// Requires no administrator rights: DXGI enumeration and NVML's queries are
/// both available to any process.
#[derive(Debug)]
pub struct WindowsGpuProvider {
    id: ProviderId,
    inventory: GpuInventory,
}

impl WindowsGpuProvider {
    pub fn new() -> Self {
        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
            inventory: GpuInventory::discover(),
        }
    }

    /// The devices this provider published its catalog from.
    pub fn descriptors(&self) -> Vec<GpuDescriptor> {
        self.inventory
            .devices
            .iter()
            .map(|device| device.descriptor.clone())
            .collect()
    }
}

impl Default for WindowsGpuProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for WindowsGpuProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        Ok(gpu::definitions(&self.id, &self.descriptors()))
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        Ok(samples_from(
            requested,
            &self.inventory.devices,
            self.inventory.nvml.as_deref(),
        ))
    }
}

/// Answers a request from freshly read telemetry.
///
/// Each device is read **at most once per request**, however many of its seven
/// metrics were asked for.
fn samples_from(
    requested: &[MetricRef],
    devices: &[Device],
    nvml: Option<&dyn NvmlBackend>,
) -> Vec<MetricSample> {
    use std::collections::BTreeMap;

    let mut readings: BTreeMap<&str, GpuTelemetry> = BTreeMap::new();
    for reference in requested {
        if reference.key.as_str() == gpu::COUNT {
            continue;
        }
        let source = reference.source_id.as_str();
        if readings.contains_key(source) {
            continue;
        }
        if let Some(device) = devices
            .iter()
            .find(|device| device.descriptor.source_id.as_str() == source)
        {
            readings.insert(source, telemetry_for(device, nvml));
        }
    }

    requested
        .iter()
        .map(|reference| {
            if reference.key.as_str() == gpu::COUNT {
                return MetricSample::number(reference.clone(), devices.len() as f64);
            }

            let source = reference.source_id.as_str();
            let Some(device) = devices
                .iter()
                .find(|device| device.descriptor.source_id.as_str() == source)
            else {
                return MetricSample::unavailable(
                    reference.clone(),
                    Availability::not_registered(format!(
                        "'{reference}' does not name a GPU this provider inventoried"
                    )),
                );
            };

            let value = readings
                .get(source)
                .and_then(|telemetry| value_for(device, telemetry, reference.key.as_str()));

            match value {
                Some(value) => MetricSample::number(reference.clone(), value),
                None => MetricSample::unavailable(
                    reference.clone(),
                    declared_availability(device, reference.key.as_str()),
                ),
            }
        })
        .collect()
}

/// The declared availability of one metric on one device.
fn declared_availability(device: &Device, key: &str) -> Availability {
    let capabilities = &device.descriptor.capabilities;

    let declared = match key {
        gpu::USAGE_CORE => &capabilities.usage_core,
        gpu::MEMORY_TOTAL => &capabilities.memory_total,
        gpu::MEMORY_USED => &capabilities.memory_used,
        gpu::MEMORY_FREE => &capabilities.memory_free,
        gpu::MEMORY_USAGE_PERCENT => &capabilities.memory_usage_percent,
        gpu::FREQUENCY_CORE => &capabilities.frequency_core,
        gpu::FREQUENCY_MEMORY => &capabilities.frequency_memory,
        _ => return Availability::not_registered(format!("'{key}' is not a GPU metric")),
    };

    if declared.is_available() {
        return Availability::temporarily_unavailable(
            "the driver did not return this figure for the current sample",
        );
    }

    declared.clone()
}

/// Builds the Windows GPU provider.
#[cfg(target_os = "windows")]
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(WindowsGpuProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::wellknown::gpu::{GpuIdentity, GpuVendor, PciAddress};
    use crate::platform::gpu::testing::{FakeDevice, FakeNvml};

    use d3dkmt::AdapterLuid;

    const GIB: u64 = 1024 * 1024 * 1024;

    /// An adapter whose bus address `D3DKMT` could not supply.
    fn adapter(index: u32, vendor: u16, device: u16, description: &str) -> dxgi::DxgiAdapter {
        dxgi::DxgiAdapter {
            index,
            description: description.to_string(),
            vendor_id: vendor,
            device_id: device,
            subsystem_id: 0x1234_5678,
            revision: 0xA1,
            dedicated_video_memory: 8 * GIB,
            software_flag: false,
            luid: AdapterLuid::new(index + 1, 0),
            pci: None,
        }
    }

    /// The same adapter, with the bus address `D3DKMT` normally resolves.
    fn addressed_adapter(
        index: u32,
        vendor: u16,
        device: u16,
        description: &str,
        pci: PciAddress,
    ) -> dxgi::DxgiAdapter {
        dxgi::DxgiAdapter {
            pci: Some(pci),
            ..adapter(index, vendor, device, description)
        }
    }

    fn warp() -> dxgi::DxgiAdapter {
        dxgi::DxgiAdapter {
            index: 1,
            description: "Microsoft Basic Render Driver".to_string(),
            vendor_id: dxgi::MICROSOFT_VENDOR_ID,
            device_id: dxgi::BASIC_RENDER_DEVICE_ID,
            subsystem_id: 0,
            revision: 0,
            dedicated_video_memory: 0,
            software_flag: true,
            luid: AdapterLuid::new(0xFFFF, 0),
            pci: None,
        }
    }

    // --- composition and deduplication ------------------------------------

    #[test]
    fn an_nvidia_card_seen_by_dxgi_and_nvml_is_published_once() {
        // The duplicate this phase forbids by name.
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-11111111-2222-3333-4444-555555555555",
            "NVIDIA GeForce RTX 4070",
            None,
        )]);

        let devices = compose(
            vec![adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070")],
            Some(&nvml),
        );

        assert_eq!(devices.len(), 1, "one card, one entry");
        assert_eq!(devices[0].descriptor.backend, "nvml");
        // Identified by hardware UUID, not by the DXGI model tuple.
        assert!(matches!(
            devices[0].descriptor.identity,
            GpuIdentity::NvmlUuid(_)
        ));
        assert!(devices[0]
            .descriptor
            .source_id
            .as_str()
            .starts_with("gpu:nvidia-"));
    }

    #[test]
    fn software_adapters_never_reach_the_inventory() {
        let devices = compose(
            vec![
                adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070"),
                warp(),
            ],
            None,
        );

        assert_eq!(devices.len(), 1);
        assert_ne!(devices[0].descriptor.vendor, GpuVendor::Other(0x1414));
    }

    #[test]
    fn a_machine_with_only_warp_reports_no_gpu() {
        assert!(compose(vec![warp()], None).is_empty());
    }

    #[test]
    fn a_mixed_machine_keeps_both_vendors() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            None,
        )]);

        let devices = compose(
            vec![
                adapter(0, 0x8086, 0xA788, "Intel Iris Xe Graphics"),
                adapter(1, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070"),
            ],
            Some(&nvml),
        );

        assert_eq!(devices.len(), 2, "iGPU plus dGPU");
        assert!(devices
            .iter()
            .any(|device| device.descriptor.vendor == GpuVendor::Intel));
        assert!(devices
            .iter()
            .any(|device| device.descriptor.backend == "nvml"));
    }

    #[test]
    fn two_identical_nvidia_cards_stay_distinct_through_nvml() {
        let nvml = FakeNvml::with_devices(vec![
            FakeDevice::healthy("GPU-aaaa", "NVIDIA GeForce RTX 4090", None),
            FakeDevice::healthy("GPU-bbbb", "NVIDIA GeForce RTX 4090", None),
        ]);

        let devices = compose(
            vec![
                adapter(0, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090"),
                adapter(1, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090"),
            ],
            Some(&nvml),
        );

        assert_eq!(devices.len(), 2);
        assert_ne!(
            devices[0].descriptor.source_id,
            devices[1].descriptor.source_id
        );
        assert!(devices
            .iter()
            .all(|device| matches!(device.descriptor.identity, GpuIdentity::NvmlUuid(_))));
    }

    #[test]
    fn two_nvidia_cards_are_paired_by_bus_address_whatever_order_each_api_used() {
        // DXGI hands them back A, B; NVML B, A. The two APIs enumerate
        // independently, so this is not a contrived ordering.
        let a = PciAddress::new(0, 0x01, 0, 0);
        let b = PciAddress::new(0, 0x41, 0, 0);

        let nvml = FakeNvml::with_devices(vec![
            FakeDevice::healthy("GPU-bbbb", "NVIDIA GeForce RTX 4090", Some(b)),
            FakeDevice::healthy("GPU-aaaa", "NVIDIA GeForce RTX 4090", Some(a)),
        ]);

        let devices = compose(
            vec![
                addressed_adapter(0, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090", a),
                addressed_adapter(1, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090", b),
            ],
            Some(&nvml),
        );

        assert_eq!(devices.len(), 2, "two cards, two entries");

        // Each published card kept the address it was actually discovered at.
        for device in &devices {
            let expected = match device.descriptor.source_id.as_str() {
                "gpu:nvidia-aaaa" => a,
                "gpu:nvidia-bbbb" => b,
                other => panic!("unexpected source {other}"),
            };
            assert_eq!(device.descriptor.pci, Some(expected));
        }
    }

    #[test]
    fn a_single_card_is_paired_even_without_a_bus_address() {
        // The documented one-to-one fallback: one NVIDIA adapter, one NVML
        // device, nothing ambiguous left to get wrong.
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070 Laptop GPU",
            None,
        )]);

        let devices = compose(
            vec![adapter(
                0,
                0x10DE,
                0x2820,
                "NVIDIA GeForce RTX 4070 Laptop GPU",
            )],
            Some(&nvml),
        );

        assert_eq!(devices.len(), 1);
        assert!(matches!(devices[0].telemetry, Telemetry::Nvml(0)));
        assert!(matches!(
            devices[0].descriptor.identity,
            GpuIdentity::NvmlUuid(_)
        ));
    }

    #[test]
    fn two_cards_without_bus_addresses_are_never_paired_by_enumeration_order() {
        // `D3DKMT` answered for neither adapter. Pairing device 0 with adapter
        // 0 would be a guess that gets written into a saved dashboard, so the
        // NVML devices are published on their own and the adapters — which must
        // be those same cards — are absorbed rather than counted again.
        let nvml = FakeNvml::with_devices(vec![
            FakeDevice::healthy("GPU-aaaa", "NVIDIA GeForce RTX 4090", None),
            FakeDevice::healthy("GPU-bbbb", "NVIDIA GeForce RTX 4090", None),
        ]);

        let devices = compose(
            vec![
                adapter(0, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090"),
                adapter(1, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090"),
            ],
            Some(&nvml),
        );

        assert_eq!(devices.len(), 2);
        assert!(
            devices
                .iter()
                .all(|device| matches!(device.telemetry, Telemetry::Nvml(_))),
            "no adapter may be published beside the device it duplicates"
        );
        // And none of them inherited a DXGI capacity it was never matched to.
        assert!(devices
            .iter()
            .all(|device| !matches!(device.telemetry, Telemetry::Capacity { .. })));
    }

    #[test]
    fn a_bus_address_match_is_unaffected_by_an_amd_card_beside_it() {
        let nvidia_pci = PciAddress::new(0, 0x01, 0, 0);
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            Some(nvidia_pci),
        )]);

        let devices = compose(
            vec![
                addressed_adapter(
                    0,
                    0x1002,
                    0x73FF,
                    "AMD Radeon RX 6600",
                    PciAddress::new(0, 0x03, 0, 0),
                ),
                addressed_adapter(1, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070", nvidia_pci),
            ],
            Some(&nvml),
        );

        assert_eq!(devices.len(), 2);
        assert_eq!(
            devices
                .iter()
                .filter(|device| device.descriptor.vendor == GpuVendor::Amd)
                .count(),
            1
        );
    }

    #[test]
    fn an_nvidia_card_without_nvml_is_still_inventoried() {
        let devices = compose(
            vec![adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070")],
            None,
        );

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].descriptor.backend, "dxgi");
        // Its capacity survives; its live telemetry does not.
        assert!(devices[0]
            .descriptor
            .capabilities
            .memory_total
            .is_available());
        assert!(!devices[0].descriptor.capabilities.usage_core.is_available());
    }

    #[test]
    fn the_composition_is_deterministic() {
        let build = || {
            compose(
                vec![
                    adapter(0, 0x1002, 0x73FF, "AMD Radeon RX 6600"),
                    adapter(1, 0x8086, 0xA788, "Intel Iris Xe Graphics"),
                ],
                None,
            )
        };

        let first: Vec<String> = build()
            .iter()
            .map(|device| device.descriptor.source_id.as_str().to_string())
            .collect();
        let second: Vec<String> = build()
            .iter()
            .map(|device| device.descriptor.source_id.as_str().to_string())
            .collect();

        assert_eq!(first, second);
    }

    // --- sampling ---------------------------------------------------------

    #[test]
    fn a_capacity_only_adapter_publishes_its_total_and_nothing_derived() {
        // The heart of the "do not publish process-scoped usage" rule.
        let devices = compose(vec![adapter(0, 0x1002, 0x73FF, "AMD Radeon RX 6600")], None);
        let source = devices[0].descriptor.source_id.clone();

        let requested: Vec<MetricRef> = gpu::PER_GPU_KEYS
            .iter()
            .map(|key| gpu::gpu_ref(key, &source))
            .collect();
        let samples = samples_from(&requested, &devices, None);

        let value_of = |key: &str| {
            samples
                .iter()
                .find(|sample| sample.metric.key.as_str() == key)
                .and_then(|sample| sample.value.as_ref())
                .and_then(|value| value.as_number())
        };

        assert_eq!(value_of(gpu::MEMORY_TOTAL), Some((8 * GIB) as f64));
        assert_eq!(
            value_of(gpu::MEMORY_USED),
            None,
            "process-scoped, not published"
        );
        assert_eq!(value_of(gpu::MEMORY_FREE), None);
        assert_eq!(value_of(gpu::MEMORY_USAGE_PERCENT), None);
        assert_eq!(value_of(gpu::USAGE_CORE), None);
        assert_eq!(value_of(gpu::FREQUENCY_CORE), None);
    }

    #[test]
    fn an_nvml_served_card_publishes_everything() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            None,
        )]);
        let devices = compose(
            vec![adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070")],
            Some(&nvml),
        );
        let source = devices[0].descriptor.source_id.clone();

        let requested: Vec<MetricRef> = gpu::PER_GPU_KEYS
            .iter()
            .map(|key| gpu::gpu_ref(key, &source))
            .collect();
        let samples = samples_from(&requested, &devices, Some(&nvml));

        assert!(
            samples.iter().all(|sample| sample.value.is_some()),
            "a fully served card publishes all seven"
        );
    }

    #[test]
    fn the_count_reflects_the_hardware_adapters_only() {
        let devices = compose(
            vec![
                adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070"),
                warp(),
            ],
            None,
        );

        let samples = samples_from(&[gpu::count_ref()], &devices, None);

        assert_eq!(
            samples[0]
                .value
                .as_ref()
                .and_then(|value| value.as_number()),
            Some(1.0)
        );
    }

    #[test]
    fn each_device_is_read_once_per_request() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy("GPU-aaaa", "RTX", None)]);
        let devices = compose(vec![adapter(0, 0x10DE, 0x2820, "RTX")], Some(&nvml));
        let source = devices[0].descriptor.source_id.clone();

        let before = nvml.call_count("utilization");
        let requested: Vec<MetricRef> = gpu::PER_GPU_KEYS
            .iter()
            .map(|key| gpu::gpu_ref(key, &source))
            .collect();
        let _ = samples_from(&requested, &devices, Some(&nvml));

        assert_eq!(
            nvml.call_count("utilization") - before,
            1,
            "seven metrics, one round trip"
        );
    }

    #[test]
    fn an_unmeasured_metric_never_becomes_a_zero() {
        let devices = compose(vec![adapter(0, 0x8086, 0xA788, "Intel Iris Xe")], None);
        let source = devices[0].descriptor.source_id.clone();

        let samples = samples_from(&[gpu::usage_core_ref(&source)], &devices, None);

        assert!(samples[0].value.is_none());
        assert!(!samples[0].availability.is_available());
    }

    #[test]
    fn a_stale_reference_is_answered_rather_than_failing() {
        let devices = compose(vec![adapter(0, 0x10DE, 0x2820, "RTX")], None);
        let stale = crate::metrics::wellknown::gpu::device_model_source_id(
            GpuVendor::Amd,
            0xFFFF,
            0,
            0,
            None,
        )
        .expect("valid");

        let samples = samples_from(&[gpu::usage_core_ref(&stale)], &devices, None);

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].availability.status_str(), "notRegistered");
    }

    #[test]
    fn a_headless_machine_reports_zero_gpus() {
        let samples = samples_from(&[gpu::count_ref()], &[], None);

        assert_eq!(
            samples[0]
                .value
                .as_ref()
                .and_then(|value| value.as_number()),
            Some(0.0)
        );
        assert!(samples[0].availability.is_available());
    }

    #[test]
    fn the_catalog_keeps_all_seven_metrics_per_device_whatever_the_backend() {
        let provider = ProviderId::new(PROVIDER_ID).expect("valid");
        let devices = compose(
            vec![
                adapter(0, 0x8086, 0xA788, "Intel Iris Xe Graphics"),
                adapter(1, 0x1002, 0x73FF, "AMD Radeon RX 6600"),
            ],
            None,
        );
        let descriptors: Vec<GpuDescriptor> = devices
            .iter()
            .map(|device| device.descriptor.clone())
            .collect();

        assert_eq!(gpu::definitions(&provider, &descriptors).len(), 1 + 14);
    }
}
