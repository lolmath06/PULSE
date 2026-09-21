//! The Windows storage IOCTL structures, and the pure logic that reads them.
//!
//! PULSE runs no `PowerShell`, no `wmic`, no `diskpart`, no `fsutil`, and
//! calls no `Get-PhysicalDisk` or `Get-Disk`. Those are programs that issue
//! the device controls declared here; spawning one per refresh would add a
//! process, a serialisation format and a failure mode for information the
//! device already answers directly.
//!
//! # Why the structures are declared here
//!
//! Everything below is a stable, documented Win32 binary layout. Declaring it
//! in PULSE rather than depending on whichever spelling a binding crate
//! version happens to use has one concrete benefit: **the parsing is pure, so
//! it compiles and is tested on Fedora.** A `STORAGE_DEVICE_DESCRIPTOR` is a
//! header followed by byte offsets into a variable-length tail, and getting
//! those offsets wrong is exactly the kind of bug that would otherwise only
//! surface on a Windows machine PULSE has not been run on yet.
//!
//! Every function in this module takes a byte slice and returns a value. The
//! FFI that fills those slices lives in `device.rs` and `volumes.rs` and is
//! gated to Windows; this file is gated to nothing.

use crate::metrics::wellknown::storage::{StorageBus, StorageIoCounters};

// --- device control codes -------------------------------------------------
//
// `CTL_CODE(DeviceType, Function, Method, Access)` expands to
// `(DeviceType << 16) | (Access << 14) | (Function << 2) | Method`. The values
// are spelled out rather than computed so each one can be checked against the
// SDK header by eye, and a test recomputes them from the formula.

/// `IOCTL_STORAGE_QUERY_PROPERTY` — device descriptors and protocol-specific
/// data, including the NVMe log pages.
pub const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
/// `IOCTL_STORAGE_GET_DEVICE_NUMBER` — which `PhysicalDriveN` a handle refers
/// to.
pub const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D_1080;
/// `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` — the device's total capacity.
pub const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_00A0;
/// `IOCTL_DISK_PERFORMANCE` — cumulative per-disk I/O counters.
pub const IOCTL_DISK_PERFORMANCE: u32 = 0x0007_0020;
/// `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` — which physical disks a volume
/// occupies.
pub const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;

/// `STORAGE_PROPERTY_ID::StorageDeviceProperty`.
pub const STORAGE_DEVICE_PROPERTY: u32 = 0;
/// `STORAGE_PROPERTY_ID::StorageDeviceProtocolSpecificProperty`.
pub const STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY: u32 = 49;
/// `STORAGE_QUERY_TYPE::PropertyStandardQuery`.
pub const PROPERTY_STANDARD_QUERY: u32 = 0;

/// `STORAGE_PROTOCOL_TYPE::ProtocolTypeNvme`.
pub const PROTOCOL_TYPE_NVME: u32 = 3;
/// `STORAGE_PROTOCOL_NVME_DATA_TYPE::NVMeDataTypeLogPage`.
pub const NVME_DATA_TYPE_LOG_PAGE: u32 = 2;

/// `STORAGE_PROPERTY_QUERY`, without its variable-length `AdditionalParameters`
/// tail.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct StoragePropertyQuery {
    pub property_id: u32,
    pub query_type: u32,
    pub additional_parameters: [u8; 1],
}

/// `STORAGE_PROTOCOL_SPECIFIC_DATA`, the `AdditionalParameters` payload used to
/// ask a device for one of its protocol's log pages.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct StorageProtocolSpecificData {
    pub protocol_type: u32,
    pub data_type: u32,
    pub protocol_data_request_value: u32,
    pub protocol_data_request_sub_value: u32,
    pub protocol_data_offset: u32,
    pub protocol_data_length: u32,
    pub fixed_protocol_return_data: u32,
    pub protocol_data_request_sub_value2: u32,
    pub protocol_data_request_sub_value3: u32,
    pub protocol_data_request_sub_value4: u32,
}

/// The header of `STORAGE_PROTOCOL_DATA_DESCRIPTOR`: two `u32` before the
/// embedded `STORAGE_PROTOCOL_SPECIFIC_DATA`.
pub const PROTOCOL_DATA_DESCRIPTOR_HEADER_LEN: usize = 8;

