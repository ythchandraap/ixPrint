use super::*;
use crate::domain::history::HistoryEntry;
use crate::infrastructure::persistence::history_store::History;

#[test]
fn reservation_replays_matching_request_and_rejects_changed_content() {
    let dir = std::env::temp_dir().join(format!("ixprint-idempotency-{}", uuid::Uuid::new_v4()));
    let store = IdempotencyStore::new(dir.clone());
    let key = "a".repeat(64);
    assert!(matches!(
        store.reserve(&key, "content-a", "job-a"),
        Ok(Reservation::New(id)) if id == "job-a"
    ));
    assert!(matches!(
        store.reserve(&key, "content-a", "job-b"),
        Ok(Reservation::Pending(Some(id))) if id == "job-a"
    ));
    store.mark_finished("job-a").unwrap();
    assert!(matches!(
        store.reserve(&key, "content-a", "job-b"),
        Ok(Reservation::Existing(id, OperationState::Finished)) if id == "job-a"
    ));
    assert!(matches!(
        store.reserve(&key, "content-b", "job-b"),
        Ok(Reservation::Conflict)
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn reserved_operation_resumes_with_its_original_job_id_after_restart() {
    let dir = std::env::temp_dir().join(format!("ixprint-reserved-recovery-{}", uuid::Uuid::new_v4()));
    let key = "b".repeat(64);
    let first = IdempotencyStore::new(dir.clone());
    assert!(matches!(
        first.reserve(&key, "fingerprint", "job-before-crash"),
        Ok(Reservation::New(_))
    ));
    drop(first);

    let restarted = IdempotencyStore::new(dir.clone());
    let resumed = restarted.reserve(&key, "fingerprint", "new-random-id").unwrap();
    assert!(matches!(resumed, Reservation::Resume(id) if id == "job-before-crash"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn submitting_without_recoverable_history_becomes_unknown_after_restart() {
    let dir = std::env::temp_dir().join(format!("ixprint-submitting-recovery-{}", uuid::Uuid::new_v4()));
    let history_dir = dir.join("history");
    let key = "c".repeat(64);
    let first = IdempotencyStore::new(dir.join("idempotency"));
    assert!(matches!(
        first.reserve(&key, "fingerprint", "job-in-flight"),
        Ok(Reservation::New(_))
    ));
    first.mark_submitting("job-in-flight").unwrap();
    drop(first);

    let restarted = IdempotencyStore::new(dir.join("idempotency"));
    let history = History::new(history_dir);
    restarted.recover(&history).unwrap();
    assert!(matches!(
        restarted.reserve(&key, "fingerprint", "new-random-id"),
        Ok(Reservation::Existing(id, OperationState::Unknown)) if id == "job-in-flight"
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn history_written_before_crash_is_recovered_as_finished() {
    let dir = std::env::temp_dir().join(format!("ixprint-finished-recovery-{}", uuid::Uuid::new_v4()));
    let idempotency_dir = dir.join("idempotency");
    let history = History::new(dir.join("history"));
    let key = "d".repeat(64);
    let first = IdempotencyStore::new(idempotency_dir.clone());
    assert!(matches!(
        first.reserve(&key, "fingerprint", "job-finished"),
        Ok(Reservation::New(_))
    ));
    first.mark_submitting("job-finished").unwrap();
    history
        .append(&HistoryEntry {
            id: "job-finished".into(),
            time: "2026-09-26T12:00:00+07:00".into(),
            source: "http".into(),
            client: None,
            token: None,
            file_name: "sample.pdf".into(),
            preset: None,
            printer: "Office".into(),
            spooler_job: Some("spooler-1".into()),
            sheets: 1,
            settings: serde_json::json!({}),
            success: true,
            error: None,
            error_code: None,
            duration_ms: 10,
            callback: None,
            remote: None,
            content: None,
        })
        .unwrap();
    drop(first);

    let restarted = IdempotencyStore::new(idempotency_dir);
    restarted.recover(&history).unwrap();
    assert!(matches!(
        restarted.reserve(&key, "fingerprint", "new-random-id"),
        Ok(Reservation::Existing(id, OperationState::Finished)) if id == "job-finished"
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn unknown_history_result_stays_unknown_after_restart() {
    let dir = std::env::temp_dir().join(format!("ixprint-unknown-recovery-{}", uuid::Uuid::new_v4()));
    let idempotency_dir = dir.join("idempotency");
    let history = History::new(dir.join("history"));
    let key = "e".repeat(64);
    let first = IdempotencyStore::new(idempotency_dir.clone());
    first.reserve(&key, "fingerprint", "job-unknown").unwrap();
    first.mark_submitting("job-unknown").unwrap();
    first.mark_unknown("job-unknown").unwrap();
    history
        .append(&HistoryEntry {
            id: "job-unknown".into(),
            time: "2026-09-26T12:00:00+07:00".into(),
            source: "http".into(),
            client: None,
            token: None,
            file_name: "sample.pdf".into(),
            preset: None,
            printer: "Office".into(),
            spooler_job: None,
            sheets: 0,
            settings: serde_json::json!({}),
            success: false,
            error: Some("The print outcome is not known".into()),
            error_code: Some("outcome_unknown".into()),
            duration_ms: 10,
            callback: None,
            remote: None,
            content: None,
        })
        .unwrap();
    drop(first);

    let restarted = IdempotencyStore::new(idempotency_dir);
    restarted.recover(&history).unwrap();
    assert!(matches!(
        restarted.reserve(&key, "fingerprint", "new-random-id"),
        Ok(Reservation::Existing(id, OperationState::Unknown)) if id == "job-unknown"
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn expired_unknown_reservation_is_never_pruned_or_reused() {
    let dir = std::env::temp_dir().join(format!("ixprint-expired-unknown-{}", uuid::Uuid::new_v4()));
    let store = IdempotencyStore::new(dir.clone());
    let key = "f".repeat(64);
    store.reserve(&key, "fingerprint", "job-unknown").unwrap();
    store.mark_submitting("job-unknown").unwrap();
    store.mark_unknown("job-unknown").unwrap();

    let path = dir.join(format!("{key}.json"));
    let old = std::time::SystemTime::now() - RETENTION - Duration::from_secs(1);
    fs::File::open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();

    store.prune();
    assert!(path.exists(), "ambiguous reservation must remain on disk");
    assert!(matches!(
        store.reserve(&key, "fingerprint", "new-job"),
        Ok(Reservation::Existing(id, OperationState::Unknown)) if id == "job-unknown"
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn expired_finished_reservation_is_removed_and_can_start_a_new_logical_job() {
    let dir = std::env::temp_dir().join(format!("ixprint-expired-finished-{}", uuid::Uuid::new_v4()));
    let store = IdempotencyStore::new(dir.clone());
    let key = "1".repeat(64);
    store.reserve(&key, "fingerprint", "job-finished").unwrap();
    store.mark_finished("job-finished").unwrap();

    let path = dir.join(format!("{key}.json"));
    let old = std::time::SystemTime::now() - RETENTION - Duration::from_secs(1);
    fs::File::open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();

    assert!(matches!(
        store.reserve(&key, "fingerprint", "new-job"),
        Ok(Reservation::New(id)) if id == "new-job"
    ));
    let _ = fs::remove_dir_all(dir);
}
