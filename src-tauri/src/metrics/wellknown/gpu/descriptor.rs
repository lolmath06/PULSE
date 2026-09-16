//! What PULSE knows about one GPU, and — the hard part — how it identifies it.
//!
//! # Identity is the difficult problem, not telemetry
//!
//! Reading a utilisation percentage is easy. Deciding *which GPU it belongs to*,
//! in a way that still means the same thing after a reboot, a driver update or
//! a second identical card being installed, is not.
//!
//! Every obvious candidate is wrong:
//!
//! | Candidate | Why it must not be an identity |
//! |---|---|
//! | `GPU 0`, `GPU 1` | Presentation ordering, nothing more |
//! | `/sys/class/drm/card0` | A DRM minor number, assigned in probe order; `card0` and `card1` can swap between boots |
//! | NVML index | Explicitly documented as unstable across reboots, and it changes when a GPU is added, removed or reset |
//! | DXGI adapter index | Enumeration order, which depends on which adapter the OS currently prefers |
//! | `AdapterLuid` | Microsoft documents it as valid only until reboot |
//! | Product name | Two identical cards collapse into one identifier; a driver update that reworded the string invalidates every saved widget |
//!
//! A dashboard stores a `SourceId`. If that identifier shifts, the user's
//! carefully arranged GPU widgets silently start showing a different card — or
//! nothing. So PULSE derives identity from the most stable thing each platform
//! genuinely offers, and **records which one it used**, so the honest level of
//! stability is inspectable rather than assumed.

use std::fmt;

use crate::metrics::model::{Availability, SourceId};

/// Who made the GPU.
///
/// Used to pick a backend and to label the device; **never** part of its
/// identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    /// A PCI vendor ID PULSE has no special handling for.
    Other(u16),
}

impl GpuVendor {
    /// PCI vendor IDs, as they appear in `/sys/.../vendor` and in DXGI.
    pub const PCI_NVIDIA: u16 = 0x10DE;
    pub const PCI_AMD: u16 = 0x1002;
    /// AMD's older ATI identifier, still used by some parts.
    pub const PCI_ATI: u16 = 0x1022;
    pub const PCI_INTEL: u16 = 0x8086;

    pub const fn from_pci_id(vendor_id: u16) -> Self {
        match vendor_id {
            Self::PCI_NVIDIA => GpuVendor::Nvidia,
            Self::PCI_AMD | Self::PCI_ATI => GpuVendor::Amd,
            Self::PCI_INTEL => GpuVendor::Intel,
            other => GpuVendor::Other(other),
        }
    }

    /// The short, lowercase token used inside a `SourceId`.
    pub fn slug(self) -> String {
        match self {
            GpuVendor::Nvidia => "nvidia".to_string(),
            GpuVendor::Amd => "amd".to_string(),
            GpuVendor::Intel => "intel".to_string(),
            // Hex so it round-trips, and so two unknown vendors never collide.
            GpuVendor::Other(id) => format!("pci{id:04x}"),
        }
    }

    /// The human-readable vendor name, for display only.
    pub fn label(self) -> String {
        match self {
            GpuVendor::Nvidia => "NVIDIA".to_string(),
            GpuVendor::Amd => "AMD".to_string(),
            GpuVendor::Intel => "Intel".to_string(),
            GpuVendor::Other(id) => format!("PCI vendor {id:#06x}"),
        }
    }
}

