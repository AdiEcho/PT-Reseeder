use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn new_stats_all_counters_start_at_zero() {
        let stats = ReseedStats::new();
        assert_eq!(stats.scanned.load(Ordering::Relaxed), 0);
        assert_eq!(stats.cached_skip.load(Ordering::Relaxed), 0);
        assert_eq!(stats.matched.load(Ordering::Relaxed), 0);
        assert_eq!(stats.added.load(Ordering::Relaxed), 0);
        assert_eq!(stats.failed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.skipped_tracker.load(Ordering::Relaxed), 0);
        assert_eq!(stats.skipped_history.load(Ordering::Relaxed), 0);
        assert_eq!(stats.skipped_exists.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn snapshot_captures_current_counter_values() {
        let stats = ReseedStats::new();
        stats.scanned.fetch_add(10, Ordering::Relaxed);
        stats.cached_skip.fetch_add(3, Ordering::Relaxed);
        stats.matched.fetch_add(7, Ordering::Relaxed);
        stats.added.fetch_add(5, Ordering::Relaxed);
        stats.failed.fetch_add(1, Ordering::Relaxed);
        stats.skipped_tracker.fetch_add(2, Ordering::Relaxed);
        stats.skipped_history.fetch_add(4, Ordering::Relaxed);
        stats.skipped_exists.fetch_add(6, Ordering::Relaxed);

        let snap = stats.snapshot();
        assert_eq!(snap.scanned, 10);
        assert_eq!(snap.cached_skip, 3);
        assert_eq!(snap.matched, 7);
        assert_eq!(snap.added, 5);
        assert_eq!(snap.failed, 1);
        assert_eq!(snap.skipped_tracker, 2);
        assert_eq!(snap.skipped_history, 4);
        assert_eq!(snap.skipped_exists, 6);
    }

    #[tokio::test]
    async fn atomic_counters_support_concurrent_increments() {
        let stats = Arc::new(ReseedStats::new());
        let num_tasks = 100;
        let increments_per_task = 1000;

        let mut handles = Vec::new();
        for _ in 0..num_tasks {
            let stats = Arc::clone(&stats);
            handles.push(tokio::spawn(async move {
                for _ in 0..increments_per_task {
                    stats.scanned.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let snap = stats.snapshot();
        assert_eq!(snap.scanned, num_tasks * increments_per_task);
    }

    #[test]
    fn total_processed_sums_correct_fields() {
        let snap = ReseedStatsSnapshot {
            scanned: 100,
            cached_skip: 50,
            matched: 80,
            added: 10,
            failed: 2,
            skipped_tracker: 3,
            skipped_history: 4,
            skipped_exists: 5,
        };
        // total_processed = added + failed + skipped_tracker + skipped_history + skipped_exists
        // = 10 + 2 + 3 + 4 + 5 = 24
        assert_eq!(snap.total_processed(), 24);
        // scanned, cached_skip, matched are NOT included
    }

    #[test]
    fn total_processed_returns_zero_for_default_snapshot() {
        let snap = ReseedStatsSnapshot {
            scanned: 0,
            cached_skip: 0,
            matched: 0,
            added: 0,
            failed: 0,
            skipped_tracker: 0,
            skipped_history: 0,
            skipped_exists: 0,
        };
        assert_eq!(snap.total_processed(), 0);
    }

    #[test]
    fn snapshot_serializes_to_json_and_back() {
        let snap = ReseedStatsSnapshot {
            scanned: 42,
            cached_skip: 10,
            matched: 32,
            added: 20,
            failed: 3,
            skipped_tracker: 1,
            skipped_history: 2,
            skipped_exists: 6,
        };

        let json = serde_json::to_string(&snap).expect("serialize to JSON");
        let deserialized: ReseedStatsSnapshot =
            serde_json::from_str(&json).expect("deserialize from JSON");

        assert_eq!(deserialized.scanned, snap.scanned);
        assert_eq!(deserialized.cached_skip, snap.cached_skip);
        assert_eq!(deserialized.matched, snap.matched);
        assert_eq!(deserialized.added, snap.added);
        assert_eq!(deserialized.failed, snap.failed);
        assert_eq!(deserialized.skipped_tracker, snap.skipped_tracker);
        assert_eq!(deserialized.skipped_history, snap.skipped_history);
        assert_eq!(deserialized.skipped_exists, snap.skipped_exists);
    }

    #[test]
    fn snapshot_clone_is_independent() {
        let original = ReseedStatsSnapshot {
            scanned: 5,
            cached_skip: 1,
            matched: 4,
            added: 3,
            failed: 0,
            skipped_tracker: 0,
            skipped_history: 1,
            skipped_exists: 0,
        };

        let mut cloned = original.clone();
        cloned.scanned = 999;
        cloned.added = 888;

        // Original is unchanged
        assert_eq!(original.scanned, 5);
        assert_eq!(original.added, 3);
        // Clone has new values
        assert_eq!(cloned.scanned, 999);
        assert_eq!(cloned.added, 888);
    }
}
