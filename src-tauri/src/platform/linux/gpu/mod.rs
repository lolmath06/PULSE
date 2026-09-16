//! The Fedora GPU provider.
//!
//! **One provider owns every GPU metric on the machine**, whatever mix of
//! vendors it holds:
//!
//! ```text
//! linux.gpu
//!  ├── generic inventory   /sys/class/drm        every adapter, every vendor
//!  ├── NVIDIA capability   NVML (runtime-loaded) usage, VRAM, clocks
//!  └── AMD capability      amdgpu sysfs          usage, VRAM, clocks
//! ```
//!
//! Registering `linux.gpu`, `nvidia.nvml` and `amd.sysfs` as three separate
//! providers would be the obvious alternative and is wrong: an NVIDIA card is
//! seen by both the DRM inventory and NVML, so two providers would claim the
//! same `MetricRef` and the engine would reject one of them outright. Owning
//! the whole family in one provider is what lets the backends be merged before
//! anything is published.
//!
//! So on a machine with a GPU:
//!
//! ```text
//! Providers = 3        linux.cpu, linux.memory, linux.gpu
//! ```
//!
//! # Degradation
//!
//! Every layer is optional and fails alone. No NVML leaves the DRM inventory
//! intact; no `amdgpu` attributes leave the card inventoried; no GPU at all
//! leaves `gpu.count` reporting zero, which is a fact rather than a failure.

pub mod amdgpu;
pub mod drm;

use std::path::PathBuf;
use std::sync::Arc;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricRef, MetricSample, ProviderId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::gpu::{self, GpuDescriptor, GpuTelemetry, GpuVendor};
use crate::platform::gpu::{self as shared, InventoriedGpu};
use crate::platform::nvml::NvmlBackend;

/// Identifier of the Linux GPU provider.
pub const PROVIDER_ID: &str = "linux.gpu";

/// Which backend serves one inventoried device.
#[derive(Debug)]
enum Telemetry {
    /// NVML, by its enumeration index for this session.
    Nvml(u32),
    /// The `amdgpu` driver's sysfs attributes, under this device directory.
    Amdgpu(PathBuf),
    /// Nothing can be measured — the device is inventoried only.
    None,
}

/// One device, with the backend that measures it.
#[derive(Debug)]
struct Device {
    descriptor: GpuDescriptor,
    telemetry: Telemetry,
}

/// What the provider discovered at startup.
///
/// Identity, names and capabilities are **static for the life of the process**
/// and are built once here. A refresh re-reads only the numbers that move; it
/// does not re-enumerate PCI, reload NVML or re-resolve any symbol.
struct GpuInventory {
    devices: Vec<Device>,
    /// Held for the life of the provider so its device handles stay valid.
    /// `None` when the NVIDIA driver is not installed, which is normal.
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
        let ids = drm::PciIds::load();
        let cards = drm::discover();

        // Loading NVML is the one step that can be genuinely absent. It is
        // reported once, here, and never retried per metric.
        let (nvml, nvml_error) = match crate::platform::nvml::library::NvmlLibrary::load() {
            Ok(library) => (Some(Arc::new(library) as Arc<dyn NvmlBackend>), None),
            Err(error) => (None, Some(error)),
        };

        let nvidia_reason = nvml_error
            .as_ref()
            .map(crate::platform::nvml::availability_for_nvml)
            .unwrap_or_else(|| {
                Availability::unsupported("this device is not served by the NVIDIA driver")
            });

        // Each inventoried card starts with no telemetry, and states why in
        // terms that fit the card rather than a generic message.
        let generic: Vec<GpuDescriptor> = cards
            .iter()
            .filter_map(|card| {
                let reason = reason_for(card, &nvidia_reason);
                drm::describe(card, &ids, &reason)
            })
            .collect();

        let vendor = nvml
            .as_ref()
            .and_then(|nvml| shared::nvml_descriptors(nvml.as_ref()).ok())
            .unwrap_or_default();

        let merged = shared::merge(generic, vendor);
        let devices = merged
            .into_iter()
            .map(|entry| Self::attach_telemetry(entry, &cards))
            .collect();