/// A PCI bus address: `domain:bus:device.function`.
///
/// Kept as **correlation data**, and on Linux also as an identity of last
/// resort. Correlating on it is what lets PULSE recognise that the card NVML
/// calls index 0 and the one DRM calls `card1` are the same physical device,
/// instead of guessing from product names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PciAddress {
    pub domain: u32,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciAddress {
    pub const fn new(domain: u32, bus: u8, device: u8, function: u8) -> Self {
        Self {
            domain,
            bus,
            device,
            function,
        }
    }

    /// Parses the canonical `0000:01:00.0` form used by Linux sysfs and NVML.
    ///
    /// Rejects anything that is not exactly that shape rather than salvaging
    /// part of it: a half-parsed bus address would correlate two different
    /// cards onto one identity.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let (domain, rest) = value.split_once(':')?;
        let (bus, rest) = rest.split_once(':')?;
        let (device, function) = rest.split_once('.')?;

        Some(Self {
            domain: u32::from_str_radix(domain, 16).ok()?,
            bus: u8::from_str_radix(bus, 16).ok()?,
            device: u8::from_str_radix(device, 16).ok()?,
            function: u8::from_str_radix(function, 16).ok()?,
        })
    }

    /// The canonical textual form, `0000:01:00.0`.
    pub fn to_bdf(self) -> String {
        format!(
            "{:04x}:{:02x}:{:02x}.{:x}",
            self.domain, self.bus, self.device, self.function
        )
    }

    /// The same address as a `SourceId` instance, which forbids `:` and `.`.
    pub fn to_source_instance(self) -> String {
        format!(
            "pci-{:04x}-{:02x}-{:02x}-{:x}",
            self.domain, self.bus, self.device, self.function
        )
    }
}

impl fmt::Display for PciAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_bdf())
    }
}

/// How stable the identity PULSE derived actually is.
///
/// Recorded rather than assumed, because the honest answer differs by platform
/// and by vendor, and a user deserves to know when their saved GPU widget is
/// resting on something weaker than a hardware serial number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityStability {
    /// Derived from something the hardware itself carries. Survives reboots,
    /// driver updates and reordering, and distinguishes two identical cards.
    Hardware,
    /// Derived from the slot the device sits in. Survives reboots and driver
    /// updates, and distinguishes identical cards — but changes if the card is
    /// physically moved to another slot.
    Slot,
    /// Derived from what the device *is* rather than which one it is. Survives
    /// reboots, but **two identical cards need a disambiguator**, and that
    /// disambiguator is only meaningful within one session.
    ModelWithSessionDisambiguator,
}

impl IdentityStability {
    /// Whether this identity is expected to mean the same thing after a
    /// reboot.
    pub const fn survives_reboot(self) -> bool {
        matches!(self, IdentityStability::Hardware | IdentityStability::Slot)
    }

    /// A short explanation, surfaced in documentation and diagnostics.
    pub const fn explanation(self) -> &'static str {
        match self {
            IdentityStability::Hardware => {
                "derived from a hardware identifier carried by the device itself"
            }
            IdentityStability::Slot => {
                "derived from the PCI address of the slot the device occupies"
            }
            IdentityStability::ModelWithSessionDisambiguator => {
                "derived from the device model; identical cards are told apart only \
                 within this session"
            }
        }
    }
}

/// Where a GPU's `SourceId` came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuIdentity {
    /// NVML's device UUID — globally unique, burned into the board, and
    /// immutable. The best identity available for any GPU on any platform.
    NvmlUuid(String),
    /// The device's PCI bus address. Used on Linux for everything NVML does
    /// not claim.
    Pci(PciAddress),
    /// The `vendor:device:subsystem:revision` tuple, optionally with an
    /// enumeration index appended when two adapters share it.
    ///
    /// The Windows fallback. See [`IdentityStability`].
    DeviceModel { disambiguated: bool },
}

impl GpuIdentity {
    pub const fn stability(&self) -> IdentityStability {
        match self {
            GpuIdentity::NvmlUuid(_) => IdentityStability::Hardware,
            GpuIdentity::Pci(_) => IdentityStability::Slot,
            GpuIdentity::DeviceModel {
                disambiguated: false,
            } => IdentityStability::Slot,
            GpuIdentity::DeviceModel {
                disambiguated: true,
            } => IdentityStability::ModelWithSessionDisambiguator,
        }
    }

