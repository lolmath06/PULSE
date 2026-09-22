//! Authenticode: does Windows trust this executable's signature, and who
//! signed it?
//!
//! # Present is not valid, valid is not trusted, trusted is not safe
//!
//! | `WinVerifyTrust` says | PULSE shows |
//! |---|---|
//! | `S_OK` | **Trusted** — valid, chains to a trusted root |
//! | untrusted root, expired, revoked, explicitly distrusted | **Signed but untrusted** |
//! | digest mismatch, bad signer certificate | **Invalid** |
//! | no signature, embedded or in a catalog | **Unsigned** |
//! | access denied | **Permission denied** |
//! | anything else | **Unavailable**, with the code |
//!
//! None of these is a verdict about the program. A trusted signature says who
//! vouched for the file, not what it does; an unsigned file is not malware.
//!
//! # Catalog signatures
//!
//! Most of Windows itself is not signed *inside* each file but in a system
//! security catalog. Asking `WinVerifyTrust` about `notepad.exe` alone
//! answers "no signature". So when the embedded check finds none, PULSE asks
//! `CryptCATAdmin*` for a catalog that lists the file's hash and verifies
//! against that — otherwise half of `System32` would be shown as unsigned.
//!
//! # Offline by construction
//!
//! Certificate revocation checking can fetch CRLs and OCSP responses from the
//! Internet. PULSE disables it (`WTD_REVOKE_NONE`,
//! `WTD_REVOCATION_CHECK_NONE`) and restricts URL retrieval to the local
//! cache (`WTD_CACHE_ONLY_URL_RETRIEVAL`), so inspecting a signature makes no
//! network request. The price is honest and documented: a revoked
//! certificate that Windows has not already cached as revoked is not
//! detected here.

use crate::processes::inspector::TrustStatus;

/// `WinVerifyTrust` results PULSE distinguishes. Raw values, so the mapping
/// is testable without Windows.
pub mod hresult {
    pub const S_OK: u32 = 0x0000_0000;
    pub const E_ACCESSDENIED: u32 = 0x8007_0005;
    pub const TRUST_E_PROVIDER_UNKNOWN: u32 = 0x800B_0001;
    pub const TRUST_E_ACTION_UNKNOWN: u32 = 0x800B_0002;
    pub const TRUST_E_SUBJECT_FORM_UNKNOWN: u32 = 0x800B_0003;
    pub const TRUST_E_SUBJECT_NOT_TRUSTED: u32 = 0x800B_0004;
    pub const TRUST_E_NOSIGNATURE: u32 = 0x800B_0100;
    pub const CERT_E_EXPIRED: u32 = 0x800B_0101;
    pub const CERT_E_UNTRUSTEDROOT: u32 = 0x800B_0109;
    pub const CERT_E_CHAINING: u32 = 0x800B_010A;
    pub const CERT_E_REVOKED: u32 = 0x800B_010C;
    pub const CERT_E_UNTRUSTEDTESTROOT: u32 = 0x800B_010D;
    pub const CERT_E_WRONG_USAGE: u32 = 0x800B_0110;
    pub const TRUST_E_EXPLICIT_DISTRUST: u32 = 0x800B_0111;
    pub const CERT_E_UNTRUSTEDCA: u32 = 0x800B_0112;
    pub const TRUST_E_NO_SIGNER_CERT: u32 = 0x8009_6002;
    pub const TRUST_E_CERT_SIGNATURE: u32 = 0x8009_6004;
    pub const TRUST_E_TIME_STAMP: u32 = 0x8009_6005;
    pub const TRUST_E_BAD_DIGEST: u32 = 0x8009_6010;
    pub const CRYPT_E_FILE_ERROR: u32 = 0x8009_2003;
    pub const CRYPT_E_SECURITY_SETTINGS: u32 = 0x8009_2026;
}

