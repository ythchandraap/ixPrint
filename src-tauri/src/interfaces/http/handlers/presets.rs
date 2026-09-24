use super::super::auth_guard::authorized;
use super::super::dto::State;
use actix_web::{HttpRequest, HttpResponse};

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
