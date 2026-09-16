//! Loading NVML at runtime, and the thin FFI layer over it.
//!
//! # Where the library is looked for, and why that is a security decision
//!
//! ## Fedora
//!
//! `dlopen("libnvidia-ml.so.1")` — the **SONAME**, never a bare
//! `libnvidia-ml.so` and never an absolute path. The SONAME is what the
//! proprietary driver package installs and registers with `ldconfig`; the
//! unversioned `.so` symlink belongs to the CUDA *development* package, which
//! most users do not have. Resolving through the normal loader search means
//! PULSE finds the driver wherever the distribution put it — `/usr/lib64`, a
//! multilib path, or a container mount — without hardcoding a layout.
//!
//! ## Windows
//!
//! `LoadLibraryExW(L"nvml.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32)`.
//!
//! The flag is the entire point. A plain `LoadLibraryW("nvml.dll")` searches
//! the **application directory first**, so anyone able to drop a file next to
//! `pulse.exe` — an installer, an unpacked archive, a shared downloads folder —
//! could have PULSE load their DLL with PULSE's privileges. That is a classic
//! DLL planting vulnerability, and a monitoring tool that loads vendor
//! libraries is exactly the kind of program it targets.
//!
//! `LOAD_LIBRARY_SEARCH_SYSTEM32` restricts the search to `%SystemRoot%\System32`
//! and nothing else: not the application directory, not the working directory,
//! not `PATH`. The NVIDIA display driver installs `nvml.dll` there, so this is
//! both the safe path and the correct one.
//!
//! PULSE deliberately does **not** fall back to a wider search when the
//! System32 lookup fails. A missing NVML means "no NVIDIA telemetry", which
//! costs one vendor's metrics; a hijacked NVML means arbitrary code inside
//! PULSE. The trade is not close.
//!
//! # Lifecycle
//!
//! `nvmlInit_v2` is called once, and `nvmlShutdown` exactly once when the
//! handle is dropped — after which the library itself is unloaded. Device
//! handles are cached inside the same object and cannot outlive it, because
//! every accessor borrows `&self`. There is therefore no way for one thread to
//! use a handle while another shuts the library down.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr};

use crate::metrics::wellknown::gpu::PciAddress;

use super::backend::{NvmlBackend, NvmlClock, NvmlDeviceInfo, NvmlError, NvmlMemory};

/// `nvmlDevice_t` — an opaque driver handle.
type NvmlDevice = *mut c_void;

/// `NVML_SUCCESS`.
const NVML_SUCCESS: c_int = 0;

/// Generous fixed buffers. NVML is told the size, so over-allocating is free
/// insurance against a future driver returning a longer string.
const UUID_BUFFER: usize = 128;
const NAME_BUFFER: usize = 128;

/// `nvmlPciInfo_t`, in its `_v3` layout.
///
/// The trailing reserve is deliberate defensive padding: NVML writes through
/// this pointer, and a future driver whose struct grew would otherwise write
/// past the allocation. PULSE reads only the three fields it needs and ignores
/// the rest.
#[repr(C)]
#[derive(Clone, Copy)]
struct NvmlPciInfo {
    bus_id_legacy: [c_char; 16],
    domain: c_uint,
    bus: c_uint,
    device: c_uint,
    pci_device_id: c_uint,
    pci_subsystem_id: c_uint,
    bus_id: [c_char; 32],
    /// Never read. Space for a struct that grew in a newer driver.
    _reserve: [u8; 128],
}

impl Default for NvmlPciInfo {
    fn default() -> Self {
        // All fields are plain old data, so a zeroed struct is valid.
        // SAFETY: `NvmlPciInfo` contains only integers and byte arrays.
        unsafe { core::mem::zeroed() }
    }
}

/// `nvmlUtilization_t`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NvmlUtilization {
    gpu: c_uint,
    memory: c_uint,
}

