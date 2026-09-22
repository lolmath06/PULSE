//! Streaming SHA-256 of an executable, with proof it did not change mid-read.
//!
//! # Lazy, always
//!
//! Hashing is the most expensive thing the inspector can do — a browser's
//! main binary is hundreds of megabytes — so it happens only when the user
//! clicks *Compute SHA-256* (or an action that needs the hash). Never on
//! Refresh, never for every row, never in the background.
//!
//! # Bounded memory
//!
//! The file is read in fixed [`CHUNK_SIZE`] pieces into one reused buffer.
//! Nothing is ever read to the end into memory, whatever the file's size.
//!
//! # A hash of a file that changed is not a hash of that file
//!
//! An installer or package update can rewrite an executable while PULSE is
//! reading it. The digest would then describe no file that ever existed. So
//! the file's identity (device, inode, size, modification time) is taken
//! before and after the read; if they differ — or if the number of bytes read
//! differs from the size — the result is [`HashStatus::ChangedWhileHashing`]
//! and no digest is presented as definitive.

use std::io::{self, Read};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The read size. Large enough that a 300 MB binary is ~5 000 reads, small
/// enough to be irrelevant to memory.
pub const CHUNK_SIZE: usize = 64 * 1024;

/// The facts that identify one version of one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    pub is_file: bool,
    pub len: u64,
    pub modified: Option<SystemTime>,
    /// `(device, inode)` where the platform has them.
    pub file_id: Option<(u64, u64)>,
}

impl Fingerprint {
    pub fn of(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        let file_id = {
            use std::os::unix::fs::MetadataExt;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(not(unix))]
        let file_id = None;

        Self {
            is_file: metadata.is_file(),
            len: metadata.len(),
            modified: metadata.modified().ok(),
            file_id,
        }
    }
}

/// Something that can be fingerprinted and read — a real file, or a test
/// double that changes under the reader.
pub trait HashInput: Read {
    fn fingerprint(&self) -> io::Result<Fingerprint>;
}

impl HashInput for std::fs::File {
    fn fingerprint(&self) -> io::Result<Fingerprint> {
        self.metadata().map(|metadata| Fingerprint::of(&metadata))
    }
}

/// How a hash attempt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HashStatus {
    Computed,
    /// The file was replaced or modified during the read.
    ChangedWhileHashing,
    PermissionDenied,
    NotFound,
    /// A directory or device, not a regular file.
    NotAFile,
    ReadError,
}

/// The outcome of hashing one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHash {
    pub status: HashStatus,
    /// Lower-case hex. Present only when `status` is `computed`.
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub reason: Option<String>,
}

impl FileHash {
    fn failed(status: HashStatus, reason: impl Into<String>) -> Self {
        Self {
            status,
            sha256: None,
            size_bytes: None,
            reason: Some(reason.into()),
        }
    }

    /// The outcome for an input that could not even be opened.
    pub fn from_open_error(error: &io::Error) -> Self {
        let (status, reason) = match error.kind() {
            io::ErrorKind::PermissionDenied => (
                HashStatus::PermissionDenied,
                "The executable is not readable by the user PULSE runs as.",
            ),
            io::ErrorKind::NotFound => (
                HashStatus::NotFound,
                "The executable no longer exists at that path.",
            ),
            _ => (HashStatus::ReadError, "The executable could not be opened."),
        };
        Self::failed(status, reason)
    }
}

/// Hashes `input` in bounded chunks.
pub fn sha256_stream<I: HashInput>(input: &mut I) -> FileHash {
    let before = match input.fingerprint() {
        Ok(fingerprint) => fingerprint,
        Err(error) => return FileHash::from_open_error(&error),
    };

    if !before.is_file {
        return FileHash::failed(HashStatus::NotAFile, "The target is not a regular file.");
    }

    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK_SIZE];
    let mut total: u64 = 0;

    loop {
        match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                hasher.update(&buffer[..read]);
                total = total.saturating_add(read as u64);
                // A file that keeps growing past its original size is being
                // written to; stop rather than follow it forever.
                if total > before.len {
                    return changed();
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return FileHash::failed(
                    HashStatus::ReadError,
                    format!("Reading the executable failed: {error}"),
                )
            }
        }
    }

    let after = match input.fingerprint() {
        Ok(fingerprint) => fingerprint,
        Err(_) => return changed(),
    };

    if after != before || total != before.len {
        return changed();
    }

    FileHash {
        status: HashStatus::Computed,
        sha256: Some(to_hex(&hasher.finalize())),
        size_bytes: Some(total),
        reason: None,
    }
}

fn changed() -> FileHash {
    FileHash::failed(
        HashStatus::ChangedWhileHashing,
        "The executable changed while it was being read, so no hash is shown as definitive.",
    )
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    text
}

