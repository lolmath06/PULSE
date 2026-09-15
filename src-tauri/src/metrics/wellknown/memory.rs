//! The `memory.*` metrics: declarations and the shared PULSE convention.
//!
//! Every platform reports two raw numbers — total and available physical
//! memory, in bytes — and this module derives the rest. That is what makes
//! `memory.used@memory:system` mean the same thing on Fedora and on Windows
//! rather than "whatever each OS calls used".

use crate::metrics::model::{
    MetricCategory, MetricDefinition, MetricDefinitionBuilder, MetricError, MetricErrorCode,
    MetricKey, MetricKind, MetricRef, MetricSample, MetricUnit, ProviderId, SourceId,
};
use crate::metrics::wellknown::availability_for;

/// `memory.total` — installed physical memory usable by the OS, in bytes.
pub const TOTAL: &str = "memory.total";
/// `memory.used` — `total - available`, in bytes.
pub const USED: &str = "memory.used";
/// `memory.available` — memory obtainable without swapping, in bytes.
pub const AVAILABLE: &str = "memory.available";
/// `memory.usage.percent` — `used / total * 100`.
pub const USAGE_PERCENT: &str = "memory.usage.percent";

/// The logical memory source.
///
/// Like `cpu:system`, this is a logical identifier for "this machine's
/// physical memory as a whole" — stable by construction and identical on every
/// platform. Per-DIMM sources, if they ever exist, will carry hardware-derived
/// identifiers of their own.
pub const SOURCE: &str = "memory:system";

fn reference(key: &str) -> MetricRef {
    MetricRef::new(
        MetricKey::new(key).expect("well-known memory key must be valid"),
        SourceId::new(SOURCE).expect("well-known memory source must be valid"),
    )
}

pub fn total_ref() -> MetricRef {
    reference(TOTAL)
}

pub fn used_ref() -> MetricRef {
    reference(USED)
}

pub fn available_ref() -> MetricRef {
    reference(AVAILABLE)
}

pub fn usage_percent_ref() -> MetricRef {
    reference(USAGE_PERCENT)
}

/// Declares every memory metric PULSE ships, attributed to `provider`.
///
/// Both the Linux and Windows memory providers call this, so their
/// declarations are identical apart from `providerId`.
pub fn definitions(provider: &ProviderId) -> Vec<MetricDefinition> {
    let byte_gauge = |reference: MetricRef, display: &str, description: &str| {
        MetricDefinitionBuilder::new(
            reference,
            provider.clone(),
            MetricCategory::Memory,
            MetricUnit::Bytes,
            MetricKind::Gauge,
        )
        .display_name(display)
        .source_label("System memory")
        .description(description)
        .build()
    };

    vec![
        byte_gauge(
            total_ref(),
            "Total memory",
            "Installed physical memory usable by the operating system.",
        ),
        byte_gauge(
            used_ref(),
            "Used memory",
            "Physical memory in use, defined as total minus available.",
        ),
        byte_gauge(
            available_ref(),
            "Available memory",
            "Physical memory that can be allocated without swapping.",
        ),
        MetricDefinitionBuilder::new(
            usage_percent_ref(),
            provider.clone(),
            MetricCategory::Memory,
            MetricUnit::Percent,
            MetricKind::Gauge,
        )
        .display_name("Memory usage")
        .source_label("System memory")
        .description("Share of physical memory in use, computed from used and total bytes.")
        .build(),
    ]
}

/// A validated snapshot of physical memory, in bytes.
///
/// Constructed through [`MemoryReading::new`], which refuses impossible
/// combinations. This is the single place the PULSE convention is applied:
///
/// ```text
/// used          = total - available
/// usage_percent = used / total * 100
/// ```
///
/// Note that `available` is deliberately **not** "free" memory. On Linux,
/// `MemFree` excludes reclaimable page cache and would make a healthy machine
/// look nearly out of memory; `MemAvailable` is the kernel's own estimate of
/// what a new allocation could actually obtain. Windows'
/// `ullAvailPhys` carries the same meaning, which is why the two platforms can
/// share one definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryReading {
    total: u64,
    available: u64,
}

