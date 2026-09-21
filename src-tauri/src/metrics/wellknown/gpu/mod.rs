//! The `gpu.*` metrics: declarations shared by every platform and vendor.
//!
//! This module owns every GPU metric key, unit, kind and user-facing string
//! PULSE ships. Platform backends supply **only raw numbers and a descriptor**;
//! they never choose a key or a unit, which is what keeps
//! `gpu.usage.core@gpu:nvidia-…` meaning the same thing whether it came from
//! NVML on Fedora, NVML on Windows, or AMDGPU sysfs.
//!
//! # The catalog is sized by the machine
//!
//! For `G` discovered GPUs:
//!
//! ```text
//! 1    gpu.count@gpu:system
//! 11G  eleven metrics per GPU
//! ```
//!
//! Nothing hardcodes `G`. A laptop with one card, a workstation with four, and
//! a headless box with none all run the same code.
//!
//! # Unsupported metrics keep their definitions
//!
//! A GPU whose driver exposes no clock interface still declares
//! `gpu.frequency.core@<its source>`, carrying an `unsupported` availability
//! and a reason. It is **not** dropped from the catalog. That is what lets a
//! dashboard built on a machine with full telemetry open on a machine without
//! it and explain itself, instead of showing empty slots where widgets used to
//! be — and start working again when a proper driver is installed.

pub mod descriptor;
pub mod memory;

use crate::metrics::model::{
    Availability, MetricCategory, MetricDefinition, MetricDefinitionBuilder, MetricKey, MetricKind,
    MetricRef, MetricUnit, ProviderId, SourceId,
};

pub use descriptor::{
    device_model_source_id, normalize_nvml_uuid, nvml_source_id, pci_source_id, GpuCapabilities,
    GpuDescriptor, GpuIdentity, GpuVendor, IdentityStability, PciAddress,
};
pub use memory::GpuMemoryReading;

// --- metric keys ----------------------------------------------------------

/// `gpu.count` — how many hardware GPUs were inventoried.
pub const COUNT: &str = "gpu.count";

/// `gpu.usage.core` — share of the GPU's graphics engine doing work.
pub const USAGE_CORE: &str = "gpu.usage.core";

/// `gpu.memory.total` — dedicated video memory installed on the board.
pub const MEMORY_TOTAL: &str = "gpu.memory.total";
/// `gpu.memory.used` — dedicated video memory in use.
pub const MEMORY_USED: &str = "gpu.memory.used";
/// `gpu.memory.free` — dedicated video memory still allocatable.
pub const MEMORY_FREE: &str = "gpu.memory.free";
/// `gpu.memory.usage.percent` — `used / total * 100`.
pub const MEMORY_USAGE_PERCENT: &str = "gpu.memory.usage.percent";

/// `gpu.frequency.core` — current graphics/shader clock, in hertz.
pub const FREQUENCY_CORE: &str = "gpu.frequency.core";
/// `gpu.frequency.memory` — current memory clock, in hertz.
pub const FREQUENCY_MEMORY: &str = "gpu.frequency.memory";

/// `gpu.temperature.core` — the GPU die's own temperature, in degrees Celsius.
pub const TEMPERATURE_CORE: &str = "gpu.temperature.core";
/// `gpu.temperature.hotspot` — the hottest point on the GPU package, in degrees
/// Celsius.
///
/// **A different sensor, never derived from the die temperature.** Vendors call
/// it the junction or hotspot reading and it runs well above the die figure
/// under load; computing one from the other would produce a number that looks
/// right and tracks nothing.
pub const TEMPERATURE_HOTSPOT: &str = "gpu.temperature.hotspot";
/// `gpu.temperature.memory` — the video memory's temperature, in degrees
/// Celsius.
///
/// Another distinct sensor. GDDR6X modules in particular run far hotter than
/// the die, which is the entire reason this metric exists separately.
pub const TEMPERATURE_MEMORY: &str = "gpu.temperature.memory";
/// `gpu.fan.speed` — fan speed, in revolutions per minute.
///
/// **RPM, and only RPM.** Several interfaces report a fan's *duty cycle* as a
/// percentage instead; that is a control setting, not a speed, and the two are
/// not convertible — a fan at 40 % duty may be stopped, spinning up, or held at
/// a curve point. A percentage is never republished here as an RPM.
pub const FAN_SPEED: &str = "gpu.fan.speed";

