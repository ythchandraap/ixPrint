use super::super::dto::{error, internal, MAX_JSON_BYTES, MAX_UPLOAD_BYTES, PDF_HEADER_WINDOW};
use crate::domain::auth;
use crate::domain::callback::Callback;
use crate::domain::job::{Document, JobRequest};
use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::io::Write;

pub(super) enum Submission {
    New(crate::domain::job::JobResult, Option<serde_json::Value>),
    Replay(crate::domain::job::JobResult, Option<serde_json::Value>),
    Pending(Option<String>),
    Conflict,
}

pub(super) fn idempotency_key(
    req: &HttpRequest,
    token: Option<&auth::ApiToken>,
) -> Result<Option<String>, HttpResponse> {
    let Some(header) = req.headers().get("Idempotency-Key") else {
        return Ok(None);
    };
    let key = header.to_str().unwrap_or_default();
    if key.is_empty() || key.len() > 128 || !key.bytes().all(|byte| (33..=126).contains(&byte)) {
        return Err(error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "Idempotency-Key must be 1–128 visible ASCII characters",
        ));
    }
    let scope = token.map(|token| format!("token:{}", token.id)).unwrap_or_else(|| {
        format!(
            "ip:{}",
            req.peer_addr().map(|addr| addr.ip().to_string()).unwrap_or_default()
        )
    });
    let hash = Sha256::digest(format!("{}\0{}", scope, key).as_bytes());
    Ok(Some(format!("{:x}", hash)))
}

pub(super) fn fingerprint(request: &JobRequest, callback: Option<&Callback>) -> Result<String, String> {
    let mut hash = Sha256::new();
    let metadata = serde_json::json!({
        "preset": request.preset,
        "settings": request.settings,
        "file_name": request.file_name,
        "callback_url": callback.map(|callback| &callback.url),
        "callback_ref": callback.and_then(|callback| callback.reference.as_ref()),
    });
    hash.update(serde_json::to_vec(&metadata).map_err(|e| e.to_string())?);
    match &request.document {
        Document::File(path) => {
            let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
        }
        Document::Data { records, header } => {
            hash.update(serde_json::to_vec(&(records, header)).map_err(|e| e.to_string())?);
        }
        Document::TestPage { pages } => hash.update(pages.to_le_bytes()),
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(super) async fn read_json_payload<T: serde::de::DeserializeOwned>(
    payload: &mut web::Payload,
) -> Result<T, HttpResponse> {
    read_json_payload_with_value(payload).await.map(|(parsed, _)| parsed)
}

pub(super) async fn read_json_payload_with_value<T: serde::de::DeserializeOwned>(
    payload: &mut web::Payload,
) -> Result<(T, serde_json::Value), HttpResponse> {
    let mut body = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        match chunk {
            Ok(c) if body.len() + c.len() <= MAX_JSON_BYTES => body.extend_from_slice(&c),
            Ok(_) => {
                return Err(error(
                    actix_web::http::StatusCode::PAYLOAD_TOO_LARGE,
                    "JSON body is too large",
                ))
            }
            Err(e) => return Err(error(actix_web::http::StatusCode::BAD_REQUEST, &e.to_string())),
        }
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        error(
            actix_web::http::StatusCode::BAD_REQUEST,
            &format!("JSON body is invalid: {}", e),
        )
    })?;
    let parsed = serde_json::from_value(value.clone()).map_err(|e| {
        error(
            actix_web::http::StatusCode::BAD_REQUEST,
            &format!("JSON body is invalid: {}", e),
        )
    })?;
    Ok((parsed, value))
}

/// Saves the first uploaded file of the multipart body; returns (path, original name).
pub(super) async fn save_upload(
    multipart: &mut Multipart,
    dir: &std::path::Path,
) -> Result<(std::path::PathBuf, String), HttpResponse> {
    let bad = |m: &str| error(actix_web::http::StatusCode::BAD_REQUEST, m);
    while let Some(item) = multipart.next().await {
        let mut field = item.map_err(|e| bad(&format!("Multipart data is invalid: {}", e)))?;
        let Some(name) = field
            .content_disposition()
            .and_then(|cd| cd.get_filename())
            .map(sanitize_filename::sanitize)
        else {
            continue; // text field
        };
        std::fs::create_dir_all(dir).map_err(internal)?;
        let path = dir.join(format!("{}.pdf", uuid::Uuid::new_v4()));
        let mut file = std::fs::File::create(&path).map_err(internal)?;
        let mut size = 0usize;
        let mut head = Vec::with_capacity(PDF_HEADER_WINDOW);
        let result = async {
            while let Some(chunk) = field.next().await {
                let data = chunk.map_err(|e| bad(&format!("Upload terputus: {}", e)))?;
                size += data.len();
                if size > MAX_UPLOAD_BYTES {
                    return Err(error(
                        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE,
                        "File terlalu besar",
                    ));
                }
                let room = PDF_HEADER_WINDOW - head.len();
                head.extend_from_slice(&data[..room.min(data.len())]);
                file.write_all(&data).map_err(internal)?;
            }
            // Only PDFs go on to PDFium; anything else is refused before parsing.
            if !head.windows(5).any(|w| w == b"%PDF-") {
                return Err(bad("File is not a PDF"));
            }
            Ok(())
        }
        .await;
        drop(file);
        if let Err(resp) = result {
            let _ = std::fs::remove_file(&path);
            return Err(resp);
        }
        let name = if name.is_empty() {
            "unnamed.pdf".to_string()
        } else {
            name
        };
        return Ok((path, name));
    }
    Err(bad("No file uploaded"))
}

/// `callback_url` / `callback_ref` from the query of an upload or test page.
pub(super) fn callback_from_query(
    query: &HashMap<String, String>,
    token: Option<&auth::ApiToken>,
) -> Result<Option<Callback>, HttpResponse> {
    let get = |k: &str| query.get(k).map(String::as_str);
    Callback::from_request(get("callback_url"), get("callback_ref"), token)
        .map_err(|e| error(actix_web::http::StatusCode::BAD_REQUEST, &e))
}
