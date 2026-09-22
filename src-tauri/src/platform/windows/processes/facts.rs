//! Pure conversions behind the Windows inspector and control backends.
//!
//! Everything here turns a raw Windows value into PULSE's vocabulary and back
//! — machine types, FILETIMEs, priority classes, affinity masks, SIDs — and
//! none of it calls Windows. It is compiled and unit-tested on every host, so
//! a Fedora test run catches a wrong constant long before a Windows machine is
//! available.

use crate::processes::control::WindowsPriorityClass;
use crate::processes::ProcessClass;

// --- architecture -------------------------------------------------------------

pub const IMAGE_FILE_MACHINE_UNKNOWN: u16 = 0;
pub const IMAGE_FILE_MACHINE_I386: u16 = 0x014c;
pub const IMAGE_FILE_MACHINE_ARMNT: u16 = 0x01c4;
pub const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
pub const IMAGE_FILE_MACHINE_ARM64: u16 = 0xaa64;
pub const IMAGE_FILE_MACHINE_IA64: u16 = 0x0200;

/// Names an `IMAGE_FILE_MACHINE_*` value.
pub fn machine_name(machine: u16) -> Option<&'static str> {
    match machine {
        IMAGE_FILE_MACHINE_I386 => Some("x86"),
        IMAGE_FILE_MACHINE_AMD64 => Some("x86_64"),
        IMAGE_FILE_MACHINE_ARM64 => Some("ARM64"),
        IMAGE_FILE_MACHINE_ARMNT => Some("ARM"),
        IMAGE_FILE_MACHINE_IA64 => Some("IA-64"),
        _ => None,
    }
}

/// The architecture a process runs as, from `IsWow64Process2`'s two answers.
///
/// `process_machine` is `IMAGE_FILE_MACHINE_UNKNOWN` for a process that is
/// **not** running under WOW64 — it is native, so the native machine is the
/// answer. Otherwise the process machine is the emulated one (e.g. `x86` on
/// an x64 host).
pub fn architecture(process_machine: u16, native_machine: u16) -> Option<&'static str> {
    if process_machine == IMAGE_FILE_MACHINE_UNKNOWN {
        machine_name(native_machine)
    } else {
        machine_name(process_machine)
    }
}

/// `SYSTEM_INFO.wProcessorArchitecture`, for the `IsWow64Process` fallback.
pub fn processor_architecture_name(architecture: u16) -> Option<&'static str> {
    match architecture {
        0 => Some("x86"),
        5 => Some("ARM"),
        6 => Some("IA-64"),
        9 => Some("x86_64"),
        12 => Some("ARM64"),
        _ => None,
    }
}

// --- time ---------------------------------------------------------------------

/// 100 ns intervals between 1601-01-01 and 1970-01-01.
const FILETIME_UNIX_EPOCH: u64 = 116_444_736_000_000_000;

/// Converts a creation `FILETIME` to Unix epoch milliseconds, for display.
///
/// The identity token stays the raw FILETIME; this conversion is only ever
/// for the "Started" line.
pub fn filetime_to_unix_ms(filetime: u64) -> Option<u64> {
    filetime
        .checked_sub(FILETIME_UNIX_EPOCH)
        .map(|since| since / 10_000)
}

// --- priority -----------------------------------------------------------------

pub const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
pub const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
pub const NORMAL_PRIORITY_CLASS: u32 = 0x0000_0020;
pub const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
pub const HIGH_PRIORITY_CLASS: u32 = 0x0000_0080;
pub const REALTIME_PRIORITY_CLASS: u32 = 0x0000_0100;

pub fn priority_class_from_raw(raw: u32) -> Option<WindowsPriorityClass> {
    match raw {
        IDLE_PRIORITY_CLASS => Some(WindowsPriorityClass::Idle),
        BELOW_NORMAL_PRIORITY_CLASS => Some(WindowsPriorityClass::BelowNormal),
        NORMAL_PRIORITY_CLASS => Some(WindowsPriorityClass::Normal),
        ABOVE_NORMAL_PRIORITY_CLASS => Some(WindowsPriorityClass::AboveNormal),
        HIGH_PRIORITY_CLASS => Some(WindowsPriorityClass::High),
        REALTIME_PRIORITY_CLASS => Some(WindowsPriorityClass::Realtime),
        _ => None,
    }
}

pub const fn priority_class_to_raw(class: WindowsPriorityClass) -> u32 {
    match class {
        WindowsPriorityClass::Idle => IDLE_PRIORITY_CLASS,
        WindowsPriorityClass::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
        WindowsPriorityClass::Normal => NORMAL_PRIORITY_CLASS,
        WindowsPriorityClass::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        WindowsPriorityClass::High => HIGH_PRIORITY_CLASS,
        WindowsPriorityClass::Realtime => REALTIME_PRIORITY_CLASS,
    }
}

// --- affinity -----------------------------------------------------------------

/// The processors a `usize` affinity mask allows, ascending.
pub fn mask_to_cpus(mask: usize) -> Vec<u32> {
    (0..usize::BITS)
        .filter(|bit| mask & (1_usize << bit) != 0)
        .collect()
}

