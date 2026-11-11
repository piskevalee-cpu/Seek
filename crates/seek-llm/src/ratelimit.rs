//! The rate limits (SPEC 6).
//!
//! SEEK honors the `429` answers and the rate limit headers of the providers, and it applies the
//! optional `rpm` and `tpm` of the configuration on its own.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use seek_core::provider::RateLimit;

/// A client side limit: a number of requests or tokens per minute.
#[derive(Debug, Clone, Default)]
pub struct Limit {
    /// The requests per minute.
    pub rpm: Option<u32>,
    /// The tokens per minute.
    pub tpm: Option<u32>,
}

impl Limit {
    /// A limit without anything.
    pub fn none() -> Self {
        Limit::default()
    }

    /// Whether the limit says anything.
    pub fn is_any(&self) -> bool {
        self.rpm.is_some() || self.tpm.is_some()
    }
}

/// The client side limiter.
#[derive(Debug)]
pub struct Limiter {
    limit: Limit,
    requests: VecDeque<Instant>,
    tokens: u32,
    last_reset: Instant,
}

impl Default for Limiter {
    fn default() -> Self {
        Limiter::new(Limit::none())
    }
}

impl Limiter {
    /// A limiter for a limit.
    pub fn new(limit: Limit) -> Self {
        Limiter {
            limit,
            requests: VecDeque::new(),
            tokens: 0,
            last_reset: Instant::now(),
        }
    }

    /// How long the caller has to wait, if it has to wait at all.
    pub fn wait_time(&mut self, tokens: u32) -> Duration {
        let now = Instant::now();
        if now.duration_since(self.last_reset) >= Duration::from_secs(60) {
            // A new minute starts.
            self.requests.clear();
            self.tokens = 0;
            self.last_reset = now;
        }
        while self
            .requests
            .front()
            .is_some_and(|when| now.duration_since(*when) >= Duration::from_secs(60))
        {
            self.requests.pop_front();
        }

        let mut wait = Duration::ZERO;

        if let Some(rpm) = self.limit.rpm
            && self.requests.len() as u32 >= rpm
        {
            // The oldest request leaves the window in this long.
            let oldest = self.requests.front().copied().unwrap_or(now);
            let until = Duration::from_secs(60).saturating_sub(now.duration_since(oldest));
            wait = wait.max(until);
        }

        if let Some(tpm) = self.limit.tpm
            && self.tokens + tokens > tpm
        {
            let until = Duration::from_secs(60).saturating_sub(now.duration_since(self.last_reset));
            wait = wait.max(until);
        }

        wait
    }

    /// Record that a request with that many tokens went out.
    pub fn record(&mut self, tokens: u32) {
        self.requests.push_back(Instant::now());
        self.tokens = self.tokens.saturating_add(tokens);
    }
}

/// Read the rate limit out of the headers of a response.
///
/// The headers differ between the providers, so every known spelling is tried.
pub fn from_headers(headers: &reqwest::header::HeaderMap) -> RateLimit {
    let number = |name: &str| -> Option<u32> {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u32>().ok())
    };
    let duration = |name: &str| -> Option<Duration> {
        let value = headers.get(name).and_then(|value| value.to_str().ok())?;
        parse_duration(value)
    };

    let limit = number("x-ratelimit-limit-requests").or_else(|| number("x-ratelimit-limit-tokens"));
    let remaining =
        number("x-ratelimit-remaining-requests").or_else(|| number("x-ratelimit-remaining-tokens"));
    let resets_in = duration("x-ratelimit-reset-requests")
        .or_else(|| duration("x-ratelimit-reset-tokens"))
        .or_else(|| duration("retry-after"));

    RateLimit {
        limit,
        remaining,
        resets_in,
    }
}

/// Parse a duration like `6ms`, `1s`, `2m` or a number of seconds.
pub fn parse_duration(value: &str) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    // The unit is one or two letters: "ms", "s", "m" or "h".
    let unit = if value.ends_with("ms") {
        "ms"
    } else if value.ends_with('s') {
        "s"
    } else if value.ends_with('m') {
        "m"
    } else if value.ends_with('h') {
        "h"
    } else {
        return None;
    };
    let number: f64 = value[..value.len() - unit.len()].parse().ok()?;
    let seconds = match unit {
        "ms" => number / 1000.0,
        "s" => number,
        "m" => number * 60.0,
        "h" => number * 3600.0,
        _ => return None,
    };
    Some(Duration::from_secs_f64(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_duration_is_parsed() {
        assert_eq!(parse_duration("6ms"), Some(Duration::from_millis(6)));
        assert_eq!(parse_duration("2s"), Some(Duration::from_secs(2)));
        assert_eq!(parse_duration("1m"), Some(Duration::from_secs(60)));
        assert_eq!(parse_duration("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("nonsense"), None);
    }

    #[test]
    fn the_limit_is_read_from_the_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "x-ratelimit-limit-requests",
            "50".parse().expect("a header"),
        );
        headers.insert(
            "x-ratelimit-remaining-requests",
            "49".parse().expect("a header"),
        );
        headers.insert(
            "x-ratelimit-reset-requests",
            "1s".parse().expect("a header"),
        );

        let limit = from_headers(&headers);
        assert_eq!(limit.limit, Some(50));
        assert_eq!(limit.remaining, Some(49));
        assert_eq!(limit.resets_in, Some(Duration::from_secs(1)));
        assert!(limit.is_reported());
        assert!(limit.text().contains("49/50"));
    }

    #[test]
    fn a_provider_without_headers_reports_nothing() {
        let headers = reqwest::header::HeaderMap::new();
        let limit = from_headers(&headers);
        assert!(!limit.is_reported());
    }

    #[test]
    fn the_limiter_waits_when_the_limit_is_reached() {
        let mut limiter = Limiter::new(Limit {
            rpm: Some(2),
            tpm: None,
        });
        assert_eq!(limiter.wait_time(10), Duration::ZERO);
        limiter.record(10);
        assert_eq!(limiter.wait_time(10), Duration::ZERO);
        limiter.record(10);
        assert!(
            limiter.wait_time(10) > Duration::ZERO,
            "the third one waits"
        );
    }

    #[test]
    fn a_limiter_without_a_limit_never_waits() {
        let mut limiter = Limiter::new(Limit::none());
        assert_eq!(limiter.wait_time(1000), Duration::ZERO);
    }
}
