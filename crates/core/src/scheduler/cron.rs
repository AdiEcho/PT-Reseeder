use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;

use crate::error::{CoreError, SchedulerError};

/// Callback type for cron-triggered task execution.
pub type CronCallback = Arc<dyn Fn(i64) + Send + Sync>;

/// Manages cron-based scheduling for tasks using tokio-cron-scheduler.
pub struct CronScheduler {
    scheduler: JobScheduler,
    /// Maps task_id -> job UUID for removal.
    jobs: Arc<RwLock<HashMap<i64, uuid::Uuid>>>,
    callback: CronCallback,
}

impl CronScheduler {
    /// Create a new CronScheduler. The callback is invoked with the task_id
    /// whenever a cron job fires.
    pub async fn new(callback: CronCallback) -> Result<Self, CoreError> {
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| CoreError::Scheduler(SchedulerError::InvalidCron(e.to_string())))?;

        Ok(Self {
            scheduler,
            jobs: Arc::new(RwLock::new(HashMap::new())),
            callback,
        })
    }

    /// Start the scheduler background loop.
    pub async fn start(&self) -> Result<(), CoreError> {
        self.scheduler
            .start()
            .await
            .map_err(|e| CoreError::Scheduler(SchedulerError::InvalidCron(e.to_string())))?;
        info!("CronScheduler started");
        Ok(())
    }

    /// Add a cron job for a task. If the task already has a job, it is replaced.
    pub async fn add_job(&self, task_id: i64, cron_expression: &str) -> Result<(), CoreError> {
        // Remove existing job if present
        self.remove_job(task_id).await?;

        let cb = Arc::clone(&self.callback);
        let tid = task_id;

        let job = Job::new_async(cron_expression, move |_uuid, _lock| {
            let cb = Arc::clone(&cb);
            Box::pin(async move {
                info!(task_id = tid, "cron trigger fired");
                cb(tid);
            })
        })
        .map_err(|e| CoreError::Scheduler(SchedulerError::InvalidCron(e.to_string())))?;

        let uuid = job.guid();
        self.scheduler
            .add(job)
            .await
            .map_err(|e| CoreError::Scheduler(SchedulerError::InvalidCron(e.to_string())))?;

        self.jobs.write().await.insert(task_id, uuid);
        info!(task_id, %cron_expression, "cron job added");
        Ok(())
    }

    /// Remove a cron job for a task.
    pub async fn remove_job(&self, task_id: i64) -> Result<(), CoreError> {
        let uuid = self.jobs.write().await.remove(&task_id);
        if let Some(uuid) = uuid {
            self.scheduler
                .remove(&uuid)
                .await
                .map_err(|e| CoreError::Scheduler(SchedulerError::InvalidCron(e.to_string())))?;
            info!(task_id, "cron job removed");
        }
        Ok(())
    }

    /// Shut down the scheduler.
    pub async fn shutdown(&mut self) -> Result<(), CoreError> {
        self.scheduler
            .shutdown()
            .await
            .map_err(|e| CoreError::Scheduler(SchedulerError::InvalidCron(e.to_string())))?;
        info!("CronScheduler shut down");
        Ok(())
    }
}
