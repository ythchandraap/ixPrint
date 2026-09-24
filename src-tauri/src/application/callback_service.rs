//! Watches submitted jobs and sends their signed result callback.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::application::state::AppState;
use crate::callback::{Attempt, Callback, Payload};
use crate::domain::history::CallbackStatus;
use crate::domain::job::JobResult;

const MAX_ACTIVE: usize = 200;
const POLL_EVERY: Duration = Duration::from_secs(3);
const GIVE_UP_AFTER: Duration = Duration::from_secs(60 * 60);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn watch(state: Arc<AppState>, job: &JobResult, callback: Callback) -> Result<(), String> {
    let record = {
        let (state, job_id, url, reference) = (
            state.clone(),
            job.job_id.clone(),
            callback.url.clone(),
            callback.reference.clone(),
        );
        move |status: &str, result: Option<&str>, attempts: u32, outcome: Option<&Attempt>| {
            let (http_status, error) = match outcome {
                Some(Attempt::Delivered(code)) => (Some(*code), None),
                Some(Attempt::Failed { status, error }) => (*status, Some(error.clone())),
                None => (None, None),
            };
            let entry = CallbackStatus {
                url: url.clone(),
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
                        Some(false),
                        "gone",
                        Some("Job left the queue without completing (it may have been cancelled)".into()),
                    )
                }
                "unknown" if should_mark_sent(&status) => {
                    break payload(
                        "job.finished",
                        Some(true),
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
            if started.elapsed() > GIVE_UP_AFTER {
                let last = text("state").unwrap_or_else(|| "unknown".into());
                break payload(
                    "job.finished",
                    Some(false),
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
