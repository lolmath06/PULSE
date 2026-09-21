//! Wi-Fi link quality, and the one conversion PULSE refuses to make.
//!
//! # Signal quality is not RSSI
//!
//! An RSSI of −54 dBm is a **measurement**: the power arriving at the antenna,
//! on a logarithmic scale, as the radio reports it. A "signal quality" of 92 %
//! is an **interpretation**: somebody decided which dBm value counts as 0 and
//! which counts as 100, and different vendors decide differently.
//!
//! Every popular formula for turning one into the other is arbitrary. The
//! common `quality = 2 × (dBm + 100)` maps −100 dBm to 0 % and −50 dBm to
//! 100 %, which makes a perfectly ordinary −55 dBm link read as 90 % and a
//! marginal −85 dBm link read as 30 %; Windows's own mapping is different
//! again. Publishing a number derived that way would look like a measurement
//! and be a guess.
//!
//! So PULSE publishes a quality percentage **only when the platform itself
//! computes one**:
//!
//! | Platform | RSSI | Quality |
//! |---|---|---|
//! | Windows | from the WLAN API | **yes** — `wlanSignalQuality` is documented as 0–100 and the OS owns the mapping |
//! | Linux | from `nl80211` | **no** — `cfg80211` reports dBm and nothing else, so the metric is `unsupported` with that reason |
//!
//! A user on Fedora sees a real RSSI and an honest "this platform does not
//! report a quality percentage". That is more useful than a fabricated 92 %.
//!
//! # Multi-link operation
//!
//! Wi-Fi 7 radios can associate over several links at once, and both
//! `nl80211` and the Windows realtime-quality API can report per-link figures.
//! **dBm cannot be averaged arithmetically** — it is logarithmic, so the mean
//! of −50 and −90 is not −70 in any meaningful sense — and taking `links[0]`
//! would silently report whichever link the driver happened to list first.
//!
//! PULSE therefore publishes the **strongest active link's** RSSI, and says so
//! in the metric's description. That is a defined, reproducible rule: it
//! answers "how good is this machine's radio link right now" with the best
//! evidence available, and it degrades to exactly the single-link answer on
//! every radio that has one link.

use crate::metrics::model::Availability;

/// What one Wi-Fi link reports.
///
/// One of these per associated link; a single-link radio produces one.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WifiLink {
    /// Received signal strength, in dBm. Negative in practice.
    pub rssi_dbm: Option<i32>,
    /// The driver's own smoothed average, in dBm, when it reports one.
    /// Carried for diagnostics; the published RSSI is the instantaneous value.
    pub rssi_average_dbm: Option<i32>,
    /// Negotiated receive rate, in bits per second.
    pub receive_bps: Option<u64>,
    /// Negotiated transmit rate, in bits per second.
    pub transmit_bps: Option<u64>,
}

impl WifiLink {
    /// Whether this link carries anything worth publishing.
    pub const fn is_empty(&self) -> bool {
        self.rssi_dbm.is_none()
            && self.rssi_average_dbm.is_none()
            && self.receive_bps.is_none()
            && self.transmit_bps.is_none()
    }
}

/// The state of one Wi-Fi interface's association.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WifiLinkInfo {
    /// Every associated link. Empty when the interface is not connected.
    pub links: Vec<WifiLink>,
    /// A 0–100 quality figure, **only** when the platform computed one
    /// itself. Never derived from `rssi_dbm` — see the module documentation.
    pub quality_percent: Option<f64>,
}

/// The plausible range for a received Wi-Fi signal, in dBm.
///
/// A radio reporting outside it has misread something: 0 dBm is a milliwatt
/// arriving at the antenna, which does not happen, and −200 dBm is far below
/// the noise floor of any receiver. Both bounds are generous.
const RSSI_MIN_DBM: i32 = -120;
const RSSI_MAX_DBM: i32 = 0;

/// Whether a raw dBm reading is physically plausible.
pub const fn is_plausible_rssi(dbm: i32) -> bool {
    dbm >= RSSI_MIN_DBM && dbm <= RSSI_MAX_DBM
}