/// `nvmlMemory_t`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NvmlMemoryRaw {
    total: u64,
    free: u64,
    used: u64,
}

/// The NVML entry points PULSE binds.
///
/// Only what this phase needs. Temperature, power, fan and encoder entry
/// points exist in NVML and are deliberately absent — they belong to the
/// sensors phase, and binding a symbol PULSE does not use would be one more
/// thing that could fail at startup for no benefit.
#[allow(non_snake_case)]
struct NvmlSymbols {
    nvmlInit_v2: unsafe extern "C" fn() -> c_int,
    nvmlShutdown: unsafe extern "C" fn() -> c_int,
    nvmlDeviceGetCount_v2: unsafe extern "C" fn(*mut c_uint) -> c_int,
    nvmlDeviceGetHandleByIndex_v2: unsafe extern "C" fn(c_uint, *mut NvmlDevice) -> c_int,
    nvmlDeviceGetUUID: unsafe extern "C" fn(NvmlDevice, *mut c_char, c_uint) -> c_int,
    nvmlDeviceGetName: unsafe extern "C" fn(NvmlDevice, *mut c_char, c_uint) -> c_int,
    nvmlDeviceGetPciInfo_v3: unsafe extern "C" fn(NvmlDevice, *mut NvmlPciInfo) -> c_int,
    nvmlDeviceGetUtilizationRates: unsafe extern "C" fn(NvmlDevice, *mut NvmlUtilization) -> c_int,
    nvmlDeviceGetMemoryInfo: unsafe extern "C" fn(NvmlDevice, *mut NvmlMemoryRaw) -> c_int,
    nvmlDeviceGetClockInfo: unsafe extern "C" fn(NvmlDevice, c_uint, *mut c_uint) -> c_int,
}

/// A loaded, initialised NVML, with its devices enumerated.
pub struct NvmlLibrary {
    /// Held purely to keep the module loaded: its `Drop` unloads the library,
    /// and it must outlive every resolved symbol and device handle below.
    #[allow(dead_code)]
    handle: ModuleHandle,
    symbols: NvmlSymbols,
    /// Device handles, cached at construction so a refresh does not re-look
    /// them up. Index into this vector is the enumeration index.
    devices: Vec<NvmlDevice>,
}

// SAFETY: NVML is documented as thread-safe; its handles are plain driver
// tokens with no thread affinity, and PULSE never mutates any of these fields
// after construction. The device handles cannot outlive the library because
// every accessor borrows `&self`, and `Drop` runs only when no borrow remains.
unsafe impl Send for NvmlLibrary {}
// SAFETY: see above — all access is read-only after construction.
unsafe impl Sync for NvmlLibrary {}

impl std::fmt::Debug for NvmlLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NvmlLibrary")
            .field("devices", &self.devices.len())
            .finish()
    }
}

impl NvmlLibrary {
    /// Loads NVML, initialises it and enumerates its devices.
    ///
    /// Every failure — library absent, symbol absent, init refused — is a
    /// structured [`NvmlError`], never a panic. There is no `unwrap` or
    /// `expect` anywhere on this path.
    pub fn load() -> Result<Self, NvmlError> {
        let handle = ModuleHandle::open()?;
        let symbols = NvmlSymbols::resolve(&handle)?;

        // SAFETY: the symbol was resolved from the loaded module and matches
        // the documented `nvmlInit_v2` signature, which takes no arguments.
        let status = unsafe { (symbols.nvmlInit_v2)() };
        if status != NVML_SUCCESS {
            // The module is dropped here, unloading the library cleanly.
            return Err(NvmlError::from_status(status));
        }

        let mut library = Self {
            handle,
            symbols,
            devices: Vec::new(),
        };

        library.devices = library.enumerate_devices()?;

        Ok(library)
    }

