//! API tokens for the HTTP server. Only a SHA-256 hash of each token is stored
//! in `config.json`; the token itself is shown once when it is created.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const TOKEN_PREFIX: &str = "fxp_";
const MAX_NAME: usize = 64;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ApiToken {
    pub id: String,
    /// Which client uses it, e.g. "SIMRS".
    pub name: String,
    /// Hex SHA-256 of the full token.
    pub hash: String,
    /// First characters of the token, to recognise it in lists.
    pub hint: String,
    pub created_at: String,
}

/// Creates a token; returns the plain token (show it once) and what to store.
pub fn generate(name: &str) -> Result<(String, ApiToken), String> {
    let name = name.trim();
    if name.is_empty() {
		return Err("Token name cannot be empty".into());
    }
    if name.chars().count() > MAX_NAME {
		return Err(format!("Token name must be at most {} characters", MAX_NAME));
    }
    // Two v4 UUIDs = 244 random bits from the OS generator.
    let token = format!(
        "{}{}{}",
        TOKEN_PREFIX,
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let stored = ApiToken {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        hash: hash(&token),
        hint: format!("{}…", &token[..TOKEN_PREFIX.len() + 6]),
        created_at: chrono::Local::now().to_rfc3339(),
    };
    Ok((token, stored))
}

pub fn hash(token: &str) -> String {
    Sha256::digest(token.trim().as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// The stored token matching `provided`, if any. Every stored hash is compared
/// in constant time, so response timing reveals nothing about the tokens.
pub fn find<'a>(tokens: &'a [ApiToken], provided: &str) -> Option<&'a ApiToken> {
    let h = hash(provided);
    tokens
        .iter()
        .fold(None, |found, t| if ct_eq(t.hash.as_bytes(), h.as_bytes()) { Some(t) } else { found })
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

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
        let map = self.0.lock().unwrap();
        let (count, since) = map.get(&ip)?;
        (*count >= MAX_FAILURES).then(|| LOCKOUT.saturating_sub(since.elapsed())).filter(|d| !d.is_zero())
    }

    pub fn fail(&self, ip: IpAddr) {
        let mut map = self.0.lock().unwrap();
        let now = Instant::now();
        // Keep the map small: forget clients whose window or lockout has passed.
        map.retain(|_, (count, since)| since.elapsed() < if *count >= MAX_FAILURES { LOCKOUT } else { FAILURE_WINDOW });
        let entry = map.entry(ip).or_insert((0, now));
        entry.0 += 1;
        if entry.0 == MAX_FAILURES {
            entry.1 = now; // lockout starts now
        }
    }

    pub fn succeed(&self, ip: IpAddr) {
        self.0.lock().unwrap().remove(&ip);
    }
}

/// Token from `Authorization: Bearer …`, `X-Api-Token`, or the `token` query parameter.
pub fn extract(authorization: Option<&str>, api_token: Option<&str>, query: Option<&str>) -> Option<String> {
    authorization
        .and_then(|v| {
            let (scheme, rest) = v.trim().split_once(' ')?;
            scheme.eq_ignore_ascii_case("bearer").then(|| rest.trim())
        })
        .or(api_token)
        .or(query)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
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
        self.0.lock().unwrap().insert(
            id.to_string(),
            TokenUse { time: chrono::Local::now().to_rfc3339(), client },
        );
    }

    pub fn snapshot(&self) -> HashMap<String, TokenUse> {
        self.0.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_find() {
        let (token, stored) = generate(" SIMRS ").unwrap();
        assert!(token.starts_with(TOKEN_PREFIX) && token.len() == TOKEN_PREFIX.len() + 64);
        assert_eq!(stored.name, "SIMRS");
        assert!(!stored.hash.contains(&token[TOKEN_PREFIX.len()..]));
        assert!(token.starts_with(stored.hint.trim_end_matches('…')));
        let list = vec![stored.clone()];
        assert_eq!(find(&list, &token).map(|t| &t.id), Some(&stored.id));
        assert!(find(&list, "fxp_wrong").is_none());
        assert!(generate("  ").is_err());
        let (other, _) = generate("x").unwrap();
        assert_ne!(token, other);
    }

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
    fn extracts_from_all_places() {
        assert_eq!(extract(Some("Bearer abc"), None, None).as_deref(), Some("abc"));
        assert_eq!(extract(Some("bearer  abc "), None, None).as_deref(), Some("abc"));
        assert_eq!(extract(Some("Basic xyz"), Some("hdr"), None).as_deref(), Some("hdr"));
        assert_eq!(extract(None, None, Some("q")).as_deref(), Some("q"));
        assert_eq!(extract(None, Some("  "), None), None);
    }
}
