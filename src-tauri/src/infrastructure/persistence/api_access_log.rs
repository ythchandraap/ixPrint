use crate::application::ports::ApiAccessLogRepository;
use crate::domain::api_access::ApiAccessEntry;
use crate::shared::sync::LockExt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 3000;

pub struct ApiAccessLog {
    path: PathBuf,
    lock: Mutex<()>,
}

impl ApiAccessLog {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("api-access-log.jsonl"),
            lock: Mutex::new(()),
        }
    }

    pub fn append(&self, entry: &ApiAccessEntry) -> Result<(), String> {
        let _guard = self.lock.lock_safe();
        fs::create_dir_all(self.path.parent().unwrap_or_else(|| Path::new("."))).map_err(|error| error.to_string())?;
        let mut line = serde_json::to_vec(entry).map_err(|error| error.to_string())?;
        line.push(b'\n');

        let current_size = fs::metadata(&self.path).map(|meta| meta.len()).unwrap_or(0);
        if current_size.saturating_add(line.len() as u64) > MAX_LOG_BYTES {
            let previous = self.path.with_extension("jsonl.1");
            let _ = fs::remove_file(&previous);
            if self.path.exists() {
                fs::rename(&self.path, previous).map_err(|error| error.to_string())?;
            }
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| error.to_string())?;
        file.write_all(&line).map_err(|error| error.to_string())
    }

    pub fn read(&self) -> Result<Vec<ApiAccessEntry>, String> {
        let _guard = self.lock.lock_safe();
        let mut entries = Vec::new();
        for path in [self.path.with_extension("jsonl.1"), self.path.clone()] {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            entries.extend(text.lines().filter_map(|line| serde_json::from_str(line).ok()));
        }
        entries.sort_by(|a: &ApiAccessEntry, b| {
            let left = chrono::DateTime::parse_from_rfc3339(&a.time).ok();
            let right = chrono::DateTime::parse_from_rfc3339(&b.time).ok();
            left.cmp(&right).then_with(|| a.time.cmp(&b.time))
        });
        if entries.len() > MAX_ENTRIES {
            entries.drain(..entries.len() - MAX_ENTRIES);
        }
        Ok(entries)
    }

    pub fn clear(&self) -> Result<(), String> {
        let _guard = self.lock.lock_safe();
        for path in [self.path.clone(), self.path.with_extension("jsonl.1")] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    }
}

impl ApiAccessLogRepository for ApiAccessLog {
    fn append(&self, entry: &ApiAccessEntry) -> Result<(), String> {
        ApiAccessLog::append(self, entry)
    }

    fn read(&self) -> Result<Vec<ApiAccessEntry>, String> {
        ApiAccessLog::read(self)
    }

    fn clear(&self) -> Result<(), String> {
        ApiAccessLog::clear(self)
    }
}