/// Maps one `WinVerifyTrust` result to a trust status and a sentence.
pub fn classify(result: u32) -> (TrustStatus, String) {
    use hresult::*;

    let (status, text) = match result {
        S_OK => (
            TrustStatus::Trusted,
            "Windows verified the signature and trusts its certificate chain.",
        ),
        TRUST_E_NOSIGNATURE => (TrustStatus::Unsigned, "The file carries no signature."),
        CERT_E_EXPIRED => (
            TrustStatus::SignedButUntrusted,
            "The signing certificate has expired.",
        ),
        CERT_E_UNTRUSTEDROOT | CERT_E_UNTRUSTEDTESTROOT | CERT_E_UNTRUSTEDCA => (
            TrustStatus::SignedButUntrusted,
            "The signature chains to a root Windows does not trust.",
        ),
        CERT_E_CHAINING => (
            TrustStatus::SignedButUntrusted,
            "The certificate chain could not be built to a trusted root.",
        ),
        CERT_E_REVOKED => (
            TrustStatus::SignedButUntrusted,
            "The signing certificate has been revoked.",
        ),
        CERT_E_WRONG_USAGE => (
            TrustStatus::SignedButUntrusted,
            "The certificate is not valid for code signing.",
        ),
        TRUST_E_EXPLICIT_DISTRUST => (
            TrustStatus::SignedButUntrusted,
            "The signer is explicitly distrusted on this machine.",
        ),
        TRUST_E_SUBJECT_NOT_TRUSTED => (
            TrustStatus::SignedButUntrusted,
            "The signature is present but not trusted by local policy.",
        ),
        TRUST_E_TIME_STAMP => (
            TrustStatus::SignedButUntrusted,
            "The signature's timestamp could not be verified.",
        ),
        TRUST_E_BAD_DIGEST => (
            TrustStatus::Invalid,
            "The signature does not match the file's contents.",
        ),
        TRUST_E_NO_SIGNER_CERT | TRUST_E_CERT_SIGNATURE => (
            TrustStatus::Invalid,
            "The signature's certificate is missing or malformed.",
        ),
        E_ACCESSDENIED => (
            TrustStatus::PermissionDenied,
            "Windows refused to read the file's signature.",
        ),
        TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN | TRUST_E_ACTION_UNKNOWN => (
            TrustStatus::Unavailable,
            "Windows cannot verify signatures for this kind of file.",
        ),
        CRYPT_E_SECURITY_SETTINGS => (
            TrustStatus::Unavailable,
            "Local security policy prevents signature verification.",
        ),
        CRYPT_E_FILE_ERROR => (
            TrustStatus::Unavailable,
            "The file could not be read for verification.",
        ),
        other => {
            return (
                TrustStatus::Unavailable,
                format!("Signature verification returned 0x{other:08X}."),
            )
        }
    };
    (status, text.to_string())
}

/// Whether a result means a signature was found at all, so there is a signer
/// worth naming.
pub fn has_signer(status: TrustStatus) -> bool {
    matches!(
        status,
        TrustStatus::Trusted | TrustStatus::SignedButUntrusted | TrustStatus::Invalid
    )
}

/// The catalog member tag: the file hash as upper-case hex.
pub fn member_tag(hash: &[u8]) -> String {
    hash.iter().map(|byte| format!("{byte:02X}")).collect()
}

#[cfg(target_os = "windows")]
pub use imp::verify;

#[cfg(target_os = "windows")]
mod imp {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::core::GUID;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Security::Cryptography::Catalog::{
        CryptCATAdminAcquireContext2, CryptCATAdminCalcHashFromFileHandle2,
        CryptCATAdminEnumCatalogFromHash, CryptCATAdminReleaseCatalogContext,
        CryptCATAdminReleaseContext, CryptCATCatalogInfoFromContext, CATALOG_INFO,
    };
    use windows_sys::Win32::Security::Cryptography::{
        CertGetNameStringW, BCRYPT_SHA256_ALGORITHM, CERT_NAME_SIMPLE_DISPLAY_TYPE,
    };
    use windows_sys::Win32::Security::WinTrust::{
        WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_CATALOG_INFO, WINTRUST_DATA, WINTRUST_DATA_0,
        WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_CATALOG, WTD_CHOICE_FILE,
        WTD_REVOCATION_CHECK_NONE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UICONTEXT_EXECUTE, WTD_UI_NONE,
    };

    use crate::processes::inspector::{SignatureInfo, SignatureSource, TrustStatus};

