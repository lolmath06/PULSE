//! Building the URLs behind *Search online*, *Search hash online* and *Check
//! hash on VirusTotal*.
//!
//! # Nothing leaves the machine until the user clicks
//!
//! PULSE makes **no** network request of its own for any of this. These
//! functions only build a URL; the command layer hands it to the operating
//! system's default browser, and only in response to a click. Inspecting a
//! process, hashing it, reading its signature or its package never touches
//! the network.
//!
//! # What a query may contain
//!
//! Only names that describe the *program*, never the *machine*:
//!
//! | Allowed | Refused |
//! |---|---|
//! | process name | full path, e.g. `/home/alice/project/tool` |
//! | executable file name | the home directory, in any term |
//! | publisher / package name | the local user name |
//! | a SHA-256 digest | PID, command line, arguments, environment |
//!
//! The interface composes queries from the allowed fields only; this module
//! checks again, because a query is the one piece of process data PULSE ever
//! sends anywhere.
//!
//! # VirusTotal: the hash, never the file
//!
//! [`virustotal_url`] builds the address of VirusTotal's page for a digest.
//! Opening it sends VirusTotal 64 hex characters — which it may or may not
//! know. PULSE never uploads a file, never calls VirusTotal's API, and does
//! nothing further if the hash is unknown there.

use super::hash::is_sha256_hex;

/// A web search engine: a display name and a URL prefix the encoded query is
/// appended to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchProvider {
    pub name: &'static str,
    pub query_prefix: &'static str,
}

/// The engine *Search online* uses. One constant, so that making it
/// configurable later is a change in one place.
pub const SEARCH_PROVIDER: SearchProvider = SearchProvider {
    name: "DuckDuckGo",
    query_prefix: "https://duckduckgo.com/?q=",
};

/// VirusTotal's page for a file digest. The digest is appended.
pub const VIRUSTOTAL_FILE_PREFIX: &str = "https://www.virustotal.com/gui/file/";

/// At most this many terms, each at most [`MAX_TERM_CHARS`] characters.
pub const MAX_TERMS: usize = 4;
pub const MAX_TERM_CHARS: usize = 120;

/// What the local machine must never contribute to a query.
#[derive(Debug, Clone, Default)]
pub struct PrivateContext {
    /// The user's home directory, e.g. `/home/alice` or `C:\Users\alice`.
    pub home: Option<String>,
    /// The user's login name.
    pub user: Option<String>,
}

impl PrivateContext {
    /// The context of the user PULSE runs as, from the environment.
    pub fn current() -> Self {
        let home = std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USERPROFILE").ok())
            .filter(|home| home.len() > 1);
        let user = std::env::var("USER")
            .ok()
            .or_else(|| std::env::var("USERNAME").ok())
            .filter(|user| !user.is_empty());
        Self { home, user }
    }
}

/// Builds the *Search online* URL from program-describing terms.
///
/// Terms are trimmed, de-duplicated case-insensitively and joined with
/// spaces. A term that is — or contains — a path, the home directory or the
/// user's name is refused rather than silently dropped, so a composition bug
/// surfaces as an error instead of a quieter leak.
pub fn web_search_url(terms: &[String], private: &PrivateContext) -> Result<String, String> {
    let query = compose_query(terms, private)?;
    Ok(format!(
        "{}{}",
        SEARCH_PROVIDER.query_prefix,
        encode_component(&query)
    ))
}

/// Builds the *Search hash online* URL.
pub fn hash_search_url(sha256: &str) -> Result<String, String> {
    check_hash(sha256)?;
    Ok(format!("{}{sha256}", SEARCH_PROVIDER.query_prefix))
}

/// Builds the *Check hash on VirusTotal* URL — the digest's page, nothing
/// else.
pub fn virustotal_url(sha256: &str) -> Result<String, String> {
    check_hash(sha256)?;
    Ok(format!("{VIRUSTOTAL_FILE_PREFIX}{sha256}"))
}

fn check_hash(sha256: &str) -> Result<(), String> {
    if is_sha256_hex(sha256) {
        Ok(())
    } else {
        Err("A SHA-256 digest is 64 lower-case hexadecimal characters.".to_string())
    }
}

/// Validates and joins the terms.
pub fn compose_query(terms: &[String], private: &PrivateContext) -> Result<String, String> {
    if terms.len() > MAX_TERMS {
        return Err(format!("At most {MAX_TERMS} search terms are accepted."));
    }

    let mut accepted: Vec<String> = Vec::new();

    for term in terms {
        let term: String = term
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        if term.is_empty() {
            continue;
        }
        if looks_like_path(&term) {
            return Err("A search term may not be a file path.".to_string());
        }
        if let Some(home) = private.home.as_deref() {
            if term.contains(home) {
                return Err("A search term may not contain the home directory.".to_string());
            }
        }
        if let Some(user) = private.user.as_deref() {
            if term.eq_ignore_ascii_case(user) {
                return Err("A search term may not be the local user name.".to_string());
            }
        }

        let term: String = term.chars().take(MAX_TERM_CHARS).collect();
        if !accepted
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&term))
        {
            accepted.push(term);
        }
    }

    if accepted.is_empty() {
        return Err("There is nothing to search for.".to_string());
    }

    Ok(accepted.join(" "))
}

