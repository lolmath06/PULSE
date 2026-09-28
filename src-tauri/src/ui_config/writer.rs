//! Coalesces configuration changes into occasional atomic writes.
//!
//! A thread that sleeps until notified, then waits for a short quiet period so
//! that a burst of changes (dragging a slider, moving an overlay) becomes one
//! write. On shutdown it flushes whatever is pending.

use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::UiConfigStore;

/// How long the writer waits after the last change before writing.
pub const QUIET_PERIOD: Duration = Duration::from_millis(400);

#[derive(Default)]
struct Signal {
    pending: bool,
    stop: bool,
}

/// The background writer. Dropping it flushes and stops the thread.
pub struct ConfigWriter {
    store: Arc<UiConfigStore>,
    signal: Arc<(Mutex<Signal>, Condvar)>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for ConfigWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigWriter").finish()
    }
}

impl ConfigWriter {
    pub fn spawn(store: Arc<UiConfigStore>, quiet: Duration) -> Self {
        let signal: Arc<(Mutex<Signal>, Condvar)> = Arc::default();
        let thread_signal = Arc::clone(&signal);
        let thread_store = Arc::clone(&store);
        let handle = std::thread::Builder::new()
            .name("pulse-ui-config".into())
            .spawn(move || run(&thread_store, &thread_signal, quiet))
            .ok();
        Self {
            store,
            signal,
            handle: Mutex::new(handle),
        }
    }

    /// Something changed; write it soon.
    pub fn notify(&self) {
        let (lock, condvar) = &*self.signal;
        if let Ok(mut signal) = lock.lock() {
            signal.pending = true;
            condvar.notify_all();
        }
    }

    /// Stops the thread and writes anything pending. Idempotent.
    pub fn shutdown(&self) {
        {
            let (lock, condvar) = &*self.signal;
            if let Ok(mut signal) = lock.lock() {
                signal.stop = true;
                condvar.notify_all();
            }
        }
        let handle = self.handle.lock().ok().and_then(|mut handle| handle.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
        if let Err(error) = self.store.flush() {
            eprintln!("PULSE: final configuration save failed: {error}");
        }
    }
}

impl Drop for ConfigWriter {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(store: &UiConfigStore, signal: &(Mutex<Signal>, Condvar), quiet: Duration) {
    let (lock, condvar) = signal;
    loop {
        // Sleep until there is something to write, or we are told to stop.
        {
            let Ok(mut state) = lock.lock() else { return };
            while !state.pending && !state.stop {
                state = match condvar.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            if state.stop {
                return;
            }
            state.pending = false;
        }

        // Let the burst finish: every new change restarts the quiet period.
        loop {
            let Ok(state) = lock.lock() else { return };
            let (mut state, timeout) = match condvar.wait_timeout(state, quiet) {
                Ok(result) => result,
                Err(_) => return,
            };
            if state.stop {
                return;
            }
            if timeout.timed_out() && !state.pending {
                break;
            }
            state.pending = false;
        }

        if let Err(error) = store.flush() {
            eprintln!("PULSE: configuration save failed: {error}");
        }
    }
}
