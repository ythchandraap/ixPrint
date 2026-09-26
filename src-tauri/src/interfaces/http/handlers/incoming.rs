use super::super::auth_guard::{acquire_in_flight, authorized_bearer};
use super::super::dto::{error, internal, State};
use super::job_support::save_upload;
use crate::domain::incoming_job::IncomingSource;
use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
struct ReadyQuery {
    printer: String,
    source: IncomingSource,
    source_job_id: String,
    copies: u32,
}

/// Receives a document that was already approved by the user on a remote ix.print computer.
/// This endpoint always requires a bearer token and never forwards to a second remote.
pub(crate) async fn print_ready(req: HttpRequest, state: State, payload: web::Payload) -> HttpResponse {
    let token = authorized_bearer!(req, state);
    if !req.headers().contains_key(crate::domain::remote::HOP_HEADER) {
        return error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "This endpoint accepts only a forwarded ix.print job",
        );
    }
    let parsed: ReadyQuery = match web::Query::<ReadyQuery>::from_query(req.query_string()) {
        Ok(query) => query.into_inner(),
        Err(_) => {
            return error(
                actix_web::http::StatusCode::BAD_REQUEST,
                "Forwarded print settings are invalid",
            )
        }
    };
    if parsed.printer.trim().is_empty()
        || !(1..=999).contains(&parsed.copies)
        || uuid::Uuid::parse_str(&parsed.source_job_id).is_err()
    {
        return error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "Forwarded print settings are invalid",
        );
    }
    let _in_flight = match acquire_in_flight(state.get_ref().as_ref(), &req, Some(&token)) {
        Ok(guard) => guard,
        Err(response) => return response,
    };
    let mut multipart = Multipart::new(req.headers(), payload);
    let (file, file_name) = match save_upload(&mut multipart, &state.jobs_dir).await {
        Ok(result) => result,
        Err(response) => return response,
    };
    let app_state = state.get_ref().clone();
    let uploaded_file = file.clone();
    let outcome = web::block(move || {
        crate::application::document_service::receive_ready_pdf(
            &app_state,
            &uploaded_file,
            &file_name,
            parsed.source,
            &parsed.source_job_id,
            parsed.copies,
            &parsed.printer,
        )
    })
    .await;
    let _ = std::fs::remove_file(&file);
    match outcome {
        Ok(Ok(result)) => HttpResponse::Ok().json(result),
        Ok(Err(message)) => error(actix_web::http::StatusCode::BAD_REQUEST, &message),
        Err(e) => internal(e),
    }
}