    /// Fetches every device handle once.
    fn enumerate_devices(&self) -> Result<Vec<NvmlDevice>, NvmlError> {
        let mut count: c_uint = 0;

        // SAFETY: `count` is a live local of the expected type.
        let status = unsafe { (self.symbols.nvmlDeviceGetCount_v2)(&mut count) };
        if status != NVML_SUCCESS {
            return Err(NvmlError::from_status(status));
        }

        let mut devices = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut device: NvmlDevice = core::ptr::null_mut();

            // SAFETY: `device` is a live local; the callee writes a handle
            // only on success, which is checked before it is stored.
            let status =
                unsafe { (self.symbols.nvmlDeviceGetHandleByIndex_v2)(index, &mut device) };

            if status == NVML_SUCCESS && !device.is_null() {
                devices.push(device);
            }
            // A device that cannot be handled is skipped rather than failing
            // the whole enumeration: one unreachable GPU must not hide the
            // others.
        }

        Ok(devices)
    }

    fn device(&self, index: u32) -> Result<NvmlDevice, NvmlError> {
        self.devices
            .get(index as usize)
            .copied()
            .ok_or(NvmlError::NotFound)
    }

    /// Reads a NUL-terminated string the driver wrote into a fixed buffer.
    ///
    /// # Safety
    ///
    /// The buffer is a live, fully initialised local of `LEN` bytes, and `LEN`
    /// is what the callee was told, so it cannot write past the end. The
    /// result is read with `CStr::from_ptr` only after a success status, and
    /// only after confirming a NUL is present within the buffer — a driver
    /// that filled it completely without terminating would otherwise read out
    /// of bounds.
    fn read_string<const LEN: usize>(
        &self,
        device: NvmlDevice,
        call: unsafe extern "C" fn(NvmlDevice, *mut c_char, c_uint) -> c_int,
    ) -> Result<String, NvmlError> {
        let mut bytes: [c_char; LEN] = [0; LEN];

        // SAFETY: see the function docs.
        let status = unsafe { call(device, bytes.as_mut_ptr(), LEN as c_uint) };
        if status != NVML_SUCCESS {
            return Err(NvmlError::from_status(status));
        }

        // A driver that filled the buffer without terminating it would make
        // `CStr::from_ptr` read past the end, so the NUL is confirmed first.
        if !bytes.contains(&0) {
            return Err(NvmlError::Other(-1));
        }

        // SAFETY: a NUL was confirmed inside the buffer, so the string is
        // terminated within the allocation.
        let text = unsafe { CStr::from_ptr(bytes.as_ptr()) };

        Ok(text.to_string_lossy().into_owned())
    }
}

impl Drop for NvmlLibrary {
    /// Shuts NVML down before the library is unloaded.
    ///
    /// Runs only when no borrow of `self` remains, so no device handle can be
    /// in use on another thread at this point.
    fn drop(&mut self) {
        // SAFETY: `nvmlShutdown` takes no arguments and is the documented
        // counterpart of the `nvmlInit_v2` that succeeded in `load`.
        unsafe {
            (self.symbols.nvmlShutdown)();
        }
        // `handle` unloads the module in its own Drop, after this.
    }
}

impl NvmlBackend for NvmlLibrary {
    fn device_count(&self) -> Result<u32, NvmlError> {
        Ok(self.devices.len() as u32)
    }

    fn device_info(&self, index: u32) -> Result<NvmlDeviceInfo, NvmlError> {
        let device = self.device(index)?;

        let uuid = self.read_string::<UUID_BUFFER>(device, self.symbols.nvmlDeviceGetUUID)?;
        let name = self
            .read_string::<NAME_BUFFER>(device, self.symbols.nvmlDeviceGetName)
            // A driver that will not name the card is odd but not fatal; the
            // UUID is what matters, and the UI can fall back to a generic name.
            .unwrap_or_else(|_| "NVIDIA GPU".to_string());

        let mut info = NvmlPciInfo::default();

        // SAFETY: `info` is a live, zeroed, fully initialised struct with
        // defensive trailing space; the callee writes only within it.
        let status = unsafe { (self.symbols.nvmlDeviceGetPciInfo_v3)(device, &mut info) };

        let pci = (status == NVML_SUCCESS).then(|| {
            PciAddress::new(
                info.domain,
                (info.bus & 0xFF) as u8,
                (info.device & 0xFF) as u8,
                // NVML reports the GPU function, which is always 0 for the
                // graphics device; the audio function is a separate device.
                0,
            )
        });

        Ok(NvmlDeviceInfo { uuid, name, pci })
    }

