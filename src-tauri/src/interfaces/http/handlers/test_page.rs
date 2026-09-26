use super::super::auth_guard::{acquire_in_flight, authorized};
use super::super::dto::{error, State};
use super::job_support::{callback_from_query, idempotency_key};
use super::jobs::submit;
use crate::domain::job::{Document, JobRequest};
use crate::domain::settings::PrintSettings;
use actix_web::{web, HttpRequest, HttpResponse};
use std::collections::HashMap;

/// Prints the calibration page (`count` pages, default 1) with `/print` settings.
pub(crate) async fn test_page(
    req: HttpRequest,
    state: State,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    let token = authorized!(req, state);
    let _in_flight = match acquire_in_flight(state.get_ref().as_ref(), &req, token.as_ref()) {
        Ok(guard) => guard,
        Err(response) => return response,
    };
    let (settings, preset) = match PrintSettings::from_query(&query) {
        Ok(v) => v,
        Err(e) => return error(actix_web::http::StatusCode::BAD_REQUEST, &e),
    };
    let count = match query.get("count").map(|c| c.trim()).filter(|c| !c.is_empty()) {
        None => 1,
        Some(c) => match c.parse::<u32>() {
            Ok(n) if (1..=crate::infrastructure::pdf::test_page::MAX_TEST_PAGES).contains(&n) => n,
            _ => {
                let msg = format!(
                    "count must be between 1 and {}",
                    crate::infrastructure::pdf::test_page::MAX_TEST_PAGES
                );
                return error(actix_web::http::StatusCode::BAD_REQUEST, &msg);
            }
        },
    };
    let callback = match callback_from_query(&query, token.as_ref()) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let request = JobRequest {
        job_id: None,
        document: Document::TestPage { pages: count },
        history_input: None,
        file_name: "ix.print test page.pdf".into(),
        settings,
        preset,
        source: "http",
        client: req.peer_addr().map(|a| a.ip().to_string()),
        token: token.as_ref().map(|t| t.name.clone()),
        hop: req.headers().contains_key(crate::domain::remote::HOP_HEADER),
    };
    let idempotency = match idempotency_key(&req, token.as_ref()) {
        Ok(value) => value,
        Err(response) => return response,
    };
    submit(
        state.get_ref().clone(),
        request,
        "Print job accepted by printer service",
        None,
        callback,
        idempotency,
    )
    .await
}