/// Offset of `ProtocolDataOffset` within the embedded
/// `STORAGE_PROTOCOL_SPECIFIC_DATA`, which itself starts after the header.
const PROTOCOL_DATA_OFFSET_FIELD: usize = PROTOCOL_DATA_DESCRIPTOR_HEADER_LEN + 16;
/// Offset of `ProtocolDataLength`, immediately after it.
const PROTOCOL_DATA_LENGTH_FIELD: usize = PROTOCOL_DATA_OFFSET_FIELD + 4;

/// `STORAGE_BUS_TYPE`, as `IOCTL_STORAGE_QUERY_PROPERTY` reports it.
///
/// The values are the ones the SDK assigns; only the ones PULSE distinguishes
/// are named, and everything else maps to [`StorageBus::Unknown`] rather than
/// being guessed at.
pub fn bus_from_storage_bus_type(raw: u8) -> StorageBus {
    match raw {
        0x01 => StorageBus::Scsi,    // BusTypeScsi
        0x03 => StorageBus::Ata,     // BusTypeAtapi
        0x04 => StorageBus::Ata,     // BusTypeAta
        0x07 => StorageBus::Usb,     // BusTypeUsb
        0x08 => StorageBus::Scsi,    // BusTypeRAID
        0x0A => StorageBus::Scsi,    // BusTypeSas
        0x0B => StorageBus::Ata,     // BusTypeSata
        0x0C => StorageBus::Mmc,     // BusTypeSd
        0x0D => StorageBus::Mmc,     // BusTypeMmc
        0x0E => StorageBus::Virtual, // BusTypeVirtual
        0x0F => StorageBus::Virtual, // BusTypeFileBackedVirtual
        0x11 => StorageBus::Nvme,    // BusTypeNvme
        _ => StorageBus::Unknown,
    }
}

/// Everything `STORAGE_DEVICE_DESCRIPTOR` says about a device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageDeviceDescriptorData {
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub revision: Option<String>,
    pub serial: Option<String>,
    pub bus: StorageBusOrUnknown,
    pub removable: bool,
}

/// A bus that may not have been reported.
///
/// A newtype rather than a bare [`StorageBus`] so `Default` can mean "not
/// reported" without that colliding with `StorageBus::Unknown`, which means
/// "reported, and PULSE has no name for it".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageBusOrUnknown(pub StorageBus);

impl Default for StorageBusOrUnknown {
    fn default() -> Self {
        Self(StorageBus::Unknown)
    }
}

/// Offsets within `STORAGE_DEVICE_DESCRIPTOR`, in bytes.
///
/// ```text
///  0  u32   Version
///  4  u32   Size
///  8  u8    DeviceType
///  9  u8    DeviceTypeModifier
/// 10  u8    RemovableMedia          (BOOLEAN)
/// 11  u8    CommandQueueing         (BOOLEAN)
/// 12  u32   VendorIdOffset
/// 16  u32   ProductIdOffset
/// 20  u32   ProductRevisionOffset
/// 24  u32   SerialNumberOffset
/// 28  u32   BusType                 (STORAGE_BUS_TYPE)
/// 32  u32   RawPropertiesLength
/// 36  u8[]  RawDeviceProperties
/// ```
const DESCRIPTOR_REMOVABLE: usize = 10;
const DESCRIPTOR_VENDOR_OFFSET: usize = 12;
const DESCRIPTOR_PRODUCT_OFFSET: usize = 16;
const DESCRIPTOR_REVISION_OFFSET: usize = 20;
const DESCRIPTOR_SERIAL_OFFSET: usize = 24;
const DESCRIPTOR_BUS_TYPE: usize = 28;
/// The fixed part of the structure, before the string tail.
pub const DESCRIPTOR_HEADER_LEN: usize = 36;

