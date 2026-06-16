use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;

use crate::{app::AppResult, config::RateLimitConfig, util};

#[derive(Clone, Default)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    window_start: i64,
    count: u32,
}

impl RateLimiter {
    pub async fn check(
        &self,
        action: &str,
        identity: &str,
        config: Option<&RateLimitConfig>,
    ) -> AppResult<()> {
        let Some(config) = config.filter(|config| config.enabled) else {
            return Ok(());
        };
        if config.requests == 0 || config.window_seconds == 0 {
            return Err(crate::app::AppError::TooManyRequests);
        }

        let now = util::now_ts();
        let key = format!("{action}:{identity}");
        let mut buckets = self.buckets.lock().await;
        let bucket = buckets.entry(key).or_insert(Bucket {
            window_start: now,
            count: 0,
        });

        if now.saturating_sub(bucket.window_start) >= config.window_seconds as i64 {
            bucket.window_start = now;
            bucket.count = 0;
        }

        if bucket.count >= config.requests {
            return Err(crate::app::AppError::TooManyRequests);
        }

        bucket.count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enforces_enabled_limit() {
        let limiter = RateLimiter::default();
        let config = RateLimitConfig {
            requests: 1,
            window_seconds: 60,
            enabled: true,
        };
        assert!(limiter.check("upload", "ip", Some(&config)).await.is_ok());
        assert!(limiter.check("upload", "ip", Some(&config)).await.is_err());
        assert!(
            limiter
                .check("upload", "other", Some(&config))
                .await
                .is_ok()
        );
    }
}
