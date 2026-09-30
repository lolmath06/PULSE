//! Overlay input mode: which overlays let the pointer through, and when the
//! native state is (re)applied.
//!
//! A Locked overlay is **click-through**: pointer events go to whatever is
//! below it (Firefox, a game) as if the overlay were not there. An overlay in
//! Edit is **interactive**. That native state is a side effect on a real window
//! (on Linux, the GTK input shape; see `crate::overlay_native`), so this module
//! only decides *when* to apply *which* mode, and the side effect is injected —
//! the tests below drive a fake and prove nothing physical.
//!
//! The rules:
//!
//! - only overlay windows are ever handled; the main window and Mini never
//!   get an input mode;
//! - the desired mode follows the lock state and nothing else: focus moving to
//!   another application, or the compositor companion re-asserting keep-above,
//!   never changes it;
//! - it is applied when it changes, when a window is created (or re-created),
//!   and when the native window is mapped again — a native surface can be
//!   replaced, and a new one must not start interactive — and retried on the
//!   next reconciliation if applying it failed. There is no timer and no
//!   polling.

use std::collections::HashMap;
use std::sync::Mutex;

use super::spec::id_from_label;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayInputMode {
    /// Edit: the overlay takes pointer input (drag, resize, its controls).
    Interactive,
    /// Locked: pointer input goes to the windows below.
    ClickThrough,
}

impl OverlayInputMode {
    pub fn for_locked(locked: bool) -> Self {
        if locked {
            Self::ClickThrough
        } else {
            Self::Interactive
        }
    }
}

/// Something that happened to an overlay window that might concern its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// A window was built for the overlay: the first one, or a replacement.
    Created { locked: bool },
    /// The configuration was reconciled; this is the overlay's lock state.
    Reconciled { locked: bool },
    /// The native window was mapped again (its surface may be new).
    Mapped,
    /// Focus moved to or away from the overlay.
    FocusChanged,
    /// The compositor companion re-asserted keep-above.
    AboveReasserted,
    /// The window is gone.
    Destroyed,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    desired: OverlayInputMode,
    /// What the native window was last successfully given.
    applied: Option<OverlayInputMode>,
}

/// The desired input mode of every overlay window, by window label.
#[derive(Debug, Default)]
pub struct OverlayInputs {
    entries: HashMap<String, Entry>,
}

impl OverlayInputs {
    /// Updates the desired state and returns the mode to apply natively now,
    /// if any.
    pub fn on_event(&mut self, label: &str, event: InputEvent) -> Option<OverlayInputMode> {
        id_from_label(label)?;
        match event {
            InputEvent::Created { locked } => {
                let desired = OverlayInputMode::for_locked(locked);
                self.entries.insert(
                    label.to_string(),
                    Entry {
                        desired,
                        applied: None,
                    },
                );
                Some(desired)
            }
            InputEvent::Reconciled { locked } => {
                let desired = OverlayInputMode::for_locked(locked);
                let entry = self.entries.entry(label.to_string()).or_insert(Entry {
                    desired,
                    applied: None,
                });
                entry.desired = desired;
                (entry.applied != Some(desired)).then_some(desired)
            }
            InputEvent::Mapped => {
                let entry = self.entries.get_mut(label)?;
                entry.applied = None;
                Some(entry.desired)
            }
            InputEvent::FocusChanged | InputEvent::AboveReasserted => None,
            InputEvent::Destroyed => {
                self.entries.remove(label);
                None
            }
        }
    }

    /// Records the outcome of applying `mode`. A failure leaves the window
    /// "not applied", so the next reconciliation tries again.
    pub fn record(&mut self, label: &str, mode: OverlayInputMode, ok: bool) {
        if let Some(entry) = self.entries.get_mut(label) {
            if entry.desired == mode {
                entry.applied = ok.then_some(mode);
            }
        }
    }

    pub fn desired(&self, label: &str) -> Option<OverlayInputMode> {
        self.entries.get(label).map(|entry| entry.desired)
    }
}