impl MemoryReading {
    /// Validates and creates a reading.
    ///
    /// Rejects the two states that would otherwise reach a widget as nonsense:
    /// a total of zero (nothing to divide by) and an available figure larger
    /// than the total. A source that produces either is malfunctioning, and
    /// PULSE reports that instead of publishing an impossible number.
    pub fn new(total: u64, available: u64) -> Result<Self, MetricError> {
        if total == 0 {
            return Err(MetricError::new(
                MetricErrorCode::Parse,
                "reported total physical memory is zero",
            )
            .with_recoverable(true));
        }

        if available > total {
            return Err(MetricError::new(
                MetricErrorCode::Parse,
                format!("available memory ({available} bytes) exceeds total ({total} bytes)"),
            )
            .with_recoverable(true));
        }

        Ok(Self { total, available })
    }

    pub const fn total(&self) -> u64 {
        self.total
    }

    pub const fn available(&self) -> u64 {
        self.available
    }

    /// `total - available`. Cannot underflow: the constructor guarantees
    /// `available <= total`.
    pub const fn used(&self) -> u64 {
        self.total - self.available
    }

    /// `used / total * 100`, always within 0–100.
    ///
    /// `total` is non-zero by construction, so this cannot produce NaN or
    /// infinity.
    pub fn usage_percent(&self) -> f64 {
        ((self.used() as f64 / self.total as f64) * 100.0).clamp(0.0, 100.0)
    }
}

/// Turns one memory reading into samples for the requested references.
///
/// Shared by the Linux and Windows providers so the four metrics are derived
/// identically — and, because they all come from a single reading, so they are
/// always mutually consistent: `used + available` really does equal `total` in
/// the UI, with no torn read between them.
///
/// A failed reading produces unavailable samples carrying the reason, never a
/// zero.
pub fn samples(
    requested: &[MetricRef],
    reading: Result<MemoryReading, MetricError>,
) -> Vec<MetricSample> {
    requested
        .iter()
        .map(|reference| match &reading {
            Ok(reading) => match value_for(reading, reference.key.as_str()) {
                Some(value) => MetricSample::number(reference.clone(), value),
                // A reference this provider does not recognise; the engine
                // normally prevents this, so it is a genuine internal slip.
                None => MetricSample::unavailable(
                    reference.clone(),
                    availability_for(MetricError::internal(format!(
                        "'{}' is not a memory metric",
                        reference.key
                    ))),
                ),
            },
            Err(error) => {
                MetricSample::unavailable(reference.clone(), availability_for(error.clone()))
            }
        })
        .collect()
}

