//! Resolving a DXGI adapter's PCI bus address on Windows, through D3DKMT.
//!
//! # Why this exists
//!
//! `DXGI_ADAPTER_DESC1` describes *what* an adapter is — vendor, device,
//! subsystem, revision — and never *which slot it sits in*. That is enough to
//! name an adapter and not enough to correlate it with anything else. NVML, by
//! contrast, reports a full bus address for every device it serves.
//!
//! Without a common key, pairing a DXGI adapter with an NVML device can only
//! fall back to enumeration order, and the two APIs enumerate independently: on
//! a machine with two NVIDIA cards, "NVML device 0 is DXGI adapter 0" is a coin
//! flip whose outcome is written into a saved dashboard. See
//! `platform::gpu`'s module documentation.
//!
//! # The route Windows actually offers
//!
//! `DXGI_ADAPTER_DESC1::AdapterLuid` is a **session identity**: Microsoft
//! documents it as valid only until the system restarts, so it is useless as a
//! stored identifier. It is, however, exactly the right handle for asking the
//! kernel graphics subsystem about this adapter *right now*:
//!
//! ```text
//! AdapterLuid
//!    └─ D3DKMTOpenAdapterFromLuid      → a kernel adapter handle
//!         └─ D3DKMTQueryAdapterInfo
//!              KMTQAITYPE_ADAPTERADDRESS → bus / device / function
//!         └─ D3DKMTCloseAdapter        → always, including on failure
//! ```
//!
//! `D3DKMT_ADAPTERADDRESS` carries no PCI *segment* (domain) field, so the
//! domain is taken as `0`. That is correct on the overwhelming majority of
//! machines — multi-segment PCI is a large-server configuration — and when it
//! is not, the address simply fails to match NVML's and the merge falls back to
//! its conservative rules rather than pairing the wrong cards.
//!
//! # Why the entry points are resolved at runtime
//!
//! The same reasoning as `platform::windows::ntdll`: these are `gdi32` exports
//! of the kernel-mode graphics interface, documented for driver-adjacent use
//! rather than as a stable application API. A load-time import would make an
//! environment without them — a hardened image, an emulation layer, a future
//! Windows — fail to *start* PULSE, instead of costing one correlation hint.
//!
//! `gdi32.dll` is loaded by `LoadLibraryExW` with `LOAD_LIBRARY_SEARCH_SYSTEM32`
//! so the lookup can never reach the application directory, the working
//! directory or `PATH`. See `platform::nvml::search` for the same rule applied
//! to NVML.

use crate::metrics::wellknown::gpu::PciAddress;

/// A `LUID`, as DXGI reports it, in the two halves Windows splits it into.
///
/// Carried as plain data so everything above the FFI boundary — including the
/// whole merge — is testable on Fedora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdapterLuid {
    pub low: u32,
    pub high: i32,
}

impl AdapterLuid {
    pub const fn new(low: u32, high: i32) -> Self {
        Self { low, high }
    }
}

/// `KMTQAITYPE_ADAPTERADDRESS`, the query that returns the bus address.
pub const KMTQAITYPE_ADAPTERADDRESS: i32 = 3;

/// The module exporting the D3DKMT entry points.
pub const GDI32_MODULE: &str = "gdi32.dll";

/// Converts a `D3DKMT_ADAPTERADDRESS` into PULSE's address type.
///
/// The domain is `0`: the structure has no segment field. Kept as its own
/// function so the assumption is visible, documented and tested rather than
/// buried in an FFI block that only compiles on Windows.
///
/// Returns `None` when a field does not fit the PCI encoding — a bus above
/// 255, a device above 31, a function above 7 — which is how a struct that was
/// not filled in, or was filled by something other than a PCI adapter, is
/// refused instead of being folded onto a plausible-looking address.
pub fn adapter_address_to_pci(bus: u32, device: u32, function: u32) -> Option<PciAddress> {
    if bus > u32::from(u8::MAX) || device > 31 || function > 7 {
        return None;
    }

    Some(PciAddress::new(0, bus as u8, device as u8, function as u8))
}

