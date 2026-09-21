//! Opening Windows disks and asking them about themselves.
//!
//! # Enumeration: SetupAPI, not a loop over `PhysicalDriveN`
//!
//! The obvious way to find the disks is to open `\\.\PhysicalDrive0`,
//! `\\.\PhysicalDrive1`, … until one fails. It is wrong in both directions:
//! the numbers are **not contiguous**, so a machine whose disks are numbered
//! 0, 1 and 3 would lose the third; and a number that fails to open for a
//! permissions reason ends the loop early, hiding every disk after it.
//!
//! `SetupDiGetClassDevsW` over `GUID_DEVINTERFACE_DISK` asks the device
//! manager which disk interfaces are actually present. That is also where the
//! **device instance ID** comes from, which is the stable Windows identifier
//! PULSE falls back to when a device reports no usable serial.
//!
//! # No subprocess
//!
//! PULSE runs no `PowerShell`, `wmic`, `diskpart`, `fsutil`, `Get-PhysicalDisk`
//! or `Get-Disk`, here or anywhere.
//!
//! # Read-only
//!
//! Every handle in this module is opened with **no access rights requested** —
//! `dwDesiredAccess = 0`. That is enough to issue the informational device
//! controls below and is not enough to read or write a single sector, so no
//! code path here can modify a disk even by mistake. Nothing in PULSE opens a
//! disk for writing.

#[cfg(target_os = "windows")]
use std::ffi::c_void;

use crate::metrics::model::{MetricError, MetricErrorCode};

#[cfg_attr(not(target_os = "windows"), allow(unused_imports))]
use super::identity;
use super::ioctl;

/// What one physical disk reported about itself.
///
/// Produced by the FFI below and consumed by the pure descriptor logic in
/// `super`, so the two can be tested apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskReport {
    /// The `PhysicalDriveN` number. A handle, never an identity.
    pub number: u32,
    /// The device instance ID SetupAPI assigns.
    pub instance_id: Option<String>,
    pub descriptor: ioctl::StorageDeviceDescriptorData,
    pub capacity_bytes: Option<u64>,
    /// Whether `IOCTL_DISK_PERFORMANCE` answered during discovery.
    ///
    /// Probed once: whether the counters are collected for a disk is a
    /// property of the machine's configuration, not of the moment.
    pub performance_available: bool,
}

/// Maps a Win32 failure onto the error code that describes it honestly.
///
/// The distinctions the availability contract needs, and that a generic "I/O
/// error" would destroy:
///
/// - `ERROR_ACCESS_DENIED` — PULSE is not elevated. Actionable.
/// - `ERROR_INVALID_FUNCTION` / `ERROR_NOT_SUPPORTED` — the driver does not
///   implement this control. Not actionable, and not a permissions problem.
/// - `ERROR_NO_SUCH_DEVICE` — it was unplugged between enumeration and now.
pub fn from_win32(code: u32, what: &str) -> MetricError {
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_INVALID_FUNCTION: u32 = 1;
    const ERROR_NOT_SUPPORTED: u32 = 50;
    const ERROR_INVALID_PARAMETER: u32 = 87;
    const ERROR_NO_SUCH_DEVICE: u32 = 433;
    const ERROR_DEVICE_NOT_CONNECTED: u32 = 1167;

    let (kind, message) = match code {
        ERROR_ACCESS_DENIED => (
            MetricErrorCode::PermissionDenied,
            format!("Windows refused {what} to an unelevated process"),
        ),
        ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED | ERROR_INVALID_PARAMETER => (
            MetricErrorCode::Unsupported,
            format!("the driver for this device does not implement {what}"),
        ),
        ERROR_FILE_NOT_FOUND | ERROR_NO_SUCH_DEVICE | ERROR_DEVICE_NOT_CONNECTED => (
            MetricErrorCode::NotDetected,
            format!("the device is no longer connected, so {what} could not be read"),
        ),
        other => (
            MetricErrorCode::Io,
            format!("{what} failed with Windows error {other}"),
        ),
    };

    MetricError::new(kind, message)
}

