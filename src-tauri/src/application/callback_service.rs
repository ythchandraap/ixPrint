//! Watches submitted jobs and sends their signed result callback.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::application::state::AppState;
use crate::domain::callback::{Attempt, Callback, Payload};
use crate::domain::history::{CallbackStatus, HistoryEntry};
use crate::domain::job::JobResult;

const MAX_ACTIVE: usize = 200;
const POLL_EVERY: Duration = Duration::from_secs(3);
const GIVE_UP_AFTER: Duration = Duration::from_secs(60 * 60);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn watch(state: Arc<AppState>, job: &JobResult, callback: Callback) -> Result<(), String> {
    watch_with_elapsed(state, job, callback, Duration::ZERO)
}

fn watch_with_elapsed(
    state: Arc<AppState>,
    job: &JobResult,
    callback: Callback,
    elapsed_before_restart: Duration,
) -> Result<(), String> {
    let record = {
        let (state, job_id, url, reference, token_id) = (
            state.clone(),
            job.job_id.clone(),
            callback.url.clone(),
            callback.reference.clone(),
            callback.token_id.clone(),
        );
        move |status: &str, result: Option<&str>, attempts: u32, outcome: Option<&Attempt>| {
            let (http_status, error) = match outcome {
                Some(Attempt::Delivered(code)) => (Some(*code), None),
                Some(Attempt::Failed { status, error }) => (*status, Some(error.clone())),
                None => (None, None),
            };
            let entry = CallbackStatus {
                url: url.clone(),
                token_id: Some(token_id.clone()),
                reference: reference.clone(),
                state: status.to_string(),
                result: result.map(str::to_string),
                attempts,
                http_status,
                error,
                time: state.clock.now_rfc3339(),
            };
            if let Err(error) = state.history.append_callback(&job_id, entry) {
                eprintln!("[ix.print] callback status was not recorded in history: {}", error);
            }
        }
    };
    if ACTIVE.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIVE {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
        let message = "too many callbacks are currently waiting".to_string();
        record(
            "refused",
            None,
            0,
            Some(&Attempt::Failed {
                status: None,
                error: message.clone(),
            }),
        );
        return Err(message);
    }
    record("watching", None, 0, None);
    let (job_id, printer, preset, sheets) = (job.job_id.clone(), job.printer.clone(), job.preset.clone(), job.sheets);
    let reference = callback.reference.clone();
    let callback = Arc::new(callback);
    std::thread::spawn(move || {
        let payload = |event, success, state_name: &str, message: Option<String>| Payload {
            event,
            job_id: job_id.clone(),
            reference: reference.clone(),
            success,
            state: state_name.to_string(),
            message,
            printer: printer.clone(),
            preset: preset.clone(),
            sheets,
            time: state.clock.now_rfc3339(),
        };
        let started = Instant::now();
        let mut problem_sent = false;
        let finished = loop {
            let status = crate::application::printer_service::job_status_json(&state, &job_id).unwrap_or_default();
            let text = |key: &str| status.get(key).and_then(|value| value.as_str()).map(str::to_string);
            let message = text("message").or_else(|| text("printer_message"));
            match text("state").as_deref().unwrap_or("unknown") {
                "completed" => break payload("job.finished", Some(true), "completed", None),
                "failed" => break payload("job.finished", Some(false), "failed", message),
                "gone" => {
                    break payload(
                        "job.finished",
                        None,
                        "gone",
                        Some("Job left the queue; print outcome is unknown".into()),
                    )
                }
                "unknown" if should_mark_sent(&status) => {
                    break payload(
                        "job.finished",
                        None,
                        "sent",
                        Some("Sent to the printer; the spooler did not report the job status".into()),
                    )
                }
                "error" if !problem_sent => {
                    problem_sent = true;
                    let problem = payload("job.problem", None, "error", message);
                    let callback = callback.clone();
                    let sender = state.callback_sender.clone();
                    std::thread::spawn(move || sender.deliver(&callback, &problem, &mut |_, _, _| {}));
                }
                _ => {}
            }
            if started.elapsed() + elapsed_before_restart > GIVE_UP_AFTER {
                let last = text("state").unwrap_or_else(|| "unknown".into());
                break payload(
                    "job.finished",
                    None,
                    "timeout",
                    Some(format!(
                        "Belum selesai setelah {} menit (status terakhir: {})",
                        GIVE_UP_AFTER.as_secs() / 60,
                        last
                    )),
                );
            }
            std::thread::sleep(POLL_EVERY);
        };
        let result = finished.state.clone();
        state
            .callback_sender
            .deliver(&callback, &finished, &mut |status, attempts, outcome| {
                record(status, Some(&result), attempts, Some(outcome))
            });
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
    });
    Ok(())
}

