use super::*;

fn entry(time: &str, success: bool) -> HistoryEntry {
    HistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        time: time.into(),
        source: "http".into(),
        client: None,
        token: None,
        file_name: "a.pdf".into(),
        preset: None,
        printer: "P".into(),
        spooler_job: None,
        sheets: 1,
        settings: serde_json::json!({}),
        success,
        error: None,
        duration_ms: 1,
        callback: None,
        remote: None,
        content: None,
    }
}

#[test]
fn merges_latest_callback_status() {
    let dir = std::env::temp_dir().join(format!("ixprint-hist-{}", uuid::Uuid::new_v4()));
    let h = History::new(dir.clone());
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let job = entry(&format!("{}T08:00:00+07:00", today), true);
    h.append(&job).unwrap();
    let status = |state: &str, attempts| CallbackStatus {
        token_id: None,
        url: "https://a.id/cb".into(),
        reference: Some("R1".into()),
        state: state.into(),
        result: None,
        attempts,
        http_status: None,
        error: None,
        time: String::new(),
    };
    h.append_callback(&job.id, status("watching", 0)).unwrap();
    h.append(&entry(&format!("{}T09:00:00+07:00", today), true)).unwrap();
    h.append_callback(&job.id, status("delivered", 2)).unwrap();

    let day = h.read_day(&today).unwrap();
    assert_eq!(day.len(), 2, "callback lines are not jobs");
    assert!(day[0].callback.is_none());
    assert_eq!(day[1].callback, Some(status("delivered", 2)));
    assert_eq!(h.days()[0].total, 2);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn groups_by_day_and_prunes() {
    let dir = std::env::temp_dir().join(format!("ixprint-hist-{}", uuid::Uuid::new_v4()));
    let h = History::new(dir.clone());
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    h.append(&entry(&format!("{}T08:00:00+07:00", today), true)).unwrap();
    h.append(&entry(&format!("{}T09:00:00+07:00", today), false)).unwrap();
    let mut old = entry("2001-01-01T09:00:00+07:00", true);
    old.content = Some(HistoryContentStatus {
        input_kind: Some(HistoryInputKind::Json),
        input_saved: true,
        output: HistoryOutputState::NotCreated,
    });
    h.append(&old).unwrap();
    h.save_input_json(&old, &serde_json::json!({ "private": "history input" }))
        .unwrap();
    let old_content = dir
        .join("history-content")
        .join("2001-01-01")
        .join(&old.id)
        .join("input.json");
    assert!(old_content.is_file());

    let days = h.days();
    assert_eq!(days[0].date, today);
    assert_eq!((days[0].total, days[0].failed), (2, 1));
    let newest = h.read_day(&today).unwrap()[0].clone();
    assert!(newest.time.contains("09:00"));
    assert_eq!(h.find(&newest.id).map(|e| e.id), Some(newest.id));
    assert!(h.find("nope").is_none());

    let p = h.read_page(&today, 2, 1, false).unwrap();
    assert_eq!((p.page, p.pages, p.total, p.entries.len()), (2, 2, 2, 1));
    assert!(p.entries[0].time.contains("08:00"));
    assert_eq!(h.read_page(&today, 99, 1, false).unwrap().page, 2);
    let f = h.read_page(&today, 1, 10, true).unwrap();
    assert_eq!((f.total, f.failed, f.entries.len()), (1, 1, 1));
    assert!(!f.entries[0].success);

    h.prune(30);
    assert_eq!(h.days().len(), 1);
    assert!(
        !old_content.exists(),
        "content follows the same history retention window"
    );
    assert!(h.read_day("../etc").is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn search_matches_job_id_and_callback_reference_across_days() {
    let dir = std::env::temp_dir().join(format!("ixprint-search-{}", uuid::Uuid::new_v4()));
    let history = History::new(dir.clone());
    let first = entry("2025-01-01T08:00:00+07:00", true);
    let second = entry("2025-01-02T09:00:00+07:00", true);
    history.append(&first).unwrap();
    history.append(&second).unwrap();
    history
        .append_callback(
            &first.id,
            CallbackStatus {
                token_id: None,
                url: "https://example.com/cb".into(),
                reference: Some("ORDER-123".into()),
                state: "delivered".into(),
                result: Some("completed".into()),
                attempts: 1,
                http_status: Some(200),
                error: None,
                time: first.time.clone(),
            },
        )
        .unwrap();
    let by_ref = history.search("order-123", 1, 20).unwrap();
    assert_eq!(by_ref.total, 1);
    assert_eq!(by_ref.entries[0].id, first.id);
    let by_id = history.search(&second.id[..8], 1, 20).unwrap();
    assert_eq!(by_id.total, 1);
    assert_eq!(by_id.entries[0].id, second.id);
    assert!(history.search("a", 1, 20).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cancelled_incoming_jobs_are_not_counted_or_filtered_as_failures() {
    let dir = std::env::temp_dir().join(format!("ixprint-hist-cancel-{}", uuid::Uuid::new_v4()));
    let history = History::new(dir.clone());
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut cancelled = entry(&format!("{}T08:00:00+07:00", today), false);
    cancelled.source = "incoming_cancelled_file_picker".into();
    cancelled.error = Some("Cancelled before printing".into());
    history.append(&cancelled).unwrap();
    let mut failed = entry(&format!("{}T09:00:00+07:00", today), false);
    failed.source = "incoming_file_picker".into();
    failed.error = Some("Printer unavailable".into());
    history.append(&failed).unwrap();

    assert!(cancelled.is_cancelled());
    assert_eq!(history.days()[0].failed, 1);
    let failures = history.read_page(&today, 1, 10, true).unwrap();
    assert_eq!(failures.total, 1);
    assert_eq!(failures.entries[0].id, failed.id);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn prune_removes_orphan_jobs_and_partial_files_but_keeps_retained_content() {
    let dir = std::env::temp_dir().join(format!("ixprint-content-prune-{}", uuid::Uuid::new_v4()));
    let history = History::new(dir.clone());
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut retained = entry(&format!("{}T08:00:00+07:00", today), true);
    retained.content = Some(HistoryContentStatus {
        input_kind: Some(HistoryInputKind::Json),
        input_saved: true,
        output: HistoryOutputState::NotCreated,
    });
    history.append(&retained).unwrap();
    history
        .save_input_json(&retained, &serde_json::json!({ "retained": true }))
        .unwrap();
    let job_dir = dir.join("history-content").join(&today).join(&retained.id);
    let partial = job_dir.join(".input.json.tmp-crash");
    fs::write(&partial, b"partial").unwrap();
    let orphan = dir
        .join("history-content")
        .join(&today)
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join(".output.pdf.tmp-crash"), b"partial").unwrap();

    history.prune(90);
    assert!(job_dir.join("input.json").is_file());
    assert!(!partial.exists());
    assert!(!orphan.exists());
    let _ = fs::remove_dir_all(dir);
}