    fn utilization(&self, index: u32) -> Result<u32, NvmlError> {
        let device = self.device(index)?;
        let mut utilization = NvmlUtilization::default();

        // SAFETY: `utilization` is a live, initialised local of the expected
        // type; the callee writes only within it.
        let status =
            unsafe { (self.symbols.nvmlDeviceGetUtilizationRates)(device, &mut utilization) };
        if status != NVML_SUCCESS {
            return Err(NvmlError::from_status(status));
        }

        Ok(utilization.gpu)
    }

    fn memory(&self, index: u32) -> Result<NvmlMemory, NvmlError> {
        let device = self.device(index)?;
        let mut memory = NvmlMemoryRaw::default();

        // SAFETY: `memory` is a live, initialised local of the expected type.
        let status = unsafe { (self.symbols.nvmlDeviceGetMemoryInfo)(device, &mut memory) };
        if status != NVML_SUCCESS {
            return Err(NvmlError::from_status(status));
        }

        Ok(NvmlMemory {
            total: memory.total,
            used: memory.used,
            free: memory.free,
        })
    }

    fn clock_mhz(&self, index: u32, clock: NvmlClock) -> Result<u32, NvmlError> {
        let device = self.device(index)?;
        let mut megahertz: c_uint = 0;

        // SAFETY: `megahertz` is a live local; the clock type is one of the
        // documented `nvmlClockType_t` values.
        let status = unsafe {
            (self.symbols.nvmlDeviceGetClockInfo)(device, clock.as_raw(), &mut megahertz)
        };
        if status != NVML_SUCCESS {
            return Err(NvmlError::from_status(status));
        }

        Ok(megahertz)
    }
}

// --- platform-specific module loading ------------------------------------

/// A loaded shared library, unloaded on drop.
struct ModuleHandle {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    raw: *mut c_void,
}

#[cfg(target_os = "linux")]
mod loader {
    use std::ffi::{c_char, c_int, c_void};

    /// `RTLD_NOW | RTLD_LOCAL`: resolve everything up front so a half-usable
    /// library is refused here rather than crashing at the first call, and keep
    /// its symbols out of the global namespace.
    pub const RTLD_NOW: c_int = 2;
    pub const RTLD_LOCAL: c_int = 0;

    // `dlopen` and friends live in libc, which every Rust binary already
    // links; this is not an optional dependency and cannot fail to resolve.
    extern "C" {
        pub fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        pub fn dlclose(handle: *mut c_void) -> c_int;
    }

    /// The driver's SONAME — never the unversioned development symlink.
    pub const NVML_SONAME: &[u8] = b"libnvidia-ml.so.1\0";
}

impl ModuleHandle {
    #[cfg(target_os = "linux")]
    fn open() -> Result<Self, NvmlError> {
        // SAFETY: a `'static` NUL-terminated name and documented flags; the
        // returned pointer is checked for null before use.
        let raw = unsafe {
            loader::dlopen(
                loader::NVML_SONAME.as_ptr() as *const c_char,
                loader::RTLD_NOW | loader::RTLD_LOCAL,
            )
        };

        if raw.is_null() {
            return Err(NvmlError::Unavailable(
                "libnvidia-ml.so.1 is not installed, so NVIDIA telemetry is unavailable \
                 (this is expected without the proprietary NVIDIA driver)"
                    .to_string(),
            ));
        }

        Ok(Self { raw })
    }

