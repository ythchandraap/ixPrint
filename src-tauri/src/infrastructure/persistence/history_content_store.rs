use crate::domain::history::{HistoryEntry, HistoryPdfKind};
use crate::shared::sync::LockExt;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const INPUT_JSON: &str = "input.json";
const INPUT_PDF: &str = "input.pdf";
const OUTPUT_PDF: &str = "output.pdf";

pub struct HistoryContentStore {
    root: PathBuf,
    active_jobs: Mutex<HashSet<String>>,
}

impl HistoryContentStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            active_jobs: Mutex::new(HashSet::new()),
        }
    }

    pub fn job_started(&self, job_id: &str) {
        self.active_jobs.lock_safe().insert(job_id.to_string());
    }

    pub fn job_finished(&self, job_id: &str) {
        self.active_jobs.lock_safe().remove(job_id);
    }

    pub fn save_json(&self, entry: &HistoryEntry, value: &serde_json::Value) -> Result<(), String> {
        let _active = self.active_jobs.lock_safe();
        let dir = self.job_dir(entry, true)?;
        self.write_atomic(&dir, INPUT_JSON, |file| {
            serde_json::to_writer_pretty(file, value).map_err(io::Error::other)
        })
    }

    pub fn save_pdf(&self, entry: &HistoryEntry, kind: HistoryPdfKind, source: &Path) -> Result<(), String> {
        let _active = self.active_jobs.lock_safe();
        let dir = self.job_dir(entry, true)?;
        let name = match kind {
            HistoryPdfKind::Input => INPUT_PDF,
            HistoryPdfKind::Output => OUTPUT_PDF,
        };
        self.write_atomic(&dir, name, |target| {
            let mut source = File::open(source)?;
            io::copy(&mut source, target)?;
            Ok(())
        })
    }

    pub fn read_json(&self, entry: &HistoryEntry) -> Result<Option<serde_json::Value>, String> {
        let _active = self.active_jobs.lock_safe();
        let path = self.file_path(entry, INPUT_JSON)?;
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Saved JSON content is unreadable".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("Saved JSON content is unreadable".into()),
        }
    }

    pub fn pdf_path(&self, entry: &HistoryEntry, kind: HistoryPdfKind) -> Result<Option<PathBuf>, String> {
        let _active = self.active_jobs.lock_safe();
        let name = match kind {
            HistoryPdfKind::Input => INPUT_PDF,
            HistoryPdfKind::Output => OUTPUT_PDF,
        };
        let path = self.file_path(entry, name)?;
        match fs::canonicalize(&path) {
            Ok(path) if self.is_within_root(&path)? => Ok(Some(path)),
            Ok(_) => Err("Saved PDF content is outside the History content directory".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("Saved PDF content is unreadable".into()),
        }
    }

    pub fn prune(&self, cutoff: &str, retained: &HashSet<(String, String)>) {
        let active = self.active_jobs.lock_safe();
        let Ok(root) = fs::canonicalize(&self.root) else {
            return;
        };
        let Ok(days) = fs::read_dir(&root) else {
            return;
        };
        for day in days.filter_map(Result::ok) {
            let day_path = day.path();
            let Ok(day_metadata) = fs::symlink_metadata(&day_path) else {
                continue;
            };
            if day_metadata.file_type().is_symlink() || !day_metadata.is_dir() {
                let _ = fs::remove_file(&day_path);
                continue;
            }
            let Some(date) = day.file_name().to_str().map(str::to_owned) else {
                let _ = fs::remove_dir_all(day_path);
                continue;
            };
            let Ok(jobs) = fs::read_dir(&day_path) else {
                let _ = fs::remove_file(&day_path);
                continue;
            };
            for job in jobs.filter_map(Result::ok) {
                let job_path = job.path();
                let job_id = job.file_name().to_string_lossy().into_owned();
                let Ok(job_metadata) = fs::symlink_metadata(&job_path) else {
                    continue;
                };
                if job_metadata.file_type().is_symlink() || !job_metadata.is_dir() {
                    let _ = fs::remove_file(&job_path);
                    continue;
                }
                if active.contains(&job_id) {
                    continue;
                }
                if date.as_str() < cutoff || !retained.contains(&(date.clone(), job_id.clone())) {
                    let _ = fs::remove_dir_all(job_path);
                } else {
                    Self::remove_partials(&job_path);
                }
            }
            if fs::read_dir(&day_path).is_ok_and(|mut jobs| jobs.next().is_none()) {
                let _ = fs::remove_dir(day_path);
            }
        }
    }

    fn remove_partials(job_dir: &Path) {
        let Ok(files) = fs::read_dir(job_dir) else {
            return;
        };
        for file in files.filter_map(Result::ok) {
            if !matches!(file.file_name().to_str(), Some(INPUT_JSON | INPUT_PDF | OUTPUT_PDF)) {
                let path = file.path();
                if path.is_dir() {
                    let _ = fs::remove_dir_all(path);
                } else {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }

    fn write_atomic(
        &self,
        dir: &Path,
        name: &str,
        write: impl FnOnce(&mut File) -> io::Result<()>,
    ) -> Result<(), String> {
        let temp = dir.join(format!(".{}.tmp-{}", name, uuid::Uuid::new_v4()));
        let destination = dir.join(name);
        let result = (|| {
            let mut file = private_file(&temp)?;
            write(&mut file)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &destination)?;
            Ok::<_, io::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result.map_err(|_| "Could not save History content".into())
    }

    fn job_dir(&self, entry: &HistoryEntry, create: bool) -> Result<PathBuf, String> {
        let date = entry.time.get(..10).ok_or("Invalid History date")?;
        if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() || uuid::Uuid::parse_str(&entry.id).is_err() {
            return Err("Invalid History content identifier".into());
        }
        if create {
            fs::create_dir_all(&self.root).map_err(|_| "Could not save History content")?;
            private_dir(&self.root)?;
        }
        let root = match fs::canonicalize(&self.root) {
            Ok(root) => root,
            Err(error) if !create && error.kind() == io::ErrorKind::NotFound => {
                return Ok(self.root.join(date).join(&entry.id));
            }
            Err(_) => return Err("History content is unavailable".into()),
        };
        let day = root.join(date);
        if !ensure_directory(&day, create)? {
            return Ok(day.join(&entry.id));
        }
        let day = fs::canonicalize(&day).map_err(|_| "History content is unavailable")?;
        if !day.starts_with(&root) {
            return Err("History content is outside its directory".into());
        }
        let job = day.join(&entry.id);
        if !ensure_directory(&job, create)? {
            return Ok(job);
        }
        let canonical = fs::canonicalize(&job).map_err(|_| "History content is unavailable")?;
        if !canonical.starts_with(root) {
            return Err("History content is outside its directory".into());
        }
        Ok(canonical)
    }

    fn file_path(&self, entry: &HistoryEntry, name: &str) -> Result<PathBuf, String> {
        let dir = self.job_dir(entry, false)?;
        let path = dir.join(name);
        match fs::canonicalize(&path) {
            Ok(path) if self.is_within_root(&path)? => Ok(path),
            Ok(_) => Err("History content is outside its directory".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path),
            Err(_) => Err("History content is unreadable".into()),
        }
    }

    fn is_within_root(&self, path: &Path) -> Result<bool, String> {
        let root = fs::canonicalize(&self.root).map_err(|_| "History content is unavailable")?;
        Ok(path.starts_with(root))
    }
}

fn ensure_directory(path: &Path, create: bool) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err("History content path is not a directory".into()),
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err("History content path is not a directory".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound && !create => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|_| "Could not save History content")?;
            private_dir(path)?;
            Ok(true)
        }
        Err(_) => Err("History content is unavailable".into()),
    }
}

fn private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn private_dir(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700))
            .map_err(|_| String::from("Could not protect History content"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
