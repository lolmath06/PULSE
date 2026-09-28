//! Where an overlay goes, across monitors and scale factors.
//!
//! An overlay's position is stored **relative to a monitor, in logical
//! pixels**: "on HDMI-1, 40 × 40 from its top-left corner, 320 × 60". Logical
//! pixels are what the user sees — the same overlay is the same size on a
//! 100 % and a 200 % screen — and the monitor makes the position survive a
//! rearrangement of the other screens.
//!
//! Converting to the window system's physical, desktop-absolute coordinates
//! happens only here, with the target monitor's own scale factor. Two rules
//! keep the overlay reachable:
//!
//! - a monitor that is gone (unplugged, renamed) falls back to the primary
//!   one, and the placement says it was **recovered**;
//! - the window is clamped inside its monitor, and never larger than it — an
//!   overlay can never be left entirely off screen.

use serde::{Deserialize, Serialize};

/// One monitor, as the window system reports it: physical pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub name: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl MonitorInfo {
    fn scale(&self) -> f64 {
        if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        }
    }

    fn logical_width(&self) -> f64 {
        f64::from(self.width) / self.scale()
    }

    fn logical_height(&self) -> f64 {
        f64::from(self.height) / self.scale()
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= f64::from(self.x)
            && y >= f64::from(self.y)
            && x < f64::from(self.x) + f64::from(self.width)
            && y < f64::from(self.y) + f64::from(self.height)
    }
}

/// An overlay's stored geometry: monitor-relative, logical pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayGeometry {
    /// The monitor's name when known.
    pub monitor: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub const MIN_SIZE: (f64, f64) = (24.0, 16.0);
pub const MAX_SIZE: (f64, f64) = (4000.0, 3000.0);

impl OverlayGeometry {
    pub const DEFAULT: OverlayGeometry = OverlayGeometry {
        monitor: None,
        x: 40.0,
        y: 40.0,
        width: 320.0,
        height: 64.0,
    };

    /// Bounds every number; NaN or negative sizes become the default.
    pub fn sanitized(&self) -> OverlayGeometry {
        let pick = |value: f64, fallback: f64, min: f64, max: f64| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        OverlayGeometry {
            monitor: self
                .monitor
                .as_ref()
                .map(|name| name.chars().take(128).collect()),
            x: pick(self.x, Self::DEFAULT.x, -100_000.0, 100_000.0),
            y: pick(self.y, Self::DEFAULT.y, -100_000.0, 100_000.0),
            width: pick(self.width, Self::DEFAULT.width, MIN_SIZE.0, MAX_SIZE.0),
            height: pick(self.height, Self::DEFAULT.height, MIN_SIZE.1, MAX_SIZE.1),
        }
    }
}

/// Where the window system should put the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// Physical, desktop-absolute position.
    pub physical_x: i32,
    pub physical_y: i32,
    /// Logical size.
    pub width: f64,
    pub height: f64,
    pub monitor: Option<String>,
    /// The stored monitor was missing (or the window was off screen) and the
    /// overlay was brought back onto a visible one.
    pub recovered: bool,
}

/// Places `geometry` on the current monitors.
///
/// `None` when no monitor is known at all — the caller then lets the window
/// system choose.
pub fn place(
    geometry: &OverlayGeometry,
    monitors: &[MonitorInfo],
    primary: Option<usize>,
) -> Option<Placement> {
    if monitors.is_empty() {
        return None;
    }
    let geometry = geometry.sanitized();
    let named = geometry.monitor.as_ref().and_then(|name| {
        monitors
            .iter()
            .position(|m| m.name.as_deref() == Some(name.as_str()))
    });
    let index = named
        .or(primary.filter(|index| *index < monitors.len()))
        .unwrap_or(0);
    let monitor = &monitors[index];
    let mut recovered = geometry.monitor.is_some() && named.is_none();

    let width = geometry.width.min(monitor.logical_width()).max(MIN_SIZE.0);
    let height = geometry
        .height
        .min(monitor.logical_height())
        .max(MIN_SIZE.1);
    let max_x = (monitor.logical_width() - width).max(0.0);
    let max_y = (monitor.logical_height() - height).max(0.0);
    let x = geometry.x.clamp(0.0, max_x);
    let y = geometry.y.clamp(0.0, max_y);
    if (x - geometry.x).abs() > 0.5 || (y - geometry.y).abs() > 0.5 {
        recovered = true;
    }

    let scale = monitor.scale();
    Some(Placement {
        physical_x: monitor.x + (x * scale).round() as i32,
        physical_y: monitor.y + (y * scale).round() as i32,
        width,
        height,
        monitor: monitor.name.clone(),
        recovered,
    })
}