// --- the Windows implementation -------------------------------------------

#[cfg(target_os = "windows")]
mod imp {
    use super::*;

    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE,
        DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVINFO_DATA,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_NO_MORE_ITEMS, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    /// What `SetupDiGetClassDevsW` returns on failure.
    const INVALID_DEVINFO_SET: HDEVINFO = -1;

    /// `GUID_DEVINTERFACE_DISK` — `{53f56307-b6bf-11d0-94f2-00a0c91efb8b}`.
    const GUID_DEVINTERFACE_DISK: GUID = GUID {
        data1: 0x53f5_6307,
        data2: 0xb6bf,
        data3: 0x11d0,
        data4: [0x94, 0xf2, 0x00, 0xa0, 0xc9, 0x1e, 0xfb, 0x8b],
    };

    /// An owned device handle, closed when it goes out of scope.
    ///
    /// A guard rather than a bare `HANDLE`: every early return in this module
    /// would otherwise be a leak, and there are several.
    pub struct DeviceHandle(HANDLE);

    impl DeviceHandle {
        /// Opens a device path for **information only**.
        ///
        /// `dwDesiredAccess = 0` requests neither read nor write access. It is
        /// what lets the queries below work without elevation, and it means
        /// this handle physically cannot touch the disk's contents.
        pub fn open(path: &str) -> Result<Self, MetricError> {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

            // SAFETY: `wide` is a valid NUL-terminated wide string that
            // outlives the call. Every other argument is a documented
            // constant, and the returned handle is checked before use.
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };

            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                // SAFETY: reads a thread-local error code set by the call above.
                return Err(from_win32(unsafe { GetLastError() }, "opening this device"));
            }

