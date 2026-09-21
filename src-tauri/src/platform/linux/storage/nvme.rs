//! NVMe health on Fedora: two sources, neither of which is a subprocess.
//!
//! PULSE does not run `nvme smart-log` or `smartctl`. Both are programs that
//! issue the ioctl below and print the result; shelling out would add a
//! runtime dependency on a package Fedora does not install by default, a
//! locale-sensitive output format to parse, and a process per refresh.
//!
//! # Two sources, deliberately
//!
//! ```text
//! hwmon      /sys/class/.../nvme/hwmonN/temp*_input   composite temperature
//! ioctl      /dev/nvmeN  NVME_IOCTL_ADMIN_CMD          the whole health log
//! ```
//!
//! The kernel's `nvme` driver registers a hwmon device for every controller
//! and exposes its **composite temperature** — the same figure the health
//! log's first field carries — as a world-readable file. The health log
//! itself needs an admin passthrough on the controller character device, and
//! `/dev/nvme0` is `crw------- root:root` on Fedora.
//!
//! So an ordinary user gets a real temperature and nothing else. That is not a
//! failure to work around: PULSE **must not require root to start**, so the
//! temperature is published from hwmon, the other five health values report
//! `PermissionDenied` with an explanation, and everything else about the
//! device keeps working. Running PULSE as root, or granting the process
//! `CAP_SYS_ADMIN`, makes the rest appear; nothing in the code changes.
//!
//! # Read-only, and narrowly so
//!
//! The only admin command this module can express is `Get Log Page` for log
//! `0x02`. The opcode is a constant, not a parameter. There is no code path
//! here — and none anywhere in PULSE — that can issue `Format NVM`,
//! `Sanitize`, `Firmware Commit`, `Namespace Management` or a destructive
//! self-test. The ioctl structure is filled field by field from constants and
//! a caller-provided buffer length, so a future change that tried to send
//! something else would have to be written on purpose.
//!
//! # Sleeping drives
//!
//! Reading a temperature from a powered-down NVMe controller wakes it. NVMe
//! devices enter and leave their low-power states in microseconds and PULSE
//! only reads on an explicit refresh, so this costs nothing measurable. A
//! spun-down **hard disk** is the case where a health read is genuinely
//! expensive — several seconds and a mechanical spin-up — and PULSE does not
//! read health from ATA devices at all in this phase, so the situation does
//! not arise. See `docs/metrics/storage.md`.

use std::path::{Path, PathBuf};

use crate::metrics::model::{Availability, MetricError, MetricErrorCode};
#[cfg_attr(not(target_os = "linux"), allow(unused_imports))]
use crate::metrics::wellknown::storage::{parse_smart_log, NvmeHealth, SMART_LOG_LEN};
use crate::platform::linux::hwmon;

/// The label the kernel's `nvme` hwmon driver gives the composite sensor.
///
/// The controller also exposes per-die sensors as `Sensor 1`, `Sensor 2` and
/// so on. Those are **not** the composite figure — on the development machine
/// they read 52.85 °C and 57.85 °C against a composite of 52.85 °C — and
/// publishing one of them as the device temperature would quietly report a
/// different sensor from the one the health log means.
const COMPOSITE_LABEL: &str = "Composite";

/// Where the controller character device for an NVMe namespace lives.
///
/// `nvme0n1` (a namespace) is served by controller `nvme0`, and the admin
/// command goes to the controller. Deriving one from the other is a string
/// operation on a kernel naming scheme, so it is a fallible function with its
/// own tests rather than a `format!` at the call site.
pub fn controller_path(namespace: &str) -> Option<PathBuf> {
    let rest = namespace.strip_prefix("nvme")?;
    let (controller, namespace_part) = rest.split_once('n')?;

    if controller.is_empty() || !controller.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // `nvme0n1` splits into `0` and `1`; `nvme0nx` is not a namespace name.
    if namespace_part.is_empty() || !namespace_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    Some(PathBuf::from(format!("/dev/nvme{controller}")))
}

/// The hwmon devices belonging to one NVMe controller.
///
/// Two layouts exist and both are real. A GPU puts its sensors under
/// `<device>/hwmon/hwmonN`, which is what [`hwmon::devices_under`] expects;
/// the kernel's `nvme` driver registers them as **direct children** of the
/// controller directory, `<device>/hwmonN`. Checking only one of the two is
/// how a working sensor ends up reported as absent, so both are searched.
fn hwmon_devices_for(device_dir: &Path) -> Vec<hwmon::HwmonDevice> {
    let mut devices = hwmon::devices_under(device_dir);

    if let Ok(entries) = std::fs::read_dir(device_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_hwmon = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("hwmon"));

            if is_hwmon {
                if let Some(device) = hwmon::HwmonDevice::read(&path) {
                    devices.push(device);
                }
            }
        }
    }

    devices.sort_by(|left, right| left.path.cmp(&right.path));
    devices.dedup_by(|left, right| left.path == right.path);
    devices
}

