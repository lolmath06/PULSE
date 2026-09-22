//! The shared vocabulary for *what a process is doing* and *what kind of thing
//! it is*.
//!
//! # Five states, not thirty
//!
//! Linux exposes a dozen scheduler states (`R D S T t W X Z P I`), Windows
//! exposes none at the process level at all — its states belong to threads.
//! Mapping every Linux letter onto an invented Windows equivalent would
//! produce a column that means something different on each platform while
//! looking identical, which is worse than a coarser column that means the same
//! thing everywhere.
//!
//! So PULSE maps only where the semantics are honest, and says
//! [`ProcessState::Other`] where they are not.

use serde::{Deserialize, Serialize};

/// What a process is doing, reduced to what both platforms can mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessState {
    /// Running on a processor, or queued and ready to. Linux `R`.
    Running,
    /// Waiting: for a timer, for I/O, for a lock. Linux `S`, `D`, `I`.
    ///
    /// Deliberately one state. The difference between an interruptible sleep
    /// and an uninterruptible one matters to a kernel engineer and to nobody
    /// looking at a process list.
    SleepingOrWaiting,
    /// Suspended by a signal or by a debugger. Linux `T`, `t`.
    Stopped,
    /// Exited, but not yet reaped by its parent. Linux `Z`.
    Zombie,
    /// A state PULSE will not claim to understand — including *every* process
    /// on Windows, which reports no process-level state.
    Other,
}

impl ProcessState {
    /// Maps one Linux state letter.
    ///
    /// Unknown letters become [`ProcessState::Other`] rather than an error: a
    /// future kernel adding a state must not break the process list.
    pub fn from_linux_char(state: char) -> Self {
        match state {
            'R' => ProcessState::Running,
            'S' | 'D' | 'I' => ProcessState::SleepingOrWaiting,
            'T' | 't' => ProcessState::Stopped,
            'Z' => ProcessState::Zombie,
            _ => ProcessState::Other,
        }
    }

    /// Whether this process counts towards `process.count.running`.
    pub const fn is_running(self) -> bool {
        matches!(self, ProcessState::Running)
    }

    /// The short machine-readable discriminant, matching the wire format.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProcessState::Running => "running",
            ProcessState::SleepingOrWaiting => "sleepingOrWaiting",
            ProcessState::Stopped => "stopped",
            ProcessState::Zombie => "zombie",
            ProcessState::Other => "other",
        }
    }
}

/// What kind of thing a process is.
///
/// Used to keep kernel threads and operating-system services from being
/// presented to the user as if they were applications they had launched.
/// Classified only where a *reliable* signal exists — a Linux kernel-thread
/// flag, a Windows system path — and [`ProcessClass::Unknown`] otherwise.
/// PULSE does not guess from names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessClass {
    /// Something the user is plausibly running: their own session's programs.
    UserApplication,
    /// A kernel thread, a system service, or a process owned by the OS.
    SystemProcess,
    /// No reliable signal either way.
    Unknown,
}

impl ProcessClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            ProcessClass::UserApplication => "userApplication",
            ProcessClass::SystemProcess => "systemProcess",
            ProcessClass::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_linux_states_pulse_commits_to() {
        assert_eq!(ProcessState::from_linux_char('R'), ProcessState::Running);
        for sleeping in ['S', 'D', 'I'] {
            assert_eq!(
                ProcessState::from_linux_char(sleeping),
                ProcessState::SleepingOrWaiting
            );
        }
        assert_eq!(ProcessState::from_linux_char('T'), ProcessState::Stopped);
        assert_eq!(ProcessState::from_linux_char('t'), ProcessState::Stopped);
        assert_eq!(ProcessState::from_linux_char('Z'), ProcessState::Zombie);
    }

    #[test]
    fn an_unrecognised_state_degrades_rather_than_failing() {
        // `X` (dead), `W` (paging, pre-2.6), `P` (parked) and anything a
        // future kernel invents.
        for exotic in ['X', 'x', 'W', 'P', 'K', '?', ' '] {
            assert_eq!(ProcessState::from_linux_char(exotic), ProcessState::Other);
        }
    }

    #[test]
    fn only_running_counts_as_running() {
        assert!(ProcessState::Running.is_running());
        for other in [
            ProcessState::SleepingOrWaiting,
            ProcessState::Stopped,
            ProcessState::Zombie,
            ProcessState::Other,
        ] {
            assert!(!other.is_running());
        }
    }

    #[test]
    fn states_serialise_as_their_documented_discriminants() {
        for state in [
            ProcessState::Running,
            ProcessState::SleepingOrWaiting,
            ProcessState::Stopped,
            ProcessState::Zombie,
            ProcessState::Other,
        ] {
            let json = serde_json::to_string(&state).expect("serialise");
            assert_eq!(json, format!("\"{}\"", state.as_str()));
        }
    }

    #[test]
    fn classes_serialise_as_their_documented_discriminants() {
        for class in [
            ProcessClass::UserApplication,
            ProcessClass::SystemProcess,
            ProcessClass::Unknown,
        ] {
            let json = serde_json::to_string(&class).expect("serialise");
            assert_eq!(json, format!("\"{}\"", class.as_str()));
        }
    }
}