/// The largest link rate PULSE will publish, in bits per second.
///
/// 100 Gbit/s is far beyond any Wi-Fi generation; a value above it is a
/// misparse — a unit confusion, or an attribute read at the wrong offset —
/// rather than a very fast link.
const MAX_LINK_BPS: u64 = 100_000_000_000;

/// Whether a raw link rate is plausible.
pub const fn is_plausible_link_rate(bps: u64) -> bool {
    bps > 0 && bps <= MAX_LINK_BPS
}

impl WifiLinkInfo {
    /// An interface that is present but not associated with any network.
    pub fn disconnected() -> Self {
        Self::default()
    }

    /// Whether the interface is associated with a network.
    pub fn is_connected(&self) -> bool {
        self.links.iter().any(|link| !link.is_empty())
    }

    /// The link with the strongest signal.
    ///
    /// The defined rule for multi-link operation. Links with no RSSI at all
    /// are skipped rather than treated as infinitely weak; if none reports
    /// one, the first non-empty link is used so that rates still surface.
    pub fn best_link(&self) -> Option<&WifiLink> {
        self.links
            .iter()
            .filter(|link| link.rssi_dbm.is_some())
            .max_by_key(|link| link.rssi_dbm.unwrap_or(i32::MIN))
            .or_else(|| self.links.iter().find(|link| !link.is_empty()))
    }

    /// The RSSI PULSE publishes: the strongest active link's, when plausible.
    pub fn rssi_dbm(&self) -> Option<f64> {
        let dbm = self.best_link()?.rssi_dbm?;

        is_plausible_rssi(dbm).then(|| f64::from(dbm))
    }

    /// The receive link rate PULSE publishes, in bits per second.
    pub fn receive_bps(&self) -> Option<f64> {
        let bps = self.best_link()?.receive_bps?;

        is_plausible_link_rate(bps).then_some(bps as f64)
    }

    /// The transmit link rate PULSE publishes, in bits per second.
    pub fn transmit_bps(&self) -> Option<f64> {
        let bps = self.best_link()?.transmit_bps?;

        is_plausible_link_rate(bps).then_some(bps as f64)
    }

    /// The quality percentage PULSE publishes, when the platform computed one.
    ///
    /// Bounded to 0–100: a platform reporting outside that range is not
    /// reporting the documented figure, and clamping would hide the fault.
    pub fn quality_percent(&self) -> Option<f64> {
        let quality = self.quality_percent?;

        (quality.is_finite() && (0.0..=100.0).contains(&quality)).then_some(quality)
    }

    /// How many links the radio is associated over.
    pub fn link_count(&self) -> usize {
        self.links.iter().filter(|link| !link.is_empty()).count()
    }
}

/// The availability of a Wi-Fi metric on an interface that is not associated.
///
/// `TemporarilyUnavailable` rather than `NotDetected`: the radio is present
/// and working, there is simply nothing to measure until it joins a network,
/// and that resolves itself without the user doing anything to PULSE.
pub fn not_connected() -> Availability {
    Availability::temporarily_unavailable(
        "this adapter is not associated with a network, so it has no link to measure",
    )
}

