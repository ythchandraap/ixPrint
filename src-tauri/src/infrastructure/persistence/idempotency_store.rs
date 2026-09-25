use crate::application::ports::{IdempotencyRepository, Reservation};
use serde::{Deserialize, Serialize};
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
}

pub struct IdempotencyStore {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl IdempotencyStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    pub fn prune(&self) {
        let _guard = self.lock.lock().unwrap_or_else(|error| error.into_inner());
        let Ok(entries) = fs::read_dir(&self.dir) else { return };
        for entry in entries.flatten() {
            let expired = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .is_some_and(|modified| modified.elapsed().is_ok_and(|age| age > RETENTION));
            if expired {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    fn expired(path: &std::path::Path) -> bool {
        path.metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .is_some_and(|modified| modified.elapsed().is_ok_and(|age| age > RETENTION))
    }
}

impl IdempotencyRepository for IdempotencyStore {
    fn reserve(&self, key_hash: &str, fingerprint: &str, job_id: &str) -> Result<Reservation, String> {
        let _guard = self.lock.lock().unwrap_or_else(|error| error.into_inner());
        if key_hash.len() != 64 || !key_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Idempotency key hash is invalid".into());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let path = self.dir.join(format!("{}.json", key_hash));
        let entry = Entry {
            fingerprint: fingerprint.into(),
            job_id: job_id.into(),
        };
        if Self::expired(&path) {
            // A key's 24-hour window also expires without requiring an app restart.
            let _ = fs::remove_file(&path);
        }
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let write = (|| -> Result<(), String> {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
                    }
                    let bytes = serde_json::to_vec(&entry).map_err(|e| e.to_string())?;
                    file.write_all(&bytes).map_err(|e| e.to_string())?;
                    file.sync_all().map_err(|e| e.to_string())?;
                    Ok(())
                })();
                if let Err(error) = write {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
                Ok(Reservation::New)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // A concurrent writer may still be flushing. The caller will
                // receive a pending response rather than risk printing twice.
                let existing: Entry = match fs::read(&path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                {
                    Some(value) => value,
                    None => return Ok(Reservation::Pending),
                };
                if existing.fingerprint == fingerprint {
                    Ok(Reservation::Existing(existing.job_id))
                } else {
                    Ok(Reservation::Conflict)
                }
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_replays_matching_request_and_rejects_changed_content() {
        let dir = std::env::temp_dir().join(format!("ixprint-idempotency-{}", uuid::Uuid::new_v4()));
        let store = IdempotencyStore::new(dir.clone());
        let key = "a".repeat(64);
        assert!(matches!(
            store.reserve(&key, "content-a", "job-a"),
            Ok(Reservation::New)
        ));
        assert!(matches!(store.reserve(&key, "content-a", "job-b"), Ok(Reservation::Existing(id)) if id == "job-a"));
        assert!(matches!(
            store.reserve(&key, "content-b", "job-b"),
            Ok(Reservation::Conflict)
        ));
        let _ = fs::remove_dir_all(dir);
    }
}