/// Every per-GPU key, in declaration order.
pub const PER_GPU_KEYS: &[&str] = &[
    USAGE_CORE,
    MEMORY_TOTAL,
    MEMORY_USED,
    MEMORY_FREE,
    MEMORY_USAGE_PERCENT,
    FREQUENCY_CORE,
    FREQUENCY_MEMORY,
    TEMPERATURE_CORE,
    TEMPERATURE_HOTSPOT,
    TEMPERATURE_MEMORY,
    FAN_SPEED,
];

/// The per-GPU keys describing **performance**: what the engine is doing.
///
/// Kept apart from [`THERMAL_KEYS`] because the two fail independently. An
/// open-source driver commonly exposes a temperature and no utilisation
/// counter, and a user told "GPU telemetry unavailable" in that situation is
/// being told something false twice over.
pub const PERFORMANCE_KEYS: &[&str] = &[
    USAGE_CORE,
    MEMORY_TOTAL,
    MEMORY_USED,
    MEMORY_FREE,
    MEMORY_USAGE_PERCENT,
    FREQUENCY_CORE,
    FREQUENCY_MEMORY,
];

/// The per-GPU keys describing **temperature and cooling**.
pub const THERMAL_KEYS: &[&str] = &[
    TEMPERATURE_CORE,
    TEMPERATURE_HOTSPOT,
    TEMPERATURE_MEMORY,
    FAN_SPEED,
];

// --- sources --------------------------------------------------------------

/// The machine-wide GPU source.
///
/// Like `cpu:system`, a *logical* identifier meaning "this machine's GPUs taken
/// together" — stable by construction and identical on every platform. Every
/// other GPU source identifies one physical device; see
/// [`descriptor`] for how those are derived.
pub const SOURCE: &str = "gpu:system";

/// The user-facing label for `gpu:system`.
const SYSTEM_LABEL: &str = "Graphics";

fn key(name: &str) -> MetricKey {
    MetricKey::new(name).expect("well-known GPU key must be valid")
}

/// Builds the `gpu.count` reference.
pub fn count_ref() -> MetricRef {
    MetricRef::new(
        key(COUNT),
        SourceId::new(SOURCE).expect("well-known GPU source must be valid"),
    )
}

/// Builds a per-GPU reference for any of [`PER_GPU_KEYS`].
pub fn gpu_ref(name: &str, source: &SourceId) -> MetricRef {
    MetricRef::new(key(name), source.clone())
}

pub fn usage_core_ref(source: &SourceId) -> MetricRef {
    gpu_ref(USAGE_CORE, source)
}

pub fn memory_total_ref(source: &SourceId) -> MetricRef {
    gpu_ref(MEMORY_TOTAL, source)
}

pub fn memory_used_ref(source: &SourceId) -> MetricRef {
    gpu_ref(MEMORY_USED, source)
}

pub fn memory_free_ref(source: &SourceId) -> MetricRef {
    gpu_ref(MEMORY_FREE, source)
}

pub fn memory_usage_percent_ref(source: &SourceId) -> MetricRef {
    gpu_ref(MEMORY_USAGE_PERCENT, source)
}

pub fn frequency_core_ref(source: &SourceId) -> MetricRef {
    gpu_ref(FREQUENCY_CORE, source)
}

pub fn frequency_memory_ref(source: &SourceId) -> MetricRef {
    gpu_ref(FREQUENCY_MEMORY, source)
}

pub fn temperature_core_ref(source: &SourceId) -> MetricRef {
    gpu_ref(TEMPERATURE_CORE, source)
}

pub fn temperature_hotspot_ref(source: &SourceId) -> MetricRef {
    gpu_ref(TEMPERATURE_HOTSPOT, source)
}

pub fn temperature_memory_ref(source: &SourceId) -> MetricRef {
    gpu_ref(TEMPERATURE_MEMORY, source)
}

pub fn fan_speed_ref(source: &SourceId) -> MetricRef {
    gpu_ref(FAN_SPEED, source)
}

// --- declarations ---------------------------------------------------------

