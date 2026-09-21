//! Composing one GPU inventory out of several backends.
//!
//! Both operating systems face the same shape of problem: a **generic
//! inventory** knows which adapters exist (DRM on Fedora, DXGI on Windows),
//! and a **vendor backend** knows how to measure them (NVML today). The two
//! see the same physical card, and PULSE must publish it once.
//!
//! ```text
//! generic inventory          vendor backend
//!   card0 / adapter 0  ◄────► NVML device 0
//!   card1 / adapter 1          (no NVML device)
//!        │                           │
//!        └────────── merge ──────────┘
//!                      │
//!            one descriptor per physical GPU
//! ```
//!
//! Getting this wrong in either direction is visible to the user: merge too
//! eagerly and two cards collapse into one; merge too little and an RTX 4070
//! appears twice, once from DXGI and once from NVML.
//!
//! # How devices are matched
//!
//! **By PCI address first.** Both DRM and NVML report the bus address of the
//! same physical slot, so this is exact — it distinguishes two identical cards
//! and cannot be fooled by naming.
//!
//! **Never by enumeration order.** Pairing the *n*-th NVIDIA adapter with the
//! *n*-th NVML device looks reasonable and is not: the two APIs enumerate
//! independently, DXGI's order reflects which adapter Windows currently
//! prefers, and NVML's reflects its own device list. On a machine with two
//! NVIDIA cards the two orders can disagree, and the result is a dashboard
//! attributing one card's telemetry to the other — silently, and permanently,
//! because the wrong identity is what gets saved. **A wrong merge is worse
//! than an unmerged inventory.**
//!
//! **A one-to-one fallback is allowed, and only that.** When exactly one
//! unmatched adapter of a vendor faces exactly one unmatched device of that
//! vendor, there is no ambiguity left to get wrong: the pairing is forced.
//! That is the case on every single-GPU laptop whose platform exposes no bus
//! address, and it is recorded here explicitly rather than falling out of an
//! ordering coincidence.
//!
//! **Never by product name.** `"NVIDIA GeForce RTX 4070"` matches both cards in
//! a two-card machine and neither after a driver reworded the string.
//!
//! # What happens to an adapter that could not be paired
//!
//! The vendor backend's devices are always published — they carry the stronger
//! identity and the telemetry. A generic entry that was not paired is then
//! **absorbed** (dropped) when it is certain to be one of those same devices:
//! that is, when the vendor serves at least as many unpaired devices as there
//! are unpaired adapters of its vendor. Absorbing is a statement about the
//! *count*, not about which adapter is which, so nothing per-adapter is
//! transferred and no identity is guessed.
//!
//! When the generic inventory holds **more** unpaired adapters of a vendor than
//! the backend has unpaired devices — NVML skipping a GPU whose UUID it cannot
//! read, for instance — they are all kept. One card may then appear twice, once
//! under each identity. That is a visible, honest degradation; dropping an
//! arbitrary one would hide real hardware, and pairing an arbitrary one would
//! mislabel it.

use std::collections::BTreeSet;

use crate::metrics::model::Availability;
use crate::metrics::wellknown::gpu::{
    nvml_source_id, utilization_percent, GpuCapabilities, GpuDescriptor, GpuIdentity,
    GpuMemoryReading, GpuTelemetry, GpuVendor,
};
use crate::metrics::wellknown::units::megahertz_to_hertz;

use super::nvml::{availability_for_nvml, NvmlBackend, NvmlClock, NvmlError};

/// One inventoried GPU, plus how to measure it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoriedGpu {
    pub descriptor: GpuDescriptor,
    /// The NVML enumeration index, when this device is served by NVML.
    ///
    /// Valid only for the life of this process — NVML documents the index as
    /// unstable — which is exactly why it lives here rather than in the
    /// descriptor's identity.
    pub nvml_index: Option<u32>,
}

/// A device NVML reported, before it is matched against the generic inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvmlDiscovered {
    pub index: u32,
    pub descriptor: GpuDescriptor,
}

/// Builds descriptors for every device NVML can see.
///
/// Each capability is probed **once**, here, so the catalog can state up front
/// what this driver offers rather than discovering it per refresh. A device
/// whose clocks are unsupported keeps its usage and memory: degradation is per
/// metric, never per device.
///
/// A device whose UUID cannot be read is skipped entirely — without a UUID
/// there is no stable identity to publish it under, and inventing one would be
/// worse than leaving the generic inventory's entry in place.
pub fn nvml_descriptors(backend: &dyn NvmlBackend) -> Result<Vec<NvmlDiscovered>, NvmlError> {
    let count = backend.device_count()?;
    let mut discovered = Vec::with_capacity(count as usize);

    for index in 0..count {
        let Ok(info) = backend.device_info(index) else {
            // One unreadable device must not hide the others.
            continue;
        };
        let Some(source_id) = nvml_source_id(&info.uuid) else {
            continue;
        };

        let capabilities = probe_capabilities(backend, index);

        discovered.push(NvmlDiscovered {
            index,
            descriptor: GpuDescriptor {
                source_id,
                display_name: info.name.clone(),
                vendor: GpuVendor::Nvidia,
                identity: GpuIdentity::NvmlUuid(info.uuid),
                pci: info.pci,
                backend: "nvml",
                capabilities,
            },
        });
    }

    Ok(discovered)
}

