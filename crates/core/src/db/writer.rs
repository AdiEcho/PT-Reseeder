use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, SqliteConnection};
use std::str::FromStr;
use tokio::sync::{mpsc, oneshot};
use tracing;

use crate::error::{CoreError, DbError};

pub enum WriteOp {
    InsertReseedHistory {
        pieces_hash: String,
        site_id: i64,
        torrent_id: Option<i64>,
        info_hash: Option<String>,
        status: String,
        error_reason: Option<String>,
    },
    UpsertPiecesCache {
        pieces_hash: String,
        info_hash: String,
        torrent_name: Option<String>,
        file_path: Option<String>,
        total_size: Option<i64>,
        announce_url: Option<String>,
    },
    InsertTaskLog {
        task_id: i64,
        status: String,
        matched_count: i64,
        succeeded_count: i64,
        failed_count: i64,
        duration_ms: Option<i64>,
        log_text: Option<String>,
    },
    InsertUserStats {
        site_id: i64,
        uploaded: Option<i64>,
        downloaded: Option<i64>,
        ratio: Option<f64>,
        bonus: Option<f64>,
        user_class: Option<String>,
        seeding_count: Option<i64>,
        leeching_count: Option<i64>,
        seeding_size: Option<i64>,
        upload_time_seconds: Option<i64>,
    },
    BulkUpsertPiecesCache(Vec<BulkPiecesCacheItem>),
    Flush(oneshot::Sender<()>),
}

pub struct BulkPiecesCacheItem {
    pub pieces_hash: String,
    pub info_hash: String,
    pub torrent_name: Option<String>,
    pub file_path: Option<String>,
    pub total_size: Option<i64>,
    pub announce_url: Option<String>,
}

#[derive(Clone)]
pub struct DbWriterHandle {
    tx: mpsc::Sender<WriteOp>,
}

impl DbWriterHandle {
    pub async fn send(&self, op: WriteOp) -> Result<(), CoreError> {
        self.tx
            .send(op)
            .await
            .map_err(|_| CoreError::Db(DbError::WriterChannelClosed))
    }

    pub async fn flush(&self) -> Result<(), CoreError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(WriteOp::Flush(tx))
            .await
            .map_err(|_| CoreError::Db(DbError::WriterChannelClosed))?;
        rx.await
            .map_err(|_| CoreError::Db(DbError::WriterChannelClosed))
    }
}

struct DbWriter;

impl DbWriter {
    fn spawn(database_url: &str, batch_size: usize) -> Result<DbWriterHandle, CoreError> {
        let (tx, rx) = mpsc::channel::<WriteOp>(1024);
        let url = database_url.to_string();
        let bs = batch_size;

        tokio::spawn(async move {
            match Self::run_loop(&url, rx, bs).await {
                Ok(()) => tracing::info!("DbWriter shut down gracefully"),
                Err(e) => tracing::error!("DbWriter exited with error: {}", e),
            }
        });

        Ok(DbWriterHandle { tx })
    }

    async fn run_loop(
        database_url: &str,
        mut rx: mpsc::Receiver<WriteOp>,
        batch_size: usize,
    ) -> Result<(), CoreError> {
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(DbError::Sqlx)?
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_millis(5000))
            .foreign_keys(true);

        let mut conn = SqliteConnection::connect_with(&options)
            .await
            .map_err(DbError::Sqlx)?;

        let mut buffer: Vec<WriteOp> = Vec::with_capacity(batch_size);

