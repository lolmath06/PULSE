//! Why history could not be read or written.

use std::fmt;
use std::path::PathBuf;

/// Everything that can go wrong with history.
///
/// None of these ever stops PULSE: a history that cannot open leaves the live
/// monitor fully working and the UI saying *History unavailable: <reason>*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    /// The database file or its directory could not be opened or created.
    Open { path: PathBuf, message: String },
    /// The file was written by a **newer** PULSE. It is left untouched —
    /// never migrated backwards, never deleted.
    FutureSchema { found: u32, supported: u32 },
    /// The file holds a schema version PULSE cannot read at all.
    InvalidSchema(String),
    /// SQLite reported an error.
    Database(String),
    /// The request itself was malformed.
    InvalidRequest(String),
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HistoryError::Open { path, message } => {
                write!(f, "cannot open {}: {message}", path.display())
            }
            HistoryError::FutureSchema { found, supported } => write!(
                f,
                "the history database uses schema version {found}, but this PULSE \
                 understands up to version {supported}; it was left untouched"
            ),
            HistoryError::InvalidSchema(message) => {
                write!(f, "unreadable history schema: {message}")
            }
            HistoryError::Database(message) => write!(f, "database error: {message}"),
            HistoryError::InvalidRequest(message) => write!(f, "invalid request: {message}"),
        }
    }
}

impl std::error::Error for HistoryError {}

impl From<rusqlite::Error> for HistoryError {
    fn from(error: rusqlite::Error) -> Self {
        HistoryError::Database(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_future_schema_says_the_file_was_left_alone() {
        let message = HistoryError::FutureSchema {
            found: 7,
            supported: 1,
        }
        .to_string();

        assert!(message.contains('7'));
        assert!(message.contains("left untouched"));
    }

    #[test]
    fn sqlite_errors_convert_without_panicking() {
        let error: HistoryError = rusqlite::Error::InvalidQuery.into();
        assert!(matches!(error, HistoryError::Database(_)));
    }
}