/// Asks the driver, once per metric family, whether it will answer at all.
fn probe_capabilities(backend: &dyn NvmlBackend, index: u32) -> GpuCapabilities {
    let usage_core = match backend.utilization(index) {
        Ok(raw) if utilization_percent(raw).is_some() => Availability::Available,
        Ok(_) => Availability::temporarily_unavailable(
            "the driver reported an implausible utilisation figure",
        ),
        Err(error) => availability_for_nvml(&error),
    };

    let memory = match backend.memory(index) {
        Ok(memory) => {
            match GpuMemoryReading::from_total_used_and_free(memory.total, memory.used, memory.free)
            {
                Ok(_) => Availability::Available,
                Err(error) => crate::metrics::wellknown::availability_for(error),
            }
        }
        Err(error) => availability_for_nvml(&error),
    };

    let clock = |domain: NvmlClock| match backend.clock_mhz(index, domain) {
        Ok(megahertz) if megahertz_to_hertz(u64::from(megahertz)).is_some() => {
            Availability::Available
        }
        // A clock domain that reports zero is powered down or unreported, not
        // running at 0 Hz.
        Ok(_) => Availability::unsupported("this clock domain reports no frequency"),
        Err(error) => availability_for_nvml(&error),
    };

    GpuCapabilities {
        usage_core,
        memory_total: memory.clone(),
        memory_used: memory.clone(),
        memory_free: memory.clone(),
        memory_usage_percent: memory,
        frequency_core: clock(NvmlClock::Graphics),
        frequency_memory: clock(NvmlClock::Memory),
    }
}

/// Reads one device's live telemetry.
///
/// Every field is independent: a driver that refuses the memory clock still
/// yields utilisation, VRAM and the core clock. Nothing here substitutes a
/// zero for a figure it could not obtain — an unread field stays `None` and
/// becomes an unavailable sample.
pub fn nvml_telemetry(backend: &dyn NvmlBackend, index: u32) -> GpuTelemetry {
    let memory = backend.memory(index).ok().and_then(|memory| {
        GpuMemoryReading::from_total_used_and_free(memory.total, memory.used, memory.free).ok()
    });

    GpuTelemetry {
        usage_core: backend
            .utilization(index)
            .ok()
            .and_then(utilization_percent),
        memory,
        frequency_core_hz: backend
            .clock_mhz(index, NvmlClock::Graphics)
            .ok()
            .and_then(|megahertz| megahertz_to_hertz(u64::from(megahertz))),
        frequency_memory_hz: backend
            .clock_mhz(index, NvmlClock::Memory)
            .ok()
            .and_then(|megahertz| megahertz_to_hertz(u64::from(megahertz))),
    }
}

