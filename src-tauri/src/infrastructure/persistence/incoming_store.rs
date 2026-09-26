use crate::application::ports::IncomingDocumentRepository;
use crate::domain::incoming_job::IncomingJob;
use crate::shared::sync::LockExt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct IncomingStore {
    root: PathBuf,
    lock: Mutex<()>,
}

impl IncomingStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            lock: Mutex::new(()),
        }
    }

    fn metadata_path(&self, id: &str) -> Result<PathBuf, String> {
        validate_id(id)?;
        Ok(self.root.join("jobs").join(format!("{}.json", id)))
    }

    fn document_file(&self, id: &str) -> Result<PathBuf, String> {
        validate_id(id)?;
        Ok(self.root.join("documents").join(format!("{}.pdf", id)))
    }

    fn write_metadata(&self, job: &IncomingJob) -> Result<(), String> {
        let path = self.metadata_path(&job.id)?;
        let directory = path.parent().ok_or("Incoming metadata directory is invalid")?;
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        set_private_dir(directory)?;
        let temp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(job).map_err(|e| e.to_string())?;
        write_private_file(&temp, &bytes)?;
        fs::rename(temp, path).map_err(|e| e.to_string())
    }
}

impl IncomingDocumentRepository for IncomingStore {
    fn canonical_file(&self, path: &Path, max_bytes: u64) -> Result<PathBuf, String> {
        let path = path
            .canonicalize()
            .map_err(|_| "The selected file is no longer available".to_string())?;
        let metadata = fs::metadata(&path).map_err(|_| "The selected file is no longer available")?;
        if !metadata.is_file() {
            return Err("Choose a regular file".into());
        }
        if metadata.len() == 0 || metadata.len() > max_bytes {
            return Err(format!(
                "File must be between 1 byte and {} MB",
                max_bytes / (1024 * 1024)
            ));
        }
        Ok(path)
    }

    fn staging_path(&self, id: &str) -> Result<PathBuf, String> {
        validate_id(id)?;
        let directory = self.root.join("staging");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        set_private_dir(&directory)?;
        Ok(directory.join(format!("{}.pdf", id)))
    }

    fn copy_to_staging(&self, id: &str, source: &Path) -> Result<PathBuf, String> {
        let source = self.canonical_file(source, 200 * 1024 * 1024)?;
        let destination = self.staging_path(id)?;
        fs::copy(source, &destination).map_err(|_| "Could not copy the selected document".to_string())?;
        set_private_file(&destination)?;
        Ok(destination)
    }

    fn discard_staging(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        let path = self.root.join("staging").join(format!("{}.pdf", id));
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn create(&self, job: IncomingJob, source: &Path) -> Result<(IncomingJob, bool), String> {
        job.validate()?;
        let _guard = self.lock.lock_safe();
        fs::create_dir_all(self.root.join("jobs")).map_err(|e| e.to_string())?;
        fs::create_dir_all(self.root.join("documents")).map_err(|e| e.to_string())?;
        set_private_dir(&self.root)?;
        set_private_dir(&self.root.join("jobs"))?;
        set_private_dir(&self.root.join("documents"))?;

        if let Some(os_job_id) = job.os_job_id.as_deref() {
            if let Some(existing) = self
                .list_unlocked()?
                .into_iter()
                .find(|existing| existing.source == job.source && existing.os_job_id.as_deref() == Some(os_job_id))
            {
                return Ok((existing, false));
            }
        }

        let destination = self.document_file(&job.id)?;
        let temp = destination.with_extension("pdf.tmp");
        fs::copy(source, &temp).map_err(|e| format!("Could not stage document: {}", e))?;
        set_private_file(&temp)?;
        if let Err(error) = fs::rename(&temp, &destination) {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
        if let Err(error) = self.write_metadata(&job) {
            let _ = fs::remove_file(destination);
            return Err(error);
        }
        Ok((job, true))
    }

    fn list(&self) -> Result<Vec<IncomingJob>, String> {
        let _guard = self.lock.lock_safe();
        self.list_unlocked()
    }

    fn get(&self, id: &str) -> Result<Option<IncomingJob>, String> {
        let _guard = self.lock.lock_safe();
        let path = self.metadata_path(id)?;
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|e| e.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn document_path(&self, id: &str) -> Result<PathBuf, String> {
        let path = self.document_file(id)?;
        if path.is_file() {
            Ok(path)
        } else {
            Err("The staged document is no longer available".into())
        }
    }

    fn save(&self, job: &IncomingJob) -> Result<(), String> {
        job.validate()?;
        let _guard = self.lock.lock_safe();
        self.write_metadata(job)
    }

    fn claim_for_submission(&self, id: &str, printer: &str) -> Result<bool, String> {
        let _guard = self.lock.lock_safe();
        let path = self.metadata_path(id)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let mut job: IncomingJob = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if job.state != crate::domain::incoming_job::IncomingState::AwaitingDestination {
            return Ok(false);
        }
        job.state = crate::domain::incoming_job::IncomingState::Submitting;
        job.printer = Some(printer.to_string());
        job.submission_started_at = Some(chrono::Utc::now().to_rfc3339());
        job.message = None;
        self.write_metadata(&job)?;
        Ok(true)
    }

    fn remove_document(&self, id: &str) -> Result<(), String> {
        let _guard = self.lock.lock_safe();
        let path = self.document_file(id)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn prune(&self, retention_days: u32) -> Result<(), String> {
        let _guard = self.lock.lock_safe();
        let staging = self.root.join("staging");
        if let Ok(entries) = fs::read_dir(&staging) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) == Some("pdf") {
                    let _ = fs::remove_file(path);
                }
            }
        }
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::days(retention_days as i64);
        for mut job in self.list_unlocked()? {
            if job.state == crate::domain::incoming_job::IncomingState::Submitting {
                let stale = job
                    .submission_started_at
                    .as_deref()
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .is_some_and(|at| at.with_timezone(&chrono::Utc) < now - chrono::Duration::minutes(10));
                if stale {
                    job.state = crate::domain::incoming_job::IncomingState::Failed;
                    job.submission_started_at = None;
                    job.message = Some("Submission outcome could not be confirmed after restart. Check History and the printer queue before sending this document again.".into());
                    self.write_metadata(&job)?;
                } else {
                    continue;
                }
            }
            let expired = chrono::DateTime::parse_from_rfc3339(&job.received_at)
                .ok()
                .is_some_and(|at| at.with_timezone(&chrono::Utc) < cutoff);
            if expired {
                let _ = fs::remove_file(self.document_file(&job.id)?);
                let _ = fs::remove_file(self.metadata_path(&job.id)?);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

impl IncomingStore {
    fn list_unlocked(&self) -> Result<Vec<IncomingJob>, String> {
        let dir = self.root.join("jobs");
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let mut jobs = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(entry.path()).map_err(|e| e.to_string())?;
            jobs.push(serde_json::from_slice::<IncomingJob>(&bytes).map_err(|e| e.to_string())?);
        }
        jobs.sort_by(|a, b| b.received_at.cmp(&a.received_at));
        Ok(jobs)
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "Invalid incoming job ID".into())
}

fn set_private_dir(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn set_private_file(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    set_private_file(path)?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}
