//! Reading an executable's architecture from its ELF header — and nothing else.
//!
//! The first 20 bytes of an ELF file say everything the inspector needs:
//!
//! ```text
//! 0x00  7f 45 4c 46      magic "\x7fELF"
//! 0x04  EI_CLASS         1 = 32-bit, 2 = 64-bit
//! 0x05  EI_DATA          1 = little-endian, 2 = big-endian
//! 0x12  e_machine        u16, in EI_DATA's byte order
//! ```
//!
//! PULSE reads those bytes and stops. No section table, no program headers,
//! no dynamic segment: parsing more of an arbitrary binary would be attack
//! surface for no information the user asked for.
//!
//! Pure, and tested on every host.

/// The length of the prefix [`parse_header`] needs.
pub const HEADER_PREFIX_LEN: usize = 20;

/// What the ELF header says about an executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfHeader {
    /// 32 or 64.
    pub bits: u8,
    pub machine: u16,
}

impl ElfHeader {
    /// The architecture's conventional name, e.g. `x86_64`.
    pub fn architecture(&self) -> String {
        machine_name(self.machine, self.bits)
    }
}

/// Parses the ELF identification and machine fields.
///
/// `None` for anything that is not a well-formed ELF prefix — a script, a
/// truncated file, an unknown class or byte order.
pub fn parse_header(bytes: &[u8]) -> Option<ElfHeader> {
    if bytes.len() < HEADER_PREFIX_LEN || bytes[..4] != [0x7f, b'E', b'L', b'F'] {
        return None;
    }

    let bits = match bytes[4] {
        1 => 32,
        2 => 64,
        _ => return None,
    };

    let raw = [bytes[18], bytes[19]];
    let machine = match bytes[5] {
        1 => u16::from_le_bytes(raw),
        2 => u16::from_be_bytes(raw),
        _ => return None,
    };

    Some(ElfHeader { bits, machine })
}

/// Names an ELF `e_machine` value.
///
/// Unknown machines are reported by number rather than guessed.
pub fn machine_name(machine: u16, bits: u8) -> String {
    let name = match (machine, bits) {
        (0x03, _) => "x86",
        (0x3e, _) => "x86_64",
        (0xb7, _) => "aarch64",
        (0x28, _) => "arm",
        (0xf3, 64) => "riscv64",
        (0xf3, _) => "riscv32",
        (0x14, _) => "ppc",
        (0x15, _) => "ppc64",
        (0x16, 64) => "s390x",
        (0x16, _) => "s390",
        (0x08, _) => "mips",
        (0x102, _) => "loongarch64",
        _ => return format!("ELF machine 0x{machine:x} ({bits}-bit)"),
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(class: u8, data: u8, machine: [u8; 2]) -> Vec<u8> {
        let mut bytes = vec![0_u8; 64];
        bytes[..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        bytes[4] = class;
        bytes[5] = data;
        bytes[18] = machine[0];
        bytes[19] = machine[1];
        bytes
    }

    #[test]
    fn recognises_x86_64() {
        let parsed = parse_header(&header(2, 1, [0x3e, 0x00])).expect("elf");
        assert_eq!(parsed.bits, 64);
        assert_eq!(parsed.architecture(), "x86_64");
    }

    #[test]
    fn recognises_32_bit_x86() {
        let parsed = parse_header(&header(1, 1, [0x03, 0x00])).expect("elf");
        assert_eq!(parsed.bits, 32);
        assert_eq!(parsed.architecture(), "x86");
    }

    #[test]
    fn recognises_aarch64_and_honours_big_endian() {
        assert_eq!(
            parse_header(&header(2, 1, [0xb7, 0x00]))
                .expect("elf")
                .architecture(),
            "aarch64"
        );
        assert_eq!(
            parse_header(&header(2, 2, [0x00, 0x16]))
                .expect("elf")
                .architecture(),
            "s390x"
        );
    }

    #[test]
    fn an_unknown_machine_is_reported_by_number() {
        assert_eq!(machine_name(0x1234, 64), "ELF machine 0x1234 (64-bit)");
    }

    #[test]
    fn rejects_what_is_not_elf() {
        assert!(parse_header(b"#!/bin/sh\necho hello world\n").is_none());
        assert!(
            parse_header(&[0x7f, b'E', b'L', b'F']).is_none(),
            "truncated"
        );
        assert!(
            parse_header(&header(3, 1, [0x3e, 0])).is_none(),
            "bad class"
        );
        assert!(parse_header(&header(2, 9, [0x3e, 0])).is_none(), "bad data");
        assert!(parse_header(b"MZ\x90\x00").is_none(), "a PE file");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reads_the_test_binarys_own_header() {
        use std::io::Read;

        let mut prefix = [0_u8; HEADER_PREFIX_LEN];
        std::fs::File::open("/proc/self/exe")
            .and_then(|mut file| file.read_exact(&mut prefix))
            .expect("read own exe");

        let parsed = parse_header(&prefix).expect("the test binary is ELF");
        let expected = std::env::consts::ARCH;
        assert_eq!(parsed.architecture(), expected);
    }
}