/// Resolves a metric key against a reading.
fn value_for(reading: &MemoryReading, key: &str) -> Option<f64> {
    match key {
        TOTAL => Some(reading.total() as f64),
        USED => Some(reading.used() as f64),
        AVAILABLE => Some(reading.available() as f64),
        USAGE_PERCENT => Some(reading.usage_percent()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::Availability;

    #[test]
    fn the_well_known_references_are_valid_and_share_one_source() {
        let references = [
            total_ref(),
            used_ref(),
            available_ref(),
            usage_percent_ref(),
        ];

        for reference in &references {
            assert_eq!(reference.source_id.as_str(), "memory:system");
        }

        let keys: Vec<&str> = references.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "memory.total",
                "memory.used",
                "memory.available",
                "memory.usage.percent"
            ]
        );
    }

    #[test]
    fn definitions_use_bytes_for_sizes_and_percent_for_the_ratio() {
        let provider = ProviderId::new("linux.memory").expect("valid");
        let definitions = definitions(&provider);

        assert_eq!(definitions.len(), 4);

        for definition in &definitions {
            assert_eq!(definition.category, MetricCategory::Memory);
            assert_eq!(definition.kind, MetricKind::Gauge);
            assert_eq!(definition.source_label, "System memory");
            // No OS name in user-facing text.
            assert!(!definition.display_name.to_lowercase().contains("linux"));
        }

        let unit_of = |key: &str| {
            definitions
                .iter()
                .find(|d| d.metric.key.as_str() == key)
                .map(|d| d.unit)
                .expect("declared")
        };

        assert_eq!(unit_of("memory.total"), MetricUnit::Bytes);
        assert_eq!(unit_of("memory.used"), MetricUnit::Bytes);
        assert_eq!(unit_of("memory.available"), MetricUnit::Bytes);
        assert_eq!(unit_of("memory.usage.percent"), MetricUnit::Percent);
    }

    #[test]
    fn applies_the_pulse_convention() {
        // 32 GiB total, 20 GiB available.
        let reading = MemoryReading::new(34_359_738_368, 21_474_836_480).expect("valid");

        assert_eq!(reading.total(), 34_359_738_368);
        assert_eq!(reading.available(), 21_474_836_480);
        assert_eq!(reading.used(), 12_884_901_888);
        assert!((reading.usage_percent() - 37.5).abs() < 1e-9);
    }

    #[test]
    fn used_and_available_always_reconstitute_total() {
        // The invariant a user can check by eye in the UI.
        for (total, available) in [
            (16_000_000_000_u64, 8_000_000_000_u64),
            (1, 0),
            (1, 1),
            (u64::MAX, 1),
            (8_589_934_592, 3_221_225_472),
        ] {
            let reading = MemoryReading::new(total, available).expect("valid");
            assert_eq!(reading.used() + reading.available(), reading.total());
        }
    }

    #[test]
    fn usage_percent_stays_within_bounds_and_is_always_finite() {
        for (total, available) in [
            (1_u64, 0_u64),
            (1, 1),
            (u64::MAX, 0),
            (u64::MAX, u64::MAX),
            (1024, 1),
            (3, 2),
        ] {
            let percent = MemoryReading::new(total, available)
                .expect("valid")
                .usage_percent();

            assert!(
                percent.is_finite(),
                "{total}/{available} produced {percent}"
            );
            assert!((0.0..=100.0).contains(&percent));
        }
    }

    #[test]
    fn a_fully_used_machine_reports_one_hundred_percent() {
        let reading = MemoryReading::new(8_000_000_000, 0).expect("valid");
        assert_eq!(reading.used(), 8_000_000_000);
        assert_eq!(reading.usage_percent(), 100.0);
    }

    #[test]
    fn an_idle_machine_reports_zero_percent() {
        let reading = MemoryReading::new(8_000_000_000, 8_000_000_000).expect("valid");
        assert_eq!(reading.used(), 0);
        assert_eq!(reading.usage_percent(), 0.0);
    }

    #[test]
    fn a_zero_total_is_rejected_rather_than_dividing_by_it() {
        let error = MemoryReading::new(0, 0).expect_err("zero total must be refused");
        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.recoverable);
    }

    #[test]
    fn available_above_total_is_rejected_rather_than_underflowing() {
        // Without this guard `used()` would wrap around to a colossal number.
        let error = MemoryReading::new(1_000, 2_000).expect_err("must be refused");
        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.message.contains("exceeds total"));
    }

    #[test]
    fn samples_derive_all_four_metrics_from_one_reading() {
        let reading = MemoryReading::new(1_000, 400).expect("valid");
        let requested = [
            total_ref(),
            used_ref(),
            available_ref(),
            usage_percent_ref(),
        ];

        let samples = samples(&requested, Ok(reading));

        assert_eq!(samples.len(), 4);
        assert!(samples.iter().all(|s| s.availability.is_available()));

        let number = |i: usize| {
            samples[i]
                .value
                .as_ref()
                .and_then(|v| v.as_number())
                .expect("numeric")
        };

        assert_eq!(number(0), 1_000.0);
        assert_eq!(number(1), 600.0);
        assert_eq!(number(2), 400.0);
        assert_eq!(number(3), 60.0);
        // The invariant the user can check by eye.
        assert_eq!(number(1) + number(2), number(0));
    }

    #[test]
    fn samples_answer_in_request_order() {
        let reading = MemoryReading::new(1_000, 400).expect("valid");
        let requested = [usage_percent_ref(), total_ref()];

        let samples = samples(&requested, Ok(reading));

        assert_eq!(samples[0].metric, usage_percent_ref());
        assert_eq!(samples[1].metric, total_ref());
    }

    #[test]
    fn a_failed_reading_yields_reasons_not_zeros() {
        let requested = [total_ref(), used_ref()];

        let samples = samples(
            &requested,
            Err(MetricError::new(MetricErrorCode::Io, "/proc/meminfo busy")),
        );

        assert_eq!(samples.len(), 2);
        for sample in &samples {
            assert!(!sample.availability.is_available());
            assert!(sample.value.is_none(), "must never fabricate a value");
            assert!(matches!(
                sample.availability,
                Availability::TemporarilyUnavailable { .. }
            ));
        }
    }

    #[test]
    fn an_empty_request_produces_no_samples() {
        let reading = MemoryReading::new(1_000, 400).expect("valid");
        assert!(samples(&[], Ok(reading)).is_empty());
    }
}
