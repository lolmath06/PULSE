//! CPU frequency: the canonical unit and what the number actually means.
//!
//! # The unit
//!
//! PULSE publishes frequency in **hertz**, always. Linux's CPUFreq interface
//! reports kilohertz, Windows' power API reports megahertz, and both are
//! converted here — in the shared layer, once — so that no provider invents
//! its own scale and no frontend ever learns that kHz exists.
//!
//! ```text
//! /sys/.../scaling_cur_freq   3200000 kHz  ─┐
//!                                            ├─► 3_200_000_000 Hz
//! CallNtPowerInformation      3200 MHz     ─┘
//! ```
//!
//! # What `cpu.frequency.current` means
//!
//! > The frequency the operating system interface PULSE reads currently
//! > reports for this logical processor.
//!
//! It is deliberately **not** claimed to be a perfect instantaneous
//! measurement of the silicon. Modern CPUs make that claim indefensible:
//!
//! - frequency scaling moves the clock continuously between samples;
//! - Turbo/boost states are brief and opportunistic;
//! - energy policies cap the clock independently of load;
//! - hybrid CPUs run P-cores and E-cores at unrelated frequencies;
//! - under virtualisation the guest may see the host's rated clock only.
//!
//! On Linux with `intel_pstate` or `amd-pstate`, `scaling_cur_freq` is read
//! from the hardware at the moment of the read and is about as close to the
//! truth as an unprivileged process can get. On Windows, `CurrentMhz` is the
//! kernel's most recent accounting figure. Both are honest answers to "what
//! does the OS say", which is what the metric promises.
//!
//! # What `cpu.frequency.max` means
//!
//! > The maximum frequency the platform reports for this logical processor.
//!
//! A *scaling* or *policy* limit is not the same thing as the hardware
//! maximum, and PULSE never lets one stand in for the other silently — see
//! [`MaxFrequencySource`].

use serde::Serialize;

// The hertz conversions live in `wellknown::units`, because GPU clocks need
// exactly the same arithmetic and a `gpu` module reaching into `cpu` for it
// would be the wrong layering. Re-exported here so the CPU code and its
// documentation keep reading naturally.
pub use crate::metrics::wellknown::units::{kilohertz_to_hertz, megahertz_to_hertz};

/// Where a `cpu.frequency.max` value came from.
///
/// The two are genuinely different quantities and the difference is visible to
/// the user: a laptop in a power-saving profile can report a scaling maximum
/// of 1.8 GHz on a chip whose hardware maximum is 5.6 GHz. Presenting the
/// former as "maximum frequency" would make PULSE look wrong to anyone who
/// knows their own hardware.
///
/// PULSE therefore publishes the metric only from a genuine hardware figure,
/// and records which one it used so the description can say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaxFrequencySource {
    /// The platform's rated hardware maximum — Linux `cpuinfo_max_freq`, or
    /// the Windows power API's `MaxMhz`.
    ///
    /// Note that even this is a *rated* figure: on Intel it is typically the
    /// maximum turbo frequency, on Windows frequently the base frequency
    /// instead. It is what the platform advertises, which is the most PULSE
    /// can honestly claim.
    Hardware,
    /// A governor or policy ceiling — Linux `scaling_max_freq`.
    ///
    /// Reflects the *current power policy*, not the chip. PULSE does not
    /// publish `cpu.frequency.max` from this; the variant exists so the
    /// distinction is nameable and testable rather than implicit.
    ScalingPolicy,
}

impl MaxFrequencySource {
    /// Whether a value from this source may be published as
    /// `cpu.frequency.max`.
    pub const fn is_publishable_as_hardware_max(self) -> bool {
        matches!(self, MaxFrequencySource::Hardware)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scaling_limit_is_never_published_as_a_hardware_maximum() {
        assert!(MaxFrequencySource::Hardware.is_publishable_as_hardware_max());
        assert!(!MaxFrequencySource::ScalingPolicy.is_publishable_as_hardware_max());
    }
}