    use super::{classify, has_signer, hresult, member_tag};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A verification whose state must be closed exactly once.
    struct Verification {
        data: WINTRUST_DATA,
        result: u32,
    }

    impl Verification {
        /// Runs `WinVerifyTrust` with PULSE's offline, silent policy.
        ///
        /// # Safety
        ///
        /// `data`'s union member must point at live structures that outlive
        /// the returned value.
        unsafe fn run(mut data: WINTRUST_DATA) -> Self {
            data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
            data.dwUIChoice = WTD_UI_NONE;
            data.fdwRevocationChecks = WTD_REVOKE_NONE;
            data.dwStateAction = WTD_STATEACTION_VERIFY;
            data.dwProvFlags = WTD_REVOCATION_CHECK_NONE | WTD_CACHE_ONLY_URL_RETRIEVAL;
            data.dwUIContext = WTD_UICONTEXT_EXECUTE;

            let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
            // SAFETY: per this function's contract; no window handle, since
            // the UI choice is "none".
            let result = WinVerifyTrust(
                std::ptr::null_mut(),
                &mut action,
                (&mut data as *mut WINTRUST_DATA).cast(),
            );
            Self {
                data,
                result: result as u32,
            }
        }

        /// The signer's display name, from the verified state.
        fn signer(&self) -> Option<String> {
            // SAFETY: `hWVTStateData` came from the VERIFY call above and is
            // not yet closed. Every pointer is null-checked before use, and
            // everything read is owned by that state.
            unsafe {
                let provider = WTHelperProvDataFromStateData(self.data.hWVTStateData);
                if provider.is_null() {
                    return None;
                }
                let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
                if signer.is_null()
                    || (*signer).csCertChain == 0
                    || (*signer).pasCertChain.is_null()
                {
                    return None;
                }
                let certificate = (*(*signer).pasCertChain).pCert;
                if certificate.is_null() {
                    return None;
                }

                let length = CertGetNameStringW(
                    certificate,
                    CERT_NAME_SIMPLE_DISPLAY_TYPE,
                    0,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    0,
                );
                if length <= 1 {
                    return None;
                }
                let mut buffer = vec![0_u16; length as usize];
                CertGetNameStringW(
                    certificate,
                    CERT_NAME_SIMPLE_DISPLAY_TYPE,
                    0,
                    std::ptr::null(),
                    buffer.as_mut_ptr(),
                    length,
                );
                let end = buffer
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(buffer.len());
                let name = String::from_utf16_lossy(&buffer[..end]).trim().to_string();
                (!name.is_empty()).then_some(name)
            }
        }
    }

    impl Drop for Verification {
        fn drop(&mut self) {
            self.data.dwStateAction = WTD_STATEACTION_CLOSE;
            let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
            // SAFETY: closes the state the VERIFY call opened, exactly once.
            unsafe {
                WinVerifyTrust(
                    std::ptr::null_mut(),
                    &mut action,
                    (&mut self.data as *mut WINTRUST_DATA).cast(),
                );
            }
        }
    }

    /// A catalog admin context, released on drop.
    struct CatalogAdmin(isize);

    impl Drop for CatalogAdmin {
        fn drop(&mut self) {
            // SAFETY: acquired by `CryptCATAdminAcquireContext2`, released once.
            unsafe { CryptCATAdminReleaseContext(self.0, 0) };
        }
    }

    /// A catalog context, released on drop.
    struct CatalogContext {
        admin: isize,
        info: isize,
    }

    impl Drop for CatalogContext {
        fn drop(&mut self) {
            // SAFETY: returned by `CryptCATAdminEnumCatalogFromHash` for this
            // admin context, released once.
            unsafe { CryptCATAdminReleaseCatalogContext(self.admin, self.info, 0) };
        }
    }

    /// Runs one verification and reads its signer while every structure it
    /// points at is still alive, then closes it.
    fn conclude(verification: Verification) -> (u32, Option<String>) {
        let result = verification.result;
        let signer = has_signer(classify(result).0)
            .then(|| verification.signer())
            .flatten();
        drop(verification);
        (result, signer)
    }