        loop {
            // Wait for the first op
            match rx.recv().await {
                None => {
                    // Channel closed, flush remaining and exit
                    if !buffer.is_empty() {
                        Self::flush_buffer(&mut conn, &mut buffer).await?;
                    }
                    return Ok(());
                }
                Some(op) => {
                    if let WriteOp::Flush(responder) = op {
                        Self::flush_buffer(&mut conn, &mut buffer).await?;
                        let _ = responder.send(());
                        continue;
                    }
                    buffer.push(op);
                }
            }

            // Try to collect more ops without blocking up to batch_size
            while buffer.len() < batch_size {
                match rx.try_recv() {
                    Ok(op) => {
                        if let WriteOp::Flush(responder) = op {
                            Self::flush_buffer(&mut conn, &mut buffer).await?;
                            let _ = responder.send(());
                            continue;
                        }
                        buffer.push(op);
                    }
                    Err(_) => break,
                }
            }

            // Flush the batch
            if !buffer.is_empty() {
                Self::flush_buffer(&mut conn, &mut buffer).await?;
            }
        }
    }

    async fn flush_buffer(
        conn: &mut SqliteConnection,
        buffer: &mut Vec<WriteOp>,
    ) -> Result<(), CoreError> {
        if buffer.is_empty() {
            return Ok(());
        }

        let ops: Vec<WriteOp> = buffer.drain(..).collect();

        // Run all ops in a single transaction
        let mut tx = conn.begin().await.map_err(DbError::Sqlx)?;

        for op in ops {
            match op {
                WriteOp::InsertReseedHistory {
                    pieces_hash,
                    site_id,
                    torrent_id,
                    info_hash,
                    status,
                    error_reason,
                } => {
                    sqlx::query(
                        "INSERT INTO reseed_history \
                         (pieces_hash, site_id, torrent_id, info_hash, status, error_reason) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&pieces_hash)
                    .bind(site_id)
                    .bind(torrent_id)
                    .bind(&info_hash)
                    .bind(&status)
                    .bind(&error_reason)
                    .execute(&mut *tx)
                    .await
                    .map_err(DbError::Sqlx)?;
                }
                WriteOp::UpsertPiecesCache {
                    pieces_hash,
                    info_hash,
                    torrent_name,
                    file_path,
                    total_size,
                    announce_url,
                } => {
                    sqlx::query(
                        "INSERT INTO pieces_cache \
                         (pieces_hash, info_hash, torrent_name, file_path, total_size, announce_url) \
                         VALUES (?, ?, ?, ?, ?, ?) \
                         ON CONFLICT(info_hash) DO UPDATE SET \
                         pieces_hash = excluded.pieces_hash, \
                         torrent_name = excluded.torrent_name, \
                         file_path = excluded.file_path, \
                         total_size = excluded.total_size, \
                         announce_url = excluded.announce_url, \
                         cached_at = datetime('now')",
                    )
                    .bind(&pieces_hash)
                    .bind(&info_hash)
                    .bind(&torrent_name)
                    .bind(&file_path)
                    .bind(total_size)
                    .bind(&announce_url)
                    .execute(&mut *tx)
                    .await
                    .map_err(DbError::Sqlx)?;
                }
                WriteOp::InsertTaskLog {
                    task_id,
                    status,
                    matched_count,
                    succeeded_count,
                    failed_count,
                    duration_ms,
                    log_text,
                } => {
                    sqlx::query(
                        "INSERT INTO task_logs \
                         (task_id, status, matched_count, succeeded_count, failed_count, \
                          duration_ms, log_text) \
                         VALUES (?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(task_id)
                    .bind(&status)
                    .bind(matched_count)
                    .bind(succeeded_count)
                    .bind(failed_count)
                    .bind(duration_ms)
                    .bind(&log_text)
                    .execute(&mut *tx)
                    .await
                    .map_err(DbError::Sqlx)?;
                }
                WriteOp::InsertUserStats {
                    site_id,
                    uploaded,
                    downloaded,
                    ratio,
                    bonus,
                    user_class,
                    seeding_count,
                    leeching_count,
                    seeding_size,
                    upload_time_seconds,
                } => {
                    sqlx::query(
                        "INSERT INTO user_stats \
                         (site_id, uploaded, downloaded, ratio, bonus, user_class, \
                          seeding_count, leeching_count, seeding_size, upload_time_seconds) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(site_id)
                    .bind(uploaded)
                    .bind(downloaded)
                    .bind(ratio)
                    .bind(bonus)
                    .bind(&user_class)
                    .bind(seeding_count)
                    .bind(leeching_count)
                    .bind(seeding_size)
                    .bind(upload_time_seconds)
                    .execute(&mut *tx)
                    .await
                    .map_err(DbError::Sqlx)?;
                }
                WriteOp::BulkUpsertPiecesCache(items) => {
                    for item in items {
                        sqlx::query(
                            "INSERT INTO pieces_cache \
                             (pieces_hash, info_hash, torrent_name, file_path, total_size, announce_url) \
                             VALUES (?, ?, ?, ?, ?, ?) \
                             ON CONFLICT(info_hash) DO UPDATE SET \
                             pieces_hash = excluded.pieces_hash, \
                             torrent_name = excluded.torrent_name, \
                             file_path = excluded.file_path, \
                             total_size = excluded.total_size, \
                             announce_url = excluded.announce_url, \
                             cached_at = datetime('now')",
                        )
                        .bind(&item.pieces_hash)
                        .bind(&item.info_hash)
                        .bind(&item.torrent_name)
                        .bind(&item.file_path)
                        .bind(item.total_size)
                        .bind(&item.announce_url)
                        .execute(&mut *tx)
                        .await
                        .map_err(DbError::Sqlx)?;
                    }
                }
                WriteOp::Flush(_) => {
                    // Should not reach here; handled before buffering
                }
            }
        }

        tx.commit().await.map_err(DbError::Sqlx)?;
        Ok(())
    }
}

/// Spawn a DbWriter background task and return its handle.
pub fn spawn_writer(database_url: &str, batch_size: usize) -> Result<DbWriterHandle, CoreError> {
    DbWriter::spawn(database_url, batch_size)
}