/// Finds the hwmon input reporting a controller's composite temperature.
///
/// Matches on the sensor **label** rather than on `temp1_input` by position.
/// The composite sensor is conventionally first, but "conventionally" is how a
/// monitor ends up publishing a per-die reading as the device temperature on
/// the one controller that orders them differently.
pub fn composite_temperature_input(device_dir: &Path) -> Option<PathBuf> {
    for device in hwmon_devices_for(device_dir) {
        for channel in device.channels("temp") {
            if channel
                .label
                .as_deref()
                .is_some_and(|label| label.eq_ignore_ascii_case(COMPOSITE_LABEL))
            {
                return Some(channel.input.clone());
            }
        }
    }

    None
}

/// Reads a composite temperature, in degrees Celsius.
pub fn read_temperature(input: &Path) -> Result<f64, MetricError> {
    hwmon::read_temperature_celsius(input)
}

// --- the admin passthrough ------------------------------------------------

/// `NVME_IOCTL_ADMIN_CMD`, as `include/uapi/linux/nvme_ioctl.h` defines it:
/// `_IOWR('N', 0x41, struct nvme_passthru_cmd)`.
#[cfg(target_os = "linux")]
const NVME_IOCTL_ADMIN_CMD: libc::c_ulong = 0xC048_4E41;

/// The `Get Log Page` admin opcode.
///
/// **The only opcode this module can send.** It is a constant here, never a
/// parameter, so nothing downstream can turn this into a write command.
#[cfg(target_os = "linux")]
const OPCODE_GET_LOG_PAGE: u8 = 0x02;

/// The namespace identifier meaning "the whole controller".
#[cfg(target_os = "linux")]
const NSID_ALL: u32 = 0xFFFF_FFFF;

/// `struct nvme_passthru_cmd`, laid out exactly as the kernel expects.
///
/// `repr(C)` and the field order are load-bearing: this structure crosses the
/// ioctl boundary and a mismatch would have the kernel read the buffer pointer
/// out of the wrong offset.
#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Default)]
struct NvmePassthruCmd {
    opcode: u8,
    flags: u8,
    rsvd1: u16,
    nsid: u32,
    cdw2: u32,
    cdw3: u32,
    metadata: u64,
    addr: u64,
    metadata_len: u32,
    data_len: u32,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
    timeout_ms: u32,
    result: u32,
}

/// Builds the `cdw10` word of a `Get Log Page` command.
///
/// Bits 7:0 are the log page identifier; bits 31:16 are `NUMD`, the number of
/// **dwords** to transfer, minus one. Getting the minus-one wrong asks the
/// controller for four bytes too many and some firmware answers with an error
/// rather than a truncated log, so it is computed here and tested.
///
/// Compiled on every platform so the arithmetic is unit-tested from Fedora and
/// from the Windows cross-check alike.
pub fn get_log_page_cdw10(log_id: u8, len_bytes: u32) -> u32 {
    let dwords = len_bytes / 4;
    let numd = dwords.saturating_sub(1);

    u32::from(log_id) | (numd << 16)
}

/// Fetches the 512-byte SMART / Health Information log from a controller.
///
/// Opens the controller **read-only** and issues exactly one `Get Log Page`.
/// The buffer is owned here and its length is what the command declares, so
/// the controller cannot be asked to write past it.
#[cfg(target_os = "linux")]
pub fn read_health(controller: &Path) -> Result<NvmeHealth, MetricError> {
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    let file = File::open(controller).map_err(|error| from_io(error, controller))?;

    let mut buffer = vec![0_u8; SMART_LOG_LEN];

    let mut command = NvmePassthruCmd {
        opcode: OPCODE_GET_LOG_PAGE,
        nsid: NSID_ALL,
        addr: buffer.as_mut_ptr() as u64,
        data_len: SMART_LOG_LEN as u32,
        cdw10: get_log_page_cdw10(
            crate::metrics::wellknown::storage::SMART_LOG_PAGE_ID,
            SMART_LOG_LEN as u32,
        ),
        ..NvmePassthruCmd::default()
    };

    // SAFETY: `file` owns a valid file descriptor for the controller;
    // `command` is a correctly laid out `nvme_passthru_cmd` whose `addr`
    // points at `buffer`, which is `data_len` bytes long and outlives the
    // call. The opcode is `Get Log Page`, a read: the controller writes into
    // `buffer` and modifies no device state.
    let status = unsafe { libc::ioctl(file.as_raw_fd(), NVME_IOCTL_ADMIN_CMD, &mut command) };

    if status < 0 {
        return Err(from_io(std::io::Error::last_os_error(), controller));
    }

    // A non-zero NVMe status means the controller refused the command. The
    // buffer then holds whatever it held before, and parsing it would publish
    // zeroes as a health report.
    if status > 0 {
        return Err(MetricError::new(
            MetricErrorCode::Io,
            format!(
                "the NVMe controller at '{}' refused the health log request (status {status:#06x})",
                controller.display()
            ),
        ));
    }

    parse_smart_log(&buffer)
}