    fn verify_embedded(path: &[u16]) -> (u32, Option<String>) {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: path.as_ptr(),
            hFile: std::ptr::null_mut(),
            pgKnownSubject: std::ptr::null_mut(),
        };
        let data = WINTRUST_DATA {
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
            ..WINTRUST_DATA::default()
        };

        // SAFETY: `file` and `path` outlive both the VERIFY call and the
        // CLOSE call inside `conclude`.
        conclude(unsafe { Verification::run(data) })
    }

    /// Looks the file up in the system catalogs and verifies against the one
    /// that lists it. `None` when no catalog does.
    fn verify_catalog(path: &str, wide_path: &[u16]) -> Option<(u32, Option<String>)> {
        let file = std::fs::File::open(path).ok()?;
        let handle = file.as_raw_handle() as HANDLE;

        // SHA-256 catalogs first (modern Windows), then the SHA-1 default.
        for algorithm in [BCRYPT_SHA256_ALGORITHM, std::ptr::null()] {
            let mut admin: isize = 0;
            // SAFETY: a live out-parameter; null subsystem and policy select
            // the defaults.
            if unsafe {
                CryptCATAdminAcquireContext2(
                    &mut admin,
                    std::ptr::null(),
                    algorithm,
                    std::ptr::null(),
                    0,
                )
            } == 0
            {
                continue;
            }
            let admin = CatalogAdmin(admin);

            let mut size: u32 = 0;
            // SAFETY: a size query: null buffer, live length.
            unsafe {
                CryptCATAdminCalcHashFromFileHandle2(
                    admin.0,
                    handle,
                    &mut size,
                    std::ptr::null_mut(),
                    0,
                )
            };
            if size == 0 || size > 64 {
                continue;
            }
            let mut hash = vec![0_u8; size as usize];
            // SAFETY: `hash` is exactly `size` bytes.
            if unsafe {
                CryptCATAdminCalcHashFromFileHandle2(
                    admin.0,
                    handle,
                    &mut size,
                    hash.as_mut_ptr(),
                    0,
                )
            } == 0
            {
                continue;
            }

            // SAFETY: a live hash buffer of the stated size.
            let info = unsafe {
                CryptCATAdminEnumCatalogFromHash(
                    admin.0,
                    hash.as_ptr(),
                    size,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if info == 0 {
                continue;
            }
            let context = CatalogContext {
                admin: admin.0,
                info,
            };

            let mut catalog = CATALOG_INFO {
                cbStruct: std::mem::size_of::<CATALOG_INFO>() as u32,
                wszCatalogFile: [0; 260],
            };
            // SAFETY: a live, sized out-structure.
            if unsafe { CryptCATCatalogInfoFromContext(context.info, &mut catalog, 0) } == 0 {
                continue;
            }

            let tag: Vec<u16> = member_tag(&hash)
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut member = WINTRUST_CATALOG_INFO {
                cbStruct: std::mem::size_of::<WINTRUST_CATALOG_INFO>() as u32,
                dwCatalogVersion: 0,
                pcwszCatalogFilePath: catalog.wszCatalogFile.as_ptr(),
                pcwszMemberTag: tag.as_ptr(),
                pcwszMemberFilePath: wide_path.as_ptr(),
                hMemberFile: handle,
                pbCalculatedFileHash: hash.as_mut_ptr(),
                cbCalculatedFileHash: size,
                pcCatalogContext: std::ptr::null_mut(),
                hCatAdmin: admin.0,
            };
            let data = WINTRUST_DATA {
                dwUnionChoice: WTD_CHOICE_CATALOG,
                Anonymous: WINTRUST_DATA_0 {
                    pCatalog: &mut member,
                },
                ..WINTRUST_DATA::default()
            };

            // SAFETY: every structure `member` points at — the catalog path,
            // the tag, the hash, the open file, the admin context — lives
            // until the end of this iteration, after `conclude` has closed
            // the verification.
            let concluded = conclude(unsafe { Verification::run(data) });
            drop(context);
            return Some(concluded);
        }

        None
    }

    /// Verifies the Authenticode signature of the file at `path`.
    pub fn verify(path: &str) -> SignatureInfo {
        let wide_path = wide(path);

        let (embedded, signer) = verify_embedded(&wide_path);
        if embedded != hresult::TRUST_E_NOSIGNATURE {
            let (trust, detail) = classify(embedded);
            return SignatureInfo {
                trust,
                source: has_signer(trust).then_some(SignatureSource::Embedded),
                publisher: signer,
                detail,
            };
        }

        match verify_catalog(path, &wide_path) {
            Some((result, signer)) => {
                let (trust, detail) = classify(result);
                SignatureInfo {
                    trust,
                    source: Some(SignatureSource::Catalog),
                    publisher: signer,
                    detail,
                }
            }
            None => SignatureInfo {
                trust: TrustStatus::Unsigned,
                source: None,
                publisher: None,
                detail: "The file carries no embedded signature and no system catalog lists it."
                    .to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hresult::*;
    use super::*;

    #[test]
    fn success_is_trusted() {
        assert_eq!(classify(S_OK).0, TrustStatus::Trusted);
    }

    #[test]
    fn no_signature_is_unsigned() {
        assert_eq!(classify(TRUST_E_NOSIGNATURE).0, TrustStatus::Unsigned);
    }

    #[test]
    fn a_signature_windows_does_not_trust_is_not_called_invalid() {
        for code in [
            CERT_E_UNTRUSTEDROOT,
            CERT_E_CHAINING,
            CERT_E_EXPIRED,
            CERT_E_REVOKED,
            TRUST_E_EXPLICIT_DISTRUST,
            TRUST_E_SUBJECT_NOT_TRUSTED,
            CERT_E_UNTRUSTEDTESTROOT,
            CERT_E_UNTRUSTEDCA,
            CERT_E_WRONG_USAGE,
            TRUST_E_TIME_STAMP,
        ] {
            assert_eq!(
                classify(code).0,
                TrustStatus::SignedButUntrusted,
                "0x{code:08X}"
            );
        }
    }

    #[test]
    fn a_tampered_file_is_invalid() {
        for code in [
            TRUST_E_BAD_DIGEST,
            TRUST_E_NO_SIGNER_CERT,
            TRUST_E_CERT_SIGNATURE,
        ] {
            assert_eq!(classify(code).0, TrustStatus::Invalid, "0x{code:08X}");
        }
    }

    #[test]
    fn access_denied_is_permission_denied_not_unsigned() {
        assert_eq!(classify(E_ACCESSDENIED).0, TrustStatus::PermissionDenied);
    }

    #[test]
    fn everything_else_is_unavailable_with_its_code() {
        for code in [
            TRUST_E_SUBJECT_FORM_UNKNOWN,
            TRUST_E_PROVIDER_UNKNOWN,
            TRUST_E_ACTION_UNKNOWN,
            CRYPT_E_SECURITY_SETTINGS,
            CRYPT_E_FILE_ERROR,
        ] {
            assert_eq!(classify(code).0, TrustStatus::Unavailable);
        }
        let (status, text) = classify(0x8000_4005);
        assert_eq!(status, TrustStatus::Unavailable);
        assert!(text.contains("0x80004005"));
    }

    #[test]
    fn only_a_found_signature_has_a_signer_worth_naming() {
        assert!(has_signer(TrustStatus::Trusted));
        assert!(has_signer(TrustStatus::SignedButUntrusted));
        assert!(has_signer(TrustStatus::Invalid));
        assert!(!has_signer(TrustStatus::Unsigned));
        assert!(!has_signer(TrustStatus::Unavailable));
        assert!(!has_signer(TrustStatus::PermissionDenied));
    }

    #[test]
    fn the_member_tag_is_upper_case_hex() {
        assert_eq!(member_tag(&[0x0a, 0xbc, 0xff]), "0ABCFF");
    }

    #[test]
    fn the_raw_codes_match_the_windows_headers() {
        // Spot checks against winerror.h, so a typo cannot silently turn a
        // verdict into "unavailable".
        assert_eq!(TRUST_E_NOSIGNATURE, 0x800B0100);
        assert_eq!(CERT_E_UNTRUSTEDROOT, 0x800B0109);
        assert_eq!(TRUST_E_BAD_DIGEST, 0x80096010);
        assert_eq!(TRUST_E_EXPLICIT_DISTRUST, 0x800B0111);
    }
}
