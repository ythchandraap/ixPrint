use std::collections::HashMap;

use serde::Serialize;

use crate::application::security::TokenUse;
use crate::application::state::AppState;
use crate::domain::auth;
use crate::domain::settings_config::{normalize_origin, Config};

#[derive(Serialize)]
pub struct CreatedToken {
    /// The only time the plain token is available.
    pub token: String,
    pub config: Config,
}

pub fn create(state: &AppState, name: &str) -> Result<CreatedToken, String> {
    let (token, stored) = auth::generate(name)?;
    let config = state.config.update(&mut |config| {
        config.tokens.push(stored.clone());
        Ok(())
    })?;
    Ok(CreatedToken { token, config })
}

pub fn revoke(state: &AppState, id: &str) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.tokens.retain(|token| token.id != id);
        Ok(())
    })
}

pub fn set_required(state: &AppState, required: bool) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.require_token = required;
        Ok(())
    })
}

pub fn set_allowed_origins(state: &AppState, origins: &[String]) -> Result<Config, String> {
    let mut normalized = Vec::new();
    for origin in origins.iter().filter(|origin| !origin.trim().is_empty()) {
        let origin = normalize_origin(origin)?;
        if !normalized.contains(&origin) {
            normalized.push(origin);
        }
    }
    state.config.update(&mut |config| {
        config.allowed_origins = normalized.clone();
        Ok(())
    })
}

pub fn usage(state: &AppState) -> HashMap<String, TokenUse> {
    state.token_usage.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::state::test_support::state;

    #[test]
    fn create_stores_only_hash_revoke_removes_and_required_can_toggle() {
        let state = state();
        let created = create(&state, "SIMRS").unwrap();
        let stored = created.config.tokens.last().unwrap();
        assert_ne!(stored.hash, created.token);
        assert_eq!(stored.hash, auth::hash(&created.token));
        assert!(!serde_json::to_string(&created.config).unwrap().contains(&created.token));

        let revoked = revoke(&state, &stored.id).unwrap();
        assert!(revoked.tokens.is_empty());
        assert!(set_required(&state, true).unwrap().require_token);
        assert!(!set_required(&state, false).unwrap().require_token);
    }
}
