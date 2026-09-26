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

    pub fn active_jobs(&self) -> HashSet<String> {
        self.active_jobs.lock_safe().clone()
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

    pub fn prune(&self, retained: &HashSet<(String, String)>) -> Result<(), String> {
        let active = self.active_jobs.lock_safe();
        let root = match fs::canonicalize(&self.root) {
            Ok(root) => root,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("Could not access History content: {}", error)),
        };
        let days = match fs::read_dir(&root) {
            Ok(days) => days,
            Err(error) => return Err(format!("Could not inspect History content: {}", error)),
        };
        let mut failures = Vec::new();
        for day in days {
            let day = match day {
                Ok(day) => day,
                Err(error) => {
                    failures.push(format!("Could not inspect a History content directory: {}", error));
                    continue;
                }
            };
            let day_path = day.path();
            let day_metadata = match fs::symlink_metadata(&day_path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    failures.push(format!("Could not inspect History content: {}", error));
                    continue;
                }
            };
            if day_metadata.file_type().is_symlink() || !day_metadata.is_dir() {
                if let Err(error) = fs::remove_file(&day_path) {
                    failures.push(format!("Could not remove unexpected History content entry: {}", error));
                }
                continue;
            }
            let Some(date) = day.file_name().to_str().map(str::to_owned) else {
                if let Err(error) = fs::remove_dir_all(&day_path) {
                    failures.push(format!("Could not remove unrecognized History content: {}", error));
                }
                continue;
            };
            let jobs = match fs::read_dir(&day_path) {
                Ok(jobs) => jobs,
                Err(error) => {
                    failures.push(format!("Could not inspect History content jobs: {}", error));
                    continue;
                }
            };
            for job in jobs {
                let job = match job {
                    Ok(job) => job,
                    Err(error) => {
                        failures.push(format!("Could not inspect a History content job: {}", error));
                        continue;
                    }
                };
                let job_path = job.path();
                let job_id = job.file_name().to_string_lossy().into_owned();
                let job_metadata = match fs::symlink_metadata(&job_path) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        failures.push(format!("Could not inspect History job content: {}", error));
                        continue;
                    }
                };
                if job_metadata.file_type().is_symlink() || !job_metadata.is_dir() {
                    if let Err(error) = fs::remove_file(&job_path) {
                        failures.push(format!("Could not remove unexpected History job content: {}", error));
                    }
                    continue;
                }
                if active.contains(&job_id) {
                    continue;
                }
                if !retained.contains(&(date.clone(), job_id.clone())) {
                    if let Err(error) = fs::remove_dir_all(&job_path) {
                        failures.push(format!(
                            "Could not remove expired or orphaned History content: {}",
                            error
                        ));
                    }
                } else {
                    if let Err(error) = Self::remove_partials(&job_path) {
                        failures.push(format!("Could not remove partial History content: {}", error));
                    }
                }
            }
            match fs::read_dir(&day_path) {
                Ok(mut jobs) => {
                    if jobs.next().is_none() {
                        if let Err(error) = fs::remove_dir(&day_path) {
                            failures.push(format!("Could not remove empty History content directory: {}", error));
                        }
                    }
                }
                Err(error) => failures.push(format!("Could not inspect History content directory: {}", error)),
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    fn remove_partials(job_dir: &Path) -> Result<(), String> {
        let files = fs::read_dir(job_dir).map_err(|error| error.to_string())?;
        let mut failures = Vec::new();
        for file in files {
            let file = match file {
                Ok(file) => file,
                Err(error) => {
                    failures.push(error.to_string());
                    continue;
                }
            };
            if matches!(file.file_name().to_str(), Some(INPUT_JSON | INPUT_PDF | OUTPUT_PDF)) {
                continue;
            }
            let path = file.path();
            let result = match file.file_type() {
                Ok(file_type) if file_type.is_dir() => fs::remove_dir_all(path),
                Ok(_) => fs::remove_file(path),
                Err(error) => {
                    failures.push(error.to_string());
                    continue;
                }
            };
            if let Err(error) = result {
                failures.push(error.to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
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
