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
    h.append(&entry("2001-01-01T09:00:00+07:00", true)).unwrap();

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