    /// A short machine-readable tag, for diagnostics and the report.
    pub const fn mechanism(&self) -> &'static str {
        match self {
            GpuIdentity::NvmlUuid(_) => "nvml-uuid",
            GpuIdentity::Pci(_) => "pci-address",
            GpuIdentity::DeviceModel { .. } => "device-model",
        }
    }
}

/// Normalises an NVML UUID into a `SourceId` instance fragment.
///
/// NVML reports `GPU-d6ca47e3-2c1e-9f3b-7a10-8e4f2b1c9d55`. A `SourceId`
/// instance allows lowercase letters, digits, `-`, `_` and `.`, so the string
/// needs lowercasing and the redundant `gpu-` prefix removing — the source
/// kind is already `gpu`.
///
/// Returns `None` when nothing usable survives, so a driver returning an empty
/// or garbage UUID falls back to another identity rather than producing
/// `gpu:nvidia-`.
pub fn normalize_nvml_uuid(uuid: &str) -> Option<String> {
    let lowered = uuid.trim().to_ascii_lowercase();

    // Anything outside the allowed set becomes `-`, so an unexpected format
    // still yields a deterministic, valid identifier rather than being
    // rejected outright.
    let mut cleaned: String = lowered
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // `GPU-xxxx` → `xxxx`: the kind already says it is a GPU.
    if let Some(rest) = cleaned.strip_prefix("gpu-") {
        cleaned = rest.to_string();
    }

    let trimmed = cleaned.trim_matches('-');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Builds the `SourceId` for an NVIDIA GPU identified by its NVML UUID.
pub fn nvml_source_id(uuid: &str) -> Option<SourceId> {
    let normalized = normalize_nvml_uuid(uuid)?;

    SourceId::new(format!("gpu:nvidia-{normalized}")).ok()
}

/// Builds the `SourceId` for a GPU identified by its PCI address.
pub fn pci_source_id(address: PciAddress) -> Option<SourceId> {
    SourceId::new(format!("gpu:{}", address.to_source_instance())).ok()
}

/// Builds the `SourceId` for a GPU identified only by its model.
///
/// `index` is `Some` only when another adapter shares the same tuple; see
/// [`IdentityStability::ModelWithSessionDisambiguator`].
pub fn device_model_source_id(
    vendor: GpuVendor,
    device_id: u16,
    subsystem_id: u32,
    revision: u8,
    index: Option<u32>,
) -> Option<SourceId> {
    let mut instance = format!(
        "{}-{device_id:04x}-{subsystem_id:08x}-{revision:02x}",
        vendor.slug()
    );

    if let Some(index) = index {
        instance.push_str(&format!("-n{index}"));
    }

    SourceId::new(format!("gpu:{instance}")).ok()
}

/// Whether each of the seven per-GPU metrics can be sampled on this device.
///
/// Carried **per metric**, never per device, because vendors differ in exactly
/// this way: an NVIDIA card on a driver that reports no memory clock is still
/// a perfectly useful GPU, and a card whose VRAM is unreadable still reports
/// its utilisation. Collapsing these into one flag is what would make PULSE
/// hide a whole GPU over one missing interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuCapabilities {
    pub usage_core: Availability,
    pub memory_total: Availability,
    pub memory_used: Availability,
    pub memory_free: Availability,
    pub memory_usage_percent: Availability,
    pub frequency_core: Availability,
    pub frequency_memory: Availability,
}

impl GpuCapabilities {
    /// Everything readable.
    pub fn all_available() -> Self {
        Self {
            usage_core: Availability::Available,
            memory_total: Availability::Available,
            memory_used: Availability::Available,
            memory_free: Availability::Available,
            memory_usage_percent: Availability::Available,
            frequency_core: Availability::Available,
            frequency_memory: Availability::Available,
        }
    }

