//! The Windows native overlay window: extended styles and Z-order for an
//! overlay's Edit / Locked state.
//!
//! **Implemented and compiled; NOT physically verified on a Windows machine.**
//! The Windows harness (`pnpm rust:windows`) type-checks the FFI below for
//! `x86_64-pc-windows-msvc`; the style arithmetic is unit-tested on every
//! host. See `docs/overlay/windows-native.md`.
//!
//! | state                | extended styles                                                    |
//! | -------------------- | ------------------------------------------------------------------ |
//! | always               | `WS_EX_TOOLWINDOW` (no taskbar button, no Alt+Tab), no `APPWINDOW` |
//! | keep above           | `WS_EX_TOPMOST` + `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE)`     |
//! | click-through        | `WS_EX_LAYERED` + `WS_EX_TRANSPARENT` (hit-testing skips it)       |
//! | not focusable        | `WS_EX_NOACTIVATE` (clicks and showing never take focus)           |
//!
//! Why a subclass: tao keeps its own window flags and rewrites the whole
//! `GWL_EXSTYLE` from them whenever one changes (`SetWindowLongW` in
//! `WindowFlags::apply_diff`). A bit set directly would be silently dropped on
//! the next change — the Windows twin of the GTK input-shape bug fixed in
//! Phase 11.5B. The overlay window is therefore subclassed
//! (`SetWindowSubclass`), and every `WM_STYLECHANGING` for `GWL_EXSTYLE`,
//! whoever sends it, gets PULSE's managed bits enforced. PULSE also drives
//! tao's own flags the same way (`set_ignore_cursor_events`, `set_focusable`,
//! `set_always_on_top`), so the two never disagree.
//!
//! `WS_EX_LAYERED` is set without `SetLayeredWindowAttributes`, exactly as tao
//! does for its own click-through: tao's transparent windows are
//! `WS_EX_NOREDIRECTIONBITMAP` and WebView2 draws through DirectComposition,
//! so there is no redirection bitmap for layered attributes to govern.

pub const WS_EX_TOPMOST: u32 = 0x0000_0008;
pub const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

/// Every extended-style bit PULSE decides on an overlay window. All other
/// bits (`WS_EX_NOREDIRECTIONBITMAP`, `WS_EX_WINDOWEDGE`, …) stay tao's.
pub const MANAGED: u32 = WS_EX_TOPMOST
    | WS_EX_TRANSPARENT
    | WS_EX_TOOLWINDOW
    | WS_EX_APPWINDOW
    | WS_EX_LAYERED
    | WS_EX_NOACTIVATE;

/// An overlay window's state, as the Win32 layer needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayWindowState {
    pub click_through: bool,
    pub focusable: bool,
    pub topmost: bool,
}

/// The managed bits `state` requires.
pub fn required_bits(state: OverlayWindowState) -> u32 {
    let mut bits = WS_EX_TOOLWINDOW;
    if state.topmost {
        bits |= WS_EX_TOPMOST;
    }
    if state.click_through {
        bits |= WS_EX_LAYERED | WS_EX_TRANSPARENT;
    }
    if !state.focusable {
        bits |= WS_EX_NOACTIVATE;
    }
    bits
}

/// `style` with the managed bits set exactly as `state` requires and every
/// other bit untouched.
pub fn enforce(style: u32, state: OverlayWindowState) -> u32 {
    (style & !MANAGED) | required_bits(state)
}

/// Whether a read-back style carries exactly the managed bits `state` needs.
pub fn verified(style: u32, state: OverlayWindowState) -> bool {
    style & MANAGED == required_bits(state)
}

/// The state, packed into a subclass's reference data (three bits).
pub fn encode(state: OverlayWindowState) -> usize {
    usize::from(state.click_through)
        | usize::from(state.focusable) << 1
        | usize::from(state.topmost) << 2
}

pub fn decode(data: usize) -> OverlayWindowState {
    OverlayWindowState {
        click_through: data & 1 != 0,
        focusable: data & 2 != 0,
        topmost: data & 4 != 0,
    }
}

