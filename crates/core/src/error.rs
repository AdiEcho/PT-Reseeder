use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("site error: {0}")]
    Site(#[from] SiteError),
    #[error("downloader error: {0}")]
    Downloader(#[from] DownloaderError),
    #[error("engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("repost error: {0}")]
    Repost(#[from] RepostError),
    #[error("scheduler error: {0}")]
    Scheduler(#[from] SchedulerError),
    #[error("config error: {0}")]
    Config(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("KDF derivation failed: {0}")]
    KdfFailed(String),
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("invalid key")]
    InvalidKey,
    #[error("vault is locked/zeroized")]
    Zeroized,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration failed: {0}")]
    MigrationFailed(String),
    #[error("writer channel closed")]
    WriterChannelClosed,
}

impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        DbError::MigrationFailed(err.to_string())
    }
}

#[derive(Debug, Error)]
pub enum SiteError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("rate limited")]
    RateLimited,
    #[error("circuit breaker open")]
    CircuitOpen,
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Error)]
pub enum DownloaderError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("torrent not found: {0}")]
    TorrentNotFound(String),
    #[error("add torrent failed: {0}")]
    AddFailed(String),
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("scan failed: {0}")]
    ScanFailed(String),
    #[error("match failed: {0}")]
    MatchFailed(String),
    #[error("add failed: {0}")]
    AddFailed(String),
    #[error("operation cancelled")]
    Cancelled,
}

#[derive(Debug, Error)]
pub enum RepostError {
    #[error("extraction failed: {0}")]
    ExtractionFailed(String),
    #[error("adaptation failed: {0}")]
    AdaptationFailed(String),
    #[error("submission failed: {0}")]
    SubmissionFailed(String),
    #[error("invalid state transition: {0}")]
    InvalidState(String),
    #[error("entry not found: {0}")]
    NotFound(String),
    #[error("site not capable: {0}")]
    SiteNotCapable(String),
}

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("task not found: {0}")]
    TaskNotFound(i64),
    #[error("task already running: {0}")]
    TaskAlreadyRunning(i64),
    #[error("invalid cron expression: {0}")]
    InvalidCron(String),
    #[error("watcher error: {0}")]
    WatcherError(String),
    #[error("executor error: {0}")]
    ExecutorError(String),
    #[error("folder not found: {0}")]
    FolderNotFound(i64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_error_display_includes_inner_message() {
        let err = CoreError::Config("test".into());
        let display = format!("{}", err);
        assert!(
            display.contains("test"),
            "expected display to contain 'test', got: {}",
            display
        );
    }

    #[test]
    fn crypto_error_variants_display() {
        let kdf = CryptoError::KdfFailed("bad salt".into());
        assert!(format!("{}", kdf).contains("bad salt"));

        let enc = CryptoError::EncryptionFailed("key too short".into());
        assert!(format!("{}", enc).contains("key too short"));

        let dec = CryptoError::DecryptionFailed("corrupt data".into());
        assert!(format!("{}", dec).contains("corrupt data"));

        let inv = CryptoError::InvalidKey;
        assert!(format!("{}", inv).contains("invalid key"));

        let zero = CryptoError::Zeroized;
        assert!(format!("{}", zero).contains("zeroized"));
    }

    #[test]
    fn engine_error_variants_display() {
        let scan = EngineError::ScanFailed("no folder".into());
        assert!(format!("{}", scan).contains("no folder"));

        let mat = EngineError::MatchFailed("timeout".into());
        assert!(format!("{}", mat).contains("timeout"));

        let add = EngineError::AddFailed("disk full".into());
        assert!(format!("{}", add).contains("disk full"));

        let cancel = EngineError::Cancelled;
        assert!(format!("{}", cancel).contains("cancelled"));
    }

    #[test]
    fn scheduler_error_task_not_found_includes_id() {
        let err = SchedulerError::TaskNotFound(42);
        let display = format!("{}", err);
        assert!(
            display.contains("42"),
            "expected display to contain '42', got: {}",
            display
        );
    }

    #[test]
    fn repost_error_variants_display() {
        let ext = RepostError::ExtractionFailed("parse error".into());
        assert!(format!("{}", ext).contains("parse error"));

        let inv = RepostError::InvalidState("wrong phase".into());
        assert!(format!("{}", inv).contains("wrong phase"));

        let nf = RepostError::NotFound("id=99".into());
        assert!(format!("{}", nf).contains("id=99"));
    }

    #[test]
    fn core_error_from_engine_error() {
        let engine_err = EngineError::Cancelled;
        let core_err: CoreError = engine_err.into();
        assert!(
            matches!(core_err, CoreError::Engine(_)),
            "expected CoreError::Engine, got: {:?}",
            core_err
        );
    }

    #[test]
    fn core_error_from_crypto_error() {
        let crypto_err = CryptoError::InvalidKey;
        let core_err: CoreError = crypto_err.into();
        assert!(
            matches!(core_err, CoreError::Crypto(_)),
            "expected CoreError::Crypto, got: {:?}",
            core_err
        );
    }
}