    /// Nothing readable, for the same stated reason.
    ///
    /// The shape of "this GPU is present and correctly identified, but PULSE
    /// has no telemetry backend for it" — a recognised card with an open-source
    /// driver that exposes no counters, for instance. The device still appears,
    /// still counts towards `gpu.count`, and still explains itself.
    pub fn none_available(reason: &Availability) -> Self {
        Self {
            usage_core: reason.clone(),
            memory_total: reason.clone(),
            memory_used: reason.clone(),
            memory_free: reason.clone(),
            memory_usage_percent: reason.clone(),
            frequency_core: reason.clone(),
            frequency_memory: reason.clone(),
        }
    }

    /// Applies one availability to all four memory metrics at once.
    pub fn with_memory(mut self, availability: &Availability) -> Self {
        self.memory_total = availability.clone();
        self.memory_used = availability.clone();
        self.memory_free = availability.clone();
        self.memory_usage_percent = availability.clone();
        self
    }

    /// Applies one availability to both clock metrics at once.
    pub fn with_frequencies(mut self, availability: &Availability) -> Self {
        self.frequency_core = availability.clone();
        self.frequency_memory = availability.clone();
        self
    }

    /// How many of the seven are currently sampleable.
    pub fn available_count(&self) -> usize {
        [
            &self.usage_core,
            &self.memory_total,
            &self.memory_used,
            &self.memory_free,
            &self.memory_usage_percent,
            &self.frequency_core,
            &self.frequency_memory,
        ]
        .iter()
        .filter(|availability| availability.is_available())
        .count()
    }
}

/// One GPU, as PULSE knows it before any value is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuDescriptor {
    /// Stable identity. The only part a saved dashboard may store.
    pub source_id: SourceId,
    /// Human-readable name, e.g. `NVIDIA GeForce RTX 4070 Laptop GPU`.
    /// **Presentation only** — never an identifier.
    pub display_name: String,
    pub vendor: GpuVendor,
    /// Which mechanism produced `source_id`.
    pub identity: GpuIdentity,
    /// PCI address, when known. Correlation data, and the reason PULSE can
    /// match an NVML device to a DRM card without comparing product names.
    pub pci: Option<PciAddress>,
    /// Which backend supplies this device's telemetry, e.g. `nvml`, `amdgpu`,
    /// `drm`. Diagnostics only.
    pub backend: &'static str,
    pub capabilities: GpuCapabilities,
}