/// Declares every GPU metric PULSE ships for a discovered device list.
///
/// Both the Linux and the Windows GPU provider call this, so their
/// declarations are identical apart from `providerId` and the availabilities
/// each platform genuinely discovered. A contract test asserts exactly that,
/// including that an NVIDIA card identified by its NVML UUID gets the *same*
/// `SourceId` on both operating systems.
///
/// Sorted by metric reference, matching the order the engine's catalog will
/// hold it in, so the output is deterministic for a given device list
/// regardless of the order the platform enumerated them.
pub fn definitions(provider: &ProviderId, gpus: &[GpuDescriptor]) -> Vec<MetricDefinition> {
    let mut definitions = Vec::with_capacity(1 + PER_GPU_KEYS.len() * gpus.len());

    definitions.push(
        MetricDefinitionBuilder::new(
            count_ref(),
            provider.clone(),
            MetricCategory::Gpu,
            MetricUnit::Count,
            // A discrete fact about the machine, not a reading that rises and
            // falls — the same reasoning as `cpu.count.*`. `State` is not
            // directly averageable, so history can never produce "1.4 GPUs".
            MetricKind::State,
        )
        .source_label(SYSTEM_LABEL)
        .display_name("GPUs")
        .description(
            "Number of hardware graphics adapters PULSE inventoried. Software \
             adapters such as Microsoft Basic Render Driver and WARP are not counted.",
        )
        .build(),
    );

    for gpu in gpus {
        definitions.extend(per_gpu_definitions(provider, gpu));
    }

    definitions.sort_by(|left, right| left.metric.cmp(&right.metric));
    definitions
}

/// The eleven metrics of one device.
fn per_gpu_definitions(provider: &ProviderId, gpu: &GpuDescriptor) -> Vec<MetricDefinition> {
    let label = gpu.display_name.as_str();
    let capabilities = &gpu.capabilities;

    let build = |reference: MetricRef,
                 unit: MetricUnit,
                 kind: MetricKind,
                 display: &str,
                 description: &str,
                 availability: &Availability| {
        MetricDefinitionBuilder::new(reference, provider.clone(), MetricCategory::Gpu, unit, kind)
            .source_label(label)
            .display_name(display)
            .description(description)
            .availability(availability.clone())
            .build()
    };

    vec![
        build(
            usage_core_ref(&gpu.source_id),
            MetricUnit::Percent,
            MetricKind::Gauge,
            "GPU usage",
            "Share of the graphics engine's capacity in use, as the driver reports it.",
            &capabilities.usage_core,
        ),
        build(
            memory_total_ref(&gpu.source_id),
            MetricUnit::Bytes,
            // Installed VRAM is a hardware fact, not a reading: averaging it
            // over time is meaningless, so it is a state like `gpu.count`.
            MetricKind::State,
            "Total VRAM",
            "Dedicated video memory installed on this adapter. System memory shared \
             with an integrated GPU is deliberately not reported here.",
            &capabilities.memory_total,
        ),
        build(
            memory_used_ref(&gpu.source_id),
            MetricUnit::Bytes,
            MetricKind::Gauge,
            "Used VRAM",
            "Dedicated video memory currently in use across the whole system.",
            &capabilities.memory_used,
        ),
        build(
            memory_free_ref(&gpu.source_id),
            MetricUnit::Bytes,
            MetricKind::Gauge,
            "Free VRAM",
            "Dedicated video memory still available for allocation.",
            &capabilities.memory_free,
        ),
        build(
            memory_usage_percent_ref(&gpu.source_id),
            MetricUnit::Percent,
            MetricKind::Gauge,
            "VRAM usage",
            "Share of dedicated video memory in use, computed from used and total bytes.",
            &capabilities.memory_usage_percent,
        ),
        build(
            frequency_core_ref(&gpu.source_id),
            MetricUnit::Hertz,
            MetricKind::Gauge,
            "Core clock",
            "Current graphics clock the driver reports. Boost behaviour and power \
             policies move it continuously, so it is the platform's latest figure \
             rather than an instantaneous measurement of the silicon.",
            &capabilities.frequency_core,
        ),
        build(
            frequency_memory_ref(&gpu.source_id),
            MetricUnit::Hertz,
            MetricKind::Gauge,
            "Memory clock",
            "Current video memory clock the driver reports.",
            &capabilities.frequency_memory,
        ),
        build(
            temperature_core_ref(&gpu.source_id),
            MetricUnit::Celsius,
            MetricKind::Gauge,
            "Temperature",
            "Temperature of the graphics processor die, as its own sensor reports it.",
            &capabilities.temperature_core,
        ),
        build(
            temperature_hotspot_ref(&gpu.source_id),
            MetricUnit::Celsius,
            MetricKind::Gauge,
            "Hotspot",
            "Hottest point measured on the graphics processor package. A separate \
             sensor from the die temperature, typically reading higher under load, \
             and never derived from it.",
            &capabilities.temperature_hotspot,
        ),
        build(
            temperature_memory_ref(&gpu.source_id),
            MetricUnit::Celsius,
            MetricKind::Gauge,
            "Memory temperature",
            "Temperature of the dedicated video memory, from its own sensor.",
            &capabilities.temperature_memory,
        ),
        build(
            fan_speed_ref(&gpu.source_id),
            MetricUnit::Rpm,
            MetricKind::Gauge,
            "Fan",
            "Speed the adapter's fan is turning at, in revolutions per minute. A \
             fan control percentage is a duty cycle rather than a speed, and is \
             never published in its place.",
            &capabilities.fan_speed,
        ),
    ]
}

