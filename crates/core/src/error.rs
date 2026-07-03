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