/// The parts that call into the kernel graphics interface.
#[cfg(target_os = "windows")]
pub mod imp {
    use std::ffi::{c_int, c_uint, c_void};

    use windows_sys::Win32::Foundation::LUID;
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };

    use crate::metrics::wellknown::gpu::PciAddress;

    use super::{adapter_address_to_pci, AdapterLuid, KMTQAITYPE_ADAPTERADDRESS};

    /// `gdi32.dll` as a NUL-terminated UTF-16 string.
    ///
    /// Written out rather than built at runtime so the lookup allocates nothing
    /// and the bytes are visible in review.
    const GDI32_W: &[u16] = &[
        b'g' as u16,
        b'd' as u16,
        b'i' as u16,
        b'3' as u16,
        b'2' as u16,
        b'.' as u16,
        b'd' as u16,
        b'l' as u16,
        b'l' as u16,
        0,
    ];

    /// `D3DKMT_OPENADAPTERFROMLUID`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct OpenAdapterFromLuid {
        adapter_luid: LUID,
        handle: c_uint,
    }

    /// `D3DKMT_QUERYADAPTERINFO`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct QueryAdapterInfo {
        handle: c_uint,
        kind: c_int,
        private_driver_data: *mut c_void,
        private_driver_data_size: c_uint,
    }

    /// `D3DKMT_ADAPTERADDRESS`.
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct AdapterAddress {
        bus_number: c_uint,
        device_number: c_uint,
        function_number: c_uint,
    }

    /// `D3DKMT_CLOSEADAPTER`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CloseAdapter {
        handle: c_uint,
    }

    type OpenFn = unsafe extern "system" fn(*mut OpenAdapterFromLuid) -> c_int;
    type QueryFn = unsafe extern "system" fn(*mut QueryAdapterInfo) -> c_int;
    type CloseFn = unsafe extern "system" fn(*mut CloseAdapter) -> c_int;

    /// The three entry points, resolved together or not at all.
    struct Entries {
        open: OpenFn,
        query: QueryFn,
        close: CloseFn,
    }

    /// Resolves the D3DKMT entry points from `gdi32`, or reports their absence.
    ///
    /// # Safety
    ///
    /// Each address comes from the module just loaded and is reinterpreted onto
    /// the documented signature of that entry point; a missing export yields
    /// `None` and is refused before any conversion. The module handle is
    /// deliberately leaked: `gdi32` is a system library that stays mapped for
    /// the life of the process anyway, and releasing it while a resolved
    /// pointer is still in use would be the one way to make this unsound.
    fn entries() -> Option<Entries> {
        // SAFETY: a `'static` NUL-terminated UTF-16 name, a null reserved
        // handle as the API requires, and a search flag that confines the
        // lookup to System32 — never the application or working directory.
        let module = unsafe {
            LoadLibraryExW(
                GDI32_W.as_ptr(),
                core::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };

        if module.is_null() {
            return None;
        }

        let symbol = |name: &[u8]| -> Option<*const c_void> {
            // SAFETY: a NUL-terminated ANSI name and a handle from a successful
            // `LoadLibraryExW`; a missing export yields `None`.
            unsafe { GetProcAddress(module, name.as_ptr()) }.map(|address| address as *const c_void)
        };

        let open = symbol(b"D3DKMTOpenAdapterFromLuid\0")?;
        let query = symbol(b"D3DKMTQueryAdapterInfo\0")?;
        let close = symbol(b"D3DKMTCloseAdapter\0")?;

        // SAFETY: see the function docs — each address is a live code address
        // in `gdi32` for the entry point of that name, and every signature
        // above is the documented one.
        unsafe {
            Some(Entries {
                open: core::mem::transmute::<*const c_void, OpenFn>(open),
                query: core::mem::transmute::<*const c_void, QueryFn>(query),
                close: core::mem::transmute::<*const c_void, CloseFn>(close),
            })
        }
    }

    /// The PCI bus address of the adapter this LUID names, when Windows will
    /// say.
    ///
    /// `None` on every failure — entry points absent, adapter already gone,
    /// query unsupported by the driver — because a missing correlation hint
    /// costs the conservative merge rules, while a wrong one costs the user a
    /// dashboard bound to the wrong card.
    pub fn pci_address_for_luid(luid: AdapterLuid) -> Option<PciAddress> {
        let entries = entries()?;

        let mut open = OpenAdapterFromLuid {
            adapter_luid: LUID {
                LowPart: luid.low,
                HighPart: luid.high,
            },
            handle: 0,
        };

        // SAFETY: `open` is a live, fully initialised local of the documented
        // layout; the callee writes only within it and reports success in its
        // return value, which is checked before the handle is used.
        if unsafe { (entries.open)(&mut open) } != 0 || open.handle == 0 {
            return None;
        }

        let mut address = AdapterAddress::default();
        let mut query = QueryAdapterInfo {
            handle: open.handle,
            kind: KMTQAITYPE_ADAPTERADDRESS,
            private_driver_data: (&mut address as *mut AdapterAddress).cast(),
            private_driver_data_size: core::mem::size_of::<AdapterAddress>() as c_uint,
        };

        // SAFETY: the output buffer is a live local whose exact size is passed
        // alongside it, so the callee cannot write past it.
        let status = unsafe { (entries.query)(&mut query) };

        let mut close = CloseAdapter {
            handle: open.handle,
        };

        // SAFETY: the handle came from a successful open and is closed exactly
        // once, on every path, including the failure one below.
        unsafe {
            (entries.close)(&mut close);
        }

        if status != 0 {
            return None;
        }

        adapter_address_to_pci(
            address.bus_number,
            address.device_number,
            address.function_number,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_bus_address_the_merge_correlates_on() {
        // The reference machine's discrete GPU, as both APIs would report it.
        assert_eq!(
            adapter_address_to_pci(1, 0, 0),
            Some(PciAddress::new(0, 1, 0, 0))
        );
        assert_eq!(
            adapter_address_to_pci(0x41, 0, 0),
            Some(PciAddress::new(0, 0x41, 0, 0))
        );
    }

    #[test]
    fn the_domain_is_zero_because_the_structure_has_no_segment_field() {
        // Documented, not assumed: an address that turns out to belong to a
        // non-zero segment simply fails to match NVML's, and the merge falls
        // back to its conservative rules.
        let address = adapter_address_to_pci(2, 0, 0).expect("valid");

        assert_eq!(address.domain, 0);
        assert_eq!(address.to_bdf(), "0000:02:00.0");
    }

    #[test]
    fn a_field_outside_the_pci_encoding_is_refused_rather_than_truncated() {
        // A truncated address would be a *plausible* address belonging to
        // another slot, which is exactly the failure the merge must not make.
        assert_eq!(adapter_address_to_pci(256, 0, 0), None);
        assert_eq!(adapter_address_to_pci(0, 32, 0), None);
        assert_eq!(adapter_address_to_pci(0, 0, 8), None);
        assert_eq!(adapter_address_to_pci(u32::MAX, u32::MAX, u32::MAX), None);
    }

    #[test]
    fn the_whole_valid_range_round_trips() {
        for (bus, device, function) in [(0, 0, 0), (255, 31, 7), (0x41, 0x1f, 1)] {
            let address = adapter_address_to_pci(bus, device, function).expect("valid");

            assert_eq!(u32::from(address.bus), bus);
            assert_eq!(u32::from(address.device), device);
            assert_eq!(u32::from(address.function), function);
        }
    }

    #[test]
    fn a_luid_keeps_both_halves_windows_splits_it_into() {
        // Truncating the high half would collapse two adapters onto one LUID.
        let luid = AdapterLuid::new(0x1234_5678, -2);

        assert_eq!(luid.low, 0x1234_5678);
        assert_eq!(luid.high, -2);
        assert_ne!(luid, AdapterLuid::new(0x1234_5678, 0));
    }
}
