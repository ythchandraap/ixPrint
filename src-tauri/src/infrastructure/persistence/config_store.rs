use crate::domain::settings_config::{Config, CONFIG_VERSION};
use crate::shared::sync::RwLockExt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub struct ConfigStore {
    path: PathBuf,
    data: RwLock<Config>,
}

impl ConfigStore {
    /// Loads `config.json` from `dir`, falling back to the backup and then to an
    /// empty config. An unreadable file is kept aside instead of being overwritten.
    pub fn load(dir: &Path) -> ConfigStore {
        let path = dir.join("config.json");
        let backup = path.with_extension("json.bak");
        let mut config = match read_config(&path) {
            Ok(Some(cfg)) => cfg,
            Ok(None) => Config::default(),
            Err(e) => {
                eprintln!("config.json is corrupted ({}), trying the backup", e);
                let aside =
                    path.with_extension(format!("json.corrupt-{}", chrono::Local::now().format("%Y%m%d%H%M%S")));
                let _ = fs::rename(&path, aside);
                read_config(&backup).ok().flatten().unwrap_or_default()
            }
        };
        if config.migrate() {
            config.version = CONFIG_VERSION;
            match write_atomic(&path, &config) {
                Ok(()) => eprintln!("[ix.print] old template moved to preset (design)"),
                Err(e) => eprintln!("[ix.print] template migration was not saved: {}", e),
            }
        }
        ConfigStore {
            path,
            data: RwLock::new(config),
        }
    }

    pub fn get(&self) -> Config {
        self.data.read_safe().clone()
    }

    /// Applies `change` to a copy, validates it, persists it and only then makes it current.
    pub fn update<F>(&self, change: F) -> Result<Config, String>
    where
        F: FnOnce(&mut Config) -> Result<(), String>,
    {
        let mut guard = self.data.write_safe();
        let mut next = guard.clone();
        change(&mut next)?;
        next.version = CONFIG_VERSION;
        next.validate()?;
        write_atomic(&self.path, &next)?;
        *guard = next.clone();
        Ok(next)
    }
}

fn read_config(path: &Path) -> Result<Option<Config>, String> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let cfg: Config = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if cfg.version > CONFIG_VERSION {
                return Err(format!(
                    "Created by a newer version of ix.print (version {})",
                    cfg.version
                ));
            }
            Ok(Some(cfg))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn write_atomic(path: &Path, config: &Config) -> Result<(), String> {
    let dir = path.parent().ok_or("invalid config path")?;
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create the config folder: {}", e))?;
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Failed to save config: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    if path.exists() {
        let _ = fs::copy(path, path.with_extension("json.bak"));
    }
    fs::rename(&tmp, path).map_err(|e| format!("Failed to save config: {}", e))
}

#[cfg(test)]
mod tests;
