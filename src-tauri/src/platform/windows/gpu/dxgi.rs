//! Generic GPU inventory on Windows, from DXGI.
//!
//! # Why DXGI
//!
//! `CreateDXGIFactory1` + `IDXGIFactory1::EnumAdapters1` is the standard way to
//! enumerate graphics adapters on Windows. It needs no privileges, no service,
//! no driver-specific SDK, and it is the same enumeration every Direct3D
//! application performs. `DXGI_ADAPTER_DESC1` carries everything the generic
//! inventory needs: the description, the PCI vendor and device IDs, the
//! subsystem and revision, and the dedicated video memory capacity.
//!
//! `DXCore` was considered. It is newer and exposes richer adapter properties,
//! but it targets compute-capable adapter enumeration and is unavailable on
//! older supported Windows versions, and nothing this phase needs is missing
//! from DXGI.
//!
//! # Software adapters are not GPUs
//!
//! Windows always presents at least one software adapter — *Microsoft Basic
//! Render Driver* (WARP) — and on a machine with no display driver installed it
//! may be the only one. Counting it would tell a user with no graphics card
//! that they have one.
//!
//! PULSE excludes an adapter when **either** signal says software:
//! `DXGI_ADAPTER_FLAG_SOFTWARE`, or the reserved Microsoft vendor/device pair
//! `0x1414:0x008C`. Older drivers do not always set the flag, so the pair is
//! checked too.
//!
//! # Identity, and its honest limits
//!
//! DXGI gives no PCI bus address and no Plug-and-Play device instance ID. What
//! it does give:
//!
//! - `AdapterLuid` — which Microsoft documents as valid **only until the system
//!   restarts**. PULSE therefore does *not* use it as a stored identity, only
//!   to correlate objects within one session.
//! - `VendorId`, `DeviceId`, `SubSysId`, `Revision` — which describe *what the
//!   adapter is* rather than *which one it is*. Stable across reboots, but
//!   identical for two identical cards.
//!
//! So a Windows adapter is identified by that tuple, and when two adapters
//! share it a session-scoped index is appended to keep their references
//! distinct. That is recorded as
//! [`IdentityStability::ModelWithSessionDisambiguator`], and it is the honest
//! ceiling of what DXGI alone can promise — PULSE does not claim otherwise.
//!
//! **NVIDIA adapters are not affected**: NVML supplies a hardware UUID for
//! them, and the merge in `platform::gpu` replaces this identity with it.
//!
//! [`IdentityStability::ModelWithSessionDisambiguator`]:
//!     crate::metrics::wellknown::gpu::IdentityStability::ModelWithSessionDisambiguator

use crate::metrics::model::Availability;
use crate::metrics::wellknown::gpu::{
    device_model_source_id, GpuCapabilities, GpuDescriptor, GpuIdentity, GpuVendor,
};

/// Microsoft's reserved vendor ID, used by its software adapters.
pub const MICROSOFT_VENDOR_ID: u16 = 0x1414;
/// *Microsoft Basic Render Driver*'s device ID.
pub const BASIC_RENDER_DEVICE_ID: u16 = 0x008C;

/// One adapter as DXGI described it, converted out of the COM struct.
///
/// Plain data, so every decision below is testable on Fedora without COM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxgiAdapter {
    /// Enumeration index. **Ordering, not identity** — it reflects which
    /// adapter Windows currently prefers and may differ between boots.
    pub index: u32,
    pub description: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_id: u32,
    pub revision: u8,
    /// `DedicatedVideoMemory`, in bytes. Zero for adapters with no dedicated
    /// pool, which is how an integrated GPU presents itself.
    pub dedicated_video_memory: u64,
    /// Whether DXGI flagged this adapter as a software renderer.
    pub software_flag: bool,
}

impl DxgiAdapter {
    pub fn vendor(&self) -> GpuVendor {
        GpuVendor::from_pci_id(self.vendor_id)
    }

    /// Whether this is a software renderer rather than a physical GPU.
    ///
    /// Two independent signals, because older drivers do not always set the
    /// flag and PULSE must not report WARP as a graphics card either way.
    pub fn is_software(&self) -> bool {
        self.software_flag
            || (self.vendor_id == MICROSOFT_VENDOR_ID && self.device_id == BASIC_RENDER_DEVICE_ID)
    }

