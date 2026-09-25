use crate::application::security::InFlightGuard;
use crate::application::state::AppState;
use crate::domain::auth;
use actix_web::http::header::HeaderValue;
use actix_web::{web, HttpRequest, HttpResponse};
use std::collections::HashMap;

use super::dto::{error, Message};

fn unauthorized(message: &str) -> HttpResponse {
    HttpResponse::Unauthorized()
        .insert_header(("WWW-Authenticate", "Bearer"))
        .json(Message { message })
}

pub(super) fn authorize(req: &HttpRequest, state: &AppState) -> Result<Option<auth::ApiToken>, HttpResponse> {
    authorize_with_policy(req, state, false)
}

pub(super) fn authorize_bearer(req: &HttpRequest, state: &AppState) -> Result<auth::ApiToken, HttpResponse> {
    authorize_with_policy(req, state, true)?.ok_or_else(|| unauthorized("A bearer token is required"))
}

fn authorize_with_policy(
    req: &HttpRequest,
    state: &AppState,
    bearer_required: bool,
) -> Result<Option<auth::ApiToken>, HttpResponse> {
    let ip = req.peer_addr().map(|a| a.ip());
    let desktop_key = req.headers().get("x-ixprint-desktop").and_then(|v| v.to_str().ok());
    if req.method() == actix_web::http::Method::POST
        && req.path() == "/print"
        && ip.is_some_and(|ip| ip.is_loopback())
        && desktop_key.is_some_and(|k| crate::domain::auth::ct_eq(k.as_bytes(), state.desktop_key.as_bytes()))
    {
        return Ok(None);
    }
    if let Some(wait) = ip.and_then(|ip| state.failed_auth.locked(ip)) {
        return Err(HttpResponse::TooManyRequests()
            .insert_header(("Retry-After", wait.as_secs().max(1).to_string()))
            .json(Message {
                message: "Too many incorrect tokens from this address; try again later",
            }));
    }
    let config = state.config.get();
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok());
    let query = (!bearer_required)
        .then(|| web::Query::<HashMap<String, String>>::from_query(req.query_string()).ok())
        .flatten();
    let provided = if bearer_required {
        auth::extract(header("authorization"), None, None)
    } else {
        auth::extract(
            header("authorization"),
            header("x-api-token"),
            query.as_ref().and_then(|q| q.get("token")).map(String::as_str),
        )
    };
    match provided {
        Some(token) => match auth::find(&config.tokens, &token) {
            Some(t) => {
                if let Some(ip) = ip {
                    state.failed_auth.succeed(ip);
                }
                state
                    .token_usage
                    .record(&t.id, req.peer_addr().map(|a| a.ip().to_string()));
                Ok(Some(t.clone()))
            }
            None => {
                if let Some(ip) = ip {
                    state.failed_auth.fail(ip);
                }
                Err(unauthorized("Token is invalid or has been revoked"))
            }
        },
        None if bearer_required => Err(unauthorized("A bearer token is required for this endpoint")),
        None if config.require_token && config.tokens.is_empty() => Err(unauthorized(
            "A token is required, but none exists: create one in ix.print → Settings → API",
        )),
        None if config.require_token => Err(unauthorized(
            "A token is required: send the 'Authorization: Bearer <token>' header (create one in ix.print → API)",
        )),
        None => Ok(None),
    }
}

pub(super) fn acquire_in_flight<'a>(
    state: &'a AppState,
    req: &HttpRequest,
    token: Option<&auth::ApiToken>,
) -> Result<Option<InFlightGuard<'a>>, HttpResponse> {
    let desktop_key = req.headers().get("x-ixprint-desktop").and_then(|v| v.to_str().ok());
    let is_desktop = req.method() == actix_web::http::Method::POST
        && req.path() == "/print"
        && req.peer_addr().is_some_and(|addr| addr.ip().is_loopback())
        && desktop_key.is_some_and(|key| crate::domain::auth::ct_eq(key.as_bytes(), state.desktop_key.as_bytes()));
    if is_desktop {
        return Ok(None);
    }

    let key = token
        .map(|token| token.id.clone())
        .or_else(|| req.peer_addr().map(|addr| format!("ip:{}", addr.ip())))
        .unwrap_or_else(|| "ip:unknown".into());
    state.in_flight.acquire(&key).map(Some).ok_or_else(|| {
        let mut response = error(
            actix_web::http::StatusCode::TOO_MANY_REQUESTS,
            "Too many print requests in progress for this client; retry shortly",
        );
        response
            .headers_mut()
            .insert(actix_web::http::header::RETRY_AFTER, HeaderValue::from_static("2"));
        response
    })
}

macro_rules! authorized {
    ($req:expr, $state:expr) => {
        match $crate::interfaces::http::auth_guard::authorize(&$req, &$state) {
            Ok(token) => token,
            Err(response) => return response,
        }
    };
}
pub(crate) use authorized;

macro_rules! authorized_bearer {
    ($req:expr, $state:expr) => {
        match $crate::interfaces::http::auth_guard::authorize_bearer(&$req, &$state) {
            Ok(token) => token,
            Err(response) => return response,
        }
    };
}
pub(crate) use authorized_bearer;