/// Plans, applies and records one event. `apply` is the native side effect;
/// the lock on `inputs` is never held while it runs, because on Linux it waits
/// for the main thread, which may itself be reporting a `Mapped` event.
pub fn handle_event<T>(
    inputs: &Mutex<OverlayInputs>,
    label: &str,
    event: InputEvent,
    apply: impl FnOnce(OverlayInputMode) -> Result<T, String>,
) -> Option<(OverlayInputMode, Result<T, String>)> {
    let lock = || {
        inputs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    };
    let mode = lock().on_event(label, event)?;
    let result = apply(mode);
    lock().record(label, mode, result.is_ok());
    Some((mode, result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use OverlayInputMode::{ClickThrough, Interactive};

    const A: &str = "overlay-a";
    const B: &str = "overlay-b";

    /// A fake native backend: remembers every call and each window's state.
    #[derive(Default)]
    struct Fake {
        calls: Vec<(String, OverlayInputMode)>,
        state: HashMap<String, OverlayInputMode>,
        fail: bool,
    }

    fn send(inputs: &Mutex<OverlayInputs>, fake: &mut Fake, label: &str, event: InputEvent) {
        handle_event(inputs, label, event, |mode| {
            fake.calls.push((label.to_string(), mode));
            if fake.fail {
                return Err("refused".into());
            }
            fake.state.insert(label.to_string(), mode);
            Ok(())
        });
    }

    fn modes(fake: &Fake) -> Vec<OverlayInputMode> {
        fake.calls.iter().map(|(_, mode)| *mode).collect()
    }

    #[test]
    fn locked_requests_click_through_and_edit_requests_interactive() {
        let inputs = Mutex::default();
        let mut fake = Fake::default();
        send(&inputs, &mut fake, A, InputEvent::Created { locked: true });
        assert_eq!(fake.state[A], ClickThrough);
        send(
            &inputs,
            &mut fake,
            A,
            InputEvent::Reconciled { locked: false },
        );
        assert_eq!(fake.state[A], Interactive);
    }

    #[test]
    fn lock_edit_lock_applies_exactly_three_transitions() {
        let inputs = Mutex::default();
        let mut fake = Fake::default();
        send(&inputs, &mut fake, A, InputEvent::Created { locked: true });
        // An unrelated reconciliation (a geometry save) changes nothing.
        send(
            &inputs,
            &mut fake,
            A,
            InputEvent::Reconciled { locked: true },
        );
        send(
            &inputs,
            &mut fake,
            A,
            InputEvent::Reconciled { locked: false },
        );
        send(
            &inputs,
            &mut fake,
            A,
            InputEvent::Reconciled { locked: false },
        );
        send(
            &inputs,
            &mut fake,
            A,
            InputEvent::Reconciled { locked: true },
        );
        assert_eq!(modes(&fake), [ClickThrough, Interactive, ClickThrough]);
    }

    #[test]
    fn a_recreated_window_gets_the_mode_again() {
        let inputs = Mutex::default();
        let mut fake = Fake::default();
        send(&inputs, &mut fake, A, InputEvent::Created { locked: true });
        send(&inputs, &mut fake, A, InputEvent::Destroyed);
        send(&inputs, &mut fake, A, InputEvent::Created { locked: true });
        assert_eq!(modes(&fake), [ClickThrough, ClickThrough]);

        send(&inputs, &mut fake, A, InputEvent::Destroyed);
        send(&inputs, &mut fake, A, InputEvent::Created { locked: false });
        assert_eq!(fake.state[A], Interactive);
    }

    #[test]
    fn a_remapped_surface_gets_the_desired_mode_again() {
        let inputs = Mutex::default();
        let mut fake = Fake::default();
        send(&inputs, &mut fake, A, InputEvent::Created { locked: true });
        send(&inputs, &mut fake, A, InputEvent::Mapped);
        assert_eq!(modes(&fake), [ClickThrough, ClickThrough]);
    }

    #[test]
    fn focus_and_keep_above_never_change_the_input_mode() {
        let inputs = Mutex::default();
        let mut fake = Fake::default();
        send(&inputs, &mut fake, A, InputEvent::Created { locked: true });
        for _ in 0..3 {
            send(&inputs, &mut fake, A, InputEvent::FocusChanged);
            send(&inputs, &mut fake, A, InputEvent::AboveReasserted);
        }
        assert_eq!(modes(&fake), [ClickThrough]);
        assert_eq!(inputs.lock().unwrap().desired(A), Some(ClickThrough));
    }

    #[test]
    fn main_and_mini_are_never_handled() {
        let inputs = Mutex::default();
        let mut fake = Fake::default();
        for label in ["main", "mini"] {
            send(
                &inputs,
                &mut fake,
                label,
                InputEvent::Created { locked: true },
            );
            send(
                &inputs,
                &mut fake,
                label,
                InputEvent::Reconciled { locked: true },
            );
            send(&inputs, &mut fake, label, InputEvent::Mapped);
        }
        assert!(fake.calls.is_empty());
    }

    #[test]
    fn each_overlay_keeps_its_own_mode() {
        let inputs = Mutex::default();
        let mut fake = Fake::default();
        send(&inputs, &mut fake, A, InputEvent::Created { locked: true });
        send(&inputs, &mut fake, B, InputEvent::Created { locked: false });
        send(&inputs, &mut fake, B, InputEvent::Mapped);
        assert_eq!(fake.state[A], ClickThrough);
        assert_eq!(fake.state[B], Interactive);
    }

    #[test]
    fn a_failed_apply_is_retried_on_the_next_reconciliation() {
        let inputs = Mutex::default();
        let mut fake = Fake {
            fail: true,
            ..Fake::default()
        };
        send(&inputs, &mut fake, A, InputEvent::Created { locked: true });
        fake.fail = false;
        send(
            &inputs,
            &mut fake,
            A,
            InputEvent::Reconciled { locked: true },
        );
        send(
            &inputs,
            &mut fake,
            A,
            InputEvent::Reconciled { locked: true },
        );
        assert_eq!(modes(&fake), [ClickThrough, ClickThrough]);
        assert_eq!(fake.state[A], ClickThrough);
    }
}