/// Records where a window is: the monitor holding its centre (or the nearest
/// one), and its position relative to that monitor in logical pixels.
pub fn capture(
    physical_x: i32,
    physical_y: i32,
    logical_width: f64,
    logical_height: f64,
    monitors: &[MonitorInfo],
) -> OverlayGeometry {
    let fallback = || OverlayGeometry {
        monitor: None,
        x: f64::from(physical_x),
        y: f64::from(physical_y),
        width: logical_width,
        height: logical_height,
    };
    if monitors.is_empty() {
        return fallback().sanitized();
    }
    let first_scale = monitors[0].scale();
    let centre = |monitor: &MonitorInfo| {
        let scale = monitor.scale();
        (
            f64::from(physical_x) + logical_width * scale / 2.0,
            f64::from(physical_y) + logical_height * scale / 2.0,
        )
    };
    let monitor = monitors
        .iter()
        .find(|monitor| {
            let (cx, cy) = centre(monitor);
            monitor.contains(cx, cy)
        })
        .unwrap_or_else(|| {
            let (cx, cy) = (
                f64::from(physical_x) + logical_width * first_scale / 2.0,
                f64::from(physical_y) + logical_height * first_scale / 2.0,
            );
            monitors
                .iter()
                .min_by(|a, b| {
                    let distance = |m: &MonitorInfo| {
                        let mx = f64::from(m.x) + f64::from(m.width) / 2.0;
                        let my = f64::from(m.y) + f64::from(m.height) / 2.0;
                        (mx - cx).powi(2) + (my - cy).powi(2)
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .expect("non-empty")
        });
    let scale = monitor.scale();
    OverlayGeometry {
        monitor: monitor.name.clone(),
        x: f64::from(physical_x - monitor.x) / scale,
        y: f64::from(physical_y - monitor.y) / scale,
        width: logical_width,
        height: logical_height,
    }
    .sanitized()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, x: i32, y: i32, width: u32, height: u32, scale: f64) -> MonitorInfo {
        MonitorInfo {
            name: Some(name.into()),
            x,
            y,
            width,
            height,
            scale,
        }
    }

    fn geometry(monitor: Option<&str>, x: f64, y: f64, width: f64, height: f64) -> OverlayGeometry {
        OverlayGeometry {
            monitor: monitor.map(String::from),
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn logical_positions_scale_with_each_monitor() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let screens = [monitor("eDP-1", 0, 0, 2560, 1600, scale)];
            let placement = place(
                &geometry(Some("eDP-1"), 40.0, 40.0, 320.0, 64.0),
                &screens,
                Some(0),
            )
            .expect("placed");
            assert_eq!(
                placement.physical_x,
                (40.0 * scale).round() as i32,
                "scale {scale}"
            );
            assert_eq!(placement.width, 320.0, "size stays logical");
            assert!(!placement.recovered);

            let back = capture(
                placement.physical_x,
                placement.physical_y,
                320.0,
                64.0,
                &screens,
            );
            assert!(
                (back.x - 40.0).abs() < 1.0,
                "round trip at {scale}: {back:?}"
            );
            assert_eq!(back.monitor.as_deref(), Some("eDP-1"));
        }
    }

    #[test]
    fn a_second_monitor_with_another_scale_is_respected() {
        let screens = [
            monitor("eDP-1", 0, 0, 2560, 1600, 2.0),
            monitor("HDMI-1", 2560, 0, 1920, 1080, 1.0),
        ];
        let placement = place(
            &geometry(Some("HDMI-1"), 100.0, 50.0, 300.0, 60.0),
            &screens,
            Some(0),
        )
        .expect("placed");
        assert_eq!((placement.physical_x, placement.physical_y), (2660, 50));

        let back = capture(2660, 50, 300.0, 60.0, &screens);
        assert_eq!(back.monitor.as_deref(), Some("HDMI-1"));
        assert_eq!((back.x, back.y), (100.0, 50.0));
    }

    #[test]
    fn a_missing_monitor_falls_back_to_the_primary_and_says_so() {
        let screens = [monitor("eDP-1", 0, 0, 1920, 1080, 1.0)];
        let placement = place(
            &geometry(Some("HDMI-9"), 1500.0, 900.0, 320.0, 64.0),
            &screens,
            Some(0),
        )
        .expect("placed");
        assert!(placement.recovered);
        assert_eq!(placement.monitor.as_deref(), Some("eDP-1"));
        assert!(placement.physical_x + 320 <= 1920);
    }

    #[test]
    fn an_off_screen_overlay_is_brought_back() {
        let screens = [monitor("eDP-1", 0, 0, 1920, 1080, 1.0)];
        let placement = place(
            &geometry(Some("eDP-1"), 5000.0, -300.0, 320.0, 64.0),
            &screens,
            Some(0),
        )
        .expect("placed");
        assert!(placement.recovered);
        assert_eq!(placement.physical_x, 1920 - 320);
        assert_eq!(placement.physical_y, 0);
    }

    #[test]
    fn an_overlay_is_never_larger_than_its_monitor() {
        let screens = [monitor("small", 0, 0, 800, 600, 1.0)];
        let placement =
            place(&geometry(None, 0.0, 0.0, 3000.0, 2000.0), &screens, Some(0)).expect("placed");
        assert_eq!((placement.width, placement.height), (800.0, 600.0));
    }

    #[test]
    fn garbage_geometry_is_sanitized() {
        let bad = geometry(None, f64::NAN, f64::INFINITY, -5.0, 1e12).sanitized();
        assert_eq!(bad.x, OverlayGeometry::DEFAULT.x);
        assert_eq!(bad.y, OverlayGeometry::DEFAULT.y);
        assert_eq!(bad.width, MIN_SIZE.0);
        assert_eq!(bad.height, MAX_SIZE.1);
    }

    #[test]
    fn no_monitors_lets_the_window_system_choose() {
        assert!(place(&OverlayGeometry::DEFAULT, &[], None).is_none());
        let captured = capture(10, 20, 100.0, 50.0, &[]);
        assert_eq!((captured.x, captured.y), (10.0, 20.0));
    }

    #[test]
    fn a_window_between_monitors_belongs_to_the_one_holding_its_centre() {
        let screens = [
            monitor("left", 0, 0, 1920, 1080, 1.0),
            monitor("right", 1920, 0, 1920, 1080, 1.0),
        ];
        let captured = capture(1800, 100, 300.0, 60.0, &screens);
        assert_eq!(captured.monitor.as_deref(), Some("right"));
        assert_eq!(captured.x, -120.0);
        let placement = place(&captured, &screens, Some(0)).expect("placed");
        assert_eq!(placement.physical_x, 1920, "clamped inside its monitor");
    }
}