/// Builds an affinity mask, refusing processors outside `system_mask`.
///
/// `SetProcessAffinityMask` requires a subset of the system mask; checking it
/// here turns an opaque `ERROR_INVALID_PARAMETER` into a sentence.
pub fn cpus_to_mask(cpus: &[u32], system_mask: usize) -> Result<usize, String> {
    let mut mask = 0_usize;
    for cpu in cpus {
        if *cpu >= usize::BITS {
            return Err(format!(
                "CPU {cpu} is outside the {}-processor range one affinity mask can address.",
                usize::BITS
            ));
        }
        let bit = 1_usize << cpu;
        if system_mask & bit == 0 {
            return Err(format!("CPU {cpu} is not available on this machine."));
        }
        mask |= bit;
    }
    if mask == 0 {
        return Err("At least one logical processor must be kept.".to_string());
    }
    Ok(mask)
}

/// Why a machine's affinity cannot be faithfully changed with one mask.
pub const MULTI_GROUP_LIMITATION: &str =
    "This machine has more than one processor group (over 64 logical processors). PULSE shows \
     the process's primary group only and does not change affinity here rather than apply a \
     partial mask.";

// --- ownership ------------------------------------------------------------------

/// Classifies a process from its token's user SID.
///
/// `LocalSystem`, `LocalService` and `NetworkService` are the operating
/// system's own accounts. Any other SID is a user account; "which user" is
/// the owner line's job.
pub fn category_from_sid(sid: &str) -> ProcessClass {
    match sid {
        "S-1-5-18" | "S-1-5-19" | "S-1-5-20" => ProcessClass::SystemProcess,
        _ if sid.starts_with("S-1-5-") => ProcessClass::UserApplication,
        _ => ProcessClass::Unknown,
    }
}

/// Joins `LookupAccountSidW`'s two halves the way Windows displays them.
pub fn account_name(domain: &str, name: &str) -> String {
    if domain.is_empty() {
        name.to_string()
    } else {
        format!("{domain}\\{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_native_process_takes_the_native_machine() {
        assert_eq!(
            architecture(IMAGE_FILE_MACHINE_UNKNOWN, IMAGE_FILE_MACHINE_AMD64),
            Some("x86_64")
        );
        assert_eq!(
            architecture(IMAGE_FILE_MACHINE_UNKNOWN, IMAGE_FILE_MACHINE_ARM64),
            Some("ARM64")
        );
    }

    #[test]
    fn a_wow64_process_takes_its_emulated_machine() {
        assert_eq!(
            architecture(IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_AMD64),
            Some("x86")
        );
        assert_eq!(
            architecture(IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_ARM64),
            Some("x86")
        );
    }

    #[test]
    fn unknown_machines_are_not_guessed() {
        assert_eq!(architecture(0x1234, IMAGE_FILE_MACHINE_AMD64), None);
        assert_eq!(architecture(IMAGE_FILE_MACHINE_UNKNOWN, 0x1234), None);
        assert_eq!(processor_architecture_name(9), Some("x86_64"));
        assert_eq!(processor_architecture_name(0xffff), None);
    }

    #[test]
    fn converts_a_creation_filetime_to_a_date() {
        // 2023-11-14T22:13:20Z.
        let filetime = FILETIME_UNIX_EPOCH + 1_700_000_000 * 10_000_000;
        assert_eq!(filetime_to_unix_ms(filetime), Some(1_700_000_000_000));
        assert_eq!(filetime_to_unix_ms(0), None, "before 1970 is not a start");
    }

    #[test]
    fn priority_classes_round_trip() {
        for class in [
            WindowsPriorityClass::Idle,
            WindowsPriorityClass::BelowNormal,
            WindowsPriorityClass::Normal,
            WindowsPriorityClass::AboveNormal,
            WindowsPriorityClass::High,
            WindowsPriorityClass::Realtime,
        ] {
            assert_eq!(
                priority_class_from_raw(priority_class_to_raw(class)),
                Some(class)
            );
        }
        assert_eq!(priority_class_from_raw(0), None);
        assert_eq!(priority_class_to_raw(WindowsPriorityClass::Normal), 0x20);
        assert_eq!(priority_class_to_raw(WindowsPriorityClass::Realtime), 0x100);
    }

    #[test]
    fn affinity_masks_round_trip() {
        assert_eq!(mask_to_cpus(0b1011), vec![0, 1, 3]);
        assert_eq!(mask_to_cpus(0), Vec::<u32>::new());
        assert_eq!(cpus_to_mask(&[0, 1, 3], 0b1111), Ok(0b1011));
    }

    #[test]
    fn affinity_masks_refuse_what_the_system_lacks() {
        assert!(cpus_to_mask(&[], 0b1111).is_err());
        assert!(cpus_to_mask(&[4], 0b1111).is_err());
        assert!(cpus_to_mask(&[usize::BITS], usize::MAX).is_err());
    }

    #[test]
    fn service_accounts_are_system_processes() {
        for sid in ["S-1-5-18", "S-1-5-19", "S-1-5-20"] {
            assert_eq!(category_from_sid(sid), ProcessClass::SystemProcess);
        }
        assert_eq!(
            category_from_sid("S-1-5-21-1004336348-1177238915-682003330-1001"),
            ProcessClass::UserApplication
        );
        assert_eq!(category_from_sid("garbage"), ProcessClass::Unknown);
    }

    #[test]
    fn account_names_are_domain_qualified() {
        assert_eq!(account_name("DESKTOP-1", "alice"), "DESKTOP-1\\alice");
        assert_eq!(account_name("", "SYSTEM"), "SYSTEM");
    }
}
