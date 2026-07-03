use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};

/// Accumulated statistics for a reseed pipeline run.
#[derive(Debug, Default)]
pub struct ReseedStats {
    pub scanned: AtomicU64,
    pub cached_skip: AtomicU64,
    pub matched: AtomicU64,
    pub added: AtomicU64,
    pub failed: AtomicU64,
    pub skipped_tracker: AtomicU64,
    pub skipped_history: AtomicU64,
    pub skipped_exists: AtomicU64,
}

impl ReseedStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> ReseedStatsSnapshot {
        ReseedStatsSnapshot {
            scanned: self.scanned.load(Ordering::Relaxed),
            cached_skip: self.cached_skip.load(Ordering::Relaxed),
            matched: self.matched.load(Ordering::Relaxed),
            added: self.added.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            skipped_tracker: self.skipped_tracker.load(Ordering::Relaxed),
            skipped_history: self.skipped_history.load(Ordering::Relaxed),
            skipped_exists: self.skipped_exists.load(Ordering::Relaxed),
        }
    }
}

/// Immutable snapshot of stats, safe to serialize and send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReseedStatsSnapshot {
    pub scanned: u64,
    pub cached_skip: u64,
    pub matched: u64,
    pub added: u64,
    pub failed: u64,
    pub skipped_tracker: u64,
    pub skipped_history: u64,
    pub skipped_exists: u64,
}

impl ReseedStatsSnapshot {
    pub fn total_processed(&self) -> u64 {
        self.added + self.failed + self.skipped_tracker + self.skipped_history + self.skipped_exists
    }
}
