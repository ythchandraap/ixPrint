use super::super::auth_guard::authorized;
use super::super::dto::{error, internal};
use crate::application::printer_service::PrinterLookupError;
use crate::application::state::AppState;
use crate::domain::remote::HOP_HEADER;
use actix_web::{web, HttpRequest, HttpResponse};
use std::collections::HashMap;
use std::sync::Arc;

type State = web::Data<Arc<AppState>>;

pub(crate) async fn list_printers(req: HttpRequest, state: State) -> HttpResponse {
    authorized!(req, state);
    let include_remote = !req.headers().contains_key(HOP_HEADER);
    let state = state.get_ref().clone();
    match web::block(move || crate::application::printer_service::printer_views(&state, include_remote)).await {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(e) => internal(e),
    }
}

type LookupError = PrinterLookupError;

fn remote_error_response(remote_error: crate::domain::remote::RemoteError) -> HttpResponse {
    let status = if remote_error.is_bad_request() {
        actix_web::http::StatusCode::BAD_REQUEST
    } else {
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
    };
    error(status, &remote_error.to_string())
}

pub(crate) async fn capabilities(
    req: HttpRequest,
    state: State,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    authorized!(req, state);
    let hop = req.headers().contains_key(HOP_HEADER);
    let state = state.get_ref().clone();
    let requested = query.get("printer").filter(|p| !p.trim().is_empty()).cloned();
    let Some(name) = requested.or_else(|| state.default_printer()) else {
        return error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "parameter 'printer' is required",
        );
    };
    match web::block(move || crate::application::printer_service::capabilities(&state, &name, hop)).await {
        Ok(Ok(caps)) => HttpResponse::Ok().json(caps),
        Ok(Err(LookupError::NotFound(e))) => error(actix_web::http::StatusCode::NOT_FOUND, &e),
        Ok(Err(LookupError::Remote(e))) => remote_error_response(e),
        Err(e) => internal(e),
    }
}

pub(crate) async fn queue(req: HttpRequest, state: State, query: web::Query<HashMap<String, String>>) -> HttpResponse {
    authorized!(req, state);
    let hop = req.headers().contains_key(HOP_HEADER);
    let state = state.get_ref().clone();
    let requested = query.get("printer").filter(|p| !p.trim().is_empty()).cloned();
    let Some(name) = requested.or_else(|| state.default_printer()) else {
        return error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "parameter 'printer' is required",
        );
    };
    match web::block(move || crate::application::printer_service::queue(&state, &name, hop)).await {
        Ok(Ok(q)) => HttpResponse::Ok().json(q),
        Ok(Err(LookupError::NotFound(e))) => error(actix_web::http::StatusCode::NOT_FOUND, &e),
        Ok(Err(LookupError::Remote(e))) => remote_error_response(e),
        Err(e) => internal(e),
    }
}
