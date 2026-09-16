//! Optional `ntdll` entry points, resolved at runtime.
//!
//! # Why not a static import
//!
//! `NtQuerySystemInformationEx` is the only way PULSE can read CPU counters per
//! logical processor across processor groups, but it is **not a guaranteed part
//! of the Windows API**. It lives in `ntdll`, Microsoft documents that layer as
//! "may be altered or unavailable in future versions", and it carries no
//! import-library contract the way a `kernel32` function does.
//!
//! Declaring it in an `extern` block makes it a **load-time import**: the
//! loader resolves every imported symbol before the first line of PULSE runs.
//! On a Windows build, emulation layer, or hardened environment where the
//! symbol is absent, the process would fail to start — with a loader error box,
//! not a PULSE message.
//!
//! That inverts the principle the whole availability model exists to enforce:
//!
//! ```text
//! a capability being unavailable  ≠  the application being unable to start
//! ```
//!
//! So the symbol is resolved **at runtime**, once, and its absence degrades
//! exactly one metric family. See `docs/metrics/cpu-advanced.md`.
//!
//! # Why `GetModuleHandleW` and not `LoadLibrary`
//!
//! `ntdll.dll` is mapped into every Win32 process before any user code runs —
//! it *is* the loader. `GetModuleHandleW` therefore looks up a module that is
//! already present and never touches the filesystem, so there is no DLL search
//! path to hijack and no arbitrary library is loaded. A `LoadLibrary` call here
//! would be both unnecessary and a needless attack surface.

use crate::metrics::model::{MetricError, MetricErrorCode};

/// The system module holding the optional entry point.
pub const NTDLL_MODULE: &str = "ntdll.dll";

/// The optional entry point PULSE resolves.
pub const PROCESSOR_PERFORMANCE_SYMBOL: &str = "NtQuerySystemInformationEx";

/// `ntdll.dll` itself could not be looked up.
///
/// Essentially impossible on a real Win32 process — but "essentially
/// impossible" is not "cannot happen", and the honest answer costs one metric
/// family rather than the application.
pub fn module_unavailable_error() -> MetricError {
    MetricError::new(
        MetricErrorCode::Unsupported,
        format!(
            "{NTDLL_MODULE} could not be looked up, so per-processor CPU counters \
             are unavailable on this system"
        ),
    )
    .with_recoverable(false)
}

/// The module is present but does not export the entry point.
///
/// The expected shape of "this Windows does not offer the capability".
pub fn symbol_unavailable_error() -> MetricError {
    MetricError::new(
        MetricErrorCode::Unsupported,
        format!(
            "{NTDLL_MODULE} does not export {PROCESSOR_PERFORMANCE_SYMBOL}, so per-processor \
             CPU counters are unavailable on this system"
        ),
    )
    .with_recoverable(false)
}

/// The resolved entry point rejected PULSE's probe.
///
/// Distinct from the two above: the symbol exists, but the information class
/// does not work here — a sandbox, or a future Windows that kept the name and
/// dropped the behaviour.
pub fn probe_failed_error(status: i32) -> MetricError {
    MetricError::new(
        MetricErrorCode::Unsupported,
        format!(
            "{PROCESSOR_PERFORMANCE_SYMBOL} is present but refused the per-processor \
             counter query (status {status:#010x})"
        ),
    )
    .with_recoverable(false)
}

/// The parts that touch the Windows loader.
#[cfg(target_os = "windows")]
pub mod imp {
    use windows_sys::Wdk::System::SystemInformation::{
        SystemProcessorPerformanceInformation, SYSTEM_INFORMATION_CLASS,
    };
    use windows_sys::Win32::Foundation::{HMODULE, NTSTATUS};
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION;

    use crate::metrics::model::MetricError;

    use super::{module_unavailable_error, probe_failed_error, symbol_unavailable_error};

    /// `ntdll.dll` as a NUL-terminated UTF-16 string.
    ///
    /// Written out rather than built at runtime so the lookup allocates
    /// nothing and the bytes are visible in review.
    const NTDLL_W: &[u16] = &[
        b'n' as u16,
        b't' as u16,
        b'd' as u16,
        b'l' as u16,
        b'l' as u16,
        b'.' as u16,
        b'd' as u16,
        b'l' as u16,
        b'l' as u16,
        0,
    ];

    /// `NtQuerySystemInformationEx` as a NUL-terminated ANSI string.
    ///
    /// `GetProcAddress` takes narrow strings only; there is no wide variant.
    const SYMBOL_A: &[u8] = b"NtQuerySystemInformationEx\0";