            Ok(Self(handle))
        }

        /// Issues one device control with no input buffer.
        pub fn control(&self, code: u32, output: &mut [u8]) -> Result<usize, MetricError> {
            self.control_with(code, &mut [], output)
        }

        /// Issues one device control.
        pub fn control_with(
            &self,
            code: u32,
            input: &mut [u8],
            output: &mut [u8],
        ) -> Result<usize, MetricError> {
            let mut returned: u32 = 0;

            // SAFETY: both slices are owned by the caller and outlive the
            // call, and their lengths are passed alongside their pointers, so
            // the driver cannot write past `output`. Every control code used
            // here is an informational query.
            let ok = unsafe {
                DeviceIoControl(
                    self.0,
                    code,
                    if input.is_empty() {
                        std::ptr::null_mut()
                    } else {
                        input.as_mut_ptr() as *mut c_void
                    },
                    input.len() as u32,
                    if output.is_empty() {
                        std::ptr::null_mut()
                    } else {
                        output.as_mut_ptr() as *mut c_void
                    },
                    output.len() as u32,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };

            if ok == 0 {
                // SAFETY: reads a thread-local error code set by the call above.
                return Err(from_win32(unsafe { GetLastError() }, "a device query"));
            }

            Ok(returned as usize)
        }
    }

    impl Drop for DeviceHandle {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a handle this type opened and has not closed.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// A SetupAPI device information set, destroyed when it goes out of scope.
    struct DeviceInfoSet(HDEVINFO);

    impl Drop for DeviceInfoSet {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a set this type created and has not destroyed.
            unsafe {
                SetupDiDestroyDeviceInfoList(self.0);
            }
        }
    }

    /// The device paths and instance IDs of every disk currently present.
    ///
    /// Returns `(interface path, device instance id)` pairs. The interface
    /// path is what `CreateFileW` opens; the instance ID is the stable Windows
    /// identifier.
    fn enumerate_disk_interfaces() -> Vec<(String, Option<String>)> {
        // SAFETY: the GUID is a documented constant and the flags are
        // documented values; the returned set is checked before use and owned
        // by the guard below.
        let set = unsafe {
            SetupDiGetClassDevsW(
                &GUID_DEVINTERFACE_DISK,
                std::ptr::null(),
                std::ptr::null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        };

        // `HDEVINFO` is an `isize`, and its documented failure value is -1.
        if set == INVALID_DEVINFO_SET {
            return Vec::new();
        }
        let set = DeviceInfoSet(set);

        let mut found = Vec::new();

        for index in 0.. {
            let mut interface: SP_DEVICE_INTERFACE_DATA = unsafe { std::mem::zeroed() };
            interface.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;

            // SAFETY: `interface` is a correctly sized, zeroed structure with
            // its `cbSize` set, as the function requires.
            let ok = unsafe {
                SetupDiEnumDeviceInterfaces(
                    set.0,
                    std::ptr::null_mut(),
                    &GUID_DEVINTERFACE_DISK,
                    index,
                    &mut interface,
                )
            };

            if ok == 0 {
                // SAFETY: reads a thread-local error code set by the call above.
                if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                    break;
                }
                // Any other failure ends the enumeration too: continuing would
                // spin on an index the API has already refused.
                break;
            }

            let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
            info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;

            // The detail structure is variable-length. A generous fixed buffer
            // avoids the two-call size dance for paths that are, in practice,
            // well under 200 characters — and the call is told the buffer's
            // real size, so a longer one fails cleanly rather than overflowing.
            let mut detail = [0_u8; 1024];
            // `cbSize` is the size of the *fixed* part of
            // `SP_DEVICE_INTERFACE_DETAIL_DATA_W`, not of the buffer: 8 on
            // 64-bit (a `u32` plus one `u16` of the path, padded).
            detail[0..4].copy_from_slice(&8_u32.to_le_bytes());

            let mut required: u32 = 0;
            // SAFETY: `detail` is a 1024-byte buffer whose length is passed to
            // the call, with the `cbSize` prefix the API documents.
            let ok = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    set.0,
                    &interface,
                    detail.as_mut_ptr() as *mut _,
                    detail.len() as u32,
                    &mut required,
                    &mut info,
                )
            };

            if ok == 0 {
                continue;
            }

            // The path is UTF-16 starting after the 4-byte `cbSize`.
            let units: Vec<u16> = detail[4..]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            let path = identity::utf16_to_string(&units);

            if path.is_empty() {
                continue;
            }

            found.push((path, instance_id(&set, &mut info)));
        }

        found
    }

    /// One device's instance ID, when SetupAPI reports one.
    fn instance_id(set: &DeviceInfoSet, info: &mut SP_DEVINFO_DATA) -> Option<String> {
        let mut buffer = [0_u16; 512];
        let mut required: u32 = 0;

        // SAFETY: `buffer` is a 512-unit array whose length is passed to the
        // call, and `info` was filled by the enumeration above.
        let ok = unsafe {
            SetupDiGetDeviceInstanceIdW(
                set.0,
                info,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut required,
            )
        };

        if ok == 0 {
            return None;
        }

        let value = identity::utf16_to_string(&buffer);
        (!value.is_empty()).then_some(value)
    }

    /// Asks one open device for its `STORAGE_DEVICE_DESCRIPTOR`.
    fn query_descriptor(
        handle: &DeviceHandle,
    ) -> Result<ioctl::StorageDeviceDescriptorData, MetricError> {
        let mut query = [0_u8; 12];
        query[0..4].copy_from_slice(&ioctl::STORAGE_DEVICE_PROPERTY.to_le_bytes());
        query[4..8].copy_from_slice(&ioctl::PROPERTY_STANDARD_QUERY.to_le_bytes());

        // Generous: the fixed header is 36 bytes and the string tail is a few
        // hundred at most.
        let mut output = [0_u8; 1024];
        let returned =
            handle.control_with(ioctl::IOCTL_STORAGE_QUERY_PROPERTY, &mut query, &mut output)?;

        ioctl::parse_device_descriptor(&output[..returned]).ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                "this device returned a storage descriptor shorter than its own header",
            )
        })
    }

    /// Reads a full report for one disk interface.
    fn report_for(path: &str, instance_id: Option<String>) -> Option<DiskReport> {
        let handle = DeviceHandle::open(path).ok()?;

        let mut number_buffer = [0_u8; 32];
        let returned = handle
            .control(ioctl::IOCTL_STORAGE_GET_DEVICE_NUMBER, &mut number_buffer)
            .ok()?;
        let number = ioctl::device_number(&number_buffer[..returned])?;

        // The descriptor and the capacity are independent: a device that
        // refuses one may still answer the other, and losing the whole disk
        // over either would be the failure mode this whole layer avoids.
        let descriptor = query_descriptor(&handle).unwrap_or_default();

        let mut geometry = [0_u8; 64];
        let capacity_bytes = handle
            .control(ioctl::IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, &mut geometry)
            .ok()
            .and_then(|returned| ioctl::disk_size_bytes(&geometry[..returned]));

        let mut performance = [0_u8; ioctl::DISK_PERFORMANCE_LEN];
        let performance_available = handle
            .control(ioctl::IOCTL_DISK_PERFORMANCE, &mut performance)
            .is_ok();

        Some(DiskReport {
            number,
            instance_id,
            descriptor,
            capacity_bytes,
            performance_available,
        })
    }

    /// Every physical disk currently present, sorted by device number so the
    /// inventory is deterministic across runs.
    pub fn discover() -> Vec<DiskReport> {
        let mut disks: Vec<DiskReport> = enumerate_disk_interfaces()
            .into_iter()
            .filter_map(|(path, instance)| report_for(&path, instance))
            .collect();

        // One physical disk can expose more than one interface. Deduplicating
        // on the device number is what keeps a machine from reporting the same
        // drive twice.
        disks.sort_by_key(|disk| disk.number);
        disks.dedup_by_key(|disk| disk.number);
        disks
    }

    /// Reads one disk's cumulative I/O counters.
    pub fn read_performance(
        number: u32,
    ) -> Result<crate::metrics::wellknown::storage::StorageIoCounters, MetricError> {
        let handle = DeviceHandle::open(&identity::physical_drive_path(number))?;

        let mut buffer = [0_u8; ioctl::DISK_PERFORMANCE_LEN];
        let returned = handle.control(ioctl::IOCTL_DISK_PERFORMANCE, &mut buffer)?;

        ioctl::parse_disk_performance(&buffer[..returned]).ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                "this disk returned a performance structure shorter than its own layout",
            )
        })
    }

    /// Reads one NVMe disk's SMART / Health Information log page.
    ///
    /// The transport differs from Linux's; the 512 bytes that come back are
    /// the same, and are handed to the same shared parser.
    pub fn read_nvme_health(
        number: u32,
    ) -> Result<crate::metrics::wellknown::storage::NvmeHealth, MetricError> {
        use crate::metrics::wellknown::storage::{
            parse_smart_log, SMART_LOG_LEN, SMART_LOG_PAGE_ID,
        };

        let handle = DeviceHandle::open(&identity::physical_drive_path(number))?;

        // `STORAGE_PROPERTY_QUERY` header, then the
        // `STORAGE_PROTOCOL_SPECIFIC_DATA` that says which log page is wanted,
        // then room for the log itself.
        const HEADER: usize = 8;
        const SPECIFIC: usize = 40;
        let mut buffer = vec![0_u8; HEADER + SPECIFIC + SMART_LOG_LEN];

        buffer[0..4]
            .copy_from_slice(&ioctl::STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY.to_le_bytes());
        buffer[4..8].copy_from_slice(&ioctl::PROPERTY_STANDARD_QUERY.to_le_bytes());

        let specific = HEADER;
        buffer[specific..specific + 4].copy_from_slice(&ioctl::PROTOCOL_TYPE_NVME.to_le_bytes());
        buffer[specific + 4..specific + 8]
            .copy_from_slice(&ioctl::NVME_DATA_TYPE_LOG_PAGE.to_le_bytes());
        // `ProtocolDataRequestValue` is the log page identifier: `0x02`, a
        // read. No other value is ever sent from here.
        buffer[specific + 8..specific + 12]
            .copy_from_slice(&u32::from(SMART_LOG_PAGE_ID).to_le_bytes());
        buffer[specific + 16..specific + 20]
            .copy_from_slice(&((HEADER + SPECIFIC) as u32).to_le_bytes());
        buffer[specific + 20..specific + 24].copy_from_slice(&(SMART_LOG_LEN as u32).to_le_bytes());

        let mut output = vec![0_u8; buffer.len()];
        let returned = handle.control_with(
            ioctl::IOCTL_STORAGE_QUERY_PROPERTY,
            &mut buffer,
            &mut output,
        )?;

        let log = ioctl::nvme_log_page(&output[..returned]).ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                "the driver returned no NVMe log page where its own descriptor said one was",
            )
        })?;

        parse_smart_log(log)
    }
}