/// Merges a vendor backend's devices into the generic inventory.
///
/// The vendor descriptor wins wherever the two describe the same card: it
/// carries a stronger identity (a hardware UUID rather than a slot address)
/// and the telemetry the generic inventory cannot provide.
///
/// Three passes, in decreasing order of confidence — see the module docs:
///
/// 1. exact PCI bus address,
/// 2. a forced one-to-one pairing, when one unmatched adapter of a vendor
///    faces exactly one unmatched device of it,
/// 3. absorption of the generic entries that must be the same devices.
///
/// The result is sorted by `SourceId`, so the catalog order does not depend on
/// which backend enumerated first.
pub fn merge(generic: Vec<GpuDescriptor>, vendor: Vec<NvmlDiscovered>) -> Vec<InventoriedGpu> {
    // `paired[i]` is the generic entry the i-th vendor device was matched to.
    let mut paired: Vec<Option<usize>> = vec![None; vendor.len()];
    let mut claimed: BTreeSet<usize> = BTreeSet::new();

    // Pass 1 — exact: the same slot on the same bus. This is the only match
    // that distinguishes two identical cards, and it cannot be fooled.
    for (index, device) in vendor.iter().enumerate() {
        let Some(pci) = device.descriptor.pci else {
            continue;
        };

        if let Some(position) = generic
            .iter()
            .enumerate()
            .find(|(position, candidate)| candidate.pci == Some(pci) && !claimed.contains(position))
            .map(|(position, _)| position)
        {
            claimed.insert(position);
            paired[index] = Some(position);
        }
    }

    // Pass 2 — forced: one unmatched device of a vendor, one unmatched adapter
    // of that vendor, therefore no choice to make. Deliberately *not* "the
    // first unmatched adapter": with two of either, nothing is paired at all.
    for index in 0..vendor.len() {
        if paired[index].is_some() {
            continue;
        }

        let vendor_of = vendor[index].descriptor.vendor;
        if unmatched_devices(&vendor, &paired, vendor_of) != 1 {
            continue;
        }

        let candidates = unclaimed_adapters(&generic, &claimed, vendor_of);
        if let [only] = candidates[..] {
            claimed.insert(only);
            paired[index] = Some(only);
        }
    }

    // Pass 3 — absorption: an adapter of a vendor whose backend still has at
    // least as many unpaired devices is certainly one of them, so publishing it
    // separately would count one card twice. Nothing per-adapter moves across.
    let served: BTreeSet<GpuVendor> = vendor
        .iter()
        .map(|device| device.descriptor.vendor)
        .collect();

    let mut absorbed: BTreeSet<usize> = BTreeSet::new();
    for vendor_of in served {
        let leftover = unclaimed_adapters(&generic, &claimed, vendor_of);
        if leftover.is_empty() {
            continue;
        }

        // More adapters than devices: one of them is a card the backend does
        // not serve, and there is no way to tell which. All are kept.
        if leftover.len() <= unmatched_devices(&vendor, &paired, vendor_of) {
            absorbed.extend(leftover);
        }
    }

    let mut merged: Vec<InventoriedGpu> = Vec::with_capacity(generic.len().max(vendor.len()));

    for device in vendor {
        merged.push(InventoriedGpu {
            descriptor: device.descriptor,
            nvml_index: Some(device.index),
        });
    }

    // Everything neither paired nor absorbed stays as the generic inventory
    // described it — an AMD card beside an NVIDIA one, or an NVIDIA card
    // running an open-source driver NVML does not serve.
    for (position, descriptor) in generic.into_iter().enumerate() {
        if claimed.contains(&position) || absorbed.contains(&position) {
            continue;
        }

        merged.push(InventoriedGpu {
            descriptor,
            nvml_index: None,
        });
    }

    merged.sort_by(|left, right| left.descriptor.source_id.cmp(&right.descriptor.source_id));
    merged.dedup_by(|left, right| left.descriptor.source_id == right.descriptor.source_id);
    merged
}

/// How many of a vendor's devices are still unpaired.
fn unmatched_devices(vendor: &[NvmlDiscovered], paired: &[Option<usize>], of: GpuVendor) -> usize {
    vendor
        .iter()
        .enumerate()
        .filter(|(index, device)| paired[*index].is_none() && device.descriptor.vendor == of)
        .count()
}

/// The positions of a vendor's generic entries that nothing has claimed.
fn unclaimed_adapters(
    generic: &[GpuDescriptor],
    claimed: &BTreeSet<usize>,
    of: GpuVendor,
) -> Vec<usize> {
    generic
        .iter()
        .enumerate()
        .filter(|(position, candidate)| candidate.vendor == of && !claimed.contains(position))
        .map(|(position, _)| position)
        .collect()
}

#[cfg(test)]
pub mod testing {
    //! A scripted NVML, so every failure path is exercised without a driver.

    use std::collections::BTreeMap;

    use super::*;
    use crate::metrics::wellknown::gpu::PciAddress;
    use crate::platform::nvml::{NvmlDeviceInfo, NvmlMemory};

    /// What the fake should answer for one device.
    #[derive(Debug, Clone)]
    pub struct FakeDevice {
        pub info: Result<NvmlDeviceInfo, NvmlError>,
        pub utilization: Result<u32, NvmlError>,
        pub memory: Result<NvmlMemory, NvmlError>,
        pub graphics_clock: Result<u32, NvmlError>,
        pub memory_clock: Result<u32, NvmlError>,
    }

    impl FakeDevice {
        /// A healthy 8 GiB card with everything readable.
        pub fn healthy(uuid: &str, name: &str, pci: Option<PciAddress>) -> Self {
            const GIB: u64 = 1024 * 1024 * 1024;

            Self {
                info: Ok(NvmlDeviceInfo {
                    uuid: uuid.to_string(),
                    name: name.to_string(),
                    pci,
                }),
                utilization: Ok(17),
                memory: Ok(NvmlMemory {
                    total: 8 * GIB,
                    used: 2 * GIB,
                    free: 6 * GIB,
                }),
                graphics_clock: Ok(2_100),
                memory_clock: Ok(8_001),
            }
        }

