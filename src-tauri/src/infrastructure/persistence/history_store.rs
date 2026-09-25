use crate::application::ports::HistoryRepository;
use crate::domain::history::*;
use crate::shared::sync::LockExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

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

    /// Looks up a job by ix.print job id across retained history.
    pub fn find(&self, job_id: &str) -> Option<HistoryEntry> {
        let mut dates: Vec<_> = self.day_files().into_iter().map(|(date, _)| date).collect();
        dates.sort_unstable_by(|a, b| b.cmp(a));
        dates
            .into_iter()
            .filter_map(|date| self.read_day(&date).ok())
            .flatten()
            .find(|entry| entry.id == job_id)
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

    pub fn search(&self, query: &str, page: usize, per_page: usize) -> Result<HistoryPage, String> {
        let query = query.trim();
        if query.chars().count() < 3 || query.chars().count() > 128 {
            return Err("Search must contain 3–128 characters".into());
        }
        let mut matches = Vec::new();
        let mut dates: Vec<_> = self.day_files().into_iter().map(|(date, _)| date).collect();
        dates.sort_unstable_by(|a, b| b.cmp(a));
        for date in dates {
            matches.extend(
                self.read_day(&date)?
                    .into_iter()
                    .filter(|entry| entry.matches_query(query)),
            );
        }
        let per_page = per_page.clamp(1, 200);
        let pages = matches.len().div_ceil(per_page).max(1);
        let page = page.clamp(1, pages);
        let total = matches.len();
        let failed = matches.iter().filter(|entry| !entry.success).count();
        Ok(HistoryPage {
            entries: matches.into_iter().skip((page - 1) * per_page).take(per_page).collect(),
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

impl HistoryRepository for History {
    fn append(&self, entry: &HistoryEntry) -> Result<(), String> {
        History::append(self, entry)
    }

    fn append_callback(&self, job_id: &str, status: CallbackStatus) -> Result<(), String> {
        History::append_callback(self, job_id, status)
    }

    fn find(&self, job_id: &str) -> Option<HistoryEntry> {
        History::find(self, job_id)
    }

    fn read_page(&self, date: &str, page: usize, per_page: usize, failed_only: bool) -> Result<HistoryPage, String> {
        History::read_page(self, date, page, per_page, failed_only)
    }

    fn search(&self, query: &str, page: usize, per_page: usize) -> Result<HistoryPage, String> {
        History::search(self, query, page, per_page)
    }

    fn days(&self) -> Vec<HistoryDay> {
        History::days(self)
    }

    fn prune(&self, retention_days: u32) {
        History::prune(self, retention_days)
    }
}

#[cfg(test)]
mod tests;
