use crate::application::ports::RemoteRepository;
use crate::domain::remote::{validate_all, CacheEntry, Remote};
use crate::shared::sync::RwLockExt;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

pub struct RemoteStore {
    path: PathBuf,
    data: RwLock<Vec<Remote>>,
}

impl RemoteRepository for RemoteStore {
    fn list(&self) -> Vec<Remote> {
        RemoteStore::list(self)
    }

    fn update(&self, change: &mut dyn FnMut(&mut Vec<Remote>) -> Result<(), String>) -> Result<Vec<Remote>, String> {
        RemoteStore::update(self, |remotes| change(remotes))
    }
}

pub type RemoteCache = Mutex<HashMap<String, CacheEntry>>;

impl RemoteStore {
    pub fn load(dir: &Path) -> RemoteStore {
        let path = dir.join("remotes.json");
        let list = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Vec<Remote>>(&text) {
                Ok(list) => match validate_all(&list) {
                    Ok(()) => list,
                    Err(e) => {
                        eprintln!("remotes.json is invalid: {}", e);
                        Vec::new()
                    }
                },
                Err(e) => {
                    eprintln!("remotes.json is corrupted: {}", e);
                    Vec::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => {
                eprintln!("remotes.json could not be read: {}", e);
                Vec::new()
            }
        };
        RemoteStore {
            path,
            data: RwLock::new(list),
        }
    }

    pub fn list(&self) -> Vec<Remote> {
        self.data.read_safe().clone()
    }

    pub fn update<F>(&self, change: F) -> Result<Vec<Remote>, String>
    where
        F: FnOnce(&mut Vec<Remote>) -> Result<(), String>,
    {
        let mut guard = self.data.write_safe();
        let mut next = guard.clone();
        change(&mut next)?;
        validate_all(&next)?;
        write_atomic(&self.path, &next)?;
        *guard = next.clone();
        Ok(next)
    }
}

fn write_atomic(path: &Path, remotes: &[Remote]) -> Result<(), String> {
    let dir = path.parent().ok_or("Invalid remotes path")?;
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create the remotes folder: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(remotes).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| format!("Failed to save remotes: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(tmp, path).map_err(|e| format!("Failed to save remotes: {}", e))
}

#[cfg(test)]
#[path = "remote_store/tests.rs"]
mod tests;
