//! Daemon lifecycle state (issue 0004): a sliding idle TTL.
//!
//! Activity (tensor ops) resets the idle clock; observing or configuring the
//! daemon does not. The deadline is always `last_activity + ttl`;
//! `ttl = None` means run forever.

use std::time::{Duration, Instant};

pub struct Lifecycle {
    started: Instant,
    last_activity: Instant,
    ttl: Option<Duration>,
}

impl Lifecycle {
    pub fn new(ttl: Option<Duration>) -> Self {
        let now = Instant::now();
        Lifecycle {
            started: now,
            last_activity: now,
            ttl,
        }
    }

    /// A tensor op happened: reset the idle clock.
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn set_ttl(&mut self, ttl: Option<Duration>) {
        self.ttl = ttl;
    }

    pub fn ttl_secs(&self) -> Option<u64> {
        self.ttl.map(|d| d.as_secs())
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    pub fn idle_secs(&self) -> u64 {
        self.last_activity.elapsed().as_secs()
    }

    /// Seconds until expiry (saturating), or None when ttl is `none`.
    pub fn remaining_secs(&self) -> Option<u64> {
        self.ttl
            .map(|ttl| ttl.saturating_sub(self.last_activity.elapsed()).as_secs())
    }

    pub fn expired(&self) -> bool {
        match self.ttl {
            Some(ttl) => self.last_activity.elapsed() > ttl,
            None => false,
        }
    }
}

/// Parse a TTL: `<n>s|m|h` (e.g. `90s`, `30m`, `2h`), bare integer seconds,
/// or `none` (run forever).
pub fn parse_ttl(text: &str) -> Result<Option<Duration>, String> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let (digits, multiplier) = match text.chars().last() {
        Some('s') => (&text[..text.len() - 1], 1),
        Some('m') => (&text[..text.len() - 1], 60),
        Some('h') => (&text[..text.len() - 1], 3600),
        Some(c) if c.is_ascii_digit() => (text, 1),
        _ => {
            return Err(format!(
                "invalid ttl: {text} (expected e.g. 90s, 30m, 2h, plain seconds, or none)"
            ))
        }
    };
    let n: u64 = digits.parse().map_err(|_| {
        format!("invalid ttl: {text} (expected e.g. 90s, 30m, 2h, plain seconds, or none)")
    })?;
    if n == 0 {
        return Err("invalid ttl: must be positive (use 'none' to run forever)".to_string());
    }
    Ok(Some(Duration::from_secs(n * multiplier)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ttl_accepts_all_forms() {
        assert_eq!(parse_ttl("90s").unwrap(), Some(Duration::from_secs(90)));
        assert_eq!(parse_ttl("30m").unwrap(), Some(Duration::from_secs(1800)));
        assert_eq!(parse_ttl("2h").unwrap(), Some(Duration::from_secs(7200)));
        assert_eq!(parse_ttl("45").unwrap(), Some(Duration::from_secs(45)));
        assert_eq!(parse_ttl("none").unwrap(), None);
        assert_eq!(parse_ttl("NONE").unwrap(), None);
    }

    #[test]
    fn parse_ttl_rejects_garbage() {
        assert!(parse_ttl("").is_err());
        assert!(parse_ttl("bogus").is_err());
        assert!(parse_ttl("1.5h").is_err());
        assert!(parse_ttl("-3s").is_err());
        assert!(parse_ttl("0").is_err());
        assert!(parse_ttl("0s").is_err());
        assert!(parse_ttl("s").is_err());
    }

    #[test]
    fn fresh_lifecycle_is_not_expired_and_counts_down() {
        let lifecycle = Lifecycle::new(Some(Duration::from_secs(3600)));
        assert!(!lifecycle.expired());
        assert_eq!(lifecycle.ttl_secs(), Some(3600));
        let remaining = lifecycle.remaining_secs().unwrap();
        assert!(remaining > 3590, "remaining={remaining}");
        assert_eq!(lifecycle.idle_secs(), 0);
    }

    #[test]
    fn none_ttl_never_expires() {
        let lifecycle = Lifecycle::new(None);
        assert!(!lifecycle.expired());
        assert_eq!(lifecycle.ttl_secs(), None);
        assert_eq!(lifecycle.remaining_secs(), None);
    }

    #[test]
    fn touch_resets_idle_and_expiry_follows_last_activity() {
        let mut lifecycle = Lifecycle::new(Some(Duration::from_millis(50)));
        std::thread::sleep(Duration::from_millis(80));
        assert!(lifecycle.expired(), "should expire 80ms into a 50ms ttl");
        lifecycle.touch();
        assert!(!lifecycle.expired(), "touch must reset the deadline");
    }

    #[test]
    fn set_ttl_moves_the_deadline_without_touching_activity() {
        let mut lifecycle = Lifecycle::new(Some(Duration::from_secs(3600)));
        lifecycle.set_ttl(Some(Duration::from_millis(1)));
        std::thread::sleep(Duration::from_millis(10));
        assert!(lifecycle.expired());
        lifecycle.set_ttl(None);
        assert!(!lifecycle.expired());
    }
}