    #[cfg(target_os = "windows")]
    fn open() -> Result<Self, NvmlError> {
        use windows_sys::Win32::System::LibraryLoader::{
            LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
        };

        // `nvml.dll`, NUL-terminated UTF-16.
        const NVML_DLL: &[u16] = &[
            b'n' as u16,
            b'v' as u16,
            b'm' as u16,
            b'l' as u16,
            b'.' as u16,
            b'd' as u16,
            b'l' as u16,
            b'l' as u16,
            0,
        ];

        // SAFETY: a `'static` NUL-terminated UTF-16 name, a null reserved
        // handle as the API requires, and a search flag that confines the
        // lookup to System32 — never the application or working directory.
        // The result is checked for null before use.
        let raw = unsafe {
            LoadLibraryExW(
                NVML_DLL.as_ptr(),
                core::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };

        if raw.is_null() {
            return Err(NvmlError::Unavailable(
                "nvml.dll was not found in the system directory, so NVIDIA telemetry is \
                 unavailable (this is expected without the NVIDIA display driver)"
                    .to_string(),
            ));
        }

        Ok(Self { raw: raw.cast() })
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn open() -> Result<Self, NvmlError> {
        Err(NvmlError::Unavailable(
            "NVIDIA telemetry is not implemented on this platform".to_string(),
        ))
    }

    /// Resolves one symbol, or reports precisely which one was missing.
    ///
    /// # Safety
    ///
    /// `name` must be a NUL-terminated byte string. The returned address is
    /// transmuted by the caller onto the signature declared in [`NvmlSymbols`],
    /// which is the documented NVML signature for that entry point.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    fn symbol(&self, name: &[u8]) -> Result<*mut c_void, NvmlError> {
        #[cfg(target_os = "linux")]
        // SAFETY: a NUL-terminated name and a handle from a successful
        // `dlopen`; a missing symbol yields null, handled below.
        let address = unsafe { loader::dlsym(self.raw, name.as_ptr() as *const c_char) };

        #[cfg(target_os = "windows")]
        let address = {
            use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

            // SAFETY: a NUL-terminated ANSI name and a handle from a
            // successful LoadLibraryExW; a missing export yields None.
            let procedure = unsafe { GetProcAddress(self.raw.cast(), name.as_ptr()) };

            match procedure {
                Some(procedure) => procedure as *mut c_void,
                None => core::ptr::null_mut(),
            }
        };

        if address.is_null() {
            let symbol = String::from_utf8_lossy(&name[..name.len().saturating_sub(1)]);
            return Err(NvmlError::Unavailable(format!(
                "the installed NVIDIA management library does not export {symbol}"
            )));
        }

        Ok(address)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn symbol(&self, _name: &[u8]) -> Result<*mut c_void, NvmlError> {
        Err(NvmlError::Unavailable(
            "NVIDIA telemetry is not implemented on this platform".to_string(),
        ))
    }
}

impl Drop for ModuleHandle {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        // SAFETY: `raw` came from a successful `dlopen` and is released once.
        unsafe {
            loader::dlclose(self.raw);
        }

        #[cfg(target_os = "windows")]
        // SAFETY: `raw` came from a successful LoadLibraryExW and is released
        // once.
        unsafe {
            windows_sys::Win32::Foundation::FreeLibrary(self.raw.cast());
        }
    }
}

impl NvmlSymbols {
    /// Resolves every entry point PULSE needs.
    ///
    /// All of these are required: a library missing any of them is too old or
    /// too unusual to serve this phase, and saying so once at load time beats
    /// discovering it per metric. Optional symbols, if a later phase needs
    /// any, would be resolved into an `Option` instead.
    ///
    /// # Safety
    ///
    /// Every address comes from the module just loaded, and a missing symbol
    /// is refused above rather than converted from null. Each conversion
    /// targets the signature declared in [`NvmlSymbols`], which is the
    /// documented NVML signature for that entry point — that declaration is
    /// the only thing checking the ABI, since a resolved address carries no
    /// type information.
    #[allow(non_snake_case)]
    fn resolve(handle: &ModuleHandle) -> Result<Self, NvmlError> {
        Ok(Self {
            // SAFETY: see the function docs, and `as_symbol` below.
            nvmlInit_v2: unsafe { as_symbol(handle.symbol(b"nvmlInit_v2\0")?) },
            // SAFETY: see the function docs.
            nvmlShutdown: unsafe { as_symbol(handle.symbol(b"nvmlShutdown\0")?) },
            // SAFETY: see the function docs.
            nvmlDeviceGetCount_v2: unsafe { as_symbol(handle.symbol(b"nvmlDeviceGetCount_v2\0")?) },
            // SAFETY: see the function docs.
            nvmlDeviceGetHandleByIndex_v2: unsafe {
                as_symbol(handle.symbol(b"nvmlDeviceGetHandleByIndex_v2\0")?)
            },
            // SAFETY: see the function docs.
            nvmlDeviceGetUUID: unsafe { as_symbol(handle.symbol(b"nvmlDeviceGetUUID\0")?) },
            // SAFETY: see the function docs.
            nvmlDeviceGetName: unsafe { as_symbol(handle.symbol(b"nvmlDeviceGetName\0")?) },
            // SAFETY: see the function docs.
            nvmlDeviceGetPciInfo_v3: unsafe {
                as_symbol(handle.symbol(b"nvmlDeviceGetPciInfo_v3\0")?)
            },
            // SAFETY: see the function docs.
            nvmlDeviceGetUtilizationRates: unsafe {
                as_symbol(handle.symbol(b"nvmlDeviceGetUtilizationRates\0")?)
            },
            // SAFETY: see the function docs.
            nvmlDeviceGetMemoryInfo: unsafe {
                as_symbol(handle.symbol(b"nvmlDeviceGetMemoryInfo\0")?)
            },
            // SAFETY: see the function docs.
            nvmlDeviceGetClockInfo: unsafe {
                as_symbol(handle.symbol(b"nvmlDeviceGetClockInfo\0")?)
            },
        })
    }
}

/// Reinterprets a resolved address as a function pointer of type `F`.
///
/// One place where the conversion happens, so the reasoning is written once
/// rather than ten times.
///
/// # Safety
///
/// `address` must be a non-null address of a function whose ABI and signature
/// are exactly `F`. The size assertion catches the one mistake the compiler
/// could otherwise not: `F` not being a plain function pointer.
unsafe fn as_symbol<F: Copy>(address: *mut c_void) -> F {
    debug_assert_eq!(
        core::mem::size_of::<F>(),
        core::mem::size_of::<*mut c_void>(),
        "a resolved symbol must be a plain function pointer"
    );

    // SAFETY: the caller guarantees the signature; both sides are pointer-sized
    // and the source is a live code address from the loaded module.
    unsafe { core::mem::transmute_copy(&address) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_library_is_reported_rather_than_fatal() {
        // On a machine without the proprietary driver this exercises the real
        // failure path; on one with it, the load succeeds. Both are correct —
        // what must never happen is a panic.
        match NvmlLibrary::load() {
            Ok(library) => {
                let count = library
                    .device_count()
                    .expect("a loaded NVML reports a count");
                // Enumeration succeeded; every handle must resolve.
                for index in 0..count {
                    assert!(library.device(index).is_ok());
                }
                assert!(
                    library.device(count).is_err(),
                    "out of range must not panic"
                );
            }
            Err(error) => {
                assert!(!error.message().is_empty());
                // And it must be classified as an absent capability, not a bug.
                assert_eq!(
                    super::super::availability_for_nvml(&error).status_str(),
                    "unsupported"
                );
            }
        }
    }

    #[test]
    fn the_library_handle_is_shareable_across_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NvmlLibrary>();
    }
}
