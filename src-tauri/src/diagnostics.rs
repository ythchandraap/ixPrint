//! Durable, user-visible diagnostics for errors and abnormal process exits.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_LOG_BYTES: u64 = 1_048_576;
const MAX_ENTRIES: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DiagnosticEntry {
    pub time: String,
    pub level: String,
    pub source: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Clone)]
pub struct Diagnostics {
    path: PathBuf,
    session_path: PathBuf,
    lock: Arc<Mutex<()>>,
}

pub struct ActiveJobMarker {
    path: PathBuf,
}

impl Drop for ActiveJobMarker {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl Diagnostics {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("error-log.jsonl"),
            session_path: data_dir.join("active-session"),
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// Starts a durable session marker so the next run can report an unclean exit.
    pub fn start_session(&self) {
        if let Ok(previous) = fs::read_to_string(&self.session_path) {
            let _ = self.record(
                "warning",
                "lifecycle",
                "The previous app session ended without a normal shutdown.",
                Some(previous.trim()),
            );
        }
        self.recover_interrupted_jobs();
        let marker = format!(
            "pid={} started_at={}\n",
            std::process::id(),
            chrono::Local::now().to_rfc3339()
        );
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.session_path)
        {
            let _ = file.write_all(marker.as_bytes());
            let _ = file.sync_all();
        }
    }

    pub fn finish_session(&self) {
        let _ = fs::remove_file(&self.session_path);
    }

    pub fn begin_job(&self, id: &str, source: &str, printer: &str) -> Result<ActiveJobMarker, String> {
        let path = self.path.with_file_name(format!("active-job-{}.json", id));
        let marker = serde_json::json!({
            "job_id": id,
            "source": source,
            "printer": printer,
            "started_at": chrono::Local::now().to_rfc3339(),
        });
        let bytes = serde_json::to_vec(&marker).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let write_result = file.write_all(&bytes);
        let write_result = write_result.and_then(|_| file.sync_all());
        if let Err(e) = write_result {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(e.to_string());
        }
        Ok(ActiveJobMarker { path })
    }

    pub fn install_panic_hook(&self) {
        let path = self.path.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let details = format!("{}\n{}", info, std::backtrace::Backtrace::force_capture());
            let entry = DiagnosticEntry {
                time: chrono::Local::now().to_rfc3339(),
                level: "fatal".into(),
                source: "rust-panic".into(),
                message: "The application encountered an unrecoverable Rust panic.".into(),
                details: Some(details),
            };
            let _ = append_entry(&path, &entry);
            previous(info);
        }));
    }

    pub fn record(
        &self,
        level: &str,
        source: &str,
        message: &str,
        details: Option<&str>,
    ) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let entry = DiagnosticEntry {
            time: chrono::Local::now().to_rfc3339(),
            level: truncate(level, 24),
            source: truncate(source, 96),
            message: truncate(message, 4096),
            details: details.map(|s| truncate(s, 16_384)),
        };
        if fs::metadata(&self.path).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false) {
            let old = self.path.with_extension("jsonl.1");
            let _ = fs::remove_file(&old);
            fs::rename(&self.path, old).map_err(|e| e.to_string())?;
        }
        append_entry(&self.path, &entry)
    }

    pub fn read(&self) -> Result<Vec<DiagnosticEntry>, String> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut entries: Vec<DiagnosticEntry> = Vec::new();
        for path in [self.path.with_extension("jsonl.1"), self.path.clone()] {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.to_string()),
            };
            entries.extend(text.lines().filter_map(|line| serde_json::from_str(line).ok()));
        }
        entries.sort_by(|a: &DiagnosticEntry, b| a.time.cmp(&b.time));
        if entries.len() > MAX_ENTRIES {
            entries.drain(..entries.len() - MAX_ENTRIES);
        }
        Ok(entries)
    }

    pub fn clear(&self) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        for path in [self.path.clone(), self.path.with_extension("jsonl.1")] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }

    fn recover_interrupted_jobs(&self) {
        let Some(dir) = self.path.parent() else { return };
        let Ok(files) = fs::read_dir(dir) else { return };
        for file in files.flatten() {
            let path = file.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !name.starts_with("active-job-") || !name.ends_with(".json") {
                continue;
            }
            let details = fs::read_to_string(&path).unwrap_or_else(|e| format!("Could not read job marker: {}", e));
            let _ = self.record(
                "fatal",
                "print-job",
                "A print job was interrupted by an abnormal app or system shutdown.",
                Some(&details),
            );
            let _ = fs::remove_file(path);
        }
    }
}

fn append_entry(path: &Path, entry: &DiagnosticEntry) -> Result<(), String> {
    let mut line = serde_json::to_vec(entry).map_err(|e| e.to_string())?;
    line.push(b'\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(&line).map_err(|e| e.to_string())?;
    file.sync_data().map_err(|e| e.to_string())
}

fn truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .take_while(|(i, _)| *i < max_bytes)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    format!("{}…", &s[..end])
}
