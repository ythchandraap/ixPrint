use super::super::auth_guard::{authorized, authorized_bearer};
use super::super::dto::{error, State};
use actix_web::{http::StatusCode, web, HttpRequest, HttpResponse};
use serde::Deserialize;

/// `GET /templates`: presets that have a design (kept for older clients).
pub(crate) async fn templates(req: HttpRequest, state: State) -> HttpResponse {
    authorized!(req, state);
    HttpResponse::Ok().json(crate::application::preset_service::templates_json(&state))
}

pub(crate) async fn presets(req: HttpRequest, state: State) -> HttpResponse {
    authorized!(req, state);
    HttpResponse::Ok().json(crate::application::preset_service::bridge_presets(&state))
}

pub(crate) async fn export_presets(req: HttpRequest, state: State) -> HttpResponse {
    authorized!(req, state);
    let bundle = crate::application::preset_service::export_bundle(&state, None, true);
    HttpResponse::Ok()
        .insert_header(("Content-Disposition", "attachment; filename=\"ix.print-presets.json\""))
        .json(bundle)
}

pub(crate) async fn shared_presets(req: HttpRequest, state: State) -> HttpResponse {
    authorized_bearer!(req, state);
    let (enabled, presets) = crate::application::preset_service::shared_listing(&state);
    if !enabled {
        return error(StatusCode::FORBIDDEN, "Shared preset listing is disabled");
    }
    HttpResponse::Ok().json(serde_json::json!({ "presets": presets }))
}

#[derive(Deserialize)]
struct SharedPresetQuery {
    name: String,
}

pub(crate) async fn export_shared_preset(req: HttpRequest, state: State) -> HttpResponse {
    authorized_bearer!(req, state);
    let query = match web::Query::<SharedPresetQuery>::from_query(req.query_string()) {
        Ok(query) => query.into_inner(),
        Err(_) => return error(StatusCode::BAD_REQUEST, "A valid preset name is required"),
    };
    if crate::domain::preset::validate_preset_name(&query.name).is_err() {
        return error(StatusCode::BAD_REQUEST, "A valid preset name is required");
    }
    let Some(bundle) = crate::application::preset_service::export_shared_preset(&state, &query.name) else {
        return error(StatusCode::NOT_FOUND, "Preset is not available");
    };
    HttpResponse::Ok().json(bundle)
}