/// The managed bits of `style`, named — for the diagnostic line.
pub fn describe(style: u32) -> String {
    let names = [
        (WS_EX_TOPMOST, "TOPMOST"),
        (WS_EX_LAYERED, "LAYERED"),
        (WS_EX_TRANSPARENT, "TRANSPARENT"),
        (WS_EX_NOACTIVATE, "NOACTIVATE"),
        (WS_EX_TOOLWINDOW, "TOOLWINDOW"),
        (WS_EX_APPWINDOW, "APPWINDOW"),
    ];
    let set: Vec<&str> = names
        .iter()
        .filter(|(bit, _)| style & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if set.is_empty() {
        "none".into()
    } else {
        set.join(" | ")
    }
}

/// Applies `state` to the overlay window `hwnd` (raw handle) and reads the
/// extended style back. Must run on the window's own thread — the UI thread.
#[cfg(target_os = "windows")]
pub fn apply(hwnd: isize, state: OverlayWindowState) -> Result<u32, String> {
    native::apply(hwnd, state)
}

#[cfg(target_os = "windows")]
mod native {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_NOTOPMOST,
        HWND_TOPMOST, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER,
        SWP_NOSIZE, WM_NCDESTROY, WM_STYLECHANGING,
    };

    use super::{decode, encode, enforce, OverlayWindowState};

    /// "PULS": the subclass id, unique to this proc.
    const SUBCLASS_ID: usize = 0x5055_4c53;

    /// Enforces the overlay's managed extended-style bits on every change,
    /// whoever makes it; removes itself when the window is destroyed.
    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        if message == WM_STYLECHANGING && wparam as i32 == GWL_EXSTYLE && lparam != 0 {
            // SAFETY: for WM_STYLECHANGING, lparam points to a STYLESTRUCT the
            // system owns for the duration of the message, and styleNew may
            // be changed (documented).
            let change = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
            change.styleNew = enforce(change.styleNew, decode(data));
        } else if message == WM_NCDESTROY {
            // SAFETY: removing our own subclass from the window being destroyed.
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
        }
        // SAFETY: forwarding the message down the subclass chain.
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    pub fn apply(raw: isize, state: OverlayWindowState) -> Result<u32, String> {
        let hwnd = raw as HWND;
        if hwnd.is_null() {
            return Err("no window handle".into());
        }
        // SAFETY: `hwnd` is a live top-level window owned by this process and
        // this runs on its thread (the caller's contract). SetWindowSubclass
        // with an existing (proc, id) pair only updates the reference data.
        unsafe {
            if SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, encode(state)) == 0 {
                return Err("SetWindowSubclass failed".into());
            }
            let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let wanted = enforce(current, state);
            if current != wanted {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted as isize);
            }
            // Re-assert the Z-order band, never activating, and let the frame
            // pick up the style change.
            let after = if state.topmost {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            };
            SetWindowPos(
                hwnd,
                after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
            );
            Ok(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;
    const WS_EX_WINDOWEDGE: u32 = 0x0000_0100;

    const EDIT: OverlayWindowState = OverlayWindowState {
        click_through: false,
        focusable: true,
        topmost: true,
    };
    const LOCKED: OverlayWindowState = OverlayWindowState {
        click_through: true,
        focusable: false,
        topmost: true,
    };

    #[test]
    fn locked_is_click_through_no_activate_topmost_and_out_of_alt_tab() {
        let bits = required_bits(LOCKED);
        for bit in [
            WS_EX_TOPMOST,
            WS_EX_LAYERED,
            WS_EX_TRANSPARENT,
            WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW,
        ] {
            assert_ne!(bits & bit, 0, "{}", describe(bit));
        }
        assert_eq!(bits & WS_EX_APPWINDOW, 0);
    }

    #[test]
    fn edit_takes_the_pointer_and_focus_but_stays_on_top() {
        let bits = required_bits(EDIT);
        assert_eq!(
            bits & (WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE),
            0
        );
        assert_ne!(bits & WS_EX_TOPMOST, 0);
        assert_ne!(bits & WS_EX_TOOLWINDOW, 0);
    }

    #[test]
    fn enforcing_keeps_every_bit_that_is_not_ours() {
        // What tao writes for a transparent, undecorated, focusable window.
        let tao = WS_EX_NOREDIRECTIONBITMAP | WS_EX_WINDOWEDGE | WS_EX_APPWINDOW;
        let locked = enforce(tao, LOCKED);
        assert_eq!(
            locked & WS_EX_NOREDIRECTIONBITMAP,
            WS_EX_NOREDIRECTIONBITMAP
        );
        assert_eq!(locked & WS_EX_WINDOWEDGE, WS_EX_WINDOWEDGE);
        assert_eq!(locked & WS_EX_APPWINDOW, 0, "never a taskbar button");
        assert!(verified(locked, LOCKED));
        assert!(!verified(locked, EDIT));
    }

    #[test]
    fn lock_edit_lock_is_exact_whatever_tao_rewrites_in_between() {
        // tao rewrites the whole style from its own flags between transitions;
        // the subclass runs `enforce` on each rewrite.
        let tao_rewrite = WS_EX_NOREDIRECTIONBITMAP | WS_EX_WINDOWEDGE;
        let mut style = enforce(tao_rewrite, LOCKED);
        assert!(verified(style, LOCKED));
        style = enforce(tao_rewrite | WS_EX_TRANSPARENT | WS_EX_LAYERED, EDIT);
        assert!(verified(style, EDIT));
        assert_eq!(style & (WS_EX_TRANSPARENT | WS_EX_LAYERED), 0);
        style = enforce(tao_rewrite, LOCKED);
        assert!(verified(style, LOCKED));
        // Enforcing twice changes nothing.
        assert_eq!(enforce(style, LOCKED), style);
    }

    #[test]
    fn the_state_survives_the_subclass_reference_data() {
        for click_through in [false, true] {
            for focusable in [false, true] {
                for topmost in [false, true] {
                    let state = OverlayWindowState {
                        click_through,
                        focusable,
                        topmost,
                    };
                    assert_eq!(decode(encode(state)), state);
                }
            }
        }
    }

    #[test]
    fn styles_are_described_by_name() {
        assert_eq!(describe(0), "none");
        assert_eq!(
            describe(required_bits(LOCKED)),
            "TOPMOST | LAYERED | TRANSPARENT | NOACTIVATE | TOOLWINDOW"
        );
    }
}