        pub fn with_utilization(mut self, value: Result<u32, NvmlError>) -> Self {
            self.utilization = value;
            self
        }

        pub fn with_memory(mut self, value: Result<NvmlMemory, NvmlError>) -> Self {
            self.memory = value;
            self
        }

        pub fn with_memory_clock(mut self, value: Result<u32, NvmlError>) -> Self {
            self.memory_clock = value;
            self
        }

        pub fn with_graphics_clock(mut self, value: Result<u32, NvmlError>) -> Self {
            self.graphics_clock = value;
            self
        }

        pub fn with_info(mut self, value: Result<NvmlDeviceInfo, NvmlError>) -> Self {
            self.info = value;
            self
        }
    }

    /// An NVML that answers from a script instead of a driver.
    #[derive(Debug, Default)]
    pub struct FakeNvml {
        devices: Vec<FakeDevice>,
        count_error: Option<NvmlError>,
        calls: std::sync::Mutex<BTreeMap<&'static str, usize>>,
    }

    impl FakeNvml {
        pub fn with_devices(devices: Vec<FakeDevice>) -> Self {
            Self {
                devices,
                count_error: None,
                calls: std::sync::Mutex::new(BTreeMap::new()),
            }
        }

        /// A library whose `device_count` itself fails.
        pub fn failing(error: NvmlError) -> Self {
            Self {
                devices: Vec::new(),
                count_error: Some(error),
                calls: std::sync::Mutex::new(BTreeMap::new()),
            }
        }

        pub fn call_count(&self, name: &str) -> usize {
            self.calls
                .lock()
                .map(|calls| calls.get(name).copied().unwrap_or(0))
                .unwrap_or(0)
        }

        fn record(&self, name: &'static str) {
            if let Ok(mut calls) = self.calls.lock() {
                *calls.entry(name).or_insert(0) += 1;
            }
        }

        fn device(&self, index: u32) -> Result<&FakeDevice, NvmlError> {
            self.devices.get(index as usize).ok_or(NvmlError::NotFound)
        }
    }

    impl NvmlBackend for FakeNvml {
        fn device_count(&self) -> Result<u32, NvmlError> {
            self.record("device_count");
            match &self.count_error {
                Some(error) => Err(error.clone()),
                None => Ok(self.devices.len() as u32),
            }
        }

        fn device_info(&self, index: u32) -> Result<NvmlDeviceInfo, NvmlError> {
            self.record("device_info");
            self.device(index)?.info.clone()
        }

        fn utilization(&self, index: u32) -> Result<u32, NvmlError> {
            self.record("utilization");
            self.device(index)?.utilization.clone()
        }

        fn memory(&self, index: u32) -> Result<NvmlMemory, NvmlError> {
            self.record("memory");
            self.device(index)?.memory.clone()
        }

        fn clock_mhz(&self, index: u32, clock: NvmlClock) -> Result<u32, NvmlError> {
            self.record("clock_mhz");
            let device = self.device(index)?;

            match clock {
                NvmlClock::Graphics => device.graphics_clock.clone(),
                NvmlClock::Memory => device.memory_clock.clone(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{FakeDevice, FakeNvml};
    use super::*;
    use crate::metrics::wellknown::gpu::{pci_source_id, PciAddress};
    use crate::platform::nvml::NvmlMemory;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn generic_nvidia(pci: PciAddress, name: &str) -> GpuDescriptor {
        GpuDescriptor {
            source_id: pci_source_id(pci).expect("valid"),
            display_name: name.to_string(),
            vendor: GpuVendor::Nvidia,
            identity: GpuIdentity::Pci(pci),
            pci: Some(pci),
            backend: "drm",
            capabilities: GpuCapabilities::none_available(&Availability::unsupported(
                "no telemetry backend",
            )),
        }
    }

    fn generic_amd(pci: PciAddress) -> GpuDescriptor {
        GpuDescriptor {
            source_id: pci_source_id(pci).expect("valid"),
            display_name: "AMD Radeon".to_string(),
            vendor: GpuVendor::Amd,
            identity: GpuIdentity::Pci(pci),
            pci: Some(pci),
            backend: "amdgpu",
            capabilities: GpuCapabilities::all_available(),
        }
    }

    // --- discovering NVML devices ----------------------------------------

    #[test]
    fn no_devices_is_an_empty_inventory_not_a_failure() {
        let nvml = FakeNvml::with_devices(Vec::new());

        assert!(nvml_descriptors(&nvml).expect("no error").is_empty());
    }

    #[test]
    fn a_failing_device_count_is_reported_rather_than_panicking() {
        let nvml = FakeNvml::failing(NvmlError::Uninitialized);

        assert_eq!(
            nvml_descriptors(&nvml).expect_err("must fail"),
            NvmlError::Uninitialized
        );
    }

    #[test]
    fn discovers_one_healthy_device() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-11111111-2222-3333-4444-555555555555",
            "NVIDIA GeForce RTX 4070",
            Some(PciAddress::new(0, 1, 0, 0)),
        )]);

