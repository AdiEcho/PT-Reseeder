use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

use crate::error::{CoreError, SchedulerError};

/// Callback type for file-watch-triggered task execution.
pub type FileWatchCallback = Arc<dyn Fn(i64, PathBuf) + Send + Sync>;

/// Watches folders for new .torrent files and triggers task execution.
pub struct FileWatcher {
    /// Maps task_id -> list of watched folder paths.
    watches: Arc<RwLock<HashMap<i64, Vec<PathBuf>>>>,
    /// The underlying notify watcher handle.
    watcher: Arc<RwLock<Option<RecommendedWatcher>>>,
    callback: FileWatchCallback,
    /// Sender to keep the event loop alive.
    _event_tx: mpsc::Sender<()>,
}

impl FileWatcher {
    /// Create a new FileWatcher. The callback is invoked with (task_id, file_path)
    /// whenever a new .torrent file is detected in a watched folder.
    pub fn new(callback: FileWatchCallback) -> Result<Self, CoreError> {
        let (event_tx, _event_rx) = mpsc::channel(1);
        Ok(Self {
            watches: Arc::new(RwLock::new(HashMap::new())),
            watcher: Arc::new(RwLock::new(None)),
            callback,
            _event_tx: event_tx,
        })
    }

    /// Start watching. Creates the underlying watcher and processes events.
    pub async fn start(&self) -> Result<(), CoreError> {
        let watches = Arc::clone(&self.watches);
        let callback = Arc::clone(&self.callback);

        let (tx, mut rx) = mpsc::channel::<Event>(256);

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    if let Err(e) = tx.blocking_send(event) {
                        warn!("file watcher event channel full or closed: {}", e);
                    }
                }
                Err(e) => {
                    error!("file watcher error: {}", e);
                }
            },
            Config::default(),
        )
        .map_err(|e| CoreError::Scheduler(SchedulerError::WatcherError(e.to_string())))?;

        *self.watcher.write().await = Some(watcher);

        // Re-register existing watches
        let watches_snapshot = self.watches.read().await.clone();
        for (_task_id, paths) in &watches_snapshot {
            for path in paths {
                if let Some(ref mut w) = *self.watcher.write().await {
                    if let Err(e) = w.watch(path, RecursiveMode::NonRecursive) {
                        warn!(?path, "failed to watch folder: {}", e);
                    }
                }
            }
        }

        // Spawn event processing loop
        let watches_for_loop = Arc::clone(&watches);
        let cb = callback;
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    continue;
                }
                for path in &event.paths {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext != "torrent" {
                        continue;
                    }
                    // Find which task owns this folder
                    let watches_guard = watches_for_loop.read().await;
                    for (&task_id, folders) in watches_guard.iter() {
                        for folder in folders {
                            if path.starts_with(folder) {
                                info!(task_id, ?path, "new .torrent file detected");
                                cb(task_id, path.clone());
                                break;
                            }
                        }
                    }
                }
            }
        });

        info!("FileWatcher started");
        Ok(())
    }

    /// Add folder watches for a task.
    pub async fn watch_task(&self, task_id: i64, folders: Vec<PathBuf>) -> Result<(), CoreError> {
        // Register with the OS watcher
        if let Some(ref mut w) = *self.watcher.write().await {
            for path in &folders {
                w.watch(path, RecursiveMode::NonRecursive).map_err(|e| {
                    CoreError::Scheduler(SchedulerError::WatcherError(e.to_string()))
                })?;
            }
        }

        self.watches.write().await.insert(task_id, folders);
        info!(task_id, "file watch registered");
        Ok(())
    }

    /// Remove folder watches for a task.
    pub async fn unwatch_task(&self, task_id: i64) -> Result<(), CoreError> {
        let paths = self.watches.write().await.remove(&task_id);
        if let Some(paths) = paths {
            if let Some(ref mut w) = *self.watcher.write().await {
                for path in &paths {
                    let _ = w.unwatch(path);
                }
            }
            info!(task_id, "file watch removed");
        }
        Ok(())
    }
}