pub fn should_mark_sent(status: &serde_json::Value) -> bool {
    status.get("remote").is_none()
        && status.get("state").and_then(|value| value.as_str()) == Some("unknown")
        && status.get("spooler_job").is_some_and(|value| value.is_null())
}

fn saved_callback(state: &AppState, status: &CallbackStatus) -> Result<Callback, String> {
    let id = status
        .token_id
        .as_deref()
        .ok_or("This callback predates restart recovery")?;
    let token = state
        .config
        .get()
        .tokens
        .into_iter()
        .find(|token| token.id == id)
        .ok_or("The API token used by this job was revoked")?;
    Ok(Callback {
        url: status.url.clone(),
        reference: status.reference.clone(),
        token_id: token.id,
        key: token.hash,
    })
}

fn result_payload(state: &AppState, entry: &HistoryEntry, callback: &Callback, result: &str) -> Payload {
    let success = match result {
        "completed" => Some(true),
        "sent" | "gone" | "timeout" => None,
        _ => Some(false),
    };
    Payload {
        event: "job.finished",
        job_id: entry.id.clone(),
        reference: callback.reference.clone(),
        success,
        state: result.into(),
        message: match result {
            "gone" => Some("Job left the queue; print outcome is unknown".into()),
            "sent" => Some("Sent to the printer; the spooler did not report the job status".into()),
            "timeout" => Some("No final spooler result within 60 minutes".into()),
            _ => None,
        },
        printer: entry.printer.clone(),
        preset: entry.preset.clone(),
        sheets: entry.sheets,
        time: state.clock.now_rfc3339(),
    }
}

fn deliver_saved(state: Arc<AppState>, entry: HistoryEntry, status: CallbackStatus, callback: Callback) {
    let Some(result) = status.result.clone() else { return };
    let payload = result_payload(&state, &entry, &callback, &result);
    let initial = CallbackStatus {
        state: "retrying".into(),
        attempts: 0,
        error: None,
        time: state.clock.now_rfc3339(),
        ..status.clone()
    };
    let _ = state.history.append_callback(&entry.id, initial);
    std::thread::spawn(move || {
        state
            .callback_sender
            .deliver(&callback, &payload, &mut |delivery_state, attempts, outcome| {
                let (http_status, error) = match outcome {
                    Attempt::Delivered(code) => (Some(*code), None),
                    Attempt::Failed { status, error } => (*status, Some(error.clone())),
                };
                let update = CallbackStatus {
                    state: delivery_state.into(),
                    result: Some(result.clone()),
                    attempts,
                    http_status,
                    error,
                    time: state.clock.now_rfc3339(),
                    ..status.clone()
                };
                let _ = state.history.append_callback(&entry.id, update);
            });
    });
}