        let discovered = nvml_descriptors(&nvml).expect("no error");

        assert_eq!(discovered.len(), 1);
        let gpu = &discovered[0].descriptor;
        assert_eq!(
            gpu.source_id.as_str(),
            "gpu:nvidia-11111111-2222-3333-4444-555555555555"
        );
        assert_eq!(gpu.display_name, "NVIDIA GeForce RTX 4070");
        assert_eq!(gpu.backend, "nvml");
        assert_eq!(gpu.capabilities.available_count(), 7);
        assert!(matches!(gpu.identity, GpuIdentity::NvmlUuid(_)));
    }

    #[test]
    fn discovers_several_devices_of_the_same_model_as_distinct_gpus() {
        // The identity rule: two identical cards must never collapse.
        let nvml = FakeNvml::with_devices(vec![
            FakeDevice::healthy(
                "GPU-aaaaaaaa-0000-0000-0000-000000000000",
                "NVIDIA GeForce RTX 4090",
                Some(PciAddress::new(0, 1, 0, 0)),
            ),
            FakeDevice::healthy(
                "GPU-bbbbbbbb-0000-0000-0000-000000000000",
                "NVIDIA GeForce RTX 4090",
                Some(PciAddress::new(0, 0x0B, 0, 0)),
            ),
        ]);

        let discovered = nvml_descriptors(&nvml).expect("no error");

        assert_eq!(discovered.len(), 2);
        assert_eq!(
            discovered[0].descriptor.display_name,
            discovered[1].descriptor.display_name
        );
        assert_ne!(
            discovered[0].descriptor.source_id,
            discovered[1].descriptor.source_id
        );
    }

    #[test]
    fn a_device_without_a_usable_uuid_is_skipped_rather_than_given_a_made_up_identity() {
        let nvml = FakeNvml::with_devices(vec![
            FakeDevice::healthy("", "NVIDIA GeForce RTX 4070", None),
            FakeDevice::healthy("GPU-cccccccc", "NVIDIA GeForce RTX 4080", None),
        ]);

        let discovered = nvml_descriptors(&nvml).expect("no error");

        assert_eq!(discovered.len(), 1);
        assert_eq!(
            discovered[0].descriptor.display_name,
            "NVIDIA GeForce RTX 4080"
        );
    }

