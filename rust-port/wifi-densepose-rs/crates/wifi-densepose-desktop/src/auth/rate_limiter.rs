//! In-memory sliding window rate limiter.
//!
//! Protects login and registration endpoints against brute force attacks.
//! No external dependencies (no Redis needed for a desktop app).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Rate limiter configuration.
pub struct RateLimiterConfig {
    /// Maximum number of attempts in the window.
    pub max_attempts: u32,
    /// Window duration.
    pub window: Duration,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            window: Duration::from_secs(60),
        }
    }
}

/// Per-key rate limit bucket.
struct Bucket {
    attempts: Vec<Instant>,
}

impl Bucket {
    fn new() -> Self {
        Self {
            attempts: Vec::new(),
        }
    }

    /// Prune expired attempts and return the count of active ones.
    fn count_active(&mut self, window: Duration) -> u32 {
        let cutoff = Instant::now() - window;
        self.attempts.retain(|t| *t > cutoff);
        self.attempts.len() as u32
    }

    fn record(&mut self) {
        self.attempts.push(Instant::now());
    }
}

/// Thread-safe in-memory rate limiter.
pub struct RateLimiter {
    config: RateLimiterConfig,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    /// Create a rate limiter with the given config.
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            config,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Create a rate limiter with default settings (5 attempts per 60 seconds).
    pub fn default_login() -> Self {
        Self::new(RateLimiterConfig::default())
    }

    /// Create a stricter rate limiter (60 requests per minute per key).
    pub fn default_api() -> Self {
        Self::new(RateLimiterConfig {
            max_attempts: 60,
            window: Duration::from_secs(60),
        })
    }

    /// Check if the key is rate limited. Returns Ok(()) if allowed,
    /// or Err with remaining seconds until the window resets.
    pub fn check(&self, key: &str) -> Result<(), u64> {
        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets.entry(key.to_string()).or_insert_with(Bucket::new);
        let active = bucket.count_active(self.config.window);

        if active >= self.config.max_attempts {
            // Calculate time until the oldest attempt in the window expires
            let oldest = bucket.attempts.first().unwrap();
            let reset_at = *oldest + self.config.window;
            let remaining = reset_at.duration_since(Instant::now()).as_secs();
            Err(remaining.max(1))
        } else {
            bucket.record();
            Ok(())
        }
    }

    /// Clear rate limit state for a key (e.g., after successful login).
    pub fn clear(&self, key: &str) {
        let mut buckets = self.buckets.lock().unwrap();
        buckets.remove(key);
    }

    /// Periodically clean up stale buckets to prevent memory leaks.
    /// Call from a background task every few minutes.
    pub fn cleanup(&self) {
        let mut buckets = self.buckets.lock().unwrap();
        buckets.retain(|_, bucket| {
            bucket.count_active(self.config.window) > 0
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_under_limit() {
        let rl = RateLimiter::new(RateLimiterConfig {
            max_attempts: 3,
            window: Duration::from_secs(60),
        });
        assert!(rl.check("test-key").is_ok());
        assert!(rl.check("test-key").is_ok());
        assert!(rl.check("test-key").is_ok());
    }

    #[test]
    fn test_blocks_over_limit() {
        let rl = RateLimiter::new(RateLimiterConfig {
            max_attempts: 2,
            window: Duration::from_secs(60),
        });
        assert!(rl.check("test-key").is_ok());
        assert!(rl.check("test-key").is_ok());
        assert!(rl.check("test-key").is_err());
    }

    #[test]
    fn test_different_keys_independent() {
        let rl = RateLimiter::new(RateLimiterConfig {
            max_attempts: 1,
            window: Duration::from_secs(60),
        });
        assert!(rl.check("key-a").is_ok());
        assert!(rl.check("key-b").is_ok());
        assert!(rl.check("key-a").is_err());
        assert!(rl.check("key-b").is_err());
    }

    #[test]
    fn test_clear_resets() {
        let rl = RateLimiter::new(RateLimiterConfig {
            max_attempts: 1,
            window: Duration::from_secs(60),
        });
        assert!(rl.check("key").is_ok());
        assert!(rl.check("key").is_err());
        rl.clear("key");
        assert!(rl.check("key").is_ok());
    }
}
