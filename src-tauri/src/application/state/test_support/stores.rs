use super::*;
use crate::application::ports::OperationState;
use std::collections::HashSet;

pub(super) struct MemoryConfig(pub(super) Mutex<Config>);

#[derive(Default)]
pub(super) struct MemoryIdempotency {
    values: Mutex<HashMap<String, (String, String, OperationState)>>,
    active: Mutex<HashSet<String>>,
}

impl IdempotencyRepository for MemoryIdempotency {
    fn reserve(
        &self,
        key_hash: &str,
        fingerprint: &str,
        job_id: &str,
    ) -> Result<crate::application::ports::Reservation, String> {
        let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        match values.get(key_hash).cloned() {
            Some((old, _, _)) if old != fingerprint => Ok(crate::application::ports::Reservation::Conflict),
            Some((_, id, state)) => {
                let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
                match state {
                    OperationState::Reserved if active.contains(&id) => {
                        Ok(crate::application::ports::Reservation::Pending(Some(id)))
                    }
                    OperationState::Reserved => {
                        active.insert(id.clone());
                        Ok(crate::application::ports::Reservation::Resume(id))
                    }
                    OperationState::Submitting if active.contains(&id) => {
                        Ok(crate::application::ports::Reservation::Pending(Some(id)))
                    }
                    OperationState::Submitting => {
                        values.get_mut(key_hash).unwrap().2 = OperationState::Unknown;
                        Ok(crate::application::ports::Reservation::Existing(
                            id,
                            OperationState::Unknown,
                        ))
                    }
                    _ => Ok(crate::application::ports::Reservation::Existing(id, state)),
                }
            }
            None => {
                values.insert(
                    key_hash.into(),
                    (fingerprint.into(), job_id.into(), OperationState::Reserved),
                );
                self.active
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(job_id.into());
                Ok(crate::application::ports::Reservation::New(job_id.into()))
            }
        }
    }

    fn operation_state(&self, job_id: &str) -> Result<Option<OperationState>, String> {
        Ok(self
            .values
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .find(|(_, id, _)| id == job_id)
            .map(|(_, _, state)| *state))
    }

    fn mark_submitting(&self, job_id: &str) -> Result<(), String> {
        self.set_state(job_id, OperationState::Submitting)
    }

    fn mark_finished(&self, job_id: &str) -> Result<(), String> {
        self.set_state(job_id, OperationState::Finished)
    }

    fn mark_unknown(&self, job_id: &str) -> Result<(), String> {
        self.set_state(job_id, OperationState::Unknown)
    }

    fn release_reserved(&self, job_id: &str) {
        self.active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(job_id);
    }

    fn recover(&self, _history: &dyn HistoryRepository) -> Result<(), String> {
        self.active.lock().unwrap_or_else(|error| error.into_inner()).clear();
        Ok(())
    }
}

