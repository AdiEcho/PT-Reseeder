use crate::error::{CoreError, SiteError};
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter as GovernorLimiter};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

type GovernorRL = GovernorLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Caps an extra wait requested by a site ("请等待 N 秒") so a bad payload
/// cannot stall the add phase indefinitely. Matches IYUUPlus SLEEP_MAX_VALUE.
pub const MAX_DOWNLOAD_WAIT_SECS: u64 = 30;

pub struct SiteRateLimiter {
    limiter: GovernorRL,
    download_limiter: GovernorRL,
    consecutive_errors: AtomicU32,
    current_interval_ms: AtomicU64,
    base_interval_ms: u64,
    extra_download_wait_ms: AtomicU64,
    circuit_open_until: Mutex<Option<Instant>>,
}

impl SiteRateLimiter {
    pub fn new(interval_ms: u64, burst: u32) -> Self {
        Self::with_download_interval(interval_ms, burst, interval_ms)
    }

    pub fn with_download_interval(interval_ms: u64, burst: u32, download_interval_ms: u64) -> Self {
        let interval_ms = interval_ms.max(1);
        let download_interval_ms = download_interval_ms.max(1);
        let burst = NonZeroU32::new(burst.max(1)).unwrap();
        let quota = Quota::with_period(Duration::from_millis(interval_ms))
            .expect("interval must be > 0")
            .allow_burst(burst);
        let download_quota = Quota::with_period(Duration::from_millis(download_interval_ms))
            .expect("download interval must be > 0")
            .allow_burst(NonZeroU32::new(1).unwrap());

        Self {
            limiter: GovernorLimiter::direct(quota),
            download_limiter: GovernorLimiter::direct(download_quota),
            consecutive_errors: AtomicU32::new(0),
            current_interval_ms: AtomicU64::new(interval_ms),
            base_interval_ms: interval_ms,
            extra_download_wait_ms: AtomicU64::new(0),
            circuit_open_until: Mutex::new(None),
        }
    }

    /// Wait for an API request permit. Returns error if circuit is open.
    pub async fn acquire(&self) -> Result<(), CoreError> {
        self.ensure_circuit_closed().await?;
        self.limiter.until_ready().await;
        self.sleep_error_backoff().await;
        Ok(())
    }

    /// Wait for a torrent-download permit. Uses `download_interval_ms` and any
    /// extra wait requested by the last "请等待 N 秒" response.
    pub async fn acquire_download(&self) -> Result<(), CoreError> {
        self.ensure_circuit_closed().await?;
        self.download_limiter.until_ready().await;
        self.sleep_error_backoff().await;
        let extra = self.extra_download_wait_ms.swap(0, Ordering::SeqCst);
        if extra > 0 {
            tokio::time::sleep(Duration::from_millis(extra)).await;
        }
        Ok(())
    }

    /// Record an extra wait requested by a site ("请等待 N 秒").
    /// The next `acquire_download` consumes it.
    pub fn record_extra_wait(&self, seconds: u64) {
        let extra = seconds.clamp(1, MAX_DOWNLOAD_WAIT_SECS) * 1000;
        self.extra_download_wait_ms.store(extra, Ordering::SeqCst);
    }

    /// Record an error (429/403/etc). Returns whether circuit breaker tripped.
    pub async fn record_error(&self) -> bool {
        let errors = self.consecutive_errors.fetch_add(1, Ordering::SeqCst) + 1;

        // Double the interval, cap at 60s
        let current = self.current_interval_ms.load(Ordering::SeqCst);
        let new_interval = (current * 2).min(60_000);
        self.current_interval_ms
            .store(new_interval, Ordering::SeqCst);

        // Circuit breaker: 5 consecutive errors -> disabled for 30 minutes
        if errors >= 5 {
            let mut open_until = self.circuit_open_until.lock().await;
            *open_until = Some(Instant::now() + Duration::from_secs(30 * 60));
            return true;
        }
        false
    }

    /// Record a successful request.
    pub fn record_success(&self) {
        self.consecutive_errors.store(0, Ordering::SeqCst);
        self.current_interval_ms
            .store(self.base_interval_ms, Ordering::SeqCst);
        self.extra_download_wait_ms.store(0, Ordering::SeqCst);
    }

    /// Check if circuit breaker is currently open
    pub async fn is_circuit_open(&self) -> bool {
        let open_until = self.circuit_open_until.lock().await;
        if let Some(until) = *open_until {
            Instant::now() < until
        } else {
            false
        }
    }

    pub fn consecutive_errors(&self) -> u32 {
        self.consecutive_errors.load(Ordering::SeqCst)
    }

    pub fn current_interval_ms(&self) -> u64 {
        self.current_interval_ms.load(Ordering::SeqCst)
    }

    async fn ensure_circuit_closed(&self) -> Result<(), CoreError> {
        let open_until = self.circuit_open_until.lock().await;
        if let Some(until) = *open_until {
            if Instant::now() < until {
                return Err(SiteError::CircuitOpen.into());
            }
        }
        Ok(())
    }

    async fn sleep_error_backoff(&self) {
        let current = self.current_interval_ms.load(Ordering::SeqCst);
        if current > self.base_interval_ms {
            tokio::time::sleep(Duration::from_millis(current - self.base_interval_ms)).await;
        }
    }
}

