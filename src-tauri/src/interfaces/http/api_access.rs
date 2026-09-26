use crate::application::state::AppState;
use crate::domain::api_access::ApiAccessEntry;
use crate::domain::auth;
use actix_web::dev::ServiceRequest;
use actix_web::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use actix_web::http::StatusCode;
use std::collections::HashMap;
use std::time::Instant;

pub(crate) fn request_entry(request: &ServiceRequest, state: &AppState) -> (ApiAccessEntry, Instant) {
    let started = Instant::now();
    let request = request.request();
    let peer = request.peer_addr();
    let client = peer.map(|address| address.ip().to_string());
    let desktop = request.method() == actix_web::http::Method::POST
        && request.path() == "/print"
        && peer.is_some_and(|address| address.ip().is_loopback())
        && request
            .headers()
            .get("x-ixprint-desktop")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|key| auth::ct_eq(key.as_bytes(), state.desktop_key.as_bytes()));

    let query = actix_web::web::Query::<HashMap<String, String>>::from_query(request.query_string()).ok();
    let provided = auth::extract(
        request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        request
            .headers()
            .get("x-api-token")
            .and_then(|value| value.to_str().ok()),
        query
            .as_ref()
            .and_then(|values| values.get("token"))
            .map(String::as_str),
    );
    let supplied_credentials = request.headers().contains_key("authorization")
        || request.headers().contains_key("x-api-token")
        || query.as_ref().is_some_and(|values| values.contains_key("token"));
    let token = provided.and_then(|secret| {
        let config = state.config.get();
        auth::find(&config.tokens, &secret).map(|token| token.name.clone())
    });
    let auth = if desktop {
        "desktop"
    } else if token.is_some() {
        "recognized-token"
    } else if supplied_credentials {
        "unrecognized-credential"
    } else {
        "no-credential"
    };

    let entry = ApiAccessEntry {
        id: uuid::Uuid::new_v4().to_string(),
        time: chrono::Local::now().to_rfc3339(),
        method: request.method().as_str().to_string(),
        path: truncate(request.path(), 256),
        client,
        auth: auth.into(),
        token,
        request_content_type: header(request.headers(), CONTENT_TYPE.as_str(), 128),
        request_bytes: content_length(request.headers()),
        origin: header(request.headers(), "origin", 256),
        user_agent: header(request.headers(), "user-agent", 256),
        response_status: 0,
        response_content_type: None,
        response_bytes: None,
        duration_ms: 0,
    };
    (entry, started)
}

pub(crate) fn response_entry(
    entry: &mut ApiAccessEntry,
    status: StatusCode,
    headers: &actix_web::http::header::HeaderMap,
    started: Instant,
) {
    entry.response_status = status.as_u16();
    entry.response_content_type = header(headers, CONTENT_TYPE.as_str(), 128);
    entry.response_bytes = content_length(headers);
    entry.duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
}

fn header(headers: &actix_web::http::header::HeaderMap, name: &str, max_chars: usize) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?;
    let value: String = value
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect();
    (!value.is_empty()).then_some(value)
}

fn content_length(headers: &actix_web::http::header::HeaderMap) -> Option<u64> {
    headers.get(CONTENT_LENGTH)?.to_str().ok()?.parse().ok()
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn api_access_entry_does_not_store_authorization_or_query_token_values() {
        let bearer_secret = "fxp-secret-header-value";
        let alternate_secret = "fxp-secret-alternate-header-value";
        let query_secret = "fxp-secret-query-value";
        let request = TestRequest::default()
            .uri(&format!("/print?token={query_secret}"))
            .insert_header(("Authorization", format!("Bearer {bearer_secret}")))
            .insert_header(("X-Api-Token", alternate_secret))
            .to_srv_request();
        let state = crate::application::state::test_support::state();

        let (entry, _) = request_entry(&request, &state);
        let serialized = serde_json::to_string(&entry).unwrap();

        assert_eq!(entry.path, "/print");
        assert!(!serialized.contains(bearer_secret));
        assert!(!serialized.contains(alternate_secret));
        assert!(!serialized.contains(query_secret));
    }
}
