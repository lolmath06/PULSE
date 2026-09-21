//! The NVML seam: what PULSE asks of NVIDIA, independent of how it is asked.
//!
//! Splitting the *questions* from the *library call* is what makes the whole
//! failure surface testable on a machine with no NVIDIA driver. The real
//! implementation lives in [`super::library`]; tests use a scripted fake.

use crate::metrics::model::{Availability, MetricError, MetricErrorCode};
use crate::metrics::wellknown::gpu::PciAddress;

/// Why an NVML call did not produce a value.
///
/// A deliberate subset of NVML's return codes, kept distinct because they mean
/// genuinely different things to a user. Collapsing them all into
/// "provider error" is exactly what PULSE's availability contract exists to
/// prevent: *"your driver does not expose a memory clock"* and *"PULSE needs
/// elevated rights"* are not the same message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvmlError {
    /// `NVML_ERROR_NOT_SUPPORTED` — this device or driver does not offer the
    /// requested figure. Extremely common, and entirely normal.
    NotSupported,
    /// `NVML_ERROR_NO_PERMISSION` — the query needs privileges PULSE does not
    /// have. Actionable by the user.
    NoPermission,
    /// `NVML_ERROR_GPU_IS_LOST` — the device fell off the bus. Transient in
    /// principle; recovers with a driver reset.
    GpuIsLost,
    /// `NVML_ERROR_UNINITIALIZED` — `nvmlInit` was not called or has been shut
    /// down. A PULSE bug rather than an environment problem.
    Uninitialized,
    /// `NVML_ERROR_NOT_FOUND` / `NVML_ERROR_INVALID_ARGUMENT` — the device
    /// index does not exist, typically because the set changed mid-enumeration.
    NotFound,
    /// The library, or a required symbol, is not present on this system.
    Unavailable(String),
    /// Any other NVML return code, carried verbatim so it can be diagnosed.
    Other(i32),
}

impl NvmlError {
    /// Maps an `nvmlReturn_t` to the classification above.
    ///
    /// The numeric values are NVML's documented, stable public constants.
    pub fn from_status(status: i32) -> Self {
        match status {
            1 => NvmlError::Uninitialized,
            2 => NvmlError::NotFound, // INVALID_ARGUMENT
            3 => NvmlError::NotSupported,
            4 => NvmlError::NoPermission,
            6 => NvmlError::NotFound, // NOT_FOUND
            15 => NvmlError::GpuIsLost,
            other => NvmlError::Other(other),
        }
    }

    /// A human-readable explanation, used as the availability reason.
    pub fn message(&self) -> String {
        match self {
            NvmlError::NotSupported => {
                "this GPU or driver does not report the requested figure".to_string()
            }
            NvmlError::NoPermission => {
                "the NVIDIA driver refused this query at the current privilege level".to_string()
            }
            NvmlError::GpuIsLost => "the GPU is not currently reachable on the bus".to_string(),
            NvmlError::Uninitialized => {
                "the NVIDIA management library is not initialised".to_string()
            }
            NvmlError::NotFound => "this GPU is no longer present".to_string(),
            NvmlError::Unavailable(detail) => detail.clone(),
            NvmlError::Other(code) => {
                format!("the NVIDIA management library returned error {code}")
            }
        }
    }
}

/// Turns an NVML failure into the availability that describes it honestly.
///
/// The distinctions survive all the way to the user's tooltip:
///
/// | NVML | Availability | Why |
/// |---|---|---|
/// | `NOT_SUPPORTED` | `unsupported` | The capability is genuinely absent here |
/// | `NO_PERMISSION` | `permissionDenied` | The user can act on this |
/// | `GPU_IS_LOST` | `temporarilyUnavailable` | Expected to recover |
/// | `NOT_FOUND` | `notDetected` | The device went away |
/// | `UNINITIALIZED` | `providerError` | PULSE's own fault, and worth reporting as such |
pub fn availability_for_nvml(error: &NvmlError) -> Availability {
    match error {
        NvmlError::NotSupported => Availability::unsupported(error.message()),
        NvmlError::NoPermission => Availability::permission_denied(error.message()),
        NvmlError::GpuIsLost => Availability::temporarily_unavailable(error.message()),
        NvmlError::NotFound => Availability::not_detected(error.message()),
        NvmlError::Unavailable(_) => Availability::unsupported(error.message()),
        NvmlError::Uninitialized => Availability::provider_error(MetricError::new(
            MetricErrorCode::ProviderUnavailable,
            error.message(),
        )),
        NvmlError::Other(_) => Availability::provider_error(MetricError::new(
            MetricErrorCode::ProviderUnavailable,
            error.message(),
        )),
    }
}

