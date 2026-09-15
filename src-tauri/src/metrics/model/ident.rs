//! Shared validation helpers for PULSE's identifier types.
//!
//! Every identifier in the metrics contract (`MetricKey`, `SourceId`,
//! `ProviderId`) is a validated newtype rather than a bare `String`. This is
//! deliberate: these identifiers end up inside saved dashboard configuration,
//! so a typo must be rejected at the boundary instead of silently producing a
//! reference that can never resolve.

use std::fmt;

/// Why an identifier was rejected.
///
/// Carries enough detail for a provider author to fix the mistake without
/// reading the validation source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentError {
    Empty {
        kind: &'static str,
    },
    TooLong {
        kind: &'static str,
        max: usize,
        actual: usize,
    },
    SegmentCount {
        kind: &'static str,
        min: usize,
        max: usize,
        actual: usize,
    },
    EmptySegment {
        kind: &'static str,
    },
    InvalidCharacter {
        kind: &'static str,
        segment: String,
        found: char,
    },
    MustStartWithLetter {
        kind: &'static str,
        segment: String,
    },
    MissingSeparator {
        kind: &'static str,
        separator: char,
    },
}

impl fmt::Display for IdentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentError::Empty { kind } => write!(f, "{kind} must not be empty"),
            IdentError::TooLong { kind, max, actual } => {
                write!(f, "{kind} is {actual} characters, maximum is {max}")
            }
            IdentError::SegmentCount {
                kind,
                min,
                max,
                actual,
            } => write!(
                f,
                "{kind} has {actual} segments, expected between {min} and {max}"
            ),
            IdentError::EmptySegment { kind } => {
                write!(f, "{kind} contains an empty segment")
            }
            IdentError::InvalidCharacter {
                kind,
                segment,
                found,
            } => write!(
                f,
                "{kind} segment '{segment}' contains the invalid character '{found}'"
            ),
            IdentError::MustStartWithLetter { kind, segment } => write!(
                f,
                "{kind} segment '{segment}' must start with a lowercase letter"
            ),
            IdentError::MissingSeparator { kind, separator } => {
                write!(f, "{kind} must contain a '{separator}' separator")
            }
        }
    }
}

impl std::error::Error for IdentError {}

/// Whether a segment is allowed to begin with a digit.
///
/// Namespace-like segments (metric keys, provider ids, source kinds) must start
/// with a letter so they read as words. Device instances must not: `cpu:0`,
/// `storage:2`, and PCI addresses are all legitimately digit-led.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeadingDigit {
    Forbidden,
    Allowed,
}

/// Validates one dot- or colon-separated segment.
///
/// Rules: continues with `a`-`z`, `0`-`9`, or any of the characters in `extra`.
/// Uppercase is rejected outright rather than silently lowercased, so two
/// providers cannot disagree about casing. The first character must be a
/// lowercase letter unless `leading` permits a digit.
pub(crate) fn validate_segment(
    kind: &'static str,
    segment: &str,
    extra: &[char],
    leading: LeadingDigit,
) -> Result<(), IdentError> {
    let mut chars = segment.chars();

    let Some(first) = chars.next() else {
        return Err(IdentError::EmptySegment { kind });
    };

    let first_ok =
        first.is_ascii_lowercase() || (leading == LeadingDigit::Allowed && first.is_ascii_digit());

    if !first_ok {
        return Err(IdentError::MustStartWithLetter {
            kind,
            segment: segment.to_string(),
        });
    }

    for ch in chars {
        let ok = ch.is_ascii_lowercase() || ch.is_ascii_digit() || extra.contains(&ch);
        if !ok {
            return Err(IdentError::InvalidCharacter {
                kind,
                segment: segment.to_string(),
                found: ch,
            });
        }
    }

    Ok(())
}

/// Validates the overall length of an identifier.
pub(crate) fn validate_len(kind: &'static str, value: &str, max: usize) -> Result<(), IdentError> {
    if value.is_empty() {
        return Err(IdentError::Empty { kind });
    }
    if value.len() > max {
        return Err(IdentError::TooLong {
            kind,
            max,
            actual: value.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_DIGIT: LeadingDigit = LeadingDigit::Forbidden;
    const DIGIT_OK: LeadingDigit = LeadingDigit::Allowed;

    #[test]
    fn accepts_a_well_formed_segment() {
        assert!(validate_segment("test", "cpu", &[], NO_DIGIT).is_ok());
        assert!(validate_segment("test", "usage2", &[], NO_DIGIT).is_ok());
        assert!(validate_segment("test", "pci-0000", &['-'], NO_DIGIT).is_ok());
    }

    #[test]
    fn rejects_uppercase_rather_than_normalising_it() {
        // Silently lowercasing would let two providers disagree about casing
        // while both "working".
        assert!(matches!(
            validate_segment("test", "CPU", &[], NO_DIGIT),
            Err(IdentError::MustStartWithLetter { .. })
        ));
        assert!(matches!(
            validate_segment("test", "cPu", &[], NO_DIGIT),
            Err(IdentError::InvalidCharacter { found: 'P', .. })
        ));
    }

    #[test]
    fn rejects_a_segment_starting_with_a_digit_or_symbol() {
        assert!(matches!(
            validate_segment("test", "0cpu", &[], NO_DIGIT),
            Err(IdentError::MustStartWithLetter { .. })
        ));
        assert!(matches!(
            validate_segment("test", "-cpu", &['-'], NO_DIGIT),
            Err(IdentError::MustStartWithLetter { .. })
        ));
    }

    #[test]
    fn allows_a_digit_led_segment_where_device_names_require_it() {
        // `cpu:0`, `storage:2` — real device instances are often just numbers.
        assert!(validate_segment("test", "0", &[], DIGIT_OK).is_ok());
        assert!(validate_segment("test", "0000-01-00-0", &['-'], DIGIT_OK).is_ok());
        // A symbol still may not lead.
        assert!(matches!(
            validate_segment("test", "-0", &['-'], DIGIT_OK),
            Err(IdentError::MustStartWithLetter { .. })
        ));
    }

    #[test]
    fn rejects_an_empty_segment() {
        assert!(matches!(
            validate_segment("test", "", &[], NO_DIGIT),
            Err(IdentError::EmptySegment { .. })
        ));
        assert!(matches!(
            validate_segment("test", "", &[], DIGIT_OK),
            Err(IdentError::EmptySegment { .. })
        ));
    }

    #[test]
    fn rejects_characters_outside_the_allowed_set() {
        assert!(matches!(
            validate_segment("test", "cpu usage", &[], NO_DIGIT),
            Err(IdentError::InvalidCharacter { found: ' ', .. })
        ));
        assert!(matches!(
            validate_segment("test", "cpu/usage", &[], NO_DIGIT),
            Err(IdentError::InvalidCharacter { found: '/', .. })
        ));
    }

    #[test]
    fn enforces_length_bounds() {
        assert!(validate_len("test", "abc", 8).is_ok());
        assert!(matches!(
            validate_len("test", "", 8),
            Err(IdentError::Empty { .. })
        ));
        assert!(matches!(
            validate_len("test", "abcdefghij", 8),
            Err(IdentError::TooLong { actual: 10, .. })
        ));
    }
}