        Self { devices, nvml }
    }

    /// Decides which backend measures a merged device, and refines its
    /// capabilities from what that backend actually reports.
    fn attach_telemetry(entry: InventoriedGpu, cards: &[drm::DrmCard]) -> Device {
        if let Some(index) = entry.nvml_index {
            // NVML already probed its own capabilities during discovery.
            return Device {
                descriptor: entry.descriptor,
                telemetry: Telemetry::Nvml(index),
            };
        }

        let card = entry
            .descriptor
            .pci
            .and_then(|pci| cards.iter().find(|card| card.pci == pci));

        let is_amdgpu = card
            .and_then(|card| card.driver.as_deref())
            .is_some_and(|driver| driver == amdgpu::DRIVER);

        if let (true, Some(card)) = (is_amdgpu, card) {
            let device_path = card.device_path(std::path::Path::new(drm::DRM_ROOT));
            let reading = amdgpu::read_device(&device_path);
            let mut descriptor = entry.descriptor;
            descriptor.backend = "amdgpu";
            descriptor.capabilities = amd_capabilities(&reading);

            return Device {
                descriptor,
                telemetry: Telemetry::Amdgpu(device_path),
            };
        }

        Device {
            descriptor: entry.descriptor,
            telemetry: Telemetry::None,
        }
    }

    fn telemetry_for(&self, device: &Device, nvml: Option<&dyn NvmlBackend>) -> GpuTelemetry {
        match &device.telemetry {
            Telemetry::Nvml(index) => match nvml {
                Some(nvml) => shared::nvml_telemetry(nvml, *index),
                None => GpuTelemetry::default(),
            },
            Telemetry::Amdgpu(path) => {
                let reading = amdgpu::read_device(path);

                GpuTelemetry {
                    usage_core: reading.usage_percent,
                    memory: reading.memory,
                    frequency_core_hz: reading.core_clock_hz,
                    frequency_memory_hz: reading.memory_clock_hz,
                }
            }
            Telemetry::None => GpuTelemetry::default(),
        }
    }
}

/// Explains, per card, why it has no telemetry yet.
///
/// An NVIDIA card without NVML is a different story from an Intel card PULSE
/// has no backend for, and the user deserves the accurate one.
fn reason_for(card: &drm::DrmCard, nvidia_reason: &Availability) -> Availability {
    match card.vendor() {
        GpuVendor::Nvidia => nvidia_reason.clone(),
        GpuVendor::Amd => {
            Availability::unsupported("this AMD device exposes no amdgpu telemetry attributes")
        }
        GpuVendor::Intel => Availability::unsupported(
            "PULSE has no Intel GPU telemetry backend yet; the device is inventoried only",
        ),
        GpuVendor::Other(_) => Availability::unsupported(
            "PULSE has no telemetry backend for this GPU vendor; the device is \
             inventoried only",
        ),
    }
}

/// Derives capabilities from what the `amdgpu` attributes actually produced.
///
/// Probed once at startup: the files a driver exposes do not change while the
/// machine runs, even though their values do.
fn amd_capabilities(reading: &amdgpu::AmdgpuReading) -> gpu::GpuCapabilities {
    let missing = |what: &str| {
        Availability::unsupported(format!(
            "the amdgpu driver on this device exposes no {what}"
        ))
    };

    let memory = match reading.memory {
        Some(_) => Availability::Available,
        None => missing("VRAM information"),
    };

    gpu::GpuCapabilities {
        usage_core: match reading.usage_percent {
            Some(_) => Availability::Available,
            None => missing("engine utilisation counter"),
        },
        memory_total: memory.clone(),
        memory_used: memory.clone(),
        memory_free: memory.clone(),
        memory_usage_percent: memory,
        frequency_core: match reading.core_clock_hz {
            Some(_) => Availability::Available,
            None => missing("active core clock state"),
        },
        frequency_memory: match reading.memory_clock_hz {
            Some(_) => Availability::Available,
            None => missing("active memory clock state"),
        },
    }
}