    /// Whether this adapter has a dedicated video memory pool.
    ///
    /// An integrated GPU reports zero here, and PULSE will not turn the
    /// adjacent `SharedSystemMemory` figure into a pretend VRAM total — that
    /// number is an addressing limit on system RAM, not video memory in use.
    pub fn has_dedicated_video_memory(&self) -> bool {
        self.dedicated_video_memory > 0
    }
}

/// Keeps only the adapters that are real graphics hardware.
pub fn hardware_adapters(adapters: Vec<DxgiAdapter>) -> Vec<DxgiAdapter> {
    adapters
        .into_iter()
        .filter(|adapter| !adapter.is_software())
        .collect()
}

/// Builds descriptors for every hardware adapter.
///
/// Adapters sharing a model tuple get a session-scoped index appended, so two
/// identical cards never collapse onto one reference. The disambiguation is
/// recorded in the identity, so the weaker guarantee is inspectable rather
/// than silent.
pub fn describe_all(adapters: &[DxgiAdapter]) -> Vec<GpuDescriptor> {
    let mut described = Vec::with_capacity(adapters.len());

    for adapter in adapters {
        // Only disambiguate when something actually collides — a single card
        // should not carry a session-scoped suffix it does not need.
        let duplicated = adapters
            .iter()
            .filter(|candidate| {
                candidate.vendor_id == adapter.vendor_id
                    && candidate.device_id == adapter.device_id
                    && candidate.subsystem_id == adapter.subsystem_id
                    && candidate.revision == adapter.revision
            })
            .count()
            > 1;

        let index = duplicated.then_some(adapter.index);

        let Some(source_id) = device_model_source_id(
            adapter.vendor(),
            adapter.device_id,
            adapter.subsystem_id,
            adapter.revision,
            index,
        ) else {
            continue;
        };

        described.push(GpuDescriptor {
            source_id,
            display_name: adapter.description.clone(),
            vendor: adapter.vendor(),
            identity: GpuIdentity::DeviceModel {
                disambiguated: duplicated,
            },
            // DXGI exposes no bus address, which is why the merge falls back
            // to vendor-and-order matching on this platform.
            pci: None,
            backend: "dxgi",
            capabilities: capabilities_for(adapter),
        });
    }

    described
}

/// What a DXGI-only adapter can publish.
///
/// Deliberately almost nothing. DXGI reports a **capacity**, not live
/// telemetry:
///
/// - `gpu.memory.total` is publishable when the adapter has a genuine
///   dedicated pool, because `DedicatedVideoMemory` is exactly that capacity.
/// - `gpu.memory.used` and `gpu.memory.free` are **not**.
///   `IDXGIAdapter3::QueryVideoMemoryInfo` looks like the answer and is not:
///   its `CurrentUsage` is the video memory attributed to *the querying
///   process*, so PULSE would be reporting its own consumption and labelling
///   it system-wide VRAM usage. On an idle machine it would read near zero
///   while a game filled the card. Showing `—` is better than showing a number
///   that is confidently wrong.
/// - `gpu.usage.core` and the clocks have no unprivileged system-wide source
///   in DXGI at all.
///
/// A vendor backend replaces all of this when one serves the adapter.
fn capabilities_for(adapter: &DxgiAdapter) -> GpuCapabilities {
    let no_source = Availability::unsupported(
        "PULSE has no vendor telemetry backend for this adapter; the graphics API \
         exposes no system-wide figure for it",
    );

    let mut capabilities = GpuCapabilities::none_available(&no_source);

    if adapter.has_dedicated_video_memory() {
        capabilities.memory_total = Availability::Available;
    } else {
        capabilities.memory_total = Availability::not_detected(
            "this adapter reports no dedicated video memory; system memory shared with \
             an integrated GPU is not reported as VRAM",
        );
    }

    capabilities
}

