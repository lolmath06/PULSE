//! NVIDIA telemetry through NVML, as an **optional** capability.
//!
//! NVML — the NVIDIA Management Library — ships with the proprietary driver on
//! both Fedora and Windows and is the only supported way to read utilisation,
//! VRAM and clocks from an NVIDIA GPU without root. It is also the library
//! `nvidia-smi` itself is built on.
//!
//! # Optional means optional
//!
//! Phase 3's `NtQuerySystemInformationEx` fix established the rule this module
//! follows without exception:
//!
//! ```text
//! an optional monitoring backend  ≠  a mandatory application dependency
//! ```
//!
//! PULSE must start normally with an NVIDIA card, without one, with the
//! open-source `nouveau` driver instead of the proprietary one, with the driver
//! present but NVML missing, and with an NVML too old to export an optional
//! symbol. Linking NVML at load time would turn every one of those into a
//! process that refuses to start.
//!
//! So the library is **loaded at runtime** and every symbol is **resolved at
//! runtime**, and each failure is one structured `Availability` rather than a
//! crash. Nothing here ever panics: there is no `unwrap` and no `expect` on any
//! path that depends on the driver being installed.
//!
//! # Why not a crate
//!
//! `libloading` would do the loading, and a `nvml-wrapper` crate would do the
//! whole job. Neither is used:
//!
//! - the Windows DLL search path is a **security** decision, not a convenience
//!   one (see [`library`]), and it must be spelled out in PULSE's own code
//!   rather than inherited from a dependency's defaults;
//! - PULSE already resolves `ntdll` natively for the same reason, so a
//!   dependency here would make two mechanisms where one suffices;
//! - a full vendor-monitoring crate would pull in a large surface, decide the
//!   metric semantics on PULSE's behalf, and bundle vendor headers PULSE has no
//!   business shipping.
//!
//! Only the symbols this phase needs are resolved. Temperature, power, fan and
//! encoder entry points exist in NVML and are deliberately **not** bound —
//! they belong to the sensors phase.
//!
//! # Testability
//!
//! Everything above the FFI boundary talks to [`NvmlBackend`], a trait with one
//! real implementation and a scripted fake in tests. That is what lets "library
//! absent", "symbol absent", "init fails", "GPU lost", "permission denied" and
//! "0 / 1 / many devices" all be covered on a machine with no NVIDIA driver at
//! all.

pub mod backend;
pub mod library;

pub use backend::{
    availability_for_nvml, NvmlBackend, NvmlClock, NvmlDeviceInfo, NvmlError, NvmlMemory,
};