    /// `STATUS_INFO_LENGTH_MISMATCH` — the expected answer to a deliberately
    /// undersized buffer.
    const STATUS_INFO_LENGTH_MISMATCH: NTSTATUS = 0xC000_0004_u32 as NTSTATUS;

    /// The ABI of the entry point PULSE resolves.
    ///
    /// Spelled out in full because nothing else checks it: a dynamically
    /// resolved pointer carries no type information, so this declaration *is*
    /// the contract. It mirrors the documented signature exactly — for
    /// `SystemProcessorPerformanceInformation` the input buffer is a single
    /// `USHORT` naming the processor group to report on.
    ///
    /// ```text
    /// NTSTATUS NtQuerySystemInformationEx(
    ///     SYSTEM_INFORMATION_CLASS SystemInformationClass,
    ///     PVOID  InputBuffer,     ULONG InputBufferLength,
    ///     PVOID  SystemInformation, ULONG SystemInformationLength,
    ///     PULONG ReturnLength);
    /// ```
    type NtQuerySystemInformationExFn = unsafe extern "system" fn(
        system_information_class: SYSTEM_INFORMATION_CLASS,
        input_buffer: *const core::ffi::c_void,
        input_buffer_length: u32,
        system_information: *mut core::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> NTSTATUS;

    /// A resolved handle to the optional per-processor counter entry point.
    ///
    /// Constructing one is the *only* way to reach the raw pointer: the
    /// function pointer is private, so no raw address ever circulates through
    /// the provider. Callers get [`NtCpuApi::query_processor_performance`],
    /// which is safe.
    ///
    /// Cheap to hold and `Send + Sync` (a function pointer is both), so the
    /// provider resolves once at construction and reuses it for every sample —
    /// there is no per-refresh or per-processor lookup, and no mutable global.
    #[derive(Clone, Copy)]
    pub struct NtCpuApi {
        query_system_information_ex: NtQuerySystemInformationExFn,
    }

    impl core::fmt::Debug for NtCpuApi {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("NtCpuApi(resolved)")
        }
    }

    impl NtCpuApi {
        /// Looks the entry point up in the already-loaded `ntdll`.
        ///
        /// Returns a structured error rather than panicking on every failure
        /// path, so a Windows without this capability yields one unavailable
        /// metric family instead of an application that cannot start. There is
        /// no `unwrap` or `expect` anywhere on this path.
        ///
        /// # Safety
        ///
        /// Two `unsafe` operations, both narrowly scoped.
        ///
        /// `GetModuleHandleW` is called with a pointer to `NTDLL_W`, a
        /// `'static` NUL-terminated UTF-16 constant, and its result is checked
        /// for null before use. `GetProcAddress` is called with that verified
        /// handle and a pointer to `SYMBOL_A`, a `'static` NUL-terminated ANSI
        /// constant; it returns `None` when the export is absent, which is
        /// handled rather than assumed away.
        ///
        /// The `transmute` converts the returned `FARPROC` —
        /// `unsafe extern "system" fn() -> isize` — into
        /// [`NtQuerySystemInformationExFn`]. Both are `extern "system"`
        /// function pointers of the same size and alignment, so the conversion
        /// is layout-valid; its correctness rests on the resolved symbol
        /// actually having the signature declared above, which is the
        /// documented signature of `NtQuerySystemInformationEx` and the reason
        /// that declaration is written out in full. A symbol of that name with
        /// a different ABI would be a different Windows, and the probe below
        /// is the last line of defence against it.
        pub fn resolve() -> Result<Self, MetricError> {
            // SAFETY: NTDLL_W is a 'static NUL-terminated UTF-16 constant;
            // the returned handle is checked for null before it is used.
            let module: HMODULE = unsafe { GetModuleHandleW(NTDLL_W.as_ptr()) };

            if module.is_null() {
                return Err(module_unavailable_error());
            }

            // SAFETY: `module` is a non-null module handle just returned by
            // the loader, and SYMBOL_A is a 'static NUL-terminated ANSI
            // constant. A missing export yields None, handled below.
            let address = unsafe { GetProcAddress(module, SYMBOL_A.as_ptr()) };

            let Some(address) = address else {
                return Err(symbol_unavailable_error());
            };

            // SAFETY: see the `# Safety` section — a same-size, same-ABI
            // function-pointer conversion onto the documented signature.
            let query_system_information_ex: NtQuerySystemInformationExFn =
                unsafe { core::mem::transmute(address) };

            let api = Self {
                query_system_information_ex,
            };

            api.probe()?;

            Ok(api)
        }