/// The identity and description of one NVML device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvmlDeviceInfo {
    /// The board's globally unique, immutable UUID — the identity PULSE stores.
    pub uuid: String,
    /// The marketing name, e.g. `NVIDIA GeForce RTX 4070 Laptop GPU`.
    /// Presentation only.
    pub name: String,
    /// The bus address, used to correlate this device with the platform's own
    /// GPU inventory. Never used as identity while a UUID is available.
    pub pci: Option<PciAddress>,
}

/// One device's memory figures, in bytes, exactly as NVML reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmlMemory {
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

/// A fan speed NVML reported, in revolutions per minute.
///
/// A struct rather than a bare `u32` so the unit is impossible to mistake at a
/// call site: NVML also reports a fan's *duty cycle* as a percentage, and the
/// two are not convertible. A fan at 40 % duty may be stopped, spinning up, or
/// sitting on a curve point; turning that into "40 RPM", or into any RPM at
/// all, would be inventing a measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmlFanRpm(pub u32);

impl NvmlFanRpm {
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Which temperature sensor to read.
///
/// One variant, deliberately. `NVML_TEMPERATURE_GPU` is the only sensor the
/// public NVML interface documents, and PULSE will not publish a hotspot or a
/// memory temperature it cannot name a documented source for. See
/// `docs/metrics/thermals.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmlTemperatureSensor {
    /// `NVML_TEMPERATURE_GPU` — the die's own sensor.
    Gpu,
}

impl NvmlTemperatureSensor {
    /// The `nvmlTemperatureSensors_t` value.
    pub const fn as_raw(self) -> u32 {
        match self {
            NvmlTemperatureSensor::Gpu => 0,
        }
    }
}

/// Which clock domain to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmlClock {
    /// `NVML_CLOCK_GRAPHICS` — the shader/core clock.
    Graphics,
    /// `NVML_CLOCK_MEM` — the video memory clock.
    Memory,
}

impl NvmlClock {
    /// The `nvmlClockType_t` value.
    pub const fn as_raw(self) -> u32 {
        match self {
            NvmlClock::Graphics => 0,
            NvmlClock::Memory => 1,
        }
    }
}

/// What PULSE needs from NVIDIA, as a testable interface.
///
/// Deliberately small: exactly the questions PULSE asks, and no more. Power,
/// voltage and encoder queries are not here because they are not in scope, not
/// because they would be hard to add.
pub trait NvmlBackend: Send + Sync {
    /// How many NVIDIA devices the driver reports.
    fn device_count(&self) -> Result<u32, NvmlError>;

    /// Identity and description of one device.
    ///
    /// `index` is valid **only during the current enumeration**: NVML
    /// documents it as unstable across reboots, driver resets and device
    /// hotplug, which is precisely why the UUID is what PULSE stores.
    fn device_info(&self, index: u32) -> Result<NvmlDeviceInfo, NvmlError>;

    /// Core utilisation as a whole percentage.
    fn utilization(&self, index: u32) -> Result<u32, NvmlError>;

    /// Dedicated video memory, in bytes.
    fn memory(&self, index: u32) -> Result<NvmlMemory, NvmlError>;

    /// A clock domain's current frequency, in megahertz.
    fn clock_mhz(&self, index: u32, clock: NvmlClock) -> Result<u32, NvmlError>;

    /// A sensor's temperature, in whole degrees Celsius.
    ///
    /// Implementations prefer the current `nvmlDeviceGetTemperatureV` entry
    /// point and fall back to the legacy `nvmlDeviceGetTemperature` when the
    /// installed library does not export it — both are optional, and neither
    /// being present costs this metric alone.
    fn temperature_c(&self, index: u32, sensor: NvmlTemperatureSensor) -> Result<u32, NvmlError>;