    #[test]
    fn a_device_whose_info_cannot_be_read_is_skipped_not_fatal() {
        let nvml = FakeNvml::with_devices(vec![
            FakeDevice::healthy("GPU-aaaa", "First", None).with_info(Err(NvmlError::GpuIsLost)),
            FakeDevice::healthy("GPU-bbbb", "Second", None),
        ]);

        let discovered = nvml_descriptors(&nvml).expect("no error");

        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].descriptor.display_name, "Second");
    }

    // --- per-metric degradation ------------------------------------------

    #[test]
    fn an_unsupported_memory_clock_costs_only_that_metric() {
        // The case the phase calls out explicitly: everything works except the
        // memory clock, and the GPU stays perfectly usable.
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            None,
        )
        .with_memory_clock(Err(NvmlError::NotSupported))]);

        let gpu = &nvml_descriptors(&nvml).expect("no error")[0].descriptor;

        assert!(gpu.capabilities.usage_core.is_available());
        assert!(gpu.capabilities.memory_total.is_available());
        assert!(gpu.capabilities.frequency_core.is_available());
        assert_eq!(
            gpu.capabilities.frequency_memory.status_str(),
            "unsupported"
        );
        assert_eq!(gpu.capabilities.available_count(), 6);
        assert!(gpu.has_telemetry());
    }

    #[test]
    fn each_nvml_failure_reaches_the_catalog_with_its_own_meaning() {
        let cases = [
            (NvmlError::NotSupported, "unsupported"),
            (NvmlError::NoPermission, "permissionDenied"),
            (NvmlError::GpuIsLost, "temporarilyUnavailable"),
        ];

        for (error, expected) in cases {
            let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
                "GPU-aaaa",
                "NVIDIA GeForce RTX 4070",
                None,
            )
            .with_utilization(Err(error.clone()))]);

            let gpu = &nvml_descriptors(&nvml).expect("no error")[0].descriptor;

            assert_eq!(
                gpu.capabilities.usage_core.status_str(),
                expected,
                "for {error:?}"
            );
            // And nothing else was affected.
            assert!(gpu.capabilities.memory_total.is_available());
        }
    }

    #[test]
    fn inconsistent_memory_makes_only_the_memory_metrics_unavailable() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            None,
        )
        .with_memory(Ok(NvmlMemory {
            total: 8 * GIB,
            used: 9 * GIB,
            free: 0,
        }))]);

        let gpu = &nvml_descriptors(&nvml).expect("no error")[0].descriptor;

        assert!(!gpu.capabilities.memory_total.is_available());
        assert!(!gpu.capabilities.memory_usage_percent.is_available());
        assert!(gpu.capabilities.usage_core.is_available());
        assert!(gpu.capabilities.frequency_core.is_available());
    }

    #[test]
    fn a_zero_clock_is_unsupported_rather_than_a_frequency_of_zero() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            None,
        )
        .with_graphics_clock(Ok(0))]);

        let gpu = &nvml_descriptors(&nvml).expect("no error")[0].descriptor;

        assert_eq!(gpu.capabilities.frequency_core.status_str(), "unsupported");
    }

    #[test]
    fn an_implausible_utilisation_does_not_become_a_published_value() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            None,
        )
        .with_utilization(Ok(4_000_000))]);

        let gpu = &nvml_descriptors(&nvml).expect("no error")[0].descriptor;
        assert!(!gpu.capabilities.usage_core.is_available());

        let telemetry = nvml_telemetry(&nvml, 0);
        assert_eq!(telemetry.usage_core, None, "never fabricated");
    }

    // --- telemetry --------------------------------------------------------

    #[test]
    fn telemetry_converts_into_the_canonical_units() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            None,
        )]);

        let telemetry = nvml_telemetry(&nvml, 0);

        assert_eq!(telemetry.usage_core, Some(17.0));
        // MHz in, hertz out — no frontend ever sees megahertz.
        assert_eq!(telemetry.frequency_core_hz, Some(2_100_000_000));
        assert_eq!(telemetry.frequency_memory_hz, Some(8_001_000_000));

        let memory = telemetry.memory.expect("readable");
        assert_eq!(memory.total(), 8 * GIB);
        assert_eq!(memory.used(), 2 * GIB);
        assert_eq!(memory.free(), 6 * GIB);
        assert_eq!(memory.usage_percent(), 25.0);
    }

    #[test]
    fn an_unreadable_field_stays_absent_instead_of_becoming_zero() {
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            None,
        )
        .with_memory(Err(NvmlError::NoPermission))
        .with_memory_clock(Err(NvmlError::NotSupported))]);

        let telemetry = nvml_telemetry(&nvml, 0);

        assert_eq!(telemetry.usage_core, Some(17.0));
        assert!(telemetry.memory.is_none());
        assert_eq!(telemetry.frequency_memory_hz, None);
        assert_eq!(telemetry.frequency_core_hz, Some(2_100_000_000));
    }

    #[test]
    fn telemetry_for_a_missing_device_is_empty_rather_than_a_panic() {
        let nvml = FakeNvml::with_devices(Vec::new());
        let telemetry = nvml_telemetry(&nvml, 7);

        assert_eq!(telemetry, GpuTelemetry::default());
    }

    #[test]
    fn one_refresh_asks_each_question_once_per_device() {
        // Four calls: utilisation, memory, and two clock domains. Nothing is
        // asked twice to serve two metrics that share an answer.
        let nvml = FakeNvml::with_devices(vec![FakeDevice::healthy("GPU-aaaa", "RTX", None)]);

        let _ = nvml_telemetry(&nvml, 0);

        assert_eq!(nvml.call_count("utilization"), 1);
        assert_eq!(nvml.call_count("memory"), 1);
        assert_eq!(nvml.call_count("clock_mhz"), 2);
    }

    // --- merging ----------------------------------------------------------

    #[test]
    fn an_nvidia_card_seen_twice_is_published_once() {
        // The duplicate this exists to prevent: the same RTX appearing as a
        // DRM card and as an NVML device.
        let pci = PciAddress::new(0, 1, 0, 0);
        let generic = vec![generic_nvidia(pci, "NVIDIA GPU 10de:2820")];
        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            Some(pci),
        )]))
        .expect("no error");

        let merged = merge(generic, vendor);

        assert_eq!(merged.len(), 1, "one physical card, one entry");
        // The vendor descriptor won: better identity, real telemetry.
        assert_eq!(merged[0].descriptor.backend, "nvml");
        assert_eq!(merged[0].nvml_index, Some(0));
        assert!(matches!(
            merged[0].descriptor.identity,
            GpuIdentity::NvmlUuid(_)
        ));
    }

    #[test]
    fn matching_is_by_bus_address_not_by_product_name() {
        // Names deliberately disagree; the bus address is what matches.
        let pci = PciAddress::new(0, 1, 0, 0);
        let generic = vec![generic_nvidia(pci, "completely different string")];
        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            Some(pci),
        )]))
        .expect("no error");

        assert_eq!(merge(generic, vendor).len(), 1);
    }

    #[test]
    fn two_identical_nvidia_cards_merge_onto_their_own_slots() {
        let first = PciAddress::new(0, 0x01, 0, 0);
        let second = PciAddress::new(0, 0x0B, 0, 0);
        let generic = vec![
            generic_nvidia(first, "NVIDIA GPU"),
            generic_nvidia(second, "NVIDIA GPU"),
        ];
        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![
            FakeDevice::healthy("GPU-aaaa", "NVIDIA GeForce RTX 4090", Some(second)),
            FakeDevice::healthy("GPU-bbbb", "NVIDIA GeForce RTX 4090", Some(first)),
        ]))
        .expect("no error");

        let merged = merge(generic, vendor);

        assert_eq!(merged.len(), 2, "two cards, two entries");
        assert!(merged.iter().all(|gpu| gpu.nvml_index.is_some()));

        let sources: BTreeSet<&str> = merged
            .iter()
            .map(|gpu| gpu.descriptor.source_id.as_str())
            .collect();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn two_unaddressable_adapters_are_absorbed_rather_than_paired_by_order() {
        // The Windows case where `D3DKMT` could not supply a bus address for
        // either adapter. Pairing NVML device 0 with DXGI adapter 0 would be a
        // coin flip whose result gets *saved*, so nothing is paired — the two
        // NVML devices are published, and the two adapters that must be those
        // same cards are absorbed so nothing is counted twice.
        let mut first = generic_nvidia(PciAddress::new(0, 1, 0, 0), "NVIDIA A");
        first.pci = None;
        let mut second = generic_nvidia(PciAddress::new(0, 2, 0, 0), "NVIDIA B");
        second.pci = None;

        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![
            FakeDevice::healthy("GPU-aaaa", "NVIDIA GeForce RTX 4090", None),
            FakeDevice::healthy("GPU-bbbb", "NVIDIA GeForce RTX 4090", None),
        ]))
        .expect("no error");

        let merged = merge(vec![first, second], vendor);

        assert_eq!(merged.len(), 2, "neither card is published twice");
        assert!(
            merged.iter().all(|gpu| gpu.nvml_index.is_some()),
            "both published devices must be the NVML ones"
        );
    }

    #[test]
    fn a_reversed_enumeration_order_still_matches_by_bus_address() {
        // DXGI hands adapters back in A, B; NVML in B, A. Order-based pairing
        // would swap the two cards' telemetry for the rest of the session.
        let a = PciAddress::new(0, 0x01, 0, 0);
        let b = PciAddress::new(0, 0x41, 0, 0);

        let generic = vec![
            generic_nvidia(a, "NVIDIA GeForce RTX 4090"),
            generic_nvidia(b, "NVIDIA GeForce RTX 4090"),
        ];

        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![
            FakeDevice::healthy("GPU-bbbb", "NVIDIA GeForce RTX 4090", Some(b)),
            FakeDevice::healthy("GPU-aaaa", "NVIDIA GeForce RTX 4090", Some(a)),
        ]))
        .expect("no error");

        let merged = merge(generic, vendor);

        assert_eq!(merged.len(), 2);
        // Each published device keeps the bus address it was discovered at,
        // so the two identical cards did not swap identities.
        for gpu in &merged {
            let expected = match gpu.descriptor.source_id.as_str() {
                "gpu:nvidia-aaaa" => a,
                "gpu:nvidia-bbbb" => b,
                other => panic!("unexpected source {other}"),
            };
            assert_eq!(gpu.descriptor.pci, Some(expected));
        }
    }

    #[test]
    fn a_single_unaddressable_adapter_takes_the_forced_one_to_one_pairing() {
        // One adapter, one device, no bus address: there is no other pairing
        // to make, so this one is not a guess.
        let mut only = generic_nvidia(PciAddress::new(0, 1, 0, 0), "NVIDIA GeForce RTX 4070");
        only.pci = None;

        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070 Laptop GPU",
            None,
        )]))
        .expect("no error");

        let merged = merge(vec![only], vendor);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].nvml_index, Some(0));
        assert_eq!(merged[0].descriptor.source_id.as_str(), "gpu:nvidia-aaaa");
    }

    #[test]
    fn an_adapter_the_backend_cannot_account_for_is_kept_rather_than_dropped() {
        // Two NVIDIA adapters, one NVML device: one of the two is a card NVML
        // does not serve, and nothing says which. Dropping one would hide real
        // hardware; pairing one would mislabel it. Both stay.
        let mut first = generic_nvidia(PciAddress::new(0, 1, 0, 0), "NVIDIA A");
        first.pci = None;
        let mut second = generic_nvidia(PciAddress::new(0, 2, 0, 0), "NVIDIA B");
        second.pci = None;

        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4090",
            None,
        )]))
        .expect("no error");

        let merged = merge(vec![first, second], vendor);

        assert_eq!(merged.len(), 3);
        assert_eq!(
            merged.iter().filter(|gpu| gpu.nvml_index.is_some()).count(),
            1
        );
    }

    #[test]
    fn a_bus_address_match_survives_a_second_unaddressable_card() {
        // The mixed case: one adapter PULSE could address, one it could not.
        let addressed = PciAddress::new(0, 0x01, 0, 0);
        let mut unaddressed = generic_nvidia(PciAddress::new(0, 0x41, 0, 0), "NVIDIA B");
        unaddressed.pci = None;

        let generic = vec![
            generic_nvidia(addressed, "NVIDIA GeForce RTX 4090"),
            unaddressed,
        ];

        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![
            FakeDevice::healthy("GPU-bbbb", "NVIDIA GeForce RTX 4090", None),
            FakeDevice::healthy("GPU-aaaa", "NVIDIA GeForce RTX 4090", Some(addressed)),
        ]))
        .expect("no error");

        let merged = merge(generic, vendor);

        // The addressed pair matched exactly; the remaining adapter and device
        // are then the only ones left, which is a forced pairing.
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(|gpu| gpu.nvml_index.is_some()));
    }

    #[test]
    fn a_non_nvidia_card_is_never_claimed_by_the_nvidia_backend() {
        let amd = PciAddress::new(0, 0x03, 0, 0);
        let nvidia = PciAddress::new(0, 0x01, 0, 0);
        let generic = vec![generic_amd(amd), generic_nvidia(nvidia, "NVIDIA GPU")];

        let vendor = nvml_descriptors(&FakeNvml::with_devices(vec![FakeDevice::healthy(
            "GPU-aaaa",
            "NVIDIA GeForce RTX 4070",
            Some(nvidia),
        )]))
        .expect("no error");

        let merged = merge(generic, vendor);

        assert_eq!(merged.len(), 2);
        let amd_entry = merged
            .iter()
            .find(|gpu| gpu.descriptor.vendor == GpuVendor::Amd)
            .expect("the AMD card survives");
        assert_eq!(amd_entry.nvml_index, None);
        assert_eq!(amd_entry.descriptor.backend, "amdgpu");
    }

    #[test]
    fn a_card_the_vendor_backend_does_not_serve_keeps_its_generic_entry() {
        // The nouveau case: an NVIDIA card with no NVML at all.
        let pci = PciAddress::new(0, 1, 0, 0);
        let generic = vec![generic_nvidia(pci, "NVIDIA GPU 10de:2820")];

        let merged = merge(generic, Vec::new());

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].nvml_index, None);
        assert_eq!(merged[0].descriptor.backend, "drm");
        // Present and identified, with nothing measurable — and not hidden.
        assert!(!merged[0].descriptor.has_telemetry());
    }

    #[test]
    fn an_nvml_device_the_inventory_missed_is_still_published() {
        let merged = merge(
            Vec::new(),
            nvml_descriptors(&FakeNvml::with_devices(vec![FakeDevice::healthy(
                "GPU-aaaa",
                "NVIDIA GeForce RTX 4070",
                None,
            )]))
            .expect("no error"),
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].descriptor.backend, "nvml");
    }

    #[test]
    fn the_merged_order_is_deterministic() {
        let a = PciAddress::new(0, 0x01, 0, 0);
        let b = PciAddress::new(0, 0x0B, 0, 0);

        let forward = merge(vec![generic_amd(b), generic_amd(a)], Vec::new());
        let backward = merge(vec![generic_amd(a), generic_amd(b)], Vec::new());

        let sources = |merged: &[InventoriedGpu]| -> Vec<String> {
            merged
                .iter()
                .map(|gpu| gpu.descriptor.source_id.as_str().to_string())
                .collect()
        };

        assert_eq!(sources(&forward), sources(&backward));
    }

    #[test]
    fn an_empty_machine_merges_to_nothing() {
        assert!(merge(Vec::new(), Vec::new()).is_empty());
    }
}
