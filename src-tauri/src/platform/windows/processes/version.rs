//! Reading a PE file's version resource: product name, description, company,
//! versions.
//!
//! These fields give the inspector a much better name than a bare
//! `2.1.278.exe` — but they are **self-declared**. Any program can write any
//! `CompanyName`; they are description, never evidence, and the interface
//! says so beside them. Authenticode is the part that is checked.
//!
//! ```text
//! GetFileVersionInfoSizeW → GetFileVersionInfoW      the whole resource, once
//! VerQueryValueW \VarFileInfo\Translation            which language/codepage blocks exist
//! VerQueryValueW \StringFileInfo\<lang><cp>\<Key>    one string per key
//! ```
//!
//! The block selection and path building are pure and tested everywhere.

/// The string keys PULSE reads.
pub const KEYS: [&str; 5] = [
    "FileDescription",
    "ProductName",
    "CompanyName",
    "FileVersion",
    "ProductVersion",
];

/// Decodes `\VarFileInfo\Translation`: an array of `(language, codepage)`
/// little-endian `u16` pairs.
pub fn parse_translations(bytes: &[u8]) -> Vec<(u16, u16)> {
    bytes
        .chunks_exact(4)
        .map(|pair| {
            (
                u16::from_le_bytes([pair[0], pair[1]]),
                u16::from_le_bytes([pair[2], pair[3]]),
            )
        })
        .collect()
}

/// The `StringFileInfo` blocks to try, most likely first.
///
/// The file's own declared translations come first, then the two blocks
/// nearly every tool writes (US English in Unicode and in Windows-1252),
/// because some files declare no translation table at all. Duplicates are
/// removed.
pub fn candidate_blocks(translations: &[(u16, u16)]) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    for (language, codepage) in translations
        .iter()
        .copied()
        .chain([(0x0409, 0x04b0), (0x0409, 0x04e4)])
    {
        let block = format!("{language:04x}{codepage:04x}");
        if !blocks.contains(&block) {
            blocks.push(block);
        }
    }
    blocks
}

/// The `VerQueryValueW` sub-block path for one key.
pub fn string_path(block: &str, key: &str) -> String {
    format!("\\StringFileInfo\\{block}\\{key}")
}

/// Cleans one value: trims, drops a terminating NUL, and treats an empty
/// string as absent.
pub fn clean(value: &str) -> Option<String> {
    let value = value.trim_end_matches('\0').trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(target_os = "windows")]
pub use imp::read;

#[cfg(target_os = "windows")]
mod imp {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    use crate::metrics::model::Availability;
    use crate::processes::field::Field;
    use crate::processes::inspector::VersionInfo;

    use super::{candidate_blocks, clean, parse_translations, string_path};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// One `VerQueryValueW` lookup into a loaded resource.
    ///
    /// Returns the bytes from the value's start to the end of the resource,
    /// plus the length the API reported — in **characters** for string
    /// values and in **bytes** for binary ones, so the caller decides.
    fn query<'a>(data: &'a [u8], path: &str) -> Option<(&'a [u8], usize)> {
        let path = wide(path);
        let mut pointer: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut length: u32 = 0;

        // SAFETY: `data` is a complete version resource loaded by
        // `GetFileVersionInfoW`; `path` is NUL-terminated. On success the
        // returned pointer points *into* `data`, which the returned slice
        // borrows, so it cannot outlive the buffer.
        let ok = unsafe {
            VerQueryValueW(
                data.as_ptr().cast(),
                path.as_ptr(),
                &mut pointer,
                &mut length,
            )
        };
        if ok == 0 || pointer.is_null() {
            return None;
        }

        let offset = (pointer as usize).checked_sub(data.as_ptr() as usize)?;
        data.get(offset..).map(|rest| (rest, length as usize))
    }

    fn string(data: &[u8], path: &str) -> Option<String> {
        let (rest, characters) = query(data, path)?;
        let bytes = &rest[..rest.len().min(characters * 2)];
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        clean(&String::from_utf16_lossy(&units))
    }

    /// Reads the version resource of the file at `path`.
    pub fn read(path: &str) -> Field<VersionInfo> {
        let wide_path = wide(path);
        let mut ignored: u32 = 0;

        // SAFETY: a NUL-terminated path and a live out-parameter.
        let size = unsafe { GetFileVersionInfoSizeW(wide_path.as_ptr(), &mut ignored) };
        if size == 0 {
            return Field::missing(Availability::not_detected(
                "this executable carries no version resource",
            ));
        }

        let mut data = vec![0_u8; size as usize];
        // SAFETY: `data` is exactly `size` bytes, as the API was told.
        let ok =
            unsafe { GetFileVersionInfoW(wide_path.as_ptr(), 0, size, data.as_mut_ptr().cast()) };
        if ok == 0 {
            return Field::missing(Availability::temporarily_unavailable(
                "the version resource could not be read",
            ));
        }

        let translations = query(&data, "\\VarFileInfo\\Translation")
            // A binary value: its length is in bytes.
            .map(|(rest, bytes)| parse_translations(&rest[..rest.len().min(bytes)]))
            .unwrap_or_default();

        for block in candidate_blocks(&translations) {
            let info = VersionInfo {
                file_description: string(&data, &string_path(&block, "FileDescription")),
                product_name: string(&data, &string_path(&block, "ProductName")),
                company_name: string(&data, &string_path(&block, "CompanyName")),
                file_version: string(&data, &string_path(&block, "FileVersion")),
                product_version: string(&data, &string_path(&block, "ProductVersion")),
            };
            if !info.is_empty() {
                return Field::available(info);
            }
        }

        Field::missing(Availability::not_detected(
            "the version resource declares no readable strings",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_translation_table() {
        // 0409 04b0, 0407 04e4
        let bytes = [0x09, 0x04, 0xb0, 0x04, 0x07, 0x04, 0xe4, 0x04];
        assert_eq!(
            parse_translations(&bytes),
            vec![(0x0409, 0x04b0), (0x0407, 0x04e4)]
        );
        assert!(
            parse_translations(&[0x09, 0x04, 0xb0]).is_empty(),
            "truncated"
        );
    }

    #[test]
    fn tries_declared_blocks_first_then_the_usual_ones() {
        assert_eq!(
            candidate_blocks(&[(0x0407, 0x04e4)]),
            vec!["040704e4", "040904b0", "040904e4"]
        );
        assert_eq!(
            candidate_blocks(&[(0x0409, 0x04b0)]),
            vec!["040904b0", "040904e4"]
        );
        assert_eq!(candidate_blocks(&[]).len(), 2);
    }

    #[test]
    fn builds_the_string_query_path() {
        assert_eq!(
            string_path("040904b0", "ProductName"),
            "\\StringFileInfo\\040904b0\\ProductName"
        );
        assert_eq!(KEYS.len(), 5);
    }

    #[test]
    fn cleans_values() {
        assert_eq!(clean("  Claude \0"), Some("Claude".to_string()));
        assert_eq!(clean("\0"), None);
        assert_eq!(clean("Zoë Éditions"), Some("Zoë Éditions".to_string()));
    }
}