impl GpuDescriptor {
    /// Whether any telemetry at all can be read from this device.
    pub fn has_telemetry(&self) -> bool {
        self.capabilities.available_count() > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_pci_vendor_ids_pulse_recognises() {
        assert_eq!(GpuVendor::from_pci_id(0x10DE), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_pci_id(0x1002), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_pci_id(0x8086), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_pci_id(0x1234), GpuVendor::Other(0x1234));
    }

    #[test]
    fn unknown_vendors_never_collide_in_a_source_id() {
        assert_eq!(GpuVendor::Other(0x1234).slug(), "pci1234");
        assert_ne!(
            GpuVendor::Other(0x1234).slug(),
            GpuVendor::Other(0x5678).slug()
        );
    }

    // --- PCI addresses ----------------------------------------------------

    #[test]
    fn parses_the_canonical_bus_address_form() {
        let address = PciAddress::parse("0000:01:00.0").expect("valid");

        assert_eq!(address, PciAddress::new(0, 1, 0, 0));
        assert_eq!(address.to_bdf(), "0000:01:00.0");
    }

    #[test]
    fn parses_hexadecimal_components() {
        let address = PciAddress::parse("0001:ff:1f.7").expect("valid");

        assert_eq!(address, PciAddress::new(1, 0xFF, 0x1F, 7));
        assert_eq!(address.to_bdf(), "0001:ff:1f.7");
    }

    #[test]
    fn accepts_the_uppercase_form_nvml_reports() {
        // NVML's busId is uppercase; sysfs is lowercase. Both must parse to
        // the same address, or correlation between them silently fails.
        let from_nvml = PciAddress::parse("0000:01:00.0").expect("valid");
        let from_sysfs = PciAddress::parse("0000:01:00.0").expect("valid");
        assert_eq!(from_nvml, from_sysfs);

        assert_eq!(
            PciAddress::parse("0000:0A:00.0"),
            PciAddress::parse("0000:0a:00.0")
        );
    }

    #[test]
    fn rejects_anything_that_is_not_a_full_bus_address() {
        // A half-parsed address would correlate two different cards onto one
        // identity, which is worse than not correlating at all.
        for malformed in [
            "",
            "0000:01:00",
            "01:00.0",
            "0000-01-00-0",
            "zzzz:01:00.0",
            "0000:01:00.",
            "0000::00.0",
            "not an address",
        ] {
            assert_eq!(
                PciAddress::parse(malformed),
                None,
                "'{malformed}' must be rejected"
            );
        }
    }

    #[test]
    fn rejects_components_that_do_not_fit_their_width() {
        assert_eq!(PciAddress::parse("0000:100:00.0"), None, "bus above 8 bits");
    }

    #[test]
    fn builds_a_valid_source_instance() {
        let address = PciAddress::new(0, 1, 0, 0);
        assert_eq!(address.to_source_instance(), "pci-0000-01-00-0");

        let source = pci_source_id(address).expect("valid");
        assert_eq!(source.as_str(), "gpu:pci-0000-01-00-0");
        assert_eq!(source.kind(), "gpu");
    }

    // --- NVML UUIDs -------------------------------------------------------

    #[test]
    fn normalises_a_real_nvml_uuid() {
        let normalized =
            normalize_nvml_uuid("GPU-d6ca47e3-2c1e-9f3b-7a10-8e4f2b1c9d55").expect("usable");

        // Lowercased, and the redundant `gpu-` prefix dropped.
        assert_eq!(normalized, "d6ca47e3-2c1e-9f3b-7a10-8e4f2b1c9d55");
    }

    #[test]
    fn a_uuid_becomes_a_valid_source_id() {
        let source = nvml_source_id("GPU-d6ca47e3-2c1e-9f3b-7a10-8e4f2b1c9d55").expect("valid");

        assert_eq!(
            source.as_str(),
            "gpu:nvidia-d6ca47e3-2c1e-9f3b-7a10-8e4f2b1c9d55"
        );
        assert!(source.has_canonical_kind());
        // Round-trips through validation, which is what the engine will do.
        assert!(SourceId::new(source.as_str().to_string()).is_ok());
    }

    #[test]
    fn normalisation_is_deterministic_and_injective_for_distinct_uuids() {
        let first = nvml_source_id("GPU-aaaaaaaa-1111-2222-3333-444444444444").expect("valid");
        let second = nvml_source_id("GPU-bbbbbbbb-1111-2222-3333-444444444444").expect("valid");

        assert_ne!(first, second);
        assert_eq!(
            first,
            nvml_source_id("GPU-aaaaaaaa-1111-2222-3333-444444444444").expect("valid")
        );
    }

    #[test]
    fn an_unexpected_uuid_shape_still_yields_a_valid_identifier() {
        // MIG and future device types use other prefixes; the result must
        // still be a legal SourceId rather than a rejected one.
        let source = nvml_source_id("MIG-GPU-1234abcd/1/0").expect("valid");
        assert!(SourceId::new(source.as_str().to_string()).is_ok());
    }

    #[test]
    fn an_empty_or_useless_uuid_is_refused_rather_than_producing_a_bare_prefix() {
        // `gpu:nvidia-` would be both invalid and a collision magnet.
        for useless in ["", "   ", "GPU-", "---", "GPU-\u{0}"] {
            assert_eq!(
                nvml_source_id(useless),
                None,
                "'{useless}' must not yield an identity"
            );
        }
    }

    // --- device-model identity -------------------------------------------

    #[test]
    fn builds_a_device_model_identity() {
        let source =
            device_model_source_id(GpuVendor::Amd, 0x73FF, 0x1002_0E3B, 0xC1, None).expect("valid");

        assert_eq!(source.as_str(), "gpu:amd-73ff-10020e3b-c1");
    }

    #[test]
    fn two_identical_cards_are_disambiguated_rather_than_collapsed() {
        let first =
            device_model_source_id(GpuVendor::Amd, 0x73FF, 0x1002_0E3B, 0xC1, Some(0)).unwrap();
        let second =
            device_model_source_id(GpuVendor::Amd, 0x73FF, 0x1002_0E3B, 0xC1, Some(1)).unwrap();

        assert_ne!(first, second, "identical models must not share an identity");
    }

    // --- stability is recorded, not assumed -------------------------------

    #[test]
    fn each_identity_mechanism_declares_its_own_stability() {
        assert_eq!(
            GpuIdentity::NvmlUuid("x".into()).stability(),
            IdentityStability::Hardware
        );
        assert_eq!(
            GpuIdentity::Pci(PciAddress::new(0, 1, 0, 0)).stability(),
            IdentityStability::Slot
        );
        assert_eq!(
            GpuIdentity::DeviceModel {
                disambiguated: true
            }
            .stability(),
            IdentityStability::ModelWithSessionDisambiguator
        );
    }

    #[test]
    fn only_the_weakest_identity_fails_to_survive_a_reboot() {
        assert!(IdentityStability::Hardware.survives_reboot());
        assert!(IdentityStability::Slot.survives_reboot());
        assert!(!IdentityStability::ModelWithSessionDisambiguator.survives_reboot());
    }

    // --- capabilities -----------------------------------------------------

    #[test]
    fn capabilities_are_tracked_per_metric() {
        let capabilities = GpuCapabilities::all_available()
            .with_frequencies(&Availability::unsupported("no clock interface"));

        assert!(capabilities.usage_core.is_available());
        assert!(capabilities.memory_total.is_available());
        assert!(!capabilities.frequency_core.is_available());
        assert!(!capabilities.frequency_memory.is_available());
        assert_eq!(capabilities.available_count(), 5);
    }

    #[test]
    fn a_gpu_with_no_backend_still_describes_itself() {
        // The nouveau case: the card is real and correctly identified, but
        // nothing can be measured. It must not vanish.
        let reason = Availability::unsupported("the open-source driver exposes no counters");
        let capabilities = GpuCapabilities::none_available(&reason);

        assert_eq!(capabilities.available_count(), 0);

        let descriptor = GpuDescriptor {
            source_id: pci_source_id(PciAddress::new(0, 1, 0, 0)).expect("valid"),
            display_name: "NVIDIA GeForce RTX 4070".to_string(),
            vendor: GpuVendor::Nvidia,
            identity: GpuIdentity::Pci(PciAddress::new(0, 1, 0, 0)),
            pci: Some(PciAddress::new(0, 1, 0, 0)),
            backend: "drm",
            capabilities,
        };

        assert!(!descriptor.has_telemetry());
        assert_eq!(descriptor.source_id.as_str(), "gpu:pci-0000-01-00-0");
    }

    #[test]
    fn the_product_name_is_never_part_of_the_identity() {
        // Two identical cards in different slots: same name, different source.
        let build = |address: PciAddress| GpuDescriptor {
            source_id: pci_source_id(address).expect("valid"),
            display_name: "AMD Radeon RX 6800 XT".to_string(),
            vendor: GpuVendor::Amd,
            identity: GpuIdentity::Pci(address),
            pci: Some(address),
            backend: "amdgpu",
            capabilities: GpuCapabilities::all_available(),
        };

        let first = build(PciAddress::new(0, 0x01, 0, 0));
        let second = build(PciAddress::new(0, 0x0B, 0, 0));

        assert_eq!(first.display_name, second.display_name);
        assert_ne!(first.source_id, second.source_id);
    }
}