        /// Checks that the resolved entry point really answers the query PULSE
        /// needs, before the catalog claims the capability.
        ///
        /// Deliberately passes a one-record buffer: the machine almost
        /// certainly has more processors than that, so the expected answer is
        /// `STATUS_INFO_LENGTH_MISMATCH` — which proves the entry point exists
        /// *and* understands the information class, without reading anything.
        fn probe(&self) -> Result<(), MetricError> {
            let mut buffer = [SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION::default(); 1];
            let mut returned_bytes: u32 = 0;

            let status = self.query_processor_performance(0, &mut buffer, &mut returned_bytes);

            if status >= 0 || status == STATUS_INFO_LENGTH_MISMATCH {
                return Ok(());
            }

            Err(probe_failed_error(status))
        }

        /// Asks for one processor group's performance counters.
        ///
        /// **Safe**: the buffer's own length decides what the call may write,
        /// and the raw pointer never leaves this function. Callers get an
        /// `NTSTATUS` and a byte count, and are responsible for interpreting
        /// only the records the byte count accounts for.
        ///
        /// # Safety
        ///
        /// The one `unsafe` call passes `buffer`'s own base pointer and its
        /// own byte length, computed from the slice rather than supplied by
        /// the caller, so the callee cannot be told the buffer is larger than
        /// it is. The group number and the returned-length counter are passed
        /// as pointers to live locals. The buffer is fully initialised before
        /// the call — `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION` is a
        /// plain-old-data struct built by `Default` — so no uninitialised
        /// memory is exposed whatever the callee writes or leaves untouched.
        pub fn query_processor_performance(
            &self,
            group: u16,
            buffer: &mut [SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION],
            returned_bytes: &mut u32,
        ) -> NTSTATUS {
            let Ok(byte_capacity) = u32::try_from(core::mem::size_of_val(buffer)) else {
                // A buffer above 4 GiB cannot be described to this API. Not
                // reachable with any real processor count, and reported as a
                // refusal rather than a truncated length.
                return STATUS_INFO_LENGTH_MISMATCH;
            };

            let group_number = group;

            // SAFETY: see the `# Safety` section.
            unsafe {
                (self.query_system_information_ex)(
                    SystemProcessorPerformanceInformation,
                    core::ptr::from_ref(&group_number).cast(),
                    core::mem::size_of::<u16>() as u32,
                    buffer.as_mut_ptr().cast(),
                    byte_capacity,
                    returned_bytes,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_module_is_unsupported_not_a_crash() {
        let error = module_unavailable_error();

        assert_eq!(error.code, MetricErrorCode::Unsupported);
        assert!(!error.recoverable, "retrying will not make ntdll appear");
        assert!(error.message.contains(NTDLL_MODULE));
    }

    #[test]
    fn a_missing_symbol_names_what_is_missing() {
        let error = symbol_unavailable_error();

        assert_eq!(error.code, MetricErrorCode::Unsupported);
        assert!(!error.recoverable);
        assert!(error.message.contains(PROCESSOR_PERFORMANCE_SYMBOL));
        assert!(error.message.contains(NTDLL_MODULE));
    }

    #[test]
    fn a_failing_probe_is_distinguishable_from_a_missing_symbol() {
        // "present but refuses the query" and "not exported at all" are
        // different diagnoses, and the message must let a user tell them apart.
        let probe = probe_failed_error(0xC000_0002_u32 as i32);
        let missing = symbol_unavailable_error();

        assert_eq!(probe.code, MetricErrorCode::Unsupported);
        assert_ne!(probe.message, missing.message);
        assert!(probe.message.contains("present"));
        assert!(probe.message.contains("0xc0000002"));
    }

    #[test]
    fn every_resolution_failure_maps_to_an_unavailable_not_a_wrong_number() {
        use crate::metrics::wellknown::availability_for;

        for error in [
            module_unavailable_error(),
            symbol_unavailable_error(),
            probe_failed_error(-1),
        ] {
            let availability = availability_for(error);

            assert_eq!(availability.status_str(), "unsupported");
            assert!(!availability.is_available());
            // Not transient: the UI must not suggest waiting for it.
            assert!(!availability.is_transient());
        }
    }
}