impl MemoryIdempotency {
    fn set_state(&self, job_id: &str, next: OperationState) -> Result<(), String> {
        let mut values = self.values.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((_, _, state)) = values.values_mut().find(|(_, id, _)| id == job_id) {
            match (*state, next) {
                (OperationState::Reserved, OperationState::Submitting)
                | (OperationState::Reserved | OperationState::Submitting, OperationState::Finished)
                | (_, OperationState::Unknown) => *state = next,
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

impl ConfigRepository for MemoryConfig {
    fn get(&self) -> Config {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, change: &mut dyn FnMut(&mut Config) -> Result<(), String>) -> Result<Config, String> {
        let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = current.clone();
        change(&mut next)?;
        next.version = CONFIG_VERSION;
        next.validate()?;
        *current = next.clone();
        Ok(next)
    }

    fn data_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }
}

#[derive(Default)]
pub(super) struct MemoryRemotes(Mutex<Vec<Remote>>);

impl RemoteRepository for MemoryRemotes {
    fn list(&self) -> Vec<Remote> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, change: &mut dyn FnMut(&mut Vec<Remote>) -> Result<(), String>) -> Result<Vec<Remote>, String> {
        let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = current.clone();
        change(&mut next)?;
        *current = next.clone();
        Ok(next)
    }
}

#[derive(Default)]
pub(super) struct MemoryHistory(Mutex<Vec<HistoryEntry>>);

impl HistoryRepository for MemoryHistory {
    fn append(&self, entry: &HistoryEntry) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(entry.clone());
        Ok(())
    }

    fn append_callback(&self, job_id: &str, status: CallbackStatus) -> Result<(), String> {
        if let Some(entry) = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter_mut()
            .find(|entry| entry.id == job_id)
        {
            entry.callback = Some(status);
        }
        Ok(())
    }

    fn find(&self, job_id: &str) -> Option<HistoryEntry> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|entry| entry.id == job_id)
            .cloned()
    }

    fn read_page(&self, date: &str, page: usize, per_page: usize, failed_only: bool) -> Result<HistoryPage, String> {
        let mut all: Vec<_> = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|entry| entry.time.starts_with(date))
            .cloned()
            .collect();
        all.sort_by(|a, b| b.time.cmp(&a.time));
        let failed = all.iter().filter(|entry| !entry.success).count();
        let entries = if failed_only {
            all.into_iter().filter(|entry| !entry.success).collect()
        } else {
            all
        };
        let per_page = per_page.clamp(1, 200);
        let pages = entries.len().div_ceil(per_page).max(1);
        let page = page.clamp(1, pages);
        let total = entries.len();
        let entries = entries.into_iter().skip((page - 1) * per_page).take(per_page).collect();
        Ok(HistoryPage {
            entries,
            total,
            failed,
            page,
            pages,
            per_page,
        })
    }

    fn days(&self) -> Vec<HistoryDay> {
        let mut days = BTreeMap::<String, (usize, usize)>::new();
        for entry in self.0.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            let date = entry.time.get(..10).unwrap_or_default().to_string();
            let counts = days.entry(date).or_default();
            counts.0 += 1;
            counts.1 += usize::from(!entry.success);
        }
        days.into_iter()
            .rev()
            .map(|(date, (total, failed))| HistoryDay { date, total, failed })
            .collect()
    }

    fn search(&self, query: &str, page: usize, per_page: usize) -> Result<HistoryPage, String> {
        let mut entries: Vec<_> = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|entry| entry.matches_query(query))
            .cloned()
            .collect();
        entries.sort_by(|a, b| b.time.cmp(&a.time));
        let failed = entries.iter().filter(|entry| !entry.success).count();
        let per_page = per_page.clamp(1, 200);
        let pages = entries.len().div_ceil(per_page).max(1);
        let page = page.clamp(1, pages);
        let total = entries.len();
        Ok(HistoryPage {
            entries: entries.into_iter().skip((page - 1) * per_page).take(per_page).collect(),
            total,
            failed,
            page,
            pages,
            per_page,
        })
    }

    fn prune(&self, _retention_days: u32) -> Result<(), String> {
        Ok(())
    }
}

struct NoopJobMarker;
impl JobMarker for NoopJobMarker {}

#[derive(Default)]
pub(super) struct MemoryDiagnostics(Mutex<Vec<DiagnosticEntry>>);

impl DiagnosticsRepository for MemoryDiagnostics {
    fn begin_job(&self, _id: &str, _source: &str, _printer: &str) -> Result<Box<dyn JobMarker>, String> {
        Ok(Box::new(NoopJobMarker))
    }

    fn record(&self, level: &str, source: &str, message: &str, details: Option<&str>) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(DiagnosticEntry {
            time: chrono::Local::now().to_rfc3339(),
            level: level.into(),
            source: source.into(),
            message: message.into(),
            details: details.map(str::to_string),
        });
        Ok(())
    }

    fn read(&self) -> Result<Vec<DiagnosticEntry>, String> {
        Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    fn clear(&self) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
        Ok(())
    }

    fn finish_session(&self) {}
}

#[derive(Default)]
pub(super) struct MemoryApiAccessLog(Mutex<Vec<ApiAccessEntry>>);

impl ApiAccessLogRepository for MemoryApiAccessLog {
    fn append(&self, entry: &ApiAccessEntry) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(entry.clone());
        Ok(())
    }

    fn read(&self) -> Result<Vec<ApiAccessEntry>, String> {
        Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    fn clear(&self) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
        Ok(())
    }
}
