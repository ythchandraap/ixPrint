//! API token representation and pure token helpers.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    tokens.iter().fold(None, |found, t| {
        if ct_eq(t.hash.as_bytes(), h.as_bytes()) {
            Some(t)
        } else {
            found
        }
    })
}

pub(crate) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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
    fn extracts_from_all_places() {
        assert_eq!(extract(Some("Bearer abc"), None, None).as_deref(), Some("abc"));
        assert_eq!(extract(Some("bearer  abc "), None, None).as_deref(), Some("abc"));
        assert_eq!(extract(Some("Basic xyz"), Some("hdr"), None).as_deref(), Some("hdr"));
        assert_eq!(extract(None, None, Some("q")).as_deref(), Some("q"));
        assert_eq!(extract(None, Some("  "), None), None);
    }
}