/// Resumes callbacks that were watching a spooler or retrying delivery when
/// the process stopped. The receiver may see a duplicate and should dedupe it.
pub fn resume_pending(state: Arc<AppState>) {
    for day in state.history.days() {
        let mut page = 1;
        while let Ok(batch) = state.history.read_page(&day.date, page, 200, false) {
            for entry in batch.entries {
                let Some(status) = entry.callback.clone() else { continue };
                if !matches!(status.state.as_str(), "watching" | "retrying") {
                    continue;
                }
                if status.state == "retrying" && status.result.is_none() {
                    let update = CallbackStatus {
                        state: "refused".into(),
                        error: Some("Saved callback has no result to deliver".into()),
                        time: state.clock.now_rfc3339(),
                        ..status
                    };
                    let _ = state.history.append_callback(&entry.id, update);
                    continue;
                }
                match saved_callback(&state, &status) {
                    Ok(callback) if status.state == "retrying" => deliver_saved(state.clone(), entry, status, callback),
                    Ok(callback) => {
                        let elapsed = chrono::DateTime::parse_from_rfc3339(&entry.time)
                            .ok()
                            .and_then(|started| {
                                (chrono::Utc::now() - started.with_timezone(&chrono::Utc)).to_std().ok()
                            })
                            .unwrap_or(Duration::ZERO);
                        let job = JobResult {
                            job_id: entry.id,
                            printer: entry.printer,
                            preset: entry.preset,
                            sheets: entry.sheets,
                            spooler_job: entry.spooler_job,
                        };
                        let _ = watch_with_elapsed(state.clone(), &job, callback, elapsed);
                    }
                    Err(message) => {
                        let update = CallbackStatus {
                            state: "refused".into(),
                            error: Some(message),
                            time: state.clock.now_rfc3339(),
                            ..status
                        };
                        let _ = state.history.append_callback(&entry.id, update);
                    }
                }
            }
            if page >= batch.pages {
                break;
            }
            page += 1;
        }
    }
}

/// Retries notification delivery only; it never submits another print job.
pub fn retry_failed(state: Arc<AppState>, job_id: &str) -> Result<(), String> {
    let entry = state.history.find(job_id).ok_or("Job was not found")?;
    let status = entry.callback.clone().ok_or("This job has no callback")?;
    if status.state != "failed" || status.result.is_none() {
        return Err("Only a failed, finished callback can be retried".into());
    }
    let callback = saved_callback(&state, &status)?;
    deliver_saved(state, entry, status, callback);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::CallbackSender;
    use crate::application::state::test_support::state;
    use std::sync::mpsc;

    struct CaptureSender(mpsc::Sender<String>);
    impl CallbackSender for CaptureSender {
        fn deliver(&self, _callback: &Callback, payload: &Payload, progress: &mut dyn FnMut(&str, u32, &Attempt)) {
            progress("delivered", 1, &Attempt::Delivered(200));
            self.0.send(payload.job_id.clone()).unwrap();
        }
    }

    #[test]
    fn manual_retry_delivers_saved_result_without_printing() {
        let mut app = Arc::try_unwrap(state()).ok().unwrap();
        let (_plain, token) = crate::domain::auth::generate("integration").unwrap();
        app.config
            .update(&mut |config| {
                config.tokens.push(token.clone());
                Ok(())
            })
            .unwrap();
        let (sender, receiver) = mpsc::channel();
        app.callback_sender = Arc::new(CaptureSender(sender));
        let app = Arc::new(app);
        let entry = HistoryEntry {
            id: "job-1".into(),
            time: chrono::Local::now().to_rfc3339(),
            source: "http".into(),
            client: None,
            token: Some("integration".into()),
            file_name: "label".into(),
            preset: None,
            printer: "Printer".into(),
            spooler_job: Some("spool-1".into()),
            sheets: 1,
            settings: serde_json::json!({}),
            success: true,
            error: None,
            duration_ms: 1,
            callback: None,
            remote: None,
        };
        app.history.append(&entry).unwrap();
        app.history
            .append_callback(
                &entry.id,
                CallbackStatus {
                    url: "https://example.com/callback".into(),
                    token_id: Some(token.id),
                    reference: None,
                    state: "failed".into(),
                    result: Some("completed".into()),
                    attempts: 4,
                    http_status: Some(500),
                    error: Some("receiver unavailable".into()),
                    time: chrono::Local::now().to_rfc3339(),
                },
            )
            .unwrap();
        retry_failed(app.clone(), &entry.id).unwrap();
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), entry.id);
        assert_eq!(
            app.history.find(&entry.id).unwrap().callback.unwrap().state,
            "delivered"
        );
        assert!(retry_failed(app, &entry.id).is_err());
    }
}