/// Resolves a per-GPU key against a set of freshly read values.
///
/// Shared so that both providers publish the same number for the same key, and
/// so the mapping is tested once.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuTelemetry {
    /// Core utilisation, 0–100.
    pub usage_core: Option<f64>,
    pub memory: Option<GpuMemoryReading>,
    /// Core clock in hertz.
    pub frequency_core_hz: Option<u64>,
    /// Memory clock in hertz.
    pub frequency_memory_hz: Option<u64>,
    /// GPU die temperature, in degrees Celsius.
    pub temperature_core_c: Option<f64>,
    /// Package hotspot temperature, in degrees Celsius. **Never copied from
    /// [`GpuTelemetry::temperature_core_c`]** — a different sensor.
    pub temperature_hotspot_c: Option<f64>,
    /// Video memory temperature, in degrees Celsius. Likewise its own sensor.
    pub temperature_memory_c: Option<f64>,
    /// Fan speed in revolutions per minute. Never converted from a duty cycle.
    pub fan_rpm: Option<f64>,
}

impl GpuTelemetry {
    /// The value for one metric key, or `None` when it was not measured.
    pub fn value_for(&self, key: &str) -> Option<f64> {
        match key {
            USAGE_CORE => self.usage_core,
            MEMORY_TOTAL => self.memory.map(|memory| memory.total() as f64),
            MEMORY_USED => self.memory.map(|memory| memory.used() as f64),
            MEMORY_FREE => self.memory.map(|memory| memory.free() as f64),
            MEMORY_USAGE_PERCENT => self.memory.map(|memory| memory.usage_percent()),
            FREQUENCY_CORE => self.frequency_core_hz.map(|hz| hz as f64),
            FREQUENCY_MEMORY => self.frequency_memory_hz.map(|hz| hz as f64),
            TEMPERATURE_CORE => self.temperature_core_c,
            TEMPERATURE_HOTSPOT => self.temperature_hotspot_c,
            TEMPERATURE_MEMORY => self.temperature_memory_c,
            FAN_SPEED => self.fan_rpm,
            _ => None,
        }
    }
}

