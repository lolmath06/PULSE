//! Compatibility for the native main-window titlebar in the locked Tao 0.35.3.
//!
//! Tao's Wayland `WlHeader` puts a GTK EventBox above its HeaderBar. That input
//! window can intercept the close/minimize/maximize buttons until a resize
//! re-stacks them (tauri-apps/tao#1218). Keep the native header and its normal
//! actions, but let its children receive input first. No CSS, resize toggle,
//! polling, global GTK setting, or change to overlay input regions is needed.

use tauri::{Runtime, WebviewWindow};

/// Called at startup and when reopening/recreating the main window. The
/// exact GTK structure is checked so this becomes a no-op with a fixed Tao.
#[cfg(target_os = "linux")]
pub fn prepare_main_window<R: Runtime>(window: &WebviewWindow<R>) {
    if window.label() != crate::desktop::MAIN_LABEL {
        return;
    }
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        if let Ok(window) = target.gtk_window() {
            repair_titlebar(&window);
        }
    });
}

/// Only Tao's GTK header needs this repair; other platforms have nothing to do.
#[cfg(not(target_os = "linux"))]
pub fn prepare_main_window<R: Runtime>(_window: &WebviewWindow<R>) {}

#[cfg(target_os = "linux")]
fn repair_titlebar(window: &gtk::ApplicationWindow) -> bool {
    use gtk::prelude::*;

    let Some(titlebar) = window.titlebar() else {
        return false;
    };
    let Ok(events) = titlebar.downcast::<gtk::EventBox>() else {
        return false;
    };
    if !events
        .child()
        .is_some_and(|child| child.is::<gtk::HeaderBar>())
    {
        return false;
    }
    events.set_above_child(false);
    true
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use gtk::prelude::*;

    /// Requires a display; run explicitly with --ignored --test-threads=1.
    /// This inspects real GDK input windows, not a mocked CSS hit target.
    #[test]
    #[ignore = "requires a GTK display"]
    fn titlebar_buttons_have_no_intercepting_input_window() {
        gtk::init().expect("GTK display required");
        let window = gtk::ApplicationWindow::builder()
            .title("PULSE isolated titlebar test")
            .default_width(640)
            .default_height(240)
            .build();
        let header = gtk::HeaderBar::builder()
            .title("PULSE")
            .show_close_button(true)
            .decoration_layout("menu:minimize,maximize,close")
            .build();
        let events = gtk::EventBox::new();
        events.set_above_child(true);
        events.add(&header);
        window.set_titlebar(Some(&events));
        window.add(&gtk::Label::new(Some("PULSE test")));
        window.show_all();
        let input_parent = events.window().expect("realized EventBox");
        let before = input_parent.children().len();
        assert!(repair_titlebar(&window));
        assert!(!events.is_above_child());
        assert_eq!(events.window().unwrap().children().len() + 1, before);
        // Reopening, resizing and idempotent setup keep the header usable.
        window.hide();
        window.show_all();
        window.resize(720, 300);
        assert!(repair_titlebar(&window));
        assert!(!events.is_above_child());
        assert!(header.shows_close_button());
        assert!(header.is_sensitive());
        // A different titlebar must not be rewritten.
        window.hide();
        window.set_titlebar(Some(&gtk::HeaderBar::new()));
        assert!(!repair_titlebar(&window));
        window.close();
    }
}
