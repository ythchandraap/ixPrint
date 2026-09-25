use crate::application::state::AppState;
use actix_cors::Cors;
use std::sync::Arc;

pub(super) fn cors(state: Arc<AppState>) -> Cors {
    Cors::default()
        .allowed_origin_fn(move |origin, head| {
            let Ok(origin) = origin.to_str() else { return false };
            let host = head.headers().get("host").and_then(|h| h.to_str().ok());
            if host.is_some_and(|h| origin.eq_ignore_ascii_case(&format!("http://{}", h))) {
                return true;
            }
            let origin = origin.to_ascii_lowercase();
            state
                .config
                .get()
                .allowed_origins
                .iter()
                .any(|o| o == crate::domain::settings_config::ANY_ORIGIN || *o == origin)
        })
        .allowed_methods(["GET", "POST"])
        .allow_any_header()
        .max_age(600)
        .block_on_origin_mismatch(true)
}
