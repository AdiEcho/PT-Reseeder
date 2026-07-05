use crate::error::{CoreError, SiteError};
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter as GovernorLimiter};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

type GovernorRL = GovernorLimiter<NotKeyed, InMemoryState, DefaultClock>;

pub struct SiteRateLimiter {
    limiter: GovernorRL,
    consecutive_errors: AtomicU32,
    current_interval_ms: AtomicU64,
    base_interval_ms: u64,
    circuit_open_until: Mutex<Option<Instant>>,
}

impl SiteRateLimiter {
    pub fn new(interval_ms: u64, burst: u32) -> Self {
        let burst = NonZeroU32::new(burst.max(1)).unwrap();
        let quota = Quota::with_period(Duration::from_millis(interval_ms))
            .expect("interval must be > 0")
            .allow_burst(burst);

        Self {
            limiter: GovernorLimiter::direct(quota),
            consecutive_errors: AtomicU32::new(0),
            current_interval_ms: AtomicU64::new(interval_ms),
            base_interval_ms: interval_ms,
            circuit_open_until: Mutex::new(None),
        }
    }

    /// Wait for rate limit permit. Returns error if circuit is open.
    pub async fn acquire(&self) -> Result<(), CoreError> {
        {
            let open_until = self.circuit_open_until.lock().await;
            if let Some(until) = *open_until {
                if Instant::now() < until {
                    return Err(SiteError::CircuitOpen.into());
                }
            }
        }

        self.limiter.until_ready().await;
        Ok(())
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

    /// Record a successful request
    pub fn record_success(&self) {
        self.consecutive_errors.store(0, Ordering::SeqCst);
        self.current_interval_ms
            .store(self.base_interval_ms, Ordering::SeqCst);
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
}
