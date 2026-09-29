//! The part of the `overlays` configuration section the backend owns.
//!
//! The frontend owns the overlay's *contents* (widgets, layout, chrome). The
//! backend reads just enough to own the *windows*: which overlays exist, which
//! are visible and locked, and where they are. It also writes back the few
//! things only it knows — the position after a drag, the lock state after the
//! global shortcut — by editing those fields in place and leaving everything
//! else untouched.

use serde_json::{json, Value};

use super::geometry::OverlayGeometry;

/// Overlay ids: the same pattern the frontend generates. Window labels are
/// built from them, so nothing a user typed ever becomes a label.
pub fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub const LABEL_PREFIX: &str = "overlay-";

pub fn label_for(id: &str) -> String {
    format!("{LABEL_PREFIX}{id}")
}

pub fn id_from_label(label: &str) -> Option<&str> {
    label
        .strip_prefix(LABEL_PREFIX)
        .filter(|id| is_valid_id(id))
}

/// The title prefix of every overlay window: the compositor-visible marker
/// the GNOME Shell bridge (`integrations/gnome-shell/`) matches, together
/// with the PID of the process that owns PULSE's D-Bus name. Main
/// ("PULSE") and Mini ("PULSE Mini") never carry it.
pub const OVERLAY_TITLE_PREFIX: &str = "PULSE Overlay :: ";

/// An overlay window's title: the marker and the overlay's validated id —
/// never its user-typed name.
pub fn window_title(id: &str) -> String {
    format!("{OVERLAY_TITLE_PREFIX}{id}")
}

/// Whether `title` is exactly an overlay marker with a valid id.
pub fn is_overlay_title(title: &str) -> bool {
    title
        .strip_prefix(OVERLAY_TITLE_PREFIX)
        .is_some_and(is_valid_id)
}

/// The most overlay windows PULSE will create.
pub const MAX_OVERLAYS: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub struct OverlaySpec {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub geometry: OverlayGeometry,
}

fn number(value: &Value, fallback: f64) -> f64 {
    value.as_f64().filter(|v| v.is_finite()).unwrap_or(fallback)
}