/// The availability of `network.wifi.signal.quality` on a platform that does
/// not compute one.
pub fn quality_unsupported() -> Availability {
    Availability::unsupported(
        "this platform reports signal strength in dBm and no quality percentage of its own. \
         PULSE publishes the measured dBm rather than inventing a percentage from it.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(rssi: i32, rx: u64, tx: u64) -> WifiLink {
        WifiLink {
            rssi_dbm: Some(rssi),
            rssi_average_dbm: None,
            receive_bps: Some(rx),
            transmit_bps: Some(tx),
        }
    }

    /// The development machine's real association, as read by `iw`.
    fn real() -> WifiLinkInfo {
        WifiLinkInfo {
            links: vec![WifiLink {
                rssi_dbm: Some(-68),
                rssi_average_dbm: Some(-67),
                receive_bps: Some(175_500_000),
                transmit_bps: Some(390_000_000),
            }],
            quality_percent: None,
        }
    }

    // --- a single link -----------------------------------------------------

    #[test]
    fn publishes_a_real_single_link_association() {
        let info = real();

        assert!(info.is_connected());
        assert_eq!(info.link_count(), 1);
        assert_eq!(info.rssi_dbm(), Some(-68.0));
        assert_eq!(info.receive_bps(), Some(175_500_000.0));
        assert_eq!(info.transmit_bps(), Some(390_000_000.0));
    }

    #[test]
    fn a_platform_that_reports_no_quality_publishes_none() {
        // Fedora. The honest answer, and better than a fabricated percentage.
        assert_eq!(real().quality_percent(), None);
        assert_eq!(quality_unsupported().status_str(), "unsupported");
    }

    #[test]
    fn a_platform_that_reports_a_quality_publishes_it_unchanged() {
        // Windows. The OS owns the mapping; PULSE passes it through.
        let info = WifiLinkInfo {
            links: vec![link(-54, 866_700_000, 866_700_000)],
            quality_percent: Some(92.0),
        };

        assert_eq!(info.quality_percent(), Some(92.0));
    }

    #[test]
    fn quality_is_never_derived_from_rssi() {
        // The conversion this module exists to refuse. A strong −54 dBm link
        // with no platform-provided quality still publishes no quality at all,
        // rather than the 92 % a common formula would produce.
        let info = WifiLinkInfo {
            links: vec![link(-54, 0, 0)],
            quality_percent: None,
        };

        assert_eq!(info.rssi_dbm(), Some(-54.0));
        assert_eq!(info.quality_percent(), None);
    }

    #[test]
    fn a_quality_outside_its_documented_range_is_refused_not_clamped() {
        for quality in [-1.0, 101.0, f64::NAN, f64::INFINITY] {
            let info = WifiLinkInfo {
                links: vec![link(-54, 0, 0)],
                quality_percent: Some(quality),
            };
            assert_eq!(info.quality_percent(), None, "for {quality}");
        }

        for quality in [0.0, 50.0, 100.0] {
            let info = WifiLinkInfo {
                links: vec![link(-54, 0, 0)],
                quality_percent: Some(quality),
            };
            assert_eq!(info.quality_percent(), Some(quality));
        }
    }

    // --- signal plausibility ----------------------------------------------

    #[test]
    fn accepts_the_range_a_wifi_radio_actually_reports() {
        assert!(is_plausible_rssi(-40));
        assert!(is_plausible_rssi(-68));
        assert!(is_plausible_rssi(-90));
        assert!(is_plausible_rssi(-100));
    }

    #[test]
    fn refuses_a_physically_impossible_signal() {
        // A positive dBm means a milliwatt arriving at the antenna; −200 is
        // far below any receiver's noise floor. Both are misreads.
        assert!(!is_plausible_rssi(10));
        assert!(!is_plausible_rssi(-200));

        let info = WifiLinkInfo {
            links: vec![link(-200, 0, 0)],
            quality_percent: None,
        };
        assert_eq!(info.rssi_dbm(), None);
    }

    #[test]
    fn a_very_weak_and_a_very_strong_link_are_both_published() {
        for dbm in [-40, -90] {
            let info = WifiLinkInfo {
                links: vec![link(dbm, 0, 0)],
                quality_percent: None,
            };
            assert_eq!(info.rssi_dbm(), Some(f64::from(dbm)));
        }
    }

    // --- link rates --------------------------------------------------------

    #[test]
    fn refuses_an_implausible_link_rate() {
        // Above 100 Gbit/s is a unit confusion or an attribute read at the
        // wrong offset, not a very fast Wi-Fi link.
        assert!(!is_plausible_link_rate(200_000_000_000));
        assert!(!is_plausible_link_rate(0));
        assert!(is_plausible_link_rate(175_500_000));
        assert!(is_plausible_link_rate(1_200_000_000));
    }

    #[test]
    fn a_link_with_no_bitrate_still_publishes_its_signal() {
        // Some drivers report a signal and no negotiated rate.
        let info = WifiLinkInfo {
            links: vec![WifiLink {
                rssi_dbm: Some(-60),
                rssi_average_dbm: None,
                receive_bps: None,
                transmit_bps: None,
            }],
            quality_percent: None,
        };

        assert_eq!(info.rssi_dbm(), Some(-60.0));
        assert_eq!(info.receive_bps(), None);
        assert_eq!(info.transmit_bps(), None);
        assert!(info.is_connected());
    }

    #[test]
    fn asymmetric_receive_and_transmit_rates_are_kept_apart() {
        // The real machine negotiates 175.5 Mbit/s down and 390 Mbit/s up.
        let info = real();
        assert_ne!(info.receive_bps(), info.transmit_bps());
    }

    // --- disconnected ------------------------------------------------------

    #[test]
    fn a_disconnected_interface_reports_nothing_rather_than_zero() {
        let info = WifiLinkInfo::disconnected();

        assert!(!info.is_connected());
        assert_eq!(info.link_count(), 0);
        assert_eq!(info.rssi_dbm(), None);
        assert_eq!(info.receive_bps(), None);
        assert_eq!(info.transmit_bps(), None);
        assert_eq!(info.quality_percent(), None);
    }

    #[test]
    fn a_disconnected_interface_is_temporarily_unavailable_not_absent() {
        // The radio is present and working; there is simply nothing to measure
        // until it joins a network, and that resolves itself.
        let availability = not_connected();

        assert_eq!(availability.status_str(), "temporarilyUnavailable");
        assert!(availability.is_transient());
    }

    #[test]
    fn a_list_of_empty_links_counts_as_disconnected() {
        let info = WifiLinkInfo {
            links: vec![WifiLink::default(), WifiLink::default()],
            quality_percent: None,
        };

        assert!(!info.is_connected());
        assert_eq!(info.link_count(), 0);
        assert_eq!(info.rssi_dbm(), None);
    }

    // --- multi-link operation ---------------------------------------------

    #[test]
    fn the_strongest_link_is_published_rather_than_the_first() {
        // Wi-Fi 7 multi-link. Taking `links[0]` would report −85 dBm for a
        // radio that also holds a −45 dBm link.
        let info = WifiLinkInfo {
            links: vec![
                link(-85, 100_000_000, 100_000_000),
                link(-45, 900_000_000, 900_000_000),
                link(-70, 400_000_000, 400_000_000),
            ],
            quality_percent: None,
        };

        assert_eq!(info.link_count(), 3);
        assert_eq!(info.rssi_dbm(), Some(-45.0));
        // …and the rates published are that same link's, not another's.
        assert_eq!(info.receive_bps(), Some(900_000_000.0));
        assert_eq!(info.transmit_bps(), Some(900_000_000.0));
    }

    #[test]
    fn dbm_values_are_never_averaged() {
        // The mean of −50 and −90 is not −70 in any meaningful sense, because
        // the scale is logarithmic.
        let info = WifiLinkInfo {
            links: vec![link(-50, 0, 0), link(-90, 0, 0)],
            quality_percent: None,
        };

        assert_eq!(info.rssi_dbm(), Some(-50.0));
        assert_ne!(info.rssi_dbm(), Some(-70.0));
    }

    #[test]
    fn a_link_with_no_signal_does_not_win_over_one_with_a_weak_signal() {
        // A missing RSSI is not an infinitely weak one.
        let info = WifiLinkInfo {
            links: vec![
                WifiLink {
                    rssi_dbm: None,
                    rssi_average_dbm: None,
                    receive_bps: Some(1_000_000),
                    transmit_bps: None,
                },
                link(-88, 2_000_000, 2_000_000),
            ],
            quality_percent: None,
        };

        assert_eq!(info.rssi_dbm(), Some(-88.0));
        assert_eq!(info.receive_bps(), Some(2_000_000.0));
    }

    #[test]
    fn links_without_any_signal_still_surface_their_rates() {
        let info = WifiLinkInfo {
            links: vec![WifiLink {
                rssi_dbm: None,
                rssi_average_dbm: None,
                receive_bps: Some(866_700_000),
                transmit_bps: Some(866_700_000),
            }],
            quality_percent: None,
        };

        assert!(info.is_connected());
        assert_eq!(info.rssi_dbm(), None);
        assert_eq!(info.receive_bps(), Some(866_700_000.0));
    }

    #[test]
    fn the_drivers_smoothed_average_is_carried_but_not_published_as_the_rssi() {
        // The published figure is the instantaneous reading; the average is
        // kept for diagnostics so the two can never be confused.
        let info = real();

        assert_eq!(info.rssi_dbm(), Some(-68.0));
        assert_eq!(info.best_link().and_then(|l| l.rssi_average_dbm), Some(-67));
    }
}