#[cfg(target_os = "windows")]
pub use imp::{discover, read_nvme_health, read_performance, DeviceHandle};

// --- the non-Windows stand-ins --------------------------------------------
//
// Compiled on Fedora so everything above them — the descriptor parsing, the
// identity rules, the provider's own logic — type checks and is unit-tested
// there. They are never reached: the Windows provider is the only caller.

#[cfg(not(target_os = "windows"))]
pub fn discover() -> Vec<DiskReport> {
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
pub fn read_performance(
    _number: u32,
) -> Result<crate::metrics::wellknown::storage::StorageIoCounters, MetricError> {
    Err(MetricError::new(
        MetricErrorCode::Unsupported,
        "IOCTL_DISK_PERFORMANCE is a Windows interface",
    ))
}

#[cfg(not(target_os = "windows"))]
pub fn read_nvme_health(
    _number: u32,
) -> Result<crate::metrics::wellknown::storage::NvmeHealth, MetricError> {
    Err(MetricError::new(
        MetricErrorCode::Unsupported,
        "IOCTL_STORAGE_QUERY_PROPERTY is a Windows interface",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_denied_is_never_reported_as_unsupported() {
        // An unelevated PULSE must say the OS refused, not that the drive
        // cannot do it.
        let error = from_win32(5, "the disk performance counters");

        assert_eq!(error.code, MetricErrorCode::PermissionDenied);
        assert_eq!(
            crate::metrics::wellknown::availability_for(error).status_str(),
            "permissionDenied"
        );
    }

    #[test]
    fn a_driver_that_does_not_implement_a_control_is_unsupported() {
        for code in [1, 50, 87] {
            let error = from_win32(code, "the NVMe health log");
            assert_eq!(
                error.code,
                MetricErrorCode::Unsupported,
                "Windows error {code}"
            );
        }
    }

    #[test]
    fn a_disconnected_device_is_not_detected() {
        for code in [2, 433, 1167] {
            assert_eq!(
                from_win32(code, "the device descriptor").code,
                MetricErrorCode::NotDetected,
                "Windows error {code}"
            );
        }
    }

    #[test]
    fn an_unrecognised_failure_stays_transient() {
        let error = from_win32(1117, "a device query");

        assert_eq!(error.code, MetricErrorCode::Io);
        assert!(crate::metrics::wellknown::availability_for(error).is_transient());
    }

    #[test]
    fn an_error_names_what_failed() {
        let message = from_win32(5, "the NVMe health log").message;
        assert!(message.contains("the NVMe health log"), "{message}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_stand_ins_report_the_interface_as_windows_only() {
        assert!(discover().is_empty());
        assert_eq!(
            read_performance(0).expect_err("unsupported").code,
            MetricErrorCode::Unsupported
        );
        assert_eq!(
            read_nvme_health(0).expect_err("unsupported").code,
            MetricErrorCode::Unsupported
        );
    }
}
