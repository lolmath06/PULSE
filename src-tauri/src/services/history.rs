//! Wires the history subsystem to the running engine.
//!
//! The composition point for history, as `services::metrics::build_engine` is
//! for the engine: it picks the historized metrics out of the live catalog and
//! starts the one scheduler. Tauri-free — the data directory and the event
//! sink come in as arguments — so the Windows harness checks it too.

use std::path::PathBuf;
use std::sync::Arc;

use crate::history::service::HistoryConfig;
use crate::history::{self, selection, HistoryEventSink, HistoryService, SystemClock};
use crate::metrics::MetricsEngine;

/// Starts history for `engine`.
///
/// `local_data_dir` is the platform's application data directory, or the
/// reason it could not be resolved — in which case history is unavailable and
/// says why, and PULSE runs live-only.
pub fn start_history(
    engine: Arc<MetricsEngine>,
    local_data_dir: Result<PathBuf, String>,
    events: Arc<dyn HistoryEventSink>,
) -> HistoryService {
    let metrics = selection::historized_refs(engine.catalog());
    let clock = Arc::new(SystemClock);

    match local_data_dir {
        Ok(dir) => HistoryService::start(
            HistoryConfig::new(history::database_path(&dir)),
            metrics,
            engine,
            clock,
            events,
        ),
        Err(reason) => HistoryService::unavailable(
            format!("no application data directory: {reason}"),
            metrics.len(),
            clock,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::testing::TempDir;
    use crate::history::{HistoryState, NoEvents};

    #[test]
    fn history_is_not_a_provider() {
        let engine = Arc::new(crate::services::metrics::build_engine());
        let providers_before = engine.status().provider_count;
        let dir = TempDir::new("compose");

        let service = start_history(
            Arc::clone(&engine),
            Ok(dir.path().to_path_buf()),
            Arc::new(NoEvents),
        );
        service.shutdown();

        assert_eq!(engine.status().provider_count, providers_before);
        if crate::platform::PlatformKind::current().is_supported() {
            assert_eq!(providers_before, 6);
            assert!(service.status(false).historized_metric_count > 0);
        }
    }

    #[test]
    fn a_missing_data_directory_makes_history_unavailable_not_fatal() {
        let engine = Arc::new(crate::services::metrics::build_engine());

        let service = start_history(engine, Err("no home".into()), Arc::new(NoEvents));

        match service.status(false).state {
            HistoryState::Unavailable { reason } => assert!(reason.contains("no home")),
            HistoryState::Recording => panic!("must not record without a directory"),
        }
    }

    #[test]
    fn cpu_total_is_historized_from_the_real_catalog() {
        if !crate::platform::PlatformKind::current().is_supported() {
            return;
        }
        let engine = crate::services::metrics::build_engine();
        let selected = selection::historized_refs(engine.catalog());

        assert!(selected
            .iter()
            .any(|metric| metric == &crate::metrics::wellknown::cpu::usage_total_ref()));
        assert!(selected
            .iter()
            .all(|metric| !metric.source_id.as_str().starts_with("process:")
                || metric.source_id.as_str() == "process:system"));
    }
}