fn read_u32(buffer: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = buffer.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn read_u64(buffer: &[u8], offset: usize) -> Option<u64> {
    let bytes: [u8; 8] = buffer.get(offset..offset + 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

/// Reads one of the descriptor's NUL-terminated ASCII strings.
///
/// The offsets are **relative to the start of the descriptor**, and an offset
/// of `0` means "this device does not report that field" rather than "the
/// string is at the beginning". Treating a zero offset as a position is the
/// classic misreading, and it makes every device appear to have a vendor
/// string that is really the structure's own version number.
///
/// Padding is trimmed: SCSI fields are space-padded to a fixed width, and a
/// serial number with eight trailing spaces must not become a different
/// identity from the same serial without them.
pub fn descriptor_string(buffer: &[u8], offset_field: usize) -> Option<String> {
    let offset = read_u32(buffer, offset_field)? as usize;

    // Zero means absent. So does an offset past the end of what the device
    // returned, which some USB bridges produce.
    if offset == 0 || offset >= buffer.len() {
        return None;
    }

    let tail = &buffer[offset..];
    let end = tail
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(tail.len());

    let value = String::from_utf8_lossy(&tail[..end]).trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// Parses a `STORAGE_DEVICE_DESCRIPTOR` returned by
/// `IOCTL_STORAGE_QUERY_PROPERTY`.
///
/// Returns `None` for a response too short to contain the fixed header — a
/// truncated answer whose offsets would point into uninitialised memory.
pub fn parse_device_descriptor(buffer: &[u8]) -> Option<StorageDeviceDescriptorData> {
    if buffer.len() < DESCRIPTOR_HEADER_LEN {
        return None;
    }

    Some(StorageDeviceDescriptorData {
        vendor: descriptor_string(buffer, DESCRIPTOR_VENDOR_OFFSET),
        product: descriptor_string(buffer, DESCRIPTOR_PRODUCT_OFFSET),
        revision: descriptor_string(buffer, DESCRIPTOR_REVISION_OFFSET),
        serial: descriptor_string(buffer, DESCRIPTOR_SERIAL_OFFSET),
        bus: StorageBusOrUnknown(bus_from_storage_bus_type(
            read_u32(buffer, DESCRIPTOR_BUS_TYPE)? as u8,
        )),
        removable: buffer[DESCRIPTOR_REMOVABLE] != 0,
    })
}

/// Extracts the NVMe log page from a `STORAGE_PROTOCOL_DATA_DESCRIPTOR`.
///
/// The descriptor does not contain the log page at a fixed position: it
/// carries a `ProtocolDataOffset` **relative to the descriptor's own start**
/// and a `ProtocolDataLength`. Assuming a fixed offset — which happens to be
/// right on many drivers — is how a parser ends up publishing part of the
/// descriptor header as a temperature on the one driver that pads differently.
///
/// Returns `None` when the response is truncated, or when the offset and
/// length it declares do not fit inside it.
pub fn nvme_log_page(buffer: &[u8]) -> Option<&[u8]> {
    if buffer.len() < PROTOCOL_DATA_LENGTH_FIELD + 4 {
        return None;
    }

    let offset = read_u32(buffer, PROTOCOL_DATA_OFFSET_FIELD)? as usize;
    let length = read_u32(buffer, PROTOCOL_DATA_LENGTH_FIELD)? as usize;

    if offset == 0 || length == 0 {
        return None;
    }

    buffer.get(offset..offset.checked_add(length)?)
}

/// The capacity from a `DISK_GEOMETRY_EX`.
///
/// ```text
///  0  DISK_GEOMETRY  Geometry    (24 bytes, 8-byte aligned)
/// 24  i64            DiskSize
/// 32  u8[]           Data
/// ```
///
/// `DiskSize` is the whole device in bytes, which is what
/// `storage.capacity.total` means. Multiplying the cylinder/track/sector
/// geometry instead — the other obvious route — reports the *reported*
/// geometry, which on any modern disk is a fiction the firmware maintains for
/// compatibility and does not equal the real size.
pub const DISK_GEOMETRY_EX_DISK_SIZE: usize = 24;

/// Reads the capacity from a `DISK_GEOMETRY_EX` response.
///
/// A negative or zero size is refused rather than coerced: a disk does not
/// have negative capacity, so such a response is a failed read.
pub fn disk_size_bytes(buffer: &[u8]) -> Option<u64> {
    let raw = read_u64(buffer, DISK_GEOMETRY_EX_DISK_SIZE)? as i64;

    (raw > 0).then_some(raw as u64)
}

/// The `DeviceNumber` field of a `STORAGE_DEVICE_NUMBER`.
///
/// ```text
///  0  u32  DeviceType
///  4  u32  DeviceNumber
///  8  u32  PartitionNumber
/// ```
pub fn device_number(buffer: &[u8]) -> Option<u32> {
    read_u32(buffer, 4)
}

/// `DISK_PERFORMANCE`, as `IOCTL_DISK_PERFORMANCE` fills it.
///
/// ```text
///  0  i64  BytesRead
///  8  i64  BytesWritten
/// 16  i64  ReadTime          100 ns units
/// 24  i64  WriteTime         100 ns units
/// 32  i64  IdleTime
/// 40  u32  ReadCount
/// 44  u32  WriteCount
/// 48  u32  QueueDepth
/// 52  u32  SplitCount
/// 56  i64  QueryTime
/// 64  u32  StorageDeviceNumber
/// 68  u16  StorageManagerName[8]
/// ```
pub const DISK_PERFORMANCE_LEN: usize = 84;

/// Windows counts service time in 100-nanosecond intervals; PULSE's contract
/// is milliseconds.
const HUNDRED_NS_PER_MS: u64 = 10_000;

/// Parses a `DISK_PERFORMANCE` into the shared counter shape.
///
/// # Two unit conversions, both easy to get wrong
///
/// `ReadTime` and `WriteTime` are in **100-nanosecond** units, the same as a
/// `FILETIME`. Publishing them as milliseconds without dividing would report
/// a 0.7 ms read as 7000 ms, and a monitor claiming seven-second latency on a
/// healthy NVMe drive is worse than one that says nothing.
///
/// `ReadCount` and `WriteCount` are 32-bit and wrap on a busy machine. They
/// are widened here, and the shared tracker treats a value that went backwards
/// as a counter reset — which for a wrapped counter is exactly the right
/// response: restart the baseline rather than publish a delta of four billion.
pub fn parse_disk_performance(buffer: &[u8]) -> Option<StorageIoCounters> {
    if buffer.len() < DISK_PERFORMANCE_LEN {
        return None;
    }

    let signed = |offset: usize| -> Option<u64> {
        let raw = read_u64(buffer, offset)? as i64;
        // A negative total is not a total.
        (raw >= 0).then_some(raw as u64)
    };

    Some(StorageIoCounters {
        read_bytes: signed(0)?,
        write_bytes: signed(8)?,
        read_time_ms: signed(16)? / HUNDRED_NS_PER_MS,
        write_time_ms: signed(24)? / HUNDRED_NS_PER_MS,
        read_operations: u64::from(read_u32(buffer, 40)?),
        write_operations: u64::from(read_u32(buffer, 44)?),
    })
}

/// One physical disk a volume occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskExtent {
    /// The `PhysicalDriveN` number.
    pub disk_number: u32,
    pub starting_offset: u64,
    pub extent_length: u64,
}

/// Parses a `VOLUME_DISK_EXTENTS` response.
///
/// ```text
///  0  u32           NumberOfDiskExtents
///  4  u32           (padding to 8-byte alignment)
///  8  DISK_EXTENT[] Extents        24 bytes each
/// ```
///
/// A volume **may** span several disks — a striped or spanned volume does —
/// so the count is read and every extent returned. Hardcoding one extent is
/// correct on almost every machine and silently wrong on the ones that matter.
pub fn parse_disk_extents(buffer: &[u8]) -> Vec<DiskExtent> {
    const HEADER: usize = 8;
    const EXTENT: usize = 24;

    let Some(count) = read_u32(buffer, 0) else {
        return Vec::new();
    };

    (0..count as usize)
        .filter_map(|index| {
            let base = HEADER + index * EXTENT;

            Some(DiskExtent {
                disk_number: read_u32(buffer, base)?,
                starting_offset: read_u64(buffer, base + 8)?,
                extent_length: read_u64(buffer, base + 16)?,
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Builds a `STORAGE_DEVICE_DESCRIPTOR` with a string tail, the way a
    /// driver lays one out.
    pub fn device_descriptor(
        bus_type: u32,
        removable: bool,
        vendor: Option<&str>,
        product: Option<&str>,
        revision: Option<&str>,
        serial: Option<&str>,
    ) -> Vec<u8> {
        let mut buffer = vec![0_u8; DESCRIPTOR_HEADER_LEN];
        let mut offsets = [0_u32; 4];

        for (slot, value) in [vendor, product, revision, serial].into_iter().enumerate() {
            if let Some(value) = value {
                offsets[slot] = buffer.len() as u32;
                buffer.extend_from_slice(value.as_bytes());
                buffer.push(0);
            }
        }

        buffer[DESCRIPTOR_REMOVABLE] = u8::from(removable);
        buffer[DESCRIPTOR_VENDOR_OFFSET..DESCRIPTOR_VENDOR_OFFSET + 4]
            .copy_from_slice(&offsets[0].to_le_bytes());
        buffer[DESCRIPTOR_PRODUCT_OFFSET..DESCRIPTOR_PRODUCT_OFFSET + 4]
            .copy_from_slice(&offsets[1].to_le_bytes());
        buffer[DESCRIPTOR_REVISION_OFFSET..DESCRIPTOR_REVISION_OFFSET + 4]
            .copy_from_slice(&offsets[2].to_le_bytes());
        buffer[DESCRIPTOR_SERIAL_OFFSET..DESCRIPTOR_SERIAL_OFFSET + 4]
            .copy_from_slice(&offsets[3].to_le_bytes());
        buffer[DESCRIPTOR_BUS_TYPE..DESCRIPTOR_BUS_TYPE + 4]
            .copy_from_slice(&bus_type.to_le_bytes());

        buffer
    }

    /// Builds a `STORAGE_PROTOCOL_DATA_DESCRIPTOR` carrying `log` at a chosen
    /// offset, with `padding` bytes of driver-specific slack in between.
    pub fn protocol_data_descriptor(log: &[u8], padding: usize) -> Vec<u8> {
        let header = PROTOCOL_DATA_LENGTH_FIELD + 4;
        let offset = header + padding;

        let mut buffer = vec![0_u8; offset];
        buffer[PROTOCOL_DATA_OFFSET_FIELD..PROTOCOL_DATA_OFFSET_FIELD + 4]
            .copy_from_slice(&(offset as u32).to_le_bytes());
        buffer[PROTOCOL_DATA_LENGTH_FIELD..PROTOCOL_DATA_LENGTH_FIELD + 4]
            .copy_from_slice(&(log.len() as u32).to_le_bytes());
        buffer.extend_from_slice(log);

        buffer
    }

    /// Builds a `DISK_GEOMETRY_EX` reporting `bytes`.
    pub fn disk_geometry_ex(bytes: i64) -> Vec<u8> {
        let mut buffer = vec![0_u8; 32];
        buffer[DISK_GEOMETRY_EX_DISK_SIZE..DISK_GEOMETRY_EX_DISK_SIZE + 8]
            .copy_from_slice(&bytes.to_le_bytes());
        buffer
    }

    /// Builds a `DISK_PERFORMANCE`.
    #[allow(clippy::too_many_arguments)]
    pub fn disk_performance(
        bytes_read: i64,
        bytes_written: i64,
        read_time_100ns: i64,
        write_time_100ns: i64,
        read_count: u32,
        write_count: u32,
    ) -> Vec<u8> {
        let mut buffer = vec![0_u8; DISK_PERFORMANCE_LEN];

        buffer[0..8].copy_from_slice(&bytes_read.to_le_bytes());
        buffer[8..16].copy_from_slice(&bytes_written.to_le_bytes());
        buffer[16..24].copy_from_slice(&read_time_100ns.to_le_bytes());
        buffer[24..32].copy_from_slice(&write_time_100ns.to_le_bytes());
        buffer[40..44].copy_from_slice(&read_count.to_le_bytes());
        buffer[44..48].copy_from_slice(&write_count.to_le_bytes());

        buffer
    }

    /// Builds a `VOLUME_DISK_EXTENTS` covering the given disks.
    pub fn volume_disk_extents(extents: &[(u32, u64, u64)]) -> Vec<u8> {
        let mut buffer = vec![0_u8; 8];
        buffer[0..4].copy_from_slice(&(extents.len() as u32).to_le_bytes());

        for (disk, offset, length) in extents {
            buffer.extend_from_slice(&disk.to_le_bytes());
            buffer.extend_from_slice(&[0_u8; 4]);
            buffer.extend_from_slice(&offset.to_le_bytes());
            buffer.extend_from_slice(&length.to_le_bytes());
        }

        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::metrics::wellknown::storage::{parse_smart_log, SMART_LOG_LEN};

    // --- control codes ----------------------------------------------------

    #[test]
    fn the_control_codes_match_the_ctl_code_formula() {
        const fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
            (device_type << 16) | (access << 14) | (function << 2) | method
        }

        const FILE_DEVICE_DISK: u32 = 0x0007;
        const FILE_DEVICE_MASS_STORAGE: u32 = 0x002D;
        const IOCTL_VOLUME_BASE: u32 = 0x0056;
        const METHOD_BUFFERED: u32 = 0;
        const FILE_ANY_ACCESS: u32 = 0;
        const FILE_READ_ACCESS: u32 = 1;

        assert_eq!(
            IOCTL_STORAGE_QUERY_PROPERTY,
            ctl_code(
                FILE_DEVICE_MASS_STORAGE,
                0x0500,
                METHOD_BUFFERED,
                FILE_ANY_ACCESS
            )
        );
        assert_eq!(
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            ctl_code(
                FILE_DEVICE_MASS_STORAGE,
                0x0420,
                METHOD_BUFFERED,
                FILE_ANY_ACCESS
            )
        );
        assert_eq!(
            IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
            ctl_code(FILE_DEVICE_DISK, 0x0028, METHOD_BUFFERED, FILE_ANY_ACCESS)
        );
        assert_eq!(
            IOCTL_DISK_PERFORMANCE,
            ctl_code(FILE_DEVICE_DISK, 0x0008, METHOD_BUFFERED, FILE_ANY_ACCESS)
        );
        assert_eq!(
            IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
            ctl_code(IOCTL_VOLUME_BASE, 0x0000, METHOD_BUFFERED, FILE_ANY_ACCESS)
        );
        // Pins the access bits so a future edit cannot silently ask for write
        // access to a disk.
        assert_eq!(FILE_READ_ACCESS, 1);
    }

    // --- device descriptor ------------------------------------------------

    #[test]
    fn parses_a_plausible_nvme_descriptor() {
        let buffer = device_descriptor(
            0x11,
            false,
            Some("NVMe"),
            Some("SAMSUNG MZVL22T0HBLB-00B00"),
            Some("GXA7801Q"),
            Some("S677NX0W"),
        );

        let parsed = parse_device_descriptor(&buffer).expect("valid");

        assert_eq!(parsed.bus.0, StorageBus::Nvme);
        assert_eq!(
            parsed.product.as_deref(),
            Some("SAMSUNG MZVL22T0HBLB-00B00")
        );
        assert_eq!(parsed.serial.as_deref(), Some("S677NX0W"));
        assert_eq!(parsed.revision.as_deref(), Some("GXA7801Q"));
        assert!(!parsed.removable);
    }

    #[test]
    fn a_zero_offset_means_absent_not_position_zero() {
        // The classic misreading. Treating offset 0 as a position would return
        // the structure's own version field as a vendor string.
        let buffer = device_descriptor(0x0B, false, None, Some("WDC WDS100T2B0A"), None, None);
        let parsed = parse_device_descriptor(&buffer).expect("valid");

        assert_eq!(parsed.vendor, None);
        assert_eq!(parsed.serial, None);
        assert_eq!(parsed.revision, None);
        assert_eq!(parsed.product.as_deref(), Some("WDC WDS100T2B0A"));
    }

    #[test]
    fn a_usb_bridge_with_no_serial_is_a_device_all_the_same() {
        // Common, and the reason serial cannot be the only identity source.
        let buffer = device_descriptor(
            0x07,
            true,
            Some("Intenso"),
            Some("External USB3.0"),
            None,
            None,
        );
        let parsed = parse_device_descriptor(&buffer).expect("valid");

        assert_eq!(parsed.bus.0, StorageBus::Usb);
        assert!(parsed.removable);
        assert_eq!(parsed.serial, None);
        assert_eq!(parsed.vendor.as_deref(), Some("Intenso"));
    }

    #[test]
    fn padded_scsi_strings_are_trimmed() {
        // SCSI fields are space-padded to a fixed width. `S677NX0W  ` and
        // `S677NX0W` must be one identity, not two.
        let padded = device_descriptor(
            0x0B,
            false,
            Some("ATA     "),
            None,
            None,
            Some("S677NX0W  "),
        );
        let parsed = parse_device_descriptor(&padded).expect("valid");

        assert_eq!(parsed.serial.as_deref(), Some("S677NX0W"));
        assert_eq!(parsed.vendor.as_deref(), Some("ATA"));
    }

    #[test]
    fn a_string_of_only_padding_is_absent_rather_than_empty() {
        let buffer = device_descriptor(0x0B, false, Some("    "), None, None, None);
        assert_eq!(
            parse_device_descriptor(&buffer).expect("valid").vendor,
            None
        );
    }

    #[test]
    fn an_offset_past_the_response_is_refused() {
        // Some bridges report an offset into a longer buffer than they
        // actually returned. Following it would read past the response.
        let mut buffer = device_descriptor(0x07, false, None, None, None, None);
        buffer[DESCRIPTOR_SERIAL_OFFSET..DESCRIPTOR_SERIAL_OFFSET + 4]
            .copy_from_slice(&9999_u32.to_le_bytes());

        assert_eq!(
            parse_device_descriptor(&buffer).expect("valid").serial,
            None
        );
    }

    #[test]
    fn a_truncated_descriptor_is_refused_rather_than_parsed() {
        assert_eq!(parse_device_descriptor(&[0_u8; 8]), None);
        assert_eq!(parse_device_descriptor(&[]), None);
        assert!(parse_device_descriptor(&[0_u8; DESCRIPTOR_HEADER_LEN]).is_some());
    }

    #[test]
    fn every_bus_type_maps_to_a_documented_attachment() {
        assert_eq!(bus_from_storage_bus_type(0x11), StorageBus::Nvme);
        assert_eq!(bus_from_storage_bus_type(0x0B), StorageBus::Ata);
        assert_eq!(bus_from_storage_bus_type(0x04), StorageBus::Ata);
        assert_eq!(bus_from_storage_bus_type(0x07), StorageBus::Usb);
        assert_eq!(bus_from_storage_bus_type(0x0A), StorageBus::Scsi);
        assert_eq!(bus_from_storage_bus_type(0x0E), StorageBus::Virtual);
        assert_eq!(bus_from_storage_bus_type(0x0D), StorageBus::Mmc);
        // Not guessed at.
        assert_eq!(bus_from_storage_bus_type(0xFE), StorageBus::Unknown);
    }

    #[test]
    fn a_usb_attachment_is_never_treated_as_nvme_health_capable() {
        // Even for an NVMe drive in a USB enclosure: what Windows reports is
        // the bridge, and the bridge does not pass the log page through.
        assert!(!bus_from_storage_bus_type(0x07).may_expose_nvme_health());
        assert!(bus_from_storage_bus_type(0x11).may_expose_nvme_health());
    }

    // --- NVMe log page ----------------------------------------------------

    #[test]
    fn the_log_page_is_located_by_the_offset_the_driver_declares() {
        let log = crate::metrics::wellknown::storage::health::fixtures::healthy();

        for padding in [0, 8, 64] {
            let response = protocol_data_descriptor(&log, padding);
            let extracted = nvme_log_page(&response).expect("located");

            assert_eq!(extracted.len(), SMART_LOG_LEN);
            let health = parse_smart_log(extracted).expect("valid");
            assert_eq!(health.percentage_used, Some(3.0));
            assert_eq!(health.power_on_hours, Some(421));
        }
    }

    #[test]
    fn a_response_whose_offset_and_length_do_not_fit_is_refused() {
        // Rather than returning a truncated log the parser would read zeroes
        // out of.
        let log = vec![7_u8; SMART_LOG_LEN];
        let mut response = protocol_data_descriptor(&log, 0);
        response.truncate(response.len() - 100);

        assert_eq!(nvme_log_page(&response), None);
    }

    #[test]
    fn a_descriptor_declaring_no_data_is_refused() {
        let empty = protocol_data_descriptor(&[], 0);
        assert_eq!(nvme_log_page(&empty), None);

        assert_eq!(nvme_log_page(&[0_u8; 4]), None);
        assert_eq!(nvme_log_page(&[]), None);
    }

    #[test]
    fn a_truncated_log_page_fails_the_parser_rather_than_the_extractor() {
        // The extractor returns what the driver said it returned; the shared
        // parser is what refuses a log shorter than the specification.
        let response = protocol_data_descriptor(&[0_u8; 64], 0);
        let extracted = nvme_log_page(&response).expect("located");

        assert_eq!(extracted.len(), 64);
        assert!(parse_smart_log(extracted).is_err());
    }

    // --- capacity ---------------------------------------------------------

    #[test]
    fn the_capacity_comes_from_disk_size_not_the_reported_geometry() {
        let buffer = disk_geometry_ex(2_048_408_248_320);
        assert_eq!(disk_size_bytes(&buffer), Some(2_048_408_248_320));
    }

    #[test]
    fn an_impossible_capacity_is_refused() {
        assert_eq!(disk_size_bytes(&disk_geometry_ex(0)), None);
        assert_eq!(disk_size_bytes(&disk_geometry_ex(-1)), None);
        assert_eq!(disk_size_bytes(&[0_u8; 8]), None);
    }

    #[test]
    fn the_device_number_is_read_from_its_own_field() {
        let mut buffer = vec![0_u8; 12];
        buffer[4..8].copy_from_slice(&3_u32.to_le_bytes());

        assert_eq!(device_number(&buffer), Some(3));
        assert_eq!(device_number(&[0_u8; 2]), None);
    }

    // --- I/O counters -----------------------------------------------------

    #[test]
    fn disk_performance_converts_service_time_from_100ns_to_milliseconds() {
        // 7_000_000 × 100 ns is 700 ms. Publishing the raw figure would claim
        // seven million milliseconds, which is nearly two hours per read.
        let buffer = disk_performance(1024, 2048, 7_000_000, 12_000_000, 100, 20);
        let counters = parse_disk_performance(&buffer).expect("valid");

        assert_eq!(counters.read_time_ms, 700);
        assert_eq!(counters.write_time_ms, 1200);
        assert_eq!(counters.read_bytes, 1024);
        assert_eq!(counters.write_bytes, 2048);
        assert_eq!(counters.read_operations, 100);
        assert_eq!(counters.write_operations, 20);
    }

    #[test]
    fn disk_performance_and_diskstats_describe_the_same_thing() {
        // The point of one shared counter type: given the same activity, both
        // platforms hand the tracker the same numbers, so the same rates come
        // out.
        let windows = parse_disk_performance(&disk_performance(
            1024 * 1024,
            512 * 1024,
            730_000,
            240_000,
            100,
            20,
        ))
        .expect("valid");

        let linux = crate::platform::linux::storage::diskstats::parse_line(
            "259 0 nvme0n1 100 0 2048 73 20 0 1024 24 0 1 1",
        )
        .expect("valid")
        .counters;

        assert_eq!(windows, linux);
    }

    #[test]
    fn a_negative_total_is_refused_rather_than_wrapped() {
        // Casting a negative `i64` to `u64` would produce a number near
        // 18 quintillion and a throughput to match.
        let buffer = disk_performance(-1, 0, 0, 0, 0, 0);
        assert_eq!(parse_disk_performance(&buffer), None);
    }

    #[test]
    fn a_truncated_performance_response_is_refused() {
        assert_eq!(parse_disk_performance(&[0_u8; 40]), None);
        assert!(parse_disk_performance(&[0_u8; DISK_PERFORMANCE_LEN]).is_some());
    }

    #[test]
    fn a_wrapped_32_bit_operation_count_is_visible_as_a_rollback() {
        // Windows counts operations in 32 bits. When they wrap, the shared
        // tracker sees a counter going backwards and restarts the baseline
        // rather than publishing a delta of four billion.
        let before =
            parse_disk_performance(&disk_performance(0, 0, 0, 0, u32::MAX - 5, 0)).expect("valid");
        let after = parse_disk_performance(&disk_performance(0, 0, 0, 0, 10, 0)).expect("valid");

        assert!(after.went_backwards(&before));
    }

    // --- volume extents ---------------------------------------------------

    #[test]
    fn a_simple_volume_maps_to_one_disk() {
        let buffer = volume_disk_extents(&[(0, 1_048_576, 500_000_000_000)]);
        let extents = parse_disk_extents(&buffer);

        assert_eq!(extents.len(), 1);
        assert_eq!(extents[0].disk_number, 0);
        assert_eq!(extents[0].extent_length, 500_000_000_000);
    }

    #[test]
    fn a_spanned_volume_keeps_every_extent() {
        // `1 volume = 1 disk` is true on almost every machine and wrong on a
        // striped or spanned one, which is exactly where getting it wrong
        // matters.
        let buffer = volume_disk_extents(&[(0, 0, 1_000), (1, 0, 2_000), (2, 4096, 3_000)]);
        let extents = parse_disk_extents(&buffer);

        assert_eq!(extents.len(), 3);
        let disks: Vec<u32> = extents.iter().map(|extent| extent.disk_number).collect();
        assert_eq!(disks, [0, 1, 2]);
    }

    #[test]
    fn a_truncated_extent_list_yields_what_it_actually_contains() {
        let mut buffer = volume_disk_extents(&[(0, 0, 1_000), (1, 0, 2_000)]);
        buffer.truncate(8 + 24 + 4);

        assert_eq!(parse_disk_extents(&buffer).len(), 1);
        assert!(parse_disk_extents(&[]).is_empty());
        assert!(parse_disk_extents(&volume_disk_extents(&[])).is_empty());
    }
}
