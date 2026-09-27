//! Test helpers shared by the history tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::metrics::model::MetricRef;

/// A scratch directory under the system temp dir, removed on drop.
///
/// Every test gets its own: history tests open real files, because WAL mode,
/// a second reader connection and a relaunch cannot be exercised in memory.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = format!(
            "pulse-history-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let path = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn database(&self) -> PathBuf {
        super::database_path(&self.0)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn metric(key: &str, source: &str) -> MetricRef {
    MetricRef::parse(key, source).expect("valid reference")
}

pub fn cpu_total() -> MetricRef {
    metric("cpu.usage.total", "cpu:system")
}

pub fn memory_percent() -> MetricRef {
    metric("memory.usage.percent", "memory:system")
}
