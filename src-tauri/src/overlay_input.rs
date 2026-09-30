//! The native side of an overlay's input mode (see `crate::overlay::input` for
//! when it is applied).
//!
//! **Linux (GTK 3).** Click-through is GTK's *widget-level* input shape on the
//! overlay's `GtkApplicationWindow`: an empty region for `ClickThrough`, none
//! for `Interactive`. GTK stores that shape on the widget and folds it into
//! every input shape it computes itself, so it survives the recomputations
//! that silently undid tao's `set_ignore_cursor_events`:
//!
//! - tao gives every Wayland window a GTK titlebar (`WlHeader`), which makes
//!   GTK treat it as client-decorated even with `decorations(false)`;
//! - a client-decorated `GtkWindow` resets its input shape to "the whole
//!   window" on each size allocation — which every `xdg_toplevel.configure`
//!   triggers, including the one a focus change sends
//!   (`update_border_windows` → `gtk_widget_set_csd_input_shape`, GTK
//!   3.24.43);
//! - tao wrote its locked shape straight onto the `GdkWindow`, below GTK, so
//!   GTK never knew about it and overwrote it.
//!
//! A native child `GdkWindow` would be its own compositor surface with its own
//! input region, so any the overlay has get the same mode. (None were seen on
//! Fedora 39 / WebKitGTK 2.46: WebKit draws into the toplevel's surface.)
//! `gdk_window_set_pass_through` is deliberately not used: it only steers
//! GDK's own dispatch between a window's children and never reaches the
//! compositor.
//!
//! **Elsewhere**, tao's `set_ignore_cursor_events`, unchanged.

use tauri::{Runtime, WebviewWindow};

use crate::overlay::input::OverlayInputMode;

/// Applies `mode` to an overlay window. On Linux it runs on the main thread
/// and waits for it; the `Ok` text describes what was done, for diagnostics.
pub fn set_overlay_input_mode<R: Runtime>(
    window: &WebviewWindow<R>,
    mode: OverlayInputMode,
) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        linux::on_main_thread(window, move |gtk_window| {
            linux::apply(gtk_window, mode).to_string()
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        window
            .set_ignore_cursor_events(mode == OverlayInputMode::ClickThrough)
            .map(|()| "ignore_cursor_events".to_string())
            .map_err(|error| error.to_string())
    }
}

/// Calls `on_map` each time the overlay's native window is mapped again,
/// with a function that applies a mode to it. Connected once per window,
/// before it is first shown; a no-op where the platform needs nothing.
pub fn watch_remap<R, F>(window: &WebviewWindow<R>, on_map: F)
where
    R: Runtime,
    F: Fn(&mut dyn FnMut(OverlayInputMode) -> Result<String, String>) + Send + 'static,
{
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;

        let target = window.clone();
        let _ = window.run_on_main_thread(move || {
            let Ok(gtk_window) = target.gtk_window() else {
                return;
            };
            gtk_window.connect_map(move |gtk_window| {
                on_map(&mut |mode| Ok(linux::apply(gtk_window, mode).to_string()));
            });
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, on_map);
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fmt;
    use std::sync::mpsc;
    use std::time::Duration;

    use gtk::cairo::Region;
    use gtk::gdk;
    use gtk::glib::translate::ToGlibPtr;
    use gtk::prelude::*;
    use tauri::{Runtime, WebviewWindow};

    use crate::overlay::input::OverlayInputMode;

    /// How long a caller waits for the main thread. It answers in far less;
    /// this only bounds a main thread that is gone (PULSE quitting).
    const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(2);

    pub fn on_main_thread<R: Runtime>(
        window: &WebviewWindow<R>,
        work: impl FnOnce(&gtk::ApplicationWindow) -> String + Send + 'static,
    ) -> Result<String, String> {
        let (tx, rx) = mpsc::channel();
        let target = window.clone();
        window
            .run_on_main_thread(move || {
                let result = target
                    .gtk_window()
                    .map(|gtk_window| work(&gtk_window))
                    .map_err(|error| error.to_string());
                let _ = tx.send(result);
            })
            .map_err(|error| error.to_string())?;
        rx.recv_timeout(MAIN_THREAD_TIMEOUT)
            .map_err(|_| "the main thread did not answer".to_string())?
    }

    /// What was found and done, for the diagnostic line.
    pub struct Report {
        mode: OverlayInputMode,
        display: String,
        client_decorated: bool,
        realized: bool,
        children: usize,
        native_children: usize,
    }

    impl fmt::Display for Report {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let shape = match self.mode {
                OverlayInputMode::ClickThrough => "empty",
                OverlayInputMode::Interactive => "none",
            };
            write!(
                f,
                "GTK widget input shape {shape}; {}, {}, {}, {} child GdkWindow(s) of which {} native",
                self.display,
                if self.client_decorated { "client-decorated" } else { "not client-decorated" },
                if self.realized { "realized" } else { "not realized yet" },
                self.children,
                self.native_children,
            )
        }
    }

    /// Must run on the main thread.
    pub fn apply(window: &impl IsA<gtk::Widget>, mode: OverlayInputMode) -> Report {
        let window = window.as_ref();
        let empty = Region::create();
        match mode {
            OverlayInputMode::ClickThrough => window.input_shape_combine_region(Some(&empty)),
            OverlayInputMode::Interactive => window.input_shape_combine_region(None),
        }
        let style = window.style_context();
        let mut report = Report {
            mode,
            display: window.display().type_().name().to_string(),
            client_decorated: style.has_class("csd") || style.has_class("solid-csd"),
            realized: window.window().is_some(),
            children: 0,
            native_children: 0,
        };
        if let Some(gdk_window) = window.window() {
            shape_native_children(&gdk_window, mode, &empty, &mut report);
        }
        report
    }

    fn shape_native_children(
        parent: &gdk::Window,
        mode: OverlayInputMode,
        empty: &Region,
        report: &mut Report,
    ) {
        for child in parent.children() {
            report.children += 1;
            if child.has_native() {
                report.native_children += 1;
                match mode {
                    OverlayInputMode::ClickThrough => child.input_shape_combine_region(empty, 0, 0),
                    // SAFETY: `child` is a live GdkWindow borrowed for the call,
                    // and NULL is GDK's documented "remove the shape".
                    OverlayInputMode::Interactive => unsafe {
                        gdk::ffi::gdk_window_input_shape_combine_region(
                            child.to_glib_none().0,
                            std::ptr::null(),
                            0,
                            0,
                        )
                    },
                }
            }
            shape_native_children(&child, mode, empty, report);
        }
    }
}