/// Whether `text` is a SHA-256 digest in the lower-case hex PULSE produces.
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A file double that can change its fingerprint during the read.
    struct Scripted {
        data: Cursor<Vec<u8>>,
        fingerprints: std::cell::RefCell<Vec<io::Result<Fingerprint>>>,
    }

    impl Scripted {
        fn new(data: &[u8], fingerprints: Vec<io::Result<Fingerprint>>) -> Self {
            Self {
                data: Cursor::new(data.to_vec()),
                fingerprints: std::cell::RefCell::new(fingerprints),
            }
        }
    }

    impl Read for Scripted {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.data.read(buffer)
        }
    }

    impl HashInput for Scripted {
        fn fingerprint(&self) -> io::Result<Fingerprint> {
            self.fingerprints.borrow_mut().remove(0)
        }
    }

    fn file(len: u64) -> Fingerprint {
        Fingerprint {
            is_file: true,
            len,
            modified: Some(SystemTime::UNIX_EPOCH),
            file_id: Some((1, 42)),
        }
    }

    #[test]
    fn hashes_a_known_fixture_exactly() {
        let data = b"abc";
        let mut input = Scripted::new(data, vec![Ok(file(3)), Ok(file(3))]);
        let hash = sha256_stream(&mut input);

        assert_eq!(hash.status, HashStatus::Computed);
        assert_eq!(
            hash.sha256.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(hash.size_bytes, Some(3));
    }

    #[test]
    fn hashes_the_empty_file() {
        let mut input = Scripted::new(b"", vec![Ok(file(0)), Ok(file(0))]);
        assert_eq!(
            sha256_stream(&mut input).sha256.as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    #[test]
    fn a_file_modified_during_the_read_has_no_definitive_hash() {
        let mut modified = file(3);
        modified.modified = Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1));
        let mut input = Scripted::new(b"abc", vec![Ok(file(3)), Ok(modified)]);

        let hash = sha256_stream(&mut input);
        assert_eq!(hash.status, HashStatus::ChangedWhileHashing);
        assert_eq!(hash.sha256, None);
    }

    #[test]
    fn a_file_replaced_during_the_read_has_no_definitive_hash() {
        let mut replaced = file(3);
        replaced.file_id = Some((1, 43));
        let mut input = Scripted::new(b"abc", vec![Ok(file(3)), Ok(replaced)]);

        assert_eq!(
            sha256_stream(&mut input).status,
            HashStatus::ChangedWhileHashing
        );
    }

    #[test]
    fn a_file_that_grows_during_the_read_is_not_followed() {
        let mut input = Scripted::new(b"abcdef", vec![Ok(file(3)), Ok(file(3))]);
        assert_eq!(
            sha256_stream(&mut input).status,
            HashStatus::ChangedWhileHashing
        );
    }

    #[test]
    fn a_file_that_shrinks_during_the_read_is_not_trusted() {
        let mut input = Scripted::new(b"ab", vec![Ok(file(3)), Ok(file(3))]);
        assert_eq!(
            sha256_stream(&mut input).status,
            HashStatus::ChangedWhileHashing
        );
    }

    #[test]
    fn a_directory_is_not_hashed() {
        let mut directory = file(4096);
        directory.is_file = false;
        let mut input = Scripted::new(b"", vec![Ok(directory)]);

        assert_eq!(sha256_stream(&mut input).status, HashStatus::NotAFile);
    }

    #[test]
    fn a_real_directory_is_refused_too() {
        let directory = std::env::temp_dir();
        match std::fs::File::open(&directory) {
            Ok(mut handle) => {
                assert_eq!(sha256_stream(&mut handle).status, HashStatus::NotAFile);
            }
            Err(error) => {
                // Windows refuses to open a directory as a file at all.
                assert_ne!(
                    FileHash::from_open_error(&error).status,
                    HashStatus::Computed
                );
            }
        }
    }

    #[test]
    fn open_errors_map_to_their_own_statuses() {
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        let missing = io::Error::from(io::ErrorKind::NotFound);
        let other = io::Error::other("boom");

        assert_eq!(
            FileHash::from_open_error(&denied).status,
            HashStatus::PermissionDenied
        );
        assert_eq!(
            FileHash::from_open_error(&missing).status,
            HashStatus::NotFound
        );
        assert_eq!(
            FileHash::from_open_error(&other).status,
            HashStatus::ReadError
        );
    }

    #[test]
    fn a_file_that_disappeared_is_not_found() {
        let path = std::env::temp_dir().join(format!("pulse-hash-missing-{}", std::process::id()));
        let error = std::fs::File::open(&path).expect_err("must not exist");
        assert_eq!(
            FileHash::from_open_error(&error).status,
            HashStatus::NotFound
        );
    }

    #[test]
    fn a_large_file_streams_in_bounded_chunks_and_matches_a_one_shot_digest() {
        // Several chunks plus a remainder, through a real file.
        let data: Vec<u8> = (0..(CHUNK_SIZE * 5 + 123))
            .map(|index| (index % 251) as u8)
            .collect();
        let path = std::env::temp_dir().join(format!("pulse-hash-large-{}", std::process::id()));
        std::fs::write(&path, &data).expect("write fixture");

        let mut handle = std::fs::File::open(&path).expect("open fixture");
        let streamed = sha256_stream(&mut handle);
        std::fs::remove_file(&path).ok();

        assert_eq!(streamed.status, HashStatus::Computed);
        assert_eq!(streamed.size_bytes, Some(data.len() as u64));
        assert_eq!(streamed.sha256, Some(to_hex(&Sha256::digest(&data))));
    }

    #[test]
    fn recognises_only_lower_case_sha256_hex() {
        assert!(is_sha256_hex(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
        assert!(!is_sha256_hex(
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
        ));
        assert!(!is_sha256_hex("abc"));
        assert!(!is_sha256_hex(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015a/"
        ));
        assert!(!is_sha256_hex(&"a".repeat(65)));
    }
}