/// Compiled on non-Linux hosts so the Windows cross-check harness type checks
/// this module. Never reached: only the Linux provider calls it.
#[cfg(not(target_os = "linux"))]
pub fn read_health(controller: &Path) -> Result<NvmeHealth, MetricError> {
    Err(MetricError::new(
        MetricErrorCode::Unsupported,
        format!(
            "the NVMe admin passthrough is a Linux interface; '{}' cannot be read here",
            controller.display()
        ),
    ))
}

/// Maps a failed open or ioctl onto the error code that describes it honestly.
///
/// The distinction that matters most: `/dev/nvme0` being root-only is
/// `PermissionDenied`, which tells the user something true and actionable.
/// Reporting it as `Unsupported` would claim their drive does not have health
/// data, which is the opposite of the situation.
pub fn from_io(error: std::io::Error, controller: &Path) -> MetricError {
    use std::io::ErrorKind;

    match error.kind() {
        ErrorKind::PermissionDenied => MetricError::new(
            MetricErrorCode::PermissionDenied,
            format!(
                "reading the NVMe health log needs privileged access to '{}'; PULSE runs \
                 unprivileged, so only the temperature the kernel publishes is available",
                controller.display()
            ),
        ),
        ErrorKind::NotFound => MetricError::new(
            MetricErrorCode::NotDetected,
            format!("no NVMe controller device at '{}'", controller.display()),
        ),
        // `ENOTTY` — the device is not an NVMe controller, or the driver does
        // not implement the passthrough.
        _ if error.raw_os_error() == Some(25) => MetricError::new(
            MetricErrorCode::Unsupported,
            format!(
                "the driver behind '{}' does not implement the NVMe admin passthrough",
                controller.display()
            ),
        ),
        _ => MetricError::new(
            MetricErrorCode::Io,
            format!(
                "could not read the NVMe health log from '{}': {error}",
                controller.display()
            ),
        ),
    }
}