/// Reads every valid overlay from the section. Invalid entries are skipped,
/// duplicates keep the first, and at most [`MAX_OVERLAYS`] are returned.
pub fn parse_overlays(section: Option<&Value>) -> Vec<OverlaySpec> {
    let Some(items) = section
        .and_then(|s| s.get("items"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut specs = Vec::new();
    for item in items {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !is_valid_id(id) || !seen.insert(id.to_string()) {
            continue;
        }
        let geometry = item.get("geometry").cloned().unwrap_or(Value::Null);
        let default = OverlayGeometry::DEFAULT;
        specs.push(OverlaySpec {
            id: id.to_string(),
            name: item
                .get("name")
                .and_then(Value::as_str)
                .map(|name| name.chars().take(40).collect())
                .unwrap_or_else(|| "Overlay".to_string()),
            visible: item.get("visible").and_then(Value::as_bool).unwrap_or(true),
            locked: item.get("locked").and_then(Value::as_bool).unwrap_or(false),
            geometry: OverlayGeometry {
                monitor: geometry
                    .get("monitor")
                    .and_then(Value::as_str)
                    .map(String::from),
                x: number(&geometry["x"], default.x),
                y: number(&geometry["y"], default.y),
                width: number(&geometry["width"], default.width),
                height: number(&geometry["height"], default.height),
            }
            .sanitized(),
        });
        if specs.len() == MAX_OVERLAYS {
            break;
        }
    }
    specs
}

fn items_mut(section: &mut Value) -> Option<&mut Vec<Value>> {
    section.get_mut("items").and_then(Value::as_array_mut)
}

/// Sets `locked` on every overlay. Returns whether anything changed.
pub fn set_all_locked(section: &mut Value, locked: bool) -> bool {
    let mut changed = false;
    if let Some(items) = items_mut(section) {
        for item in items {
            if item.get("locked").and_then(Value::as_bool) != Some(locked) {
                item["locked"] = json!(locked);
                changed = true;
            }
        }
    }
    changed
}

/// Sets `visible` on every overlay.
pub fn set_all_visible(section: &mut Value, visible: bool) -> bool {
    let mut changed = false;
    if let Some(items) = items_mut(section) {
        for item in items {
            if item.get("visible").and_then(Value::as_bool) != Some(visible) {
                item["visible"] = json!(visible);
                changed = true;
            }
        }
    }
    changed
}

/// The global shortcut's action: if any overlay is in Edit mode, lock them
/// all; otherwise unlock them all.
pub fn toggle_all_locked(section: &mut Value) -> Option<bool> {
    let specs = parse_overlays(Some(section));
    if specs.is_empty() {
        return None;
    }
    let lock = specs.iter().any(|spec| !spec.locked);
    set_all_locked(section, lock);
    Some(lock)
}

/// How many overlays are configured visible.
pub fn visible_count(section: Option<&Value>) -> usize {
    parse_overlays(section)
        .iter()
        .filter(|spec| spec.visible)
        .count()
}

/// Whether every overlay is hidden (for the tray's Show/Hide toggle).
pub fn any_visible(section: Option<&Value>) -> bool {
    parse_overlays(section).iter().any(|spec| spec.visible)
}

/// Writes a captured geometry into one overlay, leaving the rest alone.
pub fn set_geometry(
    section: &mut Value,
    id: &str,
    geometry: &OverlayGeometry,
    keep_position: bool,
) -> bool {
    let Some(items) = items_mut(section) else {
        return false;
    };
    let Some(item) = items
        .iter_mut()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
    else {
        return false;
    };
    let previous = item.get("geometry").cloned().unwrap_or(Value::Null);
    let mut next = json!({
        "monitor": geometry.monitor,
        "x": geometry.x,
        "y": geometry.y,
        "width": geometry.width,
        "height": geometry.height,
    });
    if keep_position {
        // On a system that cannot report positions (Wayland), only the size is
        // known: never overwrite a stored position with a made-up one.
        for field in ["monitor", "x", "y"] {
            next[field] = previous.get(field).cloned().unwrap_or(Value::Null);
        }
    }
    if previous == next {
        return false;
    }
    item["geometry"] = next;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> Value {
        json!({
            "version": 1,
            "items": [
                { "id": "o-aaa", "name": "CPU", "visible": true, "locked": false,
                  "geometry": { "monitor": "eDP-1", "x": 10, "y": 20, "width": 200, "height": 40 },
                  "widgets": [{ "kept": true }] },
                { "id": "o-bbb", "visible": false, "locked": true },
                { "id": "Bad Id; rm -rf", "visible": true },
                { "id": "o-aaa", "name": "duplicate" },
                { "name": "no id" }
            ]
        })
    }

    #[test]
    fn ids_and_labels_are_validated() {
        assert!(is_valid_id("o-abc123"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("-leading"));
        assert!(!is_valid_id("UPPER"));
        assert!(!is_valid_id("../../etc"));
        assert!(!is_valid_id(&"a".repeat(41)));
        assert_eq!(label_for("o-abc"), "overlay-o-abc");
        assert_eq!(id_from_label("overlay-o-abc"), Some("o-abc"));
        assert_eq!(id_from_label("overlay-Evil Label"), None);
        assert_eq!(id_from_label("main"), None);
    }

    #[test]
    fn overlay_window_titles_carry_an_exact_marker() {
        assert_eq!(window_title("o-abc123"), "PULSE Overlay :: o-abc123");
        assert!(is_overlay_title(&window_title("o-abc123")));
        for other in [
            "PULSE",
            "PULSE Mini",
            "PULSE — HUD",
            "PULSE Overlay :: ",
            "PULSE Overlay :: Bad Id",
            "Firefox — PULSE Overlay :: o-abc",
            "PULSE Overlay :: o-abc; rm",
        ] {
            assert!(!is_overlay_title(other), "{other}");
        }
    }

    #[test]
    fn parsing_keeps_valid_unique_overlays_only() {
        let specs = parse_overlays(Some(&section()));
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].name, "CPU");
        assert_eq!(specs[0].geometry.monitor.as_deref(), Some("eDP-1"));
        assert!(!specs[1].visible && specs[1].locked);
        assert_eq!(specs[1].geometry, OverlayGeometry::DEFAULT);
        assert!(parse_overlays(None).is_empty());
        assert!(parse_overlays(Some(&json!({ "items": "nope" }))).is_empty());
    }

    #[test]
    fn at_most_sixteen_overlay_windows() {
        let items: Vec<Value> = (0..40).map(|i| json!({ "id": format!("o-{i}") })).collect();
        assert_eq!(
            parse_overlays(Some(&json!({ "items": items }))).len(),
            MAX_OVERLAYS
        );
    }

    #[test]
    fn the_shortcut_locks_all_if_any_is_editable_else_unlocks_all() {
        let mut value = section();
        assert_eq!(toggle_all_locked(&mut value), Some(true));
        assert!(parse_overlays(Some(&value)).iter().all(|s| s.locked));
        assert_eq!(toggle_all_locked(&mut value), Some(false));
        assert!(parse_overlays(Some(&value)).iter().all(|s| !s.locked));
        assert_eq!(toggle_all_locked(&mut json!({ "items": [] })), None);
    }

    #[test]
    fn show_and_hide_all_keep_the_configuration() {
        let mut value = section();
        assert!(set_all_visible(&mut value, false));
        assert!(!any_visible(Some(&value)));
        assert_eq!(value["items"][0]["widgets"][0]["kept"], true);
        assert!(set_all_visible(&mut value, true));
        assert!(
            !set_all_visible(&mut value, true),
            "no change the second time"
        );
    }

    #[test]
    fn a_captured_geometry_touches_only_its_overlay() {
        let mut value = section();
        let geometry = OverlayGeometry {
            monitor: Some("HDMI-1".into()),
            x: 5.0,
            y: 6.0,
            width: 300.0,
            height: 50.0,
        };
        assert!(set_geometry(&mut value, "o-aaa", &geometry, false));
        assert_eq!(value["items"][0]["geometry"]["monitor"], "HDMI-1");
        assert_eq!(value["items"][0]["widgets"][0]["kept"], true);
        assert!(
            !set_geometry(&mut value, "o-aaa", &geometry, false),
            "idempotent"
        );
        assert!(!set_geometry(&mut value, "o-zzz", &geometry, false));
    }

    #[test]
    fn without_position_facts_only_the_size_is_written() {
        let mut value = section();
        let size_only = OverlayGeometry {
            monitor: None,
            x: 0.0,
            y: 0.0,
            width: 250.0,
            height: 44.0,
        };
        assert!(set_geometry(&mut value, "o-aaa", &size_only, true));
        let geometry = &value["items"][0]["geometry"];
        assert_eq!(
            (geometry["x"].as_f64(), geometry["y"].as_f64()),
            (Some(10.0), Some(20.0))
        );
        assert_eq!(geometry["monitor"], "eDP-1");
        assert_eq!(geometry["width"], 250.0);
    }
}