/// Clamps a driver-reported utilisation percentage, rejecting the impossible.
///
/// Drivers occasionally report values above 100 during a reset or a race.
/// Publishing one would break every gauge widget's scale, and inventing a
/// clamp silently would hide a genuinely broken driver — so a value that is
/// merely *slightly* out of range is clamped, while a nonsensical one is
/// refused outright.
pub fn utilization_percent(raw: u32) -> Option<f64> {
    // NVML and AMDGPU both report whole percent. Anything far outside the
    // range is not a rounding artefact, it is a bad read.
    if raw > 200 {
        return None;
    }

    Some(f64::from(raw).clamp(0.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::MetricValueType;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn provider() -> ProviderId {
        ProviderId::new("linux.gpu").expect("valid")
    }

    fn nvidia(uuid: &str, name: &str) -> GpuDescriptor {
        GpuDescriptor {
            source_id: nvml_source_id(uuid).expect("valid"),
            display_name: name.to_string(),
            vendor: GpuVendor::Nvidia,
            identity: GpuIdentity::NvmlUuid(uuid.to_string()),
            pci: Some(PciAddress::new(0, 1, 0, 0)),
            backend: "nvml",
            capabilities: GpuCapabilities::all_available(),
        }
    }

    #[test]
    fn the_machine_wide_reference_is_valid_and_stable() {
        assert_eq!(count_ref().to_string(), "gpu.count@gpu:system");
    }

    #[test]
    fn per_gpu_references_carry_the_devices_own_source() {
        let gpu = nvidia("GPU-1111", "NVIDIA GeForce RTX 4070");

        assert_eq!(
            usage_core_ref(&gpu.source_id).to_string(),
            "gpu.usage.core@gpu:nvidia-1111"
        );
        assert_eq!(
            memory_usage_percent_ref(&gpu.source_id).to_string(),
            "gpu.memory.usage.percent@gpu:nvidia-1111"
        );
    }

    #[test]
    fn the_catalog_grows_with_the_number_of_gpus() {
        for count in 0..=4_usize {
            let gpus: Vec<GpuDescriptor> = (0..count)
                .map(|index| nvidia(&format!("GPU-{index:08x}"), "NVIDIA GeForce RTX 4070"))
                .collect();

            assert_eq!(
                definitions(&provider(), &gpus).len(),
                1 + PER_GPU_KEYS.len() * count,
                "wrong catalog size for {count} GPUs"
            );
        }
    }

    #[test]
    fn a_machine_with_no_gpu_still_declares_the_count() {
        let definitions = definitions(&provider(), &[]);

        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].metric, count_ref());
        // Zero GPUs is a known fact, not a failure to detect.
        assert!(definitions[0].availability.is_available());
    }

    #[test]
    fn the_declaration_order_is_deterministic() {
        let gpus = vec![
            nvidia("GPU-bbbb", "NVIDIA GeForce RTX 4070"),
            nvidia("GPU-aaaa", "NVIDIA GeForce RTX 4070"),
        ];
        let forward: Vec<String> = definitions(&provider(), &gpus)
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();

        let mut sorted = forward.clone();
        sorted.sort();
        assert_eq!(forward, sorted);

        let mut reversed = gpus.clone();
        reversed.reverse();
        let backward: Vec<String> = definitions(&provider(), &reversed)
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();
        assert_eq!(forward, backward, "enumeration order must not matter");
    }

    #[test]
    fn the_types_match_the_documented_contract() {
        let gpu = nvidia("GPU-1111", "NVIDIA GeForce RTX 4070");
        let definitions = definitions(&provider(), std::slice::from_ref(&gpu));

        let find = |reference: MetricRef| {
            definitions
                .iter()
                .find(|definition| definition.metric == reference)
                .unwrap_or_else(|| panic!("{reference} must be declared"))
                .clone()
        };

        let count = find(count_ref());
        assert_eq!(count.unit, MetricUnit::Count);
        assert_eq!(count.kind, MetricKind::State);

        let usage = find(usage_core_ref(&gpu.source_id));
        assert_eq!(usage.unit, MetricUnit::Percent);
        assert_eq!(usage.kind, MetricKind::Gauge);

        let total = find(memory_total_ref(&gpu.source_id));
        assert_eq!(total.unit, MetricUnit::Bytes);
        assert_eq!(
            total.kind,
            MetricKind::State,
            "installed VRAM is a hardware fact, not an averageable reading"
        );
        assert!(!total.kind.is_directly_averageable());

        for reference in [
            memory_used_ref(&gpu.source_id),
            memory_free_ref(&gpu.source_id),
        ] {
            let definition = find(reference);
            assert_eq!(definition.unit, MetricUnit::Bytes);
            assert_eq!(definition.kind, MetricKind::Gauge);
        }

        let percent = find(memory_usage_percent_ref(&gpu.source_id));
        assert_eq!(percent.unit, MetricUnit::Percent);
        assert_eq!(percent.kind, MetricKind::Gauge);

        for reference in [
            frequency_core_ref(&gpu.source_id),
            frequency_memory_ref(&gpu.source_id),
        ] {
            let definition = find(reference);
            // Hertz, never MHz: the frontend converts for display.
            assert_eq!(definition.unit, MetricUnit::Hertz);
            assert_eq!(definition.kind, MetricKind::Gauge);
        }

        for definition in &definitions {
            assert_eq!(definition.value_type, MetricValueType::Number);
            assert_eq!(definition.category, MetricCategory::Gpu);
        }
    }

    #[test]
    fn an_unsupported_metric_keeps_its_definition() {
        // The portability promise: a driver without a clock interface must not
        // make the metric disappear from the catalog.
        let mut gpu = nvidia("GPU-1111", "NVIDIA GeForce RTX 4070");
        gpu.capabilities = gpu
            .capabilities
            .with_frequencies(&Availability::unsupported("no clock interface"));

        let definitions = definitions(&provider(), std::slice::from_ref(&gpu));

        assert_eq!(
            definitions.len(),
            1 + PER_GPU_KEYS.len(),
            "still one count plus every device metric"
        );

        let clock = definitions
            .iter()
            .find(|definition| definition.metric == frequency_core_ref(&gpu.source_id))
            .expect("still declared");
        assert_eq!(clock.availability.status_str(), "unsupported");
        // And the rest of the device is untouched.
        assert!(definitions
            .iter()
            .find(|definition| definition.metric == usage_core_ref(&gpu.source_id))
            .expect("declared")
            .availability
            .is_available());
    }

    #[test]
    fn a_gpu_with_no_telemetry_backend_still_declares_every_metric() {
        let reason = Availability::unsupported("no telemetry backend for this device");
        let mut gpu = nvidia("GPU-1111", "NVIDIA GeForce RTX 4070");
        gpu.capabilities = GpuCapabilities::none_available(&reason);

        let definitions = definitions(&provider(), std::slice::from_ref(&gpu));

        assert_eq!(definitions.len(), 1 + PER_GPU_KEYS.len());
        assert_eq!(
            definitions
                .iter()
                .filter(|definition| !definition.availability.is_available())
                .count(),
            PER_GPU_KEYS.len(),
            "every device metric is unavailable; the count is not"
        );
    }

    #[test]
    fn two_identical_cards_produce_distinct_non_colliding_references() {
        // The identity rule, at catalog level: same model, same name, two
        // different UUIDs — and therefore two full, distinct sets of
        // references.
        let gpus = vec![
            nvidia("GPU-aaaaaaaa-0000", "NVIDIA GeForce RTX 4090"),
            nvidia("GPU-bbbbbbbb-0000", "NVIDIA GeForce RTX 4090"),
        ];
        let definitions = definitions(&provider(), &gpus);

        assert_eq!(definitions.len(), 1 + 2 * PER_GPU_KEYS.len());

        let mut references: Vec<String> = definitions
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();
        let total = references.len();
        references.sort();
        references.dedup();
        assert_eq!(references.len(), total, "references must be unique");

        // Both carry the same user-facing label, which is presentation only.
        assert_eq!(gpus[0].display_name, gpus[1].display_name);
        assert_ne!(gpus[0].source_id, gpus[1].source_id);
    }

    #[test]
    fn user_facing_text_never_names_an_operating_system_or_a_backend() {
        let gpu = nvidia("GPU-1111", "NVIDIA GeForce RTX 4070");

        for definition in definitions(&provider(), std::slice::from_ref(&gpu)) {
            let lowered = definition.display_name.to_lowercase();
            for forbidden in ["linux", "windows", "fedora", "nvml", "dxgi", "/sys"] {
                assert!(
                    !lowered.contains(forbidden),
                    "'{}' leaks '{forbidden}'",
                    definition.display_name
                );
            }
        }
    }

    // --- telemetry mapping ------------------------------------------------

    #[test]
    fn telemetry_resolves_every_per_gpu_key() {
        let telemetry = GpuTelemetry {
            usage_core: Some(17.0),
            memory: Some(GpuMemoryReading::from_total_and_used(8 * GIB, 2 * GIB).expect("valid")),
            frequency_core_hz: Some(2_100_000_000),
            frequency_memory_hz: Some(8_001_000_000),
            temperature_core_c: Some(64.0),
            temperature_hotspot_c: Some(78.0),
            temperature_memory_c: Some(70.0),
            fan_rpm: Some(2_187.0),
        };

        assert_eq!(telemetry.value_for(USAGE_CORE), Some(17.0));
        assert_eq!(telemetry.value_for(MEMORY_TOTAL), Some((8 * GIB) as f64));
        assert_eq!(telemetry.value_for(MEMORY_USED), Some((2 * GIB) as f64));
        assert_eq!(telemetry.value_for(MEMORY_FREE), Some((6 * GIB) as f64));
        assert_eq!(telemetry.value_for(MEMORY_USAGE_PERCENT), Some(25.0));
        assert_eq!(telemetry.value_for(FREQUENCY_CORE), Some(2_100_000_000_f64));
        assert_eq!(
            telemetry.value_for(FREQUENCY_MEMORY),
            Some(8_001_000_000_f64)
        );
        assert_eq!(telemetry.value_for(TEMPERATURE_CORE), Some(64.0));
        assert_eq!(telemetry.value_for(TEMPERATURE_HOTSPOT), Some(78.0));
        assert_eq!(telemetry.value_for(TEMPERATURE_MEMORY), Some(70.0));
        assert_eq!(telemetry.value_for(FAN_SPEED), Some(2_187.0));
    }

    #[test]
    fn a_hotspot_is_never_answered_from_the_die_temperature() {
        // Two different sensors. A card that reports one and not the other must
        // publish one and not the other.
        let telemetry = GpuTelemetry {
            temperature_core_c: Some(64.0),
            ..GpuTelemetry::default()
        };

        assert_eq!(telemetry.value_for(TEMPERATURE_CORE), Some(64.0));
        assert_eq!(telemetry.value_for(TEMPERATURE_HOTSPOT), None);
        assert_eq!(telemetry.value_for(TEMPERATURE_MEMORY), None);
    }

    #[test]
    fn a_stopped_fan_is_a_reading_and_an_absent_one_is_not() {
        let stopped = GpuTelemetry {
            fan_rpm: Some(0.0),
            ..GpuTelemetry::default()
        };

        assert_eq!(stopped.value_for(FAN_SPEED), Some(0.0));
        assert_eq!(GpuTelemetry::default().value_for(FAN_SPEED), None);
    }

    #[test]
    fn an_unmeasured_field_resolves_to_nothing_rather_than_zero() {
        let telemetry = GpuTelemetry {
            usage_core: Some(17.0),
            ..GpuTelemetry::default()
        };

        assert_eq!(telemetry.value_for(USAGE_CORE), Some(17.0));
        for key in [MEMORY_TOTAL, MEMORY_USED, FREQUENCY_CORE, FREQUENCY_MEMORY] {
            assert_eq!(telemetry.value_for(key), None, "{key} must not be zero");
        }
    }

    #[test]
    fn an_unknown_key_resolves_to_nothing() {
        assert_eq!(GpuTelemetry::default().value_for("gpu.power.draw"), None);
    }

    #[test]
    fn every_declared_key_is_resolvable_by_the_telemetry_mapping() {
        // Guards against adding a key and forgetting to wire it up.
        let telemetry = GpuTelemetry {
            usage_core: Some(1.0),
            memory: Some(GpuMemoryReading::from_total_and_used(GIB, 0).expect("valid")),
            frequency_core_hz: Some(1),
            frequency_memory_hz: Some(1),
            temperature_core_c: Some(1.0),
            temperature_hotspot_c: Some(1.0),
            temperature_memory_c: Some(1.0),
            fan_rpm: Some(1.0),
        };

        for key in PER_GPU_KEYS {
            assert!(
                telemetry.value_for(key).is_some(),
                "{key} is declared but not resolvable"
            );
        }
    }

    // --- utilisation clamping ---------------------------------------------

    #[test]
    fn utilisation_accepts_the_normal_range() {
        assert_eq!(utilization_percent(0), Some(0.0));
        assert_eq!(utilization_percent(47), Some(47.0));
        assert_eq!(utilization_percent(100), Some(100.0));
    }

    #[test]
    fn a_slightly_out_of_range_utilisation_is_clamped() {
        // Drivers occasionally overshoot during a reset; a gauge must never be
        // handed 101%.
        assert_eq!(utilization_percent(101), Some(100.0));
        assert_eq!(utilization_percent(150), Some(100.0));
    }

    #[test]
    fn a_nonsensical_utilisation_is_refused_rather_than_clamped_into_plausibility() {
        // Clamping this would hide a genuinely broken read behind a
        // believable 100%.
        assert_eq!(utilization_percent(4_294_967_295), None);
        assert_eq!(utilization_percent(1_000), None);
    }
}