/// The parts that call into DXGI.
#[cfg(target_os = "windows")]
pub mod imp {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG, DXGI_ADAPTER_FLAG_SOFTWARE,
        DXGI_ERROR_NOT_FOUND,
    };

    use crate::metrics::model::{MetricError, MetricErrorCode};

    use super::DxgiAdapter;

    /// Enumerates every adapter DXGI reports.
    ///
    /// # Safety
    ///
    /// The three `unsafe` calls are COM invocations whose lifetimes the
    /// `windows` crate manages: `CreateDXGIFactory1` returns a reference-counted
    /// factory, `EnumAdapters1` a reference-counted adapter, and `GetDesc1`
    /// fills a caller-owned `DXGI_ADAPTER_DESC1` by value. No raw pointer is
    /// constructed, retained or freed by PULSE, and every call's `Result` is
    /// checked before its output is read.
    pub fn enumerate() -> Result<Vec<DxgiAdapter>, MetricError> {
        // SAFETY: see the function docs — a COM object construction whose
        // result is checked before use.
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(|error| {
            MetricError::new(
                MetricErrorCode::ProviderUnavailable,
                format!("DXGI could not be initialised: {error}"),
            )
        })?;

        let mut adapters = Vec::new();

        for index in 0..u32::MAX {
            // SAFETY: an ordinary COM method call; `DXGI_ERROR_NOT_FOUND` is
            // the documented way this enumeration ends.
            let adapter = match unsafe { factory.EnumAdapters1(index) } {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(_) => break,
            };

            // SAFETY: fills a caller-owned struct by value; checked before use.
            let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
                continue;
            };

            let description = String::from_utf16_lossy(
                &desc.Description[..desc
                    .Description
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(desc.Description.len())],
            );

            adapters.push(DxgiAdapter {
                index,
                description: description.trim().to_string(),
                vendor_id: (desc.VendorId & 0xFFFF) as u16,
                device_id: (desc.DeviceId & 0xFFFF) as u16,
                subsystem_id: desc.SubSysId,
                revision: (desc.Revision & 0xFF) as u8,
                dedicated_video_memory: desc.DedicatedVideoMemory as u64,
                software_flag: DXGI_ADAPTER_FLAG(desc.Flags as i32) == DXGI_ADAPTER_FLAG_SOFTWARE,
            });
        }

        Ok(adapters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn adapter(index: u32, vendor: u16, device: u16, description: &str) -> DxgiAdapter {
        DxgiAdapter {
            index,
            description: description.to_string(),
            vendor_id: vendor,
            device_id: device,
            subsystem_id: 0x1234_5678,
            revision: 0xA1,
            dedicated_video_memory: 8 * GIB,
            software_flag: false,
        }
    }

    fn warp() -> DxgiAdapter {
        DxgiAdapter {
            index: 1,
            description: "Microsoft Basic Render Driver".to_string(),
            vendor_id: MICROSOFT_VENDOR_ID,
            device_id: BASIC_RENDER_DEVICE_ID,
            subsystem_id: 0,
            revision: 0,
            dedicated_video_memory: 0,
            software_flag: true,
        }
    }

    // --- hardware versus software ----------------------------------------

    #[test]
    fn a_software_adapter_is_not_a_gpu() {
        assert!(warp().is_software());
        assert!(!adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070").is_software());
    }

    #[test]
    fn a_software_adapter_is_recognised_even_without_the_flag() {
        // Older drivers do not always set it; the reserved vendor/device pair
        // still gives it away.
        let mut basic = warp();
        basic.software_flag = false;

        assert!(
            basic.is_software(),
            "WARP must never be counted as hardware"
        );
    }

    #[test]
    fn software_adapters_are_filtered_out_of_the_inventory() {
        let adapters = vec![
            adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070"),
            warp(),
        ];

        let hardware = hardware_adapters(adapters);

        assert_eq!(hardware.len(), 1);
        assert_eq!(hardware[0].vendor(), GpuVendor::Nvidia);
    }

    #[test]
    fn a_machine_with_only_a_software_adapter_reports_no_gpu() {
        // Correct: no display driver installed means no graphics card PULSE
        // can speak about, and saying "1 GPU" would be a lie.
        assert!(hardware_adapters(vec![warp()]).is_empty());
    }

    // --- describing -------------------------------------------------------

    #[test]
    fn describes_a_single_adapter_without_a_session_suffix() {
        let adapters = vec![adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070")];
        let described = describe_all(&adapters);

        assert_eq!(described.len(), 1);
        assert_eq!(described[0].display_name, "NVIDIA GeForce RTX 4070");
        assert_eq!(described[0].vendor, GpuVendor::Nvidia);
        assert_eq!(
            described[0].identity,
            GpuIdentity::DeviceModel {
                disambiguated: false
            }
        );
        // A lone card gets the stronger guarantee.
        assert!(described[0].identity.stability().survives_reboot());
        assert!(!described[0].source_id.as_str().contains("-n0"));
    }

    #[test]
    fn two_different_models_need_no_disambiguation() {
        let adapters = vec![
            adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070"),
            adapter(1, 0x1002, 0x73FF, "AMD Radeon RX 6600"),
        ];

        let described = describe_all(&adapters);

        assert_eq!(described.len(), 2);
        assert!(described.iter().all(|gpu| gpu.identity
            == GpuIdentity::DeviceModel {
                disambiguated: false
            }));
        assert_ne!(described[0].source_id, described[1].source_id);
    }

    #[test]
    fn two_identical_adapters_are_disambiguated_rather_than_collapsed() {
        // The case DXGI cannot tell apart on its own. Producing one reference
        // for two cards would be worse than a weaker identity.
        let adapters = vec![
            adapter(0, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090"),
            adapter(1, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090"),
        ];

        let described = describe_all(&adapters);

        assert_eq!(described.len(), 2);
        assert_ne!(described[0].source_id, described[1].source_id);
        assert!(described.iter().all(|gpu| gpu.identity
            == GpuIdentity::DeviceModel {
                disambiguated: true
            }));
        // And the weaker guarantee is recorded, not hidden.
        assert!(!described[0].identity.stability().survives_reboot());
    }

    #[test]
    fn the_description_is_never_part_of_the_identity() {
        let mut first = adapter(0, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090");
        let mut second = adapter(1, 0x10DE, 0x2684, "NVIDIA GeForce RTX 4090");
        // A driver update reworded one of them.
        first.description = "NVIDIA GeForce RTX 4090".to_string();
        second.description = "NVIDIA GeForce RTX 4090 (rev 2)".to_string();

        let described = describe_all(&[first, second]);

        // Still two distinct references, still both disambiguated: the names
        // played no part.
        assert_eq!(described.len(), 2);
        assert_ne!(described[0].source_id, described[1].source_id);
    }

    #[test]
    fn the_identity_is_deterministic_across_runs() {
        let adapters = vec![adapter(0, 0x1002, 0x73FF, "AMD Radeon RX 6600")];

        assert_eq!(
            describe_all(&adapters)[0].source_id,
            describe_all(&adapters)[0].source_id
        );
        assert_eq!(
            describe_all(&adapters)[0].source_id.as_str(),
            "gpu:amd-73ff-12345678-a1"
        );
    }

    // --- what DXGI may and may not publish --------------------------------

    #[test]
    fn dedicated_video_memory_is_published_as_a_capacity() {
        let described = describe_all(&[adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070")]);

        assert!(
            described[0].capabilities.memory_total.is_available(),
            "DedicatedVideoMemory really is the installed VRAM capacity"
        );
    }

    #[test]
    fn process_scoped_memory_usage_is_never_published_as_system_vram() {
        // The trap this phase calls out by name: QueryVideoMemoryInfo reports
        // the querying process's own usage. Publishing it as system-wide VRAM
        // usage would read near zero while a game filled the card.
        let described = describe_all(&[adapter(0, 0x10DE, 0x2820, "NVIDIA GeForce RTX 4070")]);
        let capabilities = &described[0].capabilities;

        assert!(!capabilities.memory_used.is_available());
        assert!(!capabilities.memory_free.is_available());
        assert!(!capabilities.memory_usage_percent.is_available());
    }

    #[test]
    fn dxgi_alone_publishes_no_utilisation_or_clocks() {
        let described = describe_all(&[adapter(0, 0x1002, 0x73FF, "AMD Radeon RX 6600")]);
        let capabilities = &described[0].capabilities;

        assert!(!capabilities.usage_core.is_available());
        assert!(!capabilities.frequency_core.is_available());
        assert!(!capabilities.frequency_memory.is_available());
        assert_eq!(capabilities.available_count(), 1, "only the VRAM capacity");
    }

    #[test]
    fn an_integrated_adapter_reports_no_pretend_vram() {
        // Shared system memory is an addressing limit, not video memory.
        let mut integrated = adapter(0, 0x8086, 0xA788, "Intel Iris Xe Graphics");
        integrated.dedicated_video_memory = 0;

        let described = describe_all(&[integrated]);

        assert!(!described[0].has_telemetry());
        assert_eq!(
            described[0].capabilities.memory_total.status_str(),
            "notDetected"
        );
    }

    #[test]
    fn every_described_adapter_keeps_all_seven_definitions_available_to_the_catalog() {
        // Capabilities say "unavailable"; the metrics themselves still exist.
        use crate::metrics::model::ProviderId;
        use crate::metrics::wellknown::gpu;

        let described = describe_all(&[adapter(0, 0x8086, 0xA788, "Intel Iris Xe Graphics")]);
        let provider = ProviderId::new("windows.gpu").expect("valid");

        assert_eq!(gpu::definitions(&provider, &described).len(), 1 + 7);
    }
}