/// The availability to attach to the health metrics of a device whose log
/// could not be read.
pub fn availability_for(error: &MetricError) -> Availability {
    crate::metrics::wellknown::availability_for(error.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Error, ErrorKind};

    /// A throwaway NVMe controller directory with hwmon sensors under it.
    ///
    /// `nested` chooses the layout: the `nvme` driver registers `hwmonN` as a
    /// direct child of the controller, while other subsystems nest it under
    /// `hwmon/`. Both are exercised.
    struct ControllerFixture {
        root: std::path::PathBuf,
        hwmon: std::path::PathBuf,
    }

    impl ControllerFixture {
        fn new(name: &str, nested: bool) -> Self {
            let root = std::env::temp_dir().join(format!(
                "pulse-nvme-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&root);

            let hwmon = if nested {
                root.join("hwmon").join("hwmon3")
            } else {
                root.join("hwmon3")
            };
            fs::create_dir_all(&hwmon).expect("create fixture");
            fs::write(hwmon.join("name"), "nvme\n").expect("write name");

            Self { root, hwmon }
        }

        fn sensor(&self, index: u32, label: &str, millidegrees: &str) -> &Self {
            fs::write(
                self.hwmon.join(format!("temp{index}_label")),
                format!("{label}\n"),
            )
            .expect("write label");
            fs::write(
                self.hwmon.join(format!("temp{index}_input")),
                format!("{millidegrees}\n"),
            )
            .expect("write input");
            self
        }

        fn input_path(&self, index: u32) -> std::path::PathBuf {
            self.hwmon.join(format!("temp{index}_input"))
        }
    }

    impl Drop for ControllerFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_namespace_resolves_to_its_controller() {
        assert_eq!(
            controller_path("nvme0n1").as_deref(),
            Some(Path::new("/dev/nvme0"))
        );
        assert_eq!(
            controller_path("nvme1n2").as_deref(),
            Some(Path::new("/dev/nvme1"))
        );
        assert_eq!(
            controller_path("nvme12n3").as_deref(),
            Some(Path::new("/dev/nvme12"))
        );
    }

    #[test]
    fn a_non_nvme_name_resolves_to_no_controller() {
        // Sending an NVMe admin command to a SATA disk is not something to
        // attempt and see what happens.
        assert_eq!(controller_path("sda"), None);
        assert_eq!(controller_path("vda"), None);
        assert_eq!(controller_path("nvme"), None);
        assert_eq!(controller_path("nvmexn1"), None);
        assert_eq!(controller_path("nvme0nx"), None);
        assert_eq!(controller_path(""), None);
    }

    #[test]
    fn the_log_page_command_word_asks_for_exactly_512_bytes() {
        // 512 bytes is 128 dwords, so NUMD is 127.
        let cdw10 = get_log_page_cdw10(0x02, 512);

        assert_eq!(cdw10 & 0xFF, 0x02, "log page identifier");
        assert_eq!(cdw10 >> 16, 127, "NUMD is dwords minus one");
        assert_eq!(cdw10, 0x007F_0002);
    }

    #[test]
    fn the_command_word_never_underflows_for_a_tiny_request() {
        // `saturating_sub` rather than `- 1`: a zero-length request would
        // otherwise wrap NUMD to 0xFFFF and ask the controller for 256 KiB.
        assert_eq!(get_log_page_cdw10(0x02, 0) >> 16, 0);
        assert_eq!(get_log_page_cdw10(0x02, 4) >> 16, 0);
    }

    #[test]
    fn permission_denied_is_never_reported_as_unsupported() {
        // The single most important mapping in this module: an unprivileged
        // PULSE must tell the user the OS refused, not that their drive has no
        // health data.
        let error = from_io(
            Error::from(ErrorKind::PermissionDenied),
            Path::new("/dev/nvme0"),
        );

        assert_eq!(error.code, MetricErrorCode::PermissionDenied);
        assert_eq!(availability_for(&error).status_str(), "permissionDenied");
        assert!(error.message.contains("/dev/nvme0"));
    }

    #[test]
    fn a_driver_without_the_passthrough_is_unsupported() {
        let notty = Error::from_raw_os_error(25);
        let error = from_io(notty, Path::new("/dev/nvme0"));

        assert_eq!(error.code, MetricErrorCode::Unsupported);
        assert_eq!(availability_for(&error).status_str(), "unsupported");
    }

    #[test]
    fn a_missing_controller_is_not_detected_rather_than_an_error() {
        let error = from_io(Error::from(ErrorKind::NotFound), Path::new("/dev/nvme9"));

        assert_eq!(error.code, MetricErrorCode::NotDetected);
        assert_eq!(availability_for(&error).status_str(), "notDetected");
    }

    #[test]
    fn a_transient_failure_stays_transient() {
        let error = from_io(Error::from(ErrorKind::Interrupted), Path::new("/dev/nvme0"));

        assert_eq!(error.code, MetricErrorCode::Io);
        assert!(availability_for(&error).is_transient());
    }

    #[test]
    fn the_composite_sensor_is_found_where_the_nvme_driver_puts_it() {
        // The real layout on the development machine: `hwmon3` is a direct
        // child of the controller directory, and `Composite` is the figure the
        // health log would also report.
        let fixture = ControllerFixture::new("direct", false);
        fixture
            .sensor(1, "Composite", "52850")
            .sensor(2, "Sensor 1", "52850")
            .sensor(3, "Sensor 2", "57850");

        let input = composite_temperature_input(&fixture.root).expect("found");

        assert_eq!(input, fixture.input_path(1));
        assert_eq!(read_temperature(&input).expect("readable"), 52.85);
    }

    #[test]
    fn the_composite_sensor_is_also_found_in_the_nested_layout() {
        let fixture = ControllerFixture::new("nested", true);
        fixture.sensor(1, "Composite", "44000");

        let input = composite_temperature_input(&fixture.root).expect("found");
        assert_eq!(read_temperature(&input).expect("readable"), 44.0);
    }

    #[test]
    fn the_composite_sensor_is_found_by_label_not_by_position() {
        // A controller that lists its per-die sensors first. Taking
        // `temp1_input` would publish `Sensor 1`'s 57.85 °C as the device
        // temperature — five degrees hotter than the composite figure.
        let fixture = ControllerFixture::new("reordered", false);
        fixture
            .sensor(1, "Sensor 1", "57850")
            .sensor(2, "Composite", "52850");

        let input = composite_temperature_input(&fixture.root).expect("found");

        assert_eq!(input, fixture.input_path(2));
        assert_eq!(read_temperature(&input).expect("readable"), 52.85);
    }

    #[test]
    fn a_controller_with_no_composite_sensor_reports_none() {
        // Publishing `Sensor 1` in its place would be a different measurement
        // under the same name.
        let fixture = ControllerFixture::new("no-composite", false);
        fixture.sensor(1, "Sensor 1", "57850");

        assert_eq!(composite_temperature_input(&fixture.root), None);
    }

    #[test]
    fn a_device_with_no_hwmon_at_all_reports_none() {
        assert_eq!(
            composite_temperature_input(Path::new("/nonexistent/pulse/device")),
            None
        );
    }
}
