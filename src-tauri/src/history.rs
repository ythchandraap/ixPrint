//! Print history, grouped per day: `history/YYYY-MM-DD.jsonl`, one JSON object
//! per line. Appending a line never rewrites earlier entries, so a crash can at
//! worst lose the entry being written. A job's callback progress is appended
//! to the same day file as separate `{"callback_for": id, …}` lines; the latest
//! one is merged into the entry when reading.

use crate::sync::LockExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryEntry {
    pub id: String,
    /// RFC 3339, local time.
    pub time: String,
    /// "http" (from a web app) or "app" (test print from ix.print).
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// Name of the API token the request used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub printer: String,
    /// The spooler's job id, for looking up queue position / errors later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spooler_job: Option<String>,
    /// Sheets sent to the printer (all copies).
    pub sheets: u32,
    /// Resolved settings of the job.
    pub settings: serde_json::Value,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
    /// Delivery of the result callback, when the client asked for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback: Option<CallbackStatus>,
    /// Set when the job was forwarded to another ix.print.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteJob>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemoteJob {
    pub remote_id: String,
    pub remote_name: String,
    /// Job id on the other computer.
    pub job_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CallbackStatus {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// watching (job not done yet) | retrying | delivered | failed | refused
    pub state: String,
    /// Job outcome sent in the callback (completed, failed, …), once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default)]
    pub attempts: u32,
    /// Last HTTP status from the receiver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub time: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CallbackLine {
    callback_for: String,
    callback: CallbackStatus,
}

/// A line of a day file: a job, or a later update of its callback.
#[derive(Deserialize)]
#[serde(untagged)]
enum Line {
    Entry(Box<HistoryEntry>),
    Callback(CallbackLine),
}

#[derive(Serialize, Clone, Debug)]
pub struct HistoryDay {
    pub date: String,
    pub total: usize,
    pub failed: usize,
}

/// One page of a day's history, newest first.
#[derive(Serialize, Clone, Debug)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub total: usize,
    pub failed: usize,
    /// 1-based, clamped to the available pages.
    pub page: usize,
    pub pages: usize,
    pub per_page: usize,
}

pub struct History {
    dir: PathBuf,
    lock: Mutex<()>,
}

fn is_date(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

impl History {
    pub fn new(dir: PathBuf) -> History {
        History {
            dir,
            lock: Mutex::new(()),
        }
    }

    fn day_file(&self, date: &str) -> PathBuf {
        self.dir.join(format!("{}.jsonl", date))
    }

    pub fn append(&self, entry: &HistoryEntry) -> Result<(), String> {
        let date = entry.time.get(..10).unwrap_or_default().to_string();
        let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        self.append_line(&date, &line)
    }

    /// Records the callback progress of a job, in the day file of that job.
    pub fn append_callback(&self, job_id: &str, status: CallbackStatus) -> Result<(), String> {
        let date = self
            .find(job_id)
            .and_then(|e| e.time.get(..10).map(str::to_string))
            .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
        let line = serde_json::to_string(&CallbackLine {
            callback_for: job_id.to_string(),
            callback: status,
        })
        .map_err(|e| e.to_string())?;
        self.append_line(&date, &line)
    }

    fn append_line(&self, date: &str, line: &str) -> Result<(), String> {
        let _guard = self.lock.lock_safe();
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.day_file(date))
            .map_err(|e| e.to_string())?;
        writeln!(file, "{}", line).map_err(|e| e.to_string())
    }

    /// Entries of one day, newest first. Damaged lines are skipped.
    pub fn read_day(&self, date: &str) -> Result<Vec<HistoryEntry>, String> {
        if !is_date(date) {
            return Err(format!("Date '{}' is invalid (YYYY-MM-DD)", date));
        }
        let _guard = self.lock.lock_safe();
        let text = match fs::read_to_string(self.day_file(date)) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.to_string()),
        };
        let mut entries = Vec::new();
        let mut callbacks = std::collections::HashMap::new();
        for line in text.lines() {
            match serde_json::from_str(line) {
                Ok(Line::Entry(e)) => entries.push(*e),
                Ok(Line::Callback(c)) => {
                    callbacks.insert(c.callback_for, c.callback);
                }
                Err(_) => {}
            }
        }
        for e in &mut entries {
            if let Some(c) = callbacks.remove(&e.id) {
                e.callback = Some(c);
            }
        }
        entries.reverse();
        Ok(entries)
    }

    /// Looks up a job by ix.print job id in today's and yesterday's history.
    pub fn find(&self, job_id: &str) -> Option<HistoryEntry> {
        let today = chrono::Local::now().date_naive();
        [today, today - chrono::Duration::days(1)]
            .iter()
            .filter_map(|d| self.read_day(&d.format("%Y-%m-%d").to_string()).ok())
            .flatten()
            .find(|e| e.id == job_id)
    }

    /// `per_page` entries of `date`; `failed_only` keeps only failed jobs.
    pub fn read_page(
        &self,
        date: &str,
        page: usize,
        per_page: usize,
        failed_only: bool,
    ) -> Result<HistoryPage, String> {
        let all = self.read_day(date)?;
        let failed = all.iter().filter(|e| !e.success).count();
        let list: Vec<HistoryEntry> = if failed_only {
            all.into_iter().filter(|e| !e.success).collect()
        } else {
            all
        };
        let per_page = per_page.clamp(1, 200);
        let pages = list.len().div_ceil(per_page).max(1);
        let page = page.clamp(1, pages);
        let total = list.len();
        let entries = list.into_iter().skip((page - 1) * per_page).take(per_page).collect();
        Ok(HistoryPage {
            entries,
            total,
            failed,
            page,
            pages,
            per_page,
        })
    }

    /// Days that have history, newest first.
    pub fn days(&self) -> Vec<HistoryDay> {
        let mut dates: Vec<String> = self.day_files().into_iter().map(|(date, _)| date).collect();
        dates.sort_unstable_by(|a, b| b.cmp(a));
        dates
            .into_iter()
            .map(|date| {
                let entries = self.read_day(&date).unwrap_or_default();
                HistoryDay {
                    failed: entries.iter().filter(|e| !e.success).count(),
                    total: entries.len(),
                    date,
                }
            })
            .collect()
    }

    /// Deletes day files older than `retention_days`.
    pub fn prune(&self, retention_days: u32) {
        let cutoff = (chrono::Local::now().date_naive() - chrono::Duration::days(retention_days as i64))
            .format("%Y-%m-%d")
            .to_string();
        let _guard = self.lock.lock_safe();
        for (date, path) in self.day_files() {
            if date < cutoff {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn day_files(&self) -> Vec<(String, PathBuf)> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return vec![];
        };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                let stem = path.file_stem()?.to_str()?.to_string();
                (path.extension()? == "jsonl" && is_date(&stem)).then_some((stem, path))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
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
}