/// Whether a term is an absolute or home-relative path.
///
/// Process *names* may legitimately contain a slash (`kworker/3:1`), so a
/// slash alone is not enough: a path starts at a root, a drive or `~`, or
/// contains a separator followed by more path.
fn looks_like_path(term: &str) -> bool {
    let bytes = term.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');

    term.starts_with('/')
        || term.starts_with('~')
        || term.starts_with("\\\\")
        || drive
        || term.matches('/').count() >= 2
        || term.contains('\\')
}

/// Percent-encodes one URL query component (RFC 3986 unreserved set kept).
pub fn encode_component(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn alice() -> PrivateContext {
        PrivateContext {
            home: Some("/home/alice".to_string()),
            user: Some("alice".to_string()),
        }
    }

    const HASH: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn spaces_are_encoded() {
        let url = web_search_url(&terms(&["Isolated Web Content"]), &alice()).expect("url");
        assert_eq!(url, "https://duckduckgo.com/?q=Isolated%20Web%20Content");
    }

    #[test]
    fn unicode_is_encoded_as_utf8() {
        let url = web_search_url(&terms(&["Überwachung", "日本"]), &alice()).expect("url");
        assert_eq!(
            url,
            "https://duckduckgo.com/?q=%C3%9Cberwachung%20%E6%97%A5%E6%9C%AC"
        );
    }

    #[test]
    fn slash_and_ampersand_cannot_break_out_of_the_query() {
        let url = web_search_url(&terms(&["kworker/3:1", "A&B=C?#"]), &alice()).expect("url");
        assert_eq!(
            url,
            "https://duckduckgo.com/?q=kworker%2F3%3A1%20A%26B%3DC%3F%23"
        );
        assert_eq!(url.matches('&').count(), 0);
        assert_eq!(url.matches('?').count(), 1);
    }

    #[test]
    fn a_very_long_name_is_bounded() {
        let long = "x".repeat(10_000);
        let url = web_search_url(&terms(&[&long]), &alice()).expect("url");
        assert!(url.len() < SEARCH_PROVIDER.query_prefix.len() + MAX_TERM_CHARS + 1);
    }

    #[test]
    fn a_full_path_never_reaches_the_query() {
        let path = "/home/alice/private/project/token-app";
        assert!(web_search_url(&terms(&["token-app", path]), &alice()).is_err());
        assert!(web_search_url(&terms(&[path]), &PrivateContext::default()).is_err());

        let url = web_search_url(&terms(&["token-app"]), &alice()).expect("basename is fine");
        assert!(!url.contains("home"));
        assert!(!url.contains("alice"));
        assert!(!url.contains("private"));
    }

    #[test]
    fn windows_paths_are_refused_too() {
        for path in [
            r"C:\Users\alice\tool.exe",
            "C:/Users/alice/tool.exe",
            r"\\server\share\tool.exe",
            "~/bin/tool",
            "bin/local/tool",
        ] {
            assert!(
                web_search_url(&terms(&[path]), &PrivateContext::default()).is_err(),
                "accepted {path}"
            );
        }
    }

    #[test]
    fn the_home_directory_and_user_name_are_refused() {
        assert!(web_search_url(&terms(&["x /home/alice y"]), &alice()).is_err());
        assert!(web_search_url(&terms(&["alice"]), &alice()).is_err());
        assert!(web_search_url(&terms(&["ALICE"]), &alice()).is_err());
    }

    #[test]
    fn duplicate_and_empty_terms_collapse() {
        let query = compose_query(&terms(&["bash", "  ", "BASH", "bash"]), &alice()).expect("q");
        assert_eq!(query, "bash");
        assert!(compose_query(&terms(&["", "   "]), &alice()).is_err());
        assert!(compose_query(&terms(&["a", "b", "c", "d", "e"]), &alice()).is_err());
    }

    #[test]
    fn control_characters_are_stripped() {
        let query = compose_query(&terms(&["bad\nname\u{7}"]), &alice()).expect("q");
        assert_eq!(query, "badname");
    }

    #[test]
    fn hash_urls_carry_only_the_digest() {
        assert_eq!(
            virustotal_url(HASH).expect("url"),
            format!("https://www.virustotal.com/gui/file/{HASH}")
        );
        assert_eq!(
            hash_search_url(HASH).expect("url"),
            format!("https://duckduckgo.com/?q={HASH}")
        );
    }

    #[test]
    fn a_malformed_digest_is_refused() {
        for bad in [
            "",
            "abc",
            "../../etc/passwd",
            &HASH.to_uppercase(),
            &format!("{HASH}&x=1"),
        ] {
            assert!(virustotal_url(bad).is_err(), "{bad}");
            assert!(hash_search_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_providers_are_https() {
        assert!(SEARCH_PROVIDER.query_prefix.starts_with("https://"));
        assert!(VIRUSTOTAL_FILE_PREFIX.starts_with("https://"));
    }
}