/// Parse a site "please wait N seconds" message.
///
/// Accepts Chinese (`请等待 4 秒`) and English (`wait 4 seconds` / `Retry-After`).
pub fn parse_wait_seconds(body: &str) -> Option<u64> {
    parse_wait_seconds_after(body, "请等待")
        .or_else(|| parse_wait_seconds_after(body, "等待"))
        .or_else(|| parse_wait_seconds_after_ascii(body))
}

fn parse_wait_seconds_after(body: &str, marker: &str) -> Option<u64> {
    let rest = body.split_once(marker)?.1;
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let secs = digits.parse::<u64>().ok()?;
    (secs > 0).then_some(secs.clamp(1, MAX_DOWNLOAD_WAIT_SECS))
}

fn parse_wait_seconds_after_ascii(body: &str) -> Option<u64> {
    let lower = body.to_ascii_lowercase();
    for marker in ["please wait", "wait ", "retry after", "retry-after"] {
        if let Some(secs) = parse_wait_seconds_after(&lower, marker) {
            return Some(secs);
        }
    }
    None
}

/// True when the response body is a rate-limit / too-frequent message rather
/// than a real server failure. NexusPHP sites often wrap this in HTTP 500.
pub fn is_download_rate_limited(status: reqwest::StatusCode, body: &str) -> bool {
    if status.as_u16() == 429 {
        return true;
    }
    let lower = body.to_ascii_lowercase();
    body.contains("过于频繁")
        || body.contains("下载过于频繁")
        || body.contains("访问过于频繁")
        || body.contains("请等待")
        || lower.contains("too frequent")
        || lower.contains("too many requests")
        || lower.contains("rate limit")
        || lower.contains("please wait")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn shared_handle_enforces_interval_budget() {
        let limiter = Arc::new(SiteRateLimiter::new(50, 1));
        limiter.acquire().await.expect("first permit");
        let start = Instant::now();

        let first = Arc::clone(&limiter);
        let second = Arc::clone(&limiter);
        let (a, b) = tokio::join!(first.acquire(), second.acquire());
        a.expect("second permit");
        b.expect("third permit");

        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[tokio::test]
    async fn download_limiter_is_independent_of_api_limiter() {
        let limiter = SiteRateLimiter::with_download_interval(10, 100, 40);
        limiter.acquire().await.expect("api permit");
        limiter.acquire().await.expect("api still open");
        limiter.acquire_download().await.expect("first download");
        let start = Instant::now();
        limiter.acquire_download().await.expect("second download");
        assert!(start.elapsed() >= Duration::from_millis(35));
    }

    #[tokio::test]
    async fn extra_wait_is_consumed_by_next_download() {
        let limiter = SiteRateLimiter::with_download_interval(1, 100, 1);
        limiter.record_extra_wait(1);
        let start = Instant::now();
        limiter
            .acquire_download()
            .await
            .expect("download after wait");
        assert!(start.elapsed() >= Duration::from_millis(900));
        let start = Instant::now();
        limiter.acquire_download().await.expect("no leftover wait");
        assert!(start.elapsed() < Duration::from_millis(400));
    }

    #[tokio::test]
    async fn extra_wait_is_capped() {
        let limiter = SiteRateLimiter::new(10, 1);
        limiter.record_extra_wait(999);
        assert_eq!(
            limiter.extra_download_wait_ms.load(Ordering::SeqCst),
            MAX_DOWNLOAD_WAIT_SECS * 1000
        );
    }

    #[tokio::test]
    async fn errors_backoff_and_trip_circuit() {
        let limiter = SiteRateLimiter::new(10, 1);
        assert!(!limiter.record_error().await);
        assert_eq!(limiter.current_interval_ms(), 20);
        assert!(!limiter.record_error().await);
        assert_eq!(limiter.current_interval_ms(), 40);
        assert!(!limiter.record_error().await);
        assert!(!limiter.record_error().await);
        assert!(limiter.record_error().await);
        assert!(limiter.is_circuit_open().await);
    }

    #[tokio::test]
    async fn success_resets_backoff() {
        let limiter = SiteRateLimiter::new(10, 1);
        let _ = limiter.record_error().await;
        limiter.record_extra_wait(4);
        assert!(limiter.current_interval_ms() > 10);
        limiter.record_success();
        assert_eq!(limiter.current_interval_ms(), 10);
        assert_eq!(limiter.consecutive_errors(), 0);
        assert_eq!(limiter.extra_download_wait_ms.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn parse_wait_seconds_from_hdtime_message() {
        assert_eq!(
            parse_wait_seconds("下载过于频繁，请等待 4 秒后再下载。"),
            Some(4)
        );
        assert_eq!(parse_wait_seconds("请等待4秒"), Some(4));
        assert_eq!(parse_wait_seconds("please wait 8 seconds"), Some(8));
        assert_eq!(parse_wait_seconds("Retry-After: 12"), Some(12));
        assert_eq!(parse_wait_seconds("ok"), None);
        assert_eq!(
            parse_wait_seconds("请等待 99 秒"),
            Some(MAX_DOWNLOAD_WAIT_SECS)
        );
    }

    #[test]
    fn detects_rate_limited_http_500_body() {
        let status = reqwest::StatusCode::INTERNAL_SERVER_ERROR;
        assert!(is_download_rate_limited(
            status,
            "下载过于频繁，请等待 4 秒后再下载。"
        ));
        assert!(is_download_rate_limited(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "whatever"
        ));
        assert!(!is_download_rate_limited(status, "internal error"));
    }
}
