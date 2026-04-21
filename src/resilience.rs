use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

/// Health status of the gateway
#[derive(Clone, Debug, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Circuit breaker state
#[derive(Clone, Debug)]
struct CircuitState {
    /// Number of consecutive failures
    failures: u64,
    /// When the circuit was last opened
    last_failure_time: Option<Instant>,
    /// Current state: false = closed (normal), true = open (blocking)
    is_open: bool,
}

impl CircuitState {
    fn new() -> Self {
        Self {
            failures: 0,
            last_failure_time: None,
            is_open: false,
        }
    }
}

/// Circuit breaker configuration
#[derive(Clone, Debug)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    pub failure_threshold: u64,
    /// Time to wait before attempting to recover (half-open)
    pub recovery_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
        }
    }
}

/// Circuit breaker for Fluss Coordinator connections
pub struct CircuitBreaker {
    state: RwLock<CircuitState>,
    config: CircuitBreakerConfig,
    /// Total requests that were blocked by open circuit
    pub blocked_count: AtomicU64,
    /// Total successful requests
    pub success_count: AtomicU64,
    /// Total failed requests
    pub failure_count: AtomicU64,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: RwLock::new(CircuitState::new()),
            config,
            blocked_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
        }
    }

    /// Check if a request is allowed to proceed
    pub async fn allow_request(&self) -> bool {
        let state = self.state.read().await;

        if !state.is_open {
            return true;
        }

        // Check if recovery timeout has elapsed
        if let Some(last_failure) = state.last_failure_time {
            if last_failure.elapsed() >= self.config.recovery_timeout {
                drop(state);
                // Transition to half-open state - allow one request
                let mut state = self.state.write().await;
                if state.is_open {
                    state.is_open = false; // half-open
                    return true;
                }
            }
        }

        self.blocked_count.fetch_add(1, Ordering::Relaxed);
        false
    }

    /// Record a successful operation
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;
        state.failures = 0;
        state.is_open = false;
        self.success_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed operation
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        state.failures += 1;
        state.last_failure_time = Some(Instant::now());
        self.failure_count.fetch_add(1, Ordering::Relaxed);

        if state.failures >= self.config.failure_threshold {
            state.is_open = true;
            tracing::warn!(
                failures = state.failures,
                "circuit breaker opened due to consecutive failures"
            );
        }
    }

    /// Get current health status
    pub async fn health(&self) -> HealthStatus {
        let state = self.state.read().await;

        if state.is_open {
            // Check if we're in recovery
            if let Some(last_failure) = state.last_failure_time {
                if last_failure.elapsed() >= self.config.recovery_timeout {
                    HealthStatus::Degraded // half-open
                } else {
                    HealthStatus::Unhealthy
                }
            } else {
                HealthStatus::Unhealthy
            }
        } else if state.failures > 0 {
            HealthStatus::Degraded // closed but has some failures
        } else {
            HealthStatus::Healthy
        }
    }
}

/// Retry configuration
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

/// Execute an operation with retry and circuit breaker logic
pub async fn execute_with_retry<T, E, F, Fut>(
    circuit_breaker: &CircuitBreaker,
    retry_config: &RetryConfig,
    mut operation: F,
) -> Result<T, E>
where
    E: std::error::Error + std::fmt::Debug,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    // Check circuit breaker first
    if !circuit_breaker.allow_request().await {
        tracing::warn!("request blocked by open circuit breaker");
        // Return a synthetic error to indicate circuit breaker rejection
        panic!("circuit breaker is open, request rejected");
    }

    let mut attempt = 0;
    let mut backoff = retry_config.initial_backoff;

    loop {
        match operation().await {
            Ok(result) => {
                circuit_breaker.record_success().await;
                return Ok(result);
            }
            Err(e) => {
                circuit_breaker.record_failure().await;
                attempt += 1;

                if attempt > retry_config.max_retries {
                    tracing::error!(
                        error = ?e,
                        attempt,
                        max_retries = retry_config.max_retries,
                        "operation failed after all retry attempts"
                    );
                    return Err(e);
                }

                tracing::warn!(
                    error = ?e,
                    attempt,
                    backoff_ms = backoff.as_millis(),
                    "operation failed, retrying with backoff"
                );

                tokio::time::sleep(backoff).await;

                // Exponential backoff with jitter
                backoff = std::cmp::min(
                    Duration::from_secs_f64(
                        backoff.as_secs_f64() * retry_config.backoff_multiplier,
                    ),
                    retry_config.max_backoff,
                );

                // Add small jitter (up to 10%)
                let jitter = backoff.as_millis() as u64 / 10;
                if jitter > 0 {
                    use std::time::Duration;
                    let jitter_ms = (rand::random::<u64>() % jitter) as u32;
                    backoff += Duration::from_millis(jitter_ms as u64);
                }
            }
        }
    }
}

/// Simple random function without external dependency
mod rand {
    use std::cell::Cell;

    thread_local! {
        static SEED: Cell<u64> = Cell::new(0x1234567890abcdef);
    }

    pub fn random<T>() -> T
    where
        T: From<u64>,
    {
        SEED.with(|seed| {
            let mut s = seed.get();
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            seed.set(s);
            T::from(s)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_healthy_initially() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(cb.health().await, HealthStatus::Healthy);
        assert!(cb.allow_request().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(1),
        });

        for _ in 0..3 {
            cb.record_failure().await;
        }

        assert_eq!(cb.health().await, HealthStatus::Unhealthy);
        assert!(!cb.allow_request().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovers() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            recovery_timeout: Duration::from_millis(100),
        });

        // Open the circuit
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.health().await, HealthStatus::Unhealthy);

        // Wait for recovery timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should be degraded (half-open)
        assert_eq!(cb.health().await, HealthStatus::Degraded);

        // Allow request and record success
        assert!(cb.allow_request().await);
        cb.record_success().await;

        // Should be healthy again
        assert_eq!(cb.health().await, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_retry_with_success() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        let config = RetryConfig::default();

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = execute_with_retry(&cb, &config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok::<_, std::io::Error>("success")
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
