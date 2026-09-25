use crate::application::state::AppState;
use crate::infrastructure::os::lan_url;
use actix_web::{web, HttpResponse, Responder};
use std::sync::Arc;
type State = web::Data<Arc<AppState>>;

pub(crate) async fn hello() -> impl Responder {
    "Hello, world!"
}

const DOCS_HTML: &str = include_str!("../assets/docs.html");

/// Interactive API documentation; its "try it" forms call this same server.
pub(crate) async fn docs(state: State) -> HttpResponse {
    let port = state.config.get().server_port;
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header((
            "Content-Security-Policy",
            "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
             connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; \
             frame-ancestors 'none'",
        ))
        .body(DOCS_HTML.replace("{{LAN_URL}}", &lan_url(port).unwrap_or_default()))
}
