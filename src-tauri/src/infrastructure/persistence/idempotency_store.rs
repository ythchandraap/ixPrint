use crate::application::ports::{HistoryRepository, IdempotencyRepository, OperationState, Reservation};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Serialize, Deserialize)]
struct Entry {
    fingerprint: String,
    job_id: String,
    #[serde(default = "legacy_state")]
    state: OperationState,
}

fn legacy_state() -> OperationState {
    // Old reservation files did not say whether submission had started.
    OperationState::Unknown
}

pub struct IdempotencyStore {
    dir: PathBuf,
    lock: Mutex<()>,
    active: Mutex<HashSet<String>>,
}

impl IdempotencyStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            lock: Mutex::new(()),
            active: Mutex::new(HashSet::new()),
        }
    }

    fn ensure_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn read_entry(path: &std::path::Path) -> Result<Entry, String> {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())
    }

    fn write_entry(path: &std::path::Path, entry: &Entry) -> Result<(), String> {
        let parent = path.parent().ok_or("Idempotency entry has no parent directory")?;
        let temp = parent.join(format!(".{}.{}.tmp", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), String> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|error| error.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&temp, fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())?;
            }
            file.write_all(&serde_json::to_vec(entry).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            fs::rename(&temp, path).map_err(|error| error.to_string())?;
            #[cfg(unix)]
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| error.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }

    fn create_entry(path: &std::path::Path, entry: &Entry) -> Result<(), std::io::Error> {
        let parent = path.parent().expect("validated idempotency path has a parent");
        let temp = parent.join(format!(".{}.{}.tmp", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
            }
            let bytes = serde_json::to_vec(entry).map_err(std::io::Error::other)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::hard_link(&temp, path)?;
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        let _ = fs::remove_file(temp);
        result
    }

    fn find_job(&self, job_id: &str) -> Result<Option<(PathBuf, Entry)>, String> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(None);
        };
        for path in entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        {
            let Ok(entry) = Self::read_entry(&path) else { continue };
            if entry.job_id == job_id {
                return Ok(Some((path, entry)));
            }
        }
        Ok(None)
    }

    pub fn prune(&self) {
        let _guard = self.lock.lock().unwrap_or_else(|error| error.into_inner());
        let Ok(entries) = fs::read_dir(&self.dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") && Self::finished_and_expired(&path) {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn finished_and_expired(path: &std::path::Path) -> bool {
        let old_enough = path
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .is_some_and(|modified| modified.elapsed().is_ok_and(|age| age > RETENTION));
        old_enough && Self::read_entry(path).is_ok_and(|entry| entry.state == OperationState::Finished)
    }
}

impl IdempotencyRepository for IdempotencyStore {
    fn reserve(&self, key_hash: &str, fingerprint: &str, job_id: &str) -> Result<Reservation, String> {
        let _guard = self.lock.lock().unwrap_or_else(|error| error.into_inner());
        if key_hash.len() != 64 || !key_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Idempotency key hash is invalid".into());
        }
        self.ensure_dir()?;
        let path = self.dir.join(format!("{}.json", key_hash));
        if Self::finished_and_expired(&path) {
            // A key's 24-hour window also expires without requiring an app restart.
            let _ = fs::remove_file(&path);
        }
        if path.exists() {
            let existing = match Self::read_entry(&path) {
                Ok(existing) => existing,
                Err(_) => return Ok(Reservation::Unknown(None)),
            };
            if existing.fingerprint != fingerprint {
                return Ok(Reservation::Conflict);
            }
            let mut active = self.active.lock().unwrap_or_else(|error| error.into_inner());
            return match existing.state {
                OperationState::Reserved if active.contains(&existing.job_id) => {
                    Ok(Reservation::Pending(Some(existing.job_id)))
                }
                OperationState::Reserved => {
                    active.insert(existing.job_id.clone());
                    Ok(Reservation::Resume(existing.job_id))
                }
                OperationState::Submitting if active.contains(&existing.job_id) => {
                    Ok(Reservation::Pending(Some(existing.job_id)))
                }
                OperationState::Submitting => Ok(Reservation::Existing(existing.job_id, OperationState::Submitting)),
                state => Ok(Reservation::Existing(existing.job_id, state)),
            };
        }
        let create_result = {
            let entry = Entry {
                fingerprint: fingerprint.into(),
                job_id: job_id.into(),
                state: OperationState::Reserved,
            };
            Self::create_entry(&path, &entry)
        };
        match create_result {
            Ok(()) => {
                self.active
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .insert(job_id.into());
                Ok(Reservation::New(job_id.into()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(Reservation::Pending(None)),
            Err(error) => Err(error.to_string()),
        }
    }

    fn operation_state(&self, job_id: &str) -> Result<Option<OperationState>, String> {
        let _guard = self.lock.lock().unwrap_or_else(|error| error.into_inner());
        Ok(self.find_job(job_id)?.map(|(_, entry)| entry.state))
    }

    fn mark_submitting(&self, job_id: &str) -> Result<(), String> {
        self.transition(job_id, OperationState::Submitting)
    }

    fn mark_finished(&self, job_id: &str) -> Result<(), String> {
        self.transition(job_id, OperationState::Finished)
    }

    fn mark_unknown(&self, job_id: &str) -> Result<(), String> {
        self.transition(job_id, OperationState::Unknown)
    }

    fn release_reserved(&self, job_id: &str) {
        self.active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(job_id);
    }

    fn recover(&self, history: &dyn HistoryRepository) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap_or_else(|error| error.into_inner());
        self.active.lock().unwrap_or_else(|error| error.into_inner()).clear();
        self.ensure_dir()?;
        let entries = fs::read_dir(&self.dir).map_err(|error| error.to_string())?;
        for path in entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        {
            let Ok(mut entry) = Self::read_entry(&path) else {
                continue;
            };
            let history_entry = history.find(&entry.job_id);
            let has_history = history_entry.is_some();
            let history_unknown = history_entry
                .as_ref()
                .is_some_and(|entry| entry.error_code.as_deref() == Some("outcome_unknown"));
            match entry.state {
                _ if history_unknown => entry.state = OperationState::Unknown,
                OperationState::Reserved | OperationState::Submitting if has_history => {
                    entry.state = OperationState::Finished
                }
                OperationState::Submitting | OperationState::Finished if !has_history => {
                    entry.state = OperationState::Unknown
                }
                OperationState::Unknown if has_history => entry.state = OperationState::Finished,
                OperationState::Reserved | OperationState::Finished if has_history => continue,
                _ => continue,
            }
            Self::write_entry(&path, &entry)?;
        }
        Ok(())
    }
}

impl IdempotencyStore {
    fn transition(&self, job_id: &str, next: OperationState) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((path, mut entry)) = self.find_job(job_id)? {
            match (entry.state, next) {
                (OperationState::Reserved, OperationState::Submitting)
                | (OperationState::Reserved | OperationState::Submitting, OperationState::Finished)
                | (_, OperationState::Unknown) => {
                    entry.state = next;
                    Self::write_entry(&path, &entry)?;
                }
                _ => {}
            }
        }
        if matches!(next, OperationState::Finished | OperationState::Unknown) {
            self.active
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(job_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