/// Publishes every GPU metric on Fedora.
///
/// Requires no elevated privileges: `/sys/class/drm` is world-readable and
/// NVML's queries work for any user.
#[derive(Debug)]
pub struct LinuxGpuProvider {
    id: ProviderId,
    inventory: GpuInventory,
}

impl LinuxGpuProvider {
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

impl Default for LinuxGpuProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for LinuxGpuProvider {
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
            |device| {
                self.inventory
                    .telemetry_for(device, self.inventory.nvml.as_deref())
            },
            self.inventory.devices.len(),
        ))
    }
}

/// Answers a request from freshly read telemetry.
///
/// Each device is read **at most once per request**, however many of its seven
/// metrics were asked for — one NVML round trip or one sysfs pass, not seven.
fn samples_from(
    requested: &[MetricRef],
    devices: &[Device],
    mut read: impl FnMut(&Device) -> GpuTelemetry,
    count: usize,
) -> Vec<MetricSample> {
    use std::collections::BTreeMap;

    // Only the devices this request actually touches.
    let mut needed: BTreeMap<&str, GpuTelemetry> = BTreeMap::new();
    for reference in requested {
        if reference.key.as_str() == gpu::COUNT {
            continue;
        }
        let source = reference.source_id.as_str();
        if needed.contains_key(source) {
            continue;
        }
        if let Some(device) = devices
            .iter()
            .find(|device| device.descriptor.source_id.as_str() == source)
        {
            needed.insert(source, read(device));
        }
    }

    requested
        .iter()
        .map(|reference| {
            if reference.key.as_str() == gpu::COUNT {
                return MetricSample::number(reference.clone(), count as f64);
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

            let value = needed
                .get(source)
                .and_then(|telemetry| telemetry.value_for(reference.key.as_str()));

            match value {
                Some(value) => MetricSample::number(reference.clone(), value),
                // Never a fabricated zero: the catalog's own availability
                // already explains why this device cannot answer.
                None => MetricSample::unavailable(
                    reference.clone(),
                    capability_for(device, reference.key.as_str()),
                ),
            }
        })
        .collect()
}

/// The declared availability of one metric on one device.
fn capability_for(device: &Device, key: &str) -> Availability {
    let capabilities = &device.descriptor.capabilities;

    let declared = match key {
        gpu::USAGE_CORE => &capabilities.usage_core,
        gpu::MEMORY_TOTAL => &capabilities.memory_total,
        gpu::MEMORY_USED => &capabilities.memory_used,
        gpu::MEMORY_FREE => &capabilities.memory_free,
        gpu::MEMORY_USAGE_PERCENT => &capabilities.memory_usage_percent,
        gpu::FREQUENCY_CORE => &capabilities.frequency_core,
        gpu::FREQUENCY_MEMORY => &capabilities.frequency_memory,
        _ => {
            return Availability::not_registered(format!("'{key}' is not a GPU metric"));
        }
    };

    if declared.is_available() {
        // The catalog said it was readable and this read failed: a transient
        // problem rather than a missing capability.
        return Availability::temporarily_unavailable(
            "the driver did not return this figure for the current sample",
        );
    }

    declared.clone()
}

/// Builds the Linux GPU provider.
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(LinuxGpuProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::wellknown::gpu::{pci_source_id, GpuCapabilities, GpuIdentity, PciAddress};

    fn device(bus: u8, capabilities: GpuCapabilities) -> Device {
        let pci = PciAddress::new(0, bus, 0, 0);

        Device {
            descriptor: GpuDescriptor {
                source_id: pci_source_id(pci).expect("valid"),
                display_name: "Test GPU".to_string(),
                vendor: GpuVendor::Amd,
                identity: GpuIdentity::Pci(pci),
                pci: Some(pci),
                backend: "amdgpu",
                capabilities,
            },
            telemetry: Telemetry::None,
        }
    }

    #[test]
    fn the_count_is_answered_without_touching_any_device() {
        let devices = vec![
            device(1, GpuCapabilities::all_available()),
            device(2, GpuCapabilities::all_available()),
        ];
        let mut reads = 0;

        let samples = samples_from(
            &[gpu::count_ref()],
            &devices,
            |_| {
                reads += 1;
                GpuTelemetry::default()
            },
            devices.len(),
        );

        assert_eq!(reads, 0, "the count needs no telemetry");
        assert_eq!(
            samples[0].value.as_ref().and_then(|v| v.as_number()),
            Some(2.0)
        );
    }

    #[test]
    fn each_device_is_read_once_however_many_of_its_metrics_are_requested() {
        // The performance promise: seven metrics, one round trip.
        let devices = vec![device(1, GpuCapabilities::all_available())];
        let source = devices[0].descriptor.source_id.clone();
        let requested: Vec<MetricRef> = gpu::PER_GPU_KEYS
            .iter()
            .map(|key| gpu::gpu_ref(key, &source))
            .collect();

        let mut reads = 0;
        let samples = samples_from(
            &requested,
            &devices,
            |_| {
                reads += 1;
                GpuTelemetry {
                    usage_core: Some(17.0),
                    ..GpuTelemetry::default()
                }
            },
            devices.len(),
        );

        assert_eq!(reads, 1, "one read served all seven metrics");
        assert_eq!(samples.len(), 7);
    }

    #[test]
    fn a_device_not_asked_about_is_never_read() {
        let devices = vec![
            device(1, GpuCapabilities::all_available()),
            device(2, GpuCapabilities::all_available()),
        ];
        let source = devices[0].descriptor.source_id.clone();

        let mut read_sources = Vec::new();
        let _ = samples_from(
            &[gpu::usage_core_ref(&source)],
            &devices,
            |device| {
                read_sources.push(device.descriptor.source_id.as_str().to_string());
                GpuTelemetry::default()
            },
            devices.len(),
        );

        assert_eq!(read_sources, vec![source.as_str().to_string()]);
    }

    #[test]
    fn an_unmeasured_metric_carries_its_declared_reason_never_a_zero() {
        let reason = Availability::unsupported("the amdgpu driver exposes no clock state");
        let capabilities = GpuCapabilities::all_available().with_frequencies(&reason);
        let devices = vec![device(1, capabilities)];
        let source = devices[0].descriptor.source_id.clone();

        let samples = samples_from(
            &[gpu::frequency_core_ref(&source)],
            &devices,
            |_| GpuTelemetry::default(),
            devices.len(),
        );

        assert!(samples[0].value.is_none(), "no fabricated value");
        assert_eq!(samples[0].availability.status_str(), "unsupported");
        assert_eq!(samples[0].availability, reason);
    }

    #[test]
    fn a_metric_that_was_readable_but_failed_this_time_is_transient() {
        // Declared available, yet this sample produced nothing: the honest
        // answer is "try again", not "your GPU lacks this".
        let devices = vec![device(1, GpuCapabilities::all_available())];
        let source = devices[0].descriptor.source_id.clone();

        let samples = samples_from(
            &[gpu::usage_core_ref(&source)],
            &devices,
            |_| GpuTelemetry::default(),
            devices.len(),
        );

        assert!(samples[0].value.is_none());
        assert_eq!(
            samples[0].availability.status_str(),
            "temporarilyUnavailable"
        );
        assert!(samples[0].availability.is_transient());
    }

    #[test]
    fn a_reference_to_an_unknown_gpu_is_answered_rather_than_failing() {
        let devices = vec![device(1, GpuCapabilities::all_available())];
        let stale = pci_source_id(PciAddress::new(0, 0x7F, 0, 0)).expect("valid");

        let samples = samples_from(
            &[gpu::usage_core_ref(&stale)],
            &devices,
            |_| GpuTelemetry::default(),
            devices.len(),
        );

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].availability.status_str(), "notRegistered");
    }

    #[test]
    fn every_requested_reference_is_answered_in_order() {
        let devices = vec![device(1, GpuCapabilities::all_available())];
        let source = devices[0].descriptor.source_id.clone();
        let requested = vec![
            gpu::count_ref(),
            gpu::usage_core_ref(&source),
            gpu::memory_total_ref(&source),
        ];

        let samples = samples_from(
            &requested,
            &devices,
            |_| GpuTelemetry {
                usage_core: Some(50.0),
                ..GpuTelemetry::default()
            },
            devices.len(),
        );

        assert_eq!(samples.len(), requested.len());
        for (sample, reference) in samples.iter().zip(requested.iter()) {
            assert_eq!(&sample.metric, reference);
        }
    }

    #[test]
    fn a_machine_with_no_gpu_reports_zero_rather_than_failing() {
        let samples = samples_from(&[gpu::count_ref()], &[], |_| GpuTelemetry::default(), 0);

        assert_eq!(
            samples[0].value.as_ref().and_then(|v| v.as_number()),
            Some(0.0)
        );
        assert!(samples[0].availability.is_available());
    }

    // --- AMD capability derivation ---------------------------------------

    #[test]
    fn amd_capabilities_follow_what_the_driver_exposed() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let reading = amdgpu::AmdgpuReading {
            usage_percent: Some(47.0),
            memory: crate::metrics::wellknown::gpu::GpuMemoryReading::from_total_and_used(
                8 * GIB,
                2 * GIB,
            )
            .ok(),
            core_clock_hz: Some(2_100_000_000),
            memory_clock_hz: None,
        };

        let capabilities = amd_capabilities(&reading);

        assert!(capabilities.usage_core.is_available());
        assert!(capabilities.memory_total.is_available());
        assert!(capabilities.frequency_core.is_available());
        assert_eq!(
            capabilities.frequency_memory.status_str(),
            "unsupported",
            "an absent pp_dpm_mclk costs only the memory clock"
        );
        assert_eq!(capabilities.available_count(), 6);
    }

    #[test]
    fn a_card_exposing_nothing_declares_all_seven_unavailable() {
        let capabilities = amd_capabilities(&amdgpu::AmdgpuReading::default());

        assert_eq!(capabilities.available_count(), 0);
    }

    // --- per-vendor reasons ----------------------------------------------

    #[test]
    fn each_vendor_gets_an_accurate_explanation() {
        let nvidia_reason = Availability::unsupported("libnvidia-ml.so.1 is not installed");
        let card = |vendor_id: u16| drm::DrmCard {
            index: 0,
            pci: PciAddress::new(0, 1, 0, 0),
            vendor_id,
            device_id: 0x1234,
            driver: None,
        };

        // An NVIDIA card without NVML says so, rather than "no backend".
        let nvidia = reason_for(&card(0x10DE), &nvidia_reason);
        assert_eq!(nvidia, nvidia_reason);

        let intel = reason_for(&card(0x8086), &nvidia_reason);
        assert!(format!("{intel:?}").contains("Intel"));

        let unknown = reason_for(&card(0x1234), &nvidia_reason);
        assert_eq!(unknown.status_str(), "unsupported");
    }

    // --- host check -------------------------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn the_provider_describes_this_host_consistently() {
        let provider = LinuxGpuProvider::new();
        let descriptors = provider.descriptors();
        let definitions = provider.describe().expect("describe");

        assert_eq!(definitions.len(), 1 + 7 * descriptors.len());
        assert_eq!(provider.id().as_str(), "linux.gpu");

        // Every device has a unique, valid identity.
        let mut sources: Vec<&str> = descriptors
            .iter()
            .map(|gpu| gpu.source_id.as_str())
            .collect();
        let total = sources.len();
        sources.sort_unstable();
        sources.dedup();
        assert_eq!(sources.len(), total, "no GPU published twice");

        // And the whole catalog samples without panicking, whatever this
        // machine's driver situation is.
        let requested: Vec<MetricRef> = definitions
            .iter()
            .map(|definition| definition.metric.clone())
            .collect();
        let samples = provider.sample(&requested).expect("sample");
        assert_eq!(samples.len(), requested.len());

        for sample in &samples {
            if !sample.availability.is_available() {
                assert!(sample.value.is_none(), "{} invented a value", sample.metric);
            }
        }
    }
}
