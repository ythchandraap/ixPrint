mod api_access;
pub mod auth_guard;
pub mod cors;
pub mod dto;
pub mod handlers;
pub mod security;

#[cfg(test)]
mod tests;

use crate::application::ports::ServerControl;
use crate::application::state::AppState;
use crate::shared::sync::LockExt;
use actix_web::{web, App, HttpServer};
use std::sync::Arc;

pub type State = web::Data<Arc<AppState>>;

pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(handlers::meta::hello))
        .route("/docs", web::get().to(handlers::meta::docs))
        .route("/list-printer", web::get().to(handlers::printers::list_printers))
        .route("/printer-capabilities", web::get().to(handlers::printers::capabilities))
        .route("/queue", web::get().to(handlers::printers::queue))
        .route("/job-status", web::get().to(handlers::jobs::job_status))
        .route("/presets", web::get().to(handlers::presets::presets))
        .route("/presets/export", web::get().to(handlers::presets::export_presets))
        .route("/presets/shared", web::get().to(handlers::presets::shared_presets))
        .route(
            "/presets/shared/export",
            web::get().to(handlers::presets::export_shared_preset),
        )
        .route("/print", web::post().to(handlers::jobs::print))
        .route("/print-ready", web::post().to(handlers::incoming::print_ready))
        .route("/preflight", web::post().to(handlers::jobs::preflight))
        .route("/test-page", web::post().to(handlers::test_page::test_page))
        .route("/templates", web::get().to(handlers::presets::templates))
        .route("/print-template", web::post().to(handlers::jobs::print_template));
}

#[derive(Clone, Default)]
pub struct HttpServerControl {
    runtime: Arc<ServerRuntime>,
}

#[derive(Default)]
struct ServerRuntime {
    handle: std::sync::Mutex<Option<actix_web::dev::ServerHandle>>,
    error: std::sync::Mutex<Option<String>>,
}

impl ServerControl for HttpServerControl {
    fn start(&self, state: Arc<AppState>) {
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            if let Err(e) = actix_web::rt::System::new().block_on(run_server(state.clone(), runtime.clone())) {
                let msg = format!("Server port {} failed: {}", state.config.get().server_port, e);
                eprintln!("{}", msg);
                *runtime.error.lock_safe() = Some(msg);
            }
        });
    }

    fn stop(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let handle = self.runtime.handle.lock_safe().take();
            if let Some(handle) = handle {
                handle.stop(true).await;
            }
        })
    }

    fn is_running(&self) -> bool {
        self.runtime.handle.lock_safe().is_some()
    }
    fn last_error(&self) -> Option<String> {
        self.runtime.error.lock_safe().clone()
    }
    fn clear_error(&self) {
        self.runtime.error.lock_safe().take();
    }
}

async fn run_server(state: Arc<AppState>, runtime: Arc<ServerRuntime>) -> std::io::Result<()> {
    let config = state.config.get();
    if !config.bridge_enabled {
        return Ok(());
    }
    let port = config.server_port;
    let host = if config.lan_access { "0.0.0.0" } else { "127.0.0.1" };
    let data = web::Data::new(state.clone());
    let cors_state = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .wrap(security::security_headers())
            .wrap(cors::cors(cors_state.clone()))
            .app_data(data.clone())
            .configure(routes)
            .wrap_fn(|req, srv| {
                use actix_web::dev::Service;

                let state = req
                    .app_data::<web::Data<Arc<AppState>>>()
                    .map(|data| data.get_ref().clone());
                let mut access = state.as_ref().map(|state| api_access::request_entry(&req, state));
                let future = srv.call(req);
                async move {
                    match future.await {
                        Ok(response) => {
                            if let (Some(state), Some((mut entry, started))) = (state, access.take()) {
                                api_access::response_entry(
                                    &mut entry,
                                    response.status(),
                                    response.response().headers(),
                                    started,
                                );
                                let log = state.api_access_log.clone();
                                actix_web::rt::task::spawn_blocking(move || {
                                    if let Err(error) = log.append(&entry) {
                                        eprintln!("[ix.print] failed to write API access log: {}", error);
                                    }
                                });
                            }
                            Ok(response)
                        }
                        Err(error) => {
                            if let (Some(state), Some((mut entry, started))) = (state, access.take()) {
                                let status = error.as_response_error().status_code();
                                entry.response_status = status.as_u16();
                                entry.duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                                let log = state.api_access_log.clone();
                                actix_web::rt::task::spawn_blocking(move || {
                                    if let Err(error) = log.append(&entry) {
                                        eprintln!("[ix.print] failed to write API access log: {}", error);
                                    }
                                });
                            }
                            Err(error)
                        }
                    }
                }
            })
    })
    .client_request_timeout(std::time::Duration::from_secs(30))
    .client_disconnect_timeout(std::time::Duration::from_secs(5))
    .keep_alive(std::time::Duration::from_secs(30))
    .max_connections(256)
    .bind((host, port))?
    .run();
    *runtime.error.lock_safe() = None;
    *runtime.handle.lock_safe() = Some(server.handle());
    let result = server.await;
    runtime.handle.lock_safe().take();
    result
}

pub fn spawn(state: Arc<AppState>) {
    state.server.start(state.clone());
}