    /// The adapter's fan speed, in revolutions per minute.
    ///
    /// Returns [`NvmlError::Unavailable`] — which reads as `unsupported` — when
    /// the library exports no RPM query, when the board has no fan sensor, and
    /// when the board has **several independent fans**: PULSE's contract has one
    /// `gpu.fan.speed` per GPU, and picking fan 0 out of three would silently
    /// publish a partial answer as the whole one.
    fn fan_rpm(&self, index: u32) -> Result<NvmlFanRpm, NvmlError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_nvml_return_codes_pulse_distinguishes() {
        assert_eq!(NvmlError::from_status(1), NvmlError::Uninitialized);
        assert_eq!(NvmlError::from_status(2), NvmlError::NotFound);
        assert_eq!(NvmlError::from_status(3), NvmlError::NotSupported);
        assert_eq!(NvmlError::from_status(4), NvmlError::NoPermission);
        assert_eq!(NvmlError::from_status(6), NvmlError::NotFound);
        assert_eq!(NvmlError::from_status(15), NvmlError::GpuIsLost);
        assert_eq!(NvmlError::from_status(999), NvmlError::Other(999));
    }

    #[test]
    fn each_failure_keeps_its_own_meaning() {
        // The whole point of not collapsing everything into providerError:
        // these four tell the user four different things.
        let cases = [
            (NvmlError::NotSupported, "unsupported"),
            (NvmlError::NoPermission, "permissionDenied"),
            (NvmlError::GpuIsLost, "temporarilyUnavailable"),
            (NvmlError::NotFound, "notDetected"),
        ];

        let mut statuses = Vec::new();
        for (error, expected) in cases {
            let availability = availability_for_nvml(&error);
            assert_eq!(availability.status_str(), expected, "for {error:?}");
            assert!(!availability.is_available());
            statuses.push(availability.status_str());
        }

        statuses.dedup();
        assert_eq!(statuses.len(), 4, "the four must stay distinguishable");
    }

    #[test]
    fn an_unsupported_metric_is_never_reported_as_a_provider_error() {
        // By far the most common NVML outcome. Reporting it as a provider
        // failure would make a perfectly healthy GPU look broken.
        let availability = availability_for_nvml(&NvmlError::NotSupported);

        assert_eq!(availability.status_str(), "unsupported");
        assert!(!availability.is_transient());
    }

    #[test]
    fn a_lost_gpu_is_transient_so_the_ui_suggests_waiting() {
        assert!(availability_for_nvml(&NvmlError::GpuIsLost).is_transient());
    }

    #[test]
    fn a_missing_library_is_unsupported_rather_than_an_error() {
        // "You do not have the NVIDIA driver installed" is not a malfunction.
        let availability = availability_for_nvml(&NvmlError::Unavailable(
            "libnvidia-ml.so.1 not found".into(),
        ));

        assert_eq!(availability.status_str(), "unsupported");
        assert!(availability.clone().status_str().eq("unsupported"));
    }

    #[test]
    fn pulse_own_mistakes_are_reported_as_provider_errors() {
        // Uninitialised means PULSE mismanaged the lifecycle, not that the
        // user's machine lacks something.
        for error in [NvmlError::Uninitialized, NvmlError::Other(42)] {
            assert_eq!(
                availability_for_nvml(&error).status_str(),
                "providerError",
                "for {error:?}"
            );
        }
    }

    #[test]
    fn every_error_produces_a_non_empty_explanation() {
        for error in [
            NvmlError::NotSupported,
            NvmlError::NoPermission,
            NvmlError::GpuIsLost,
            NvmlError::Uninitialized,
            NvmlError::NotFound,
            NvmlError::Unavailable("detail".into()),
            NvmlError::Other(7),
        ] {
            assert!(!error.message().is_empty(), "{error:?} has no message");
        }
    }

    #[test]
    fn the_clock_domain_values_match_nvml() {
        assert_eq!(NvmlClock::Graphics.as_raw(), 0);
        assert_eq!(NvmlClock::Memory.as_raw(), 1);
    }

    #[test]
    fn the_temperature_sensor_value_matches_nvml() {
        assert_eq!(NvmlTemperatureSensor::Gpu.as_raw(), 0);
    }

    #[test]
    fn a_fan_speed_carries_its_unit_in_its_type() {
        // The mistake this shape exists to prevent: a duty-cycle percentage
        // reaching `gpu.fan.speed`, which is declared in RPM.
        let speed = NvmlFanRpm(2187);

        assert_eq!(speed.get(), 2187);
    }
}
