use crate::shared::sync::LockExt;
use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const MAX_IN_FLIGHT_PER_CLIENT: usize = 8;
const MAX_FAILURES: u32 = 10;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);
const LOCKOUT: Duration = Duration::from_secs(5 * 60);

/// Clients (by IP) that sent wrong tokens: after `MAX_FAILURES` within
/// `FAILURE_WINDOW` they are refused for `LOCKOUT`, even with a valid token.
#[derive(Default)]
pub struct FailedAttempts(Mutex<HashMap<IpAddr, (u32, Instant)>>);

impl FailedAttempts {
    /// Remaining lockout for `ip`, if it is locked out.
    pub fn locked(&self, ip: IpAddr) -> Option<Duration> {
        let map = self.0.lock_safe();
        let (count, since) = map.get(&ip)?;
        (*count >= MAX_FAILURES)
            .then(|| LOCKOUT.saturating_sub(since.elapsed()))
            .filter(|d| !d.is_zero())
    }

    pub fn fail(&self, ip: IpAddr) {
        let mut map = self.0.lock_safe();
        let now = Instant::now();
        // Keep the map small: forget clients whose window or lockout has passed.
        map.retain(|_, (count, since)| {
            since.elapsed()
                < if *count >= MAX_FAILURES {
                    LOCKOUT
                } else {
                    FAILURE_WINDOW
                }
        });
        let entry = map.entry(ip).or_insert((0, now));
        entry.0 += 1;
        if entry.0 == MAX_FAILURES {
            entry.1 = now; // lockout starts now
        }
    }

    pub fn succeed(&self, ip: IpAddr) {
        self.0.lock_safe().remove(&ip);
    }
}

/// When and from where each token was last used (kept in memory only, so a
/// busy printer doesn't rewrite the config file on every job).
#[derive(Serialize, Clone, Debug)]
pub struct TokenUse {
    pub time: String,
    pub client: Option<String>,
}

#[derive(Default)]
pub struct TokenUsage(Mutex<HashMap<String, TokenUse>>);

impl TokenUsage {
    pub fn record(&self, id: &str, client: Option<String>) {
        self.0.lock_safe().insert(
            id.to_string(),
            TokenUse {
                time: chrono::Local::now().to_rfc3339(),
                client,
            },
        );
    }

    pub fn snapshot(&self) -> HashMap<String, TokenUse> {
        self.0.lock_safe().clone()
    }
}

#[derive(Default)]
pub struct InFlight(Mutex<HashMap<String, usize>>);

impl InFlight {
    pub fn acquire(&self, key: &str) -> Option<InFlightGuard<'_>> {
        let mut counts = self.0.lock_safe();
        let count = counts.entry(key.to_string()).or_default();
        if *count >= MAX_IN_FLIGHT_PER_CLIENT {
            return None;
        }
        *count += 1;
        Some(InFlightGuard {
            in_flight: self,
            key: key.to_string(),
        })
    }
}

pub struct InFlightGuard<'a> {
    in_flight: &'a InFlight,
    key: String,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        let mut counts = self.in_flight.0.lock_safe();
        match counts.get(&self.key).copied() {
            Some(1) => {
                counts.remove(&self.key);
            }
            Some(count) => {
                counts.insert(self.key.clone(), count - 1);
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn locks_out_after_repeated_failures() {
        let attempts = FailedAttempts::default();
        let ip: IpAddr = "10.0.0.5".parse().unwrap();
        let other: IpAddr = "10.0.0.6".parse().unwrap();
        for _ in 0..MAX_FAILURES - 1 {
            attempts.fail(ip);
        }
        assert!(attempts.locked(ip).is_none());
        attempts.fail(ip);
        assert!(attempts.locked(ip).is_some());
        assert!(attempts.locked(other).is_none());
        attempts.succeed(ip);
        assert!(attempts.locked(ip).is_none());
    }

    #[test]
    fn limits_in_flight_and_releases_dropped_guard() {
        let in_flight = InFlight::default();
        let mut guards: Vec<_> = (0..MAX_IN_FLIGHT_PER_CLIENT)
            .map(|_| in_flight.acquire("token-id").unwrap())
            .collect();
        assert!(in_flight.acquire("token-id").is_none());

        drop(guards.pop());
        assert!(in_flight.acquire("token-id").is_some());
    }

    #[test]
    fn separate_clients_have_separate_limits() {
        let in_flight = InFlight::default();
        let _first: Vec<_> = (0..MAX_IN_FLIGHT_PER_CLIENT)
            .map(|_| in_flight.acquire("token-a").unwrap())
            .collect();
        assert!(in_flight.acquire("token-a").is_none());
        assert!(in_flight.acquire("token-b").is_some());
    }
}
