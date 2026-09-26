use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryInputKind {
    Json,
    Pdf,
    Sample,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryOutputState {
    #[default]
    NotCreated,
    Saved,
    SaveFailed,
    RemoteUnavailable,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HistoryContentStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_kind: Option<HistoryInputKind>,
    #[serde(default)]
    pub input_saved: bool,
    #[serde(default)]
    pub output: HistoryOutputState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPdfKind {
    Input,
    Output,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryEntry {
    pub id: String,
    /// RFC 3339, local time.
    pub time: String,
    /// "http" (from a web app) or "app" (test print from ix.print).
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// Name of the API token the request used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub printer: String,
    /// The spooler's job id, for looking up queue position / errors later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spooler_job: Option<String>,
    /// Sheets sent to the printer (all copies).
    pub sheets: u32,
    /// Resolved settings of the job.
    pub settings: serde_json::Value,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Stable machine-readable failure state for client recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub duration_ms: u64,
    /// Delivery of the result callback, when the client asked for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback: Option<CallbackStatus>,
    /// Set when the job was forwarded to another ix.print.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteJob>,
    /// Availability metadata only; payloads are stored separately from JSONL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<HistoryContentStatus>,
}

impl HistoryEntry {
    pub fn is_cancelled(&self) -> bool {
        self.source.starts_with("incoming_cancelled_")
    }

    pub fn matches_query(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        [
            Some(self.id.as_str()),
            Some(self.file_name.as_str()),
            Some(self.printer.as_str()),
            Some(self.source.as_str()),
            self.client.as_deref(),
            self.token.as_deref(),
            self.preset.as_deref(),
            self.callback.as_ref().and_then(|c| c.reference.as_deref()),
            self.remote.as_ref().map(|remote| remote.remote_name.as_str()),
        ]
        .into_iter()
        .flatten()
        .any(|value| value.to_lowercase().contains(&query))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemoteJob {
    pub remote_id: String,
    pub remote_name: String,
    /// Job id on the other computer.
    pub job_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CallbackStatus {
    pub url: String,
    /// Token identifier allows a pending signed delivery to resume after restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// watching (job not done yet) | retrying | delivered | failed | refused
    pub state: String,
    /// Job outcome sent in the callback (completed, failed, …), once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default)]
    pub attempts: u32,
    /// Last HTTP status from the receiver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub time: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct HistoryDay {
    pub date: String,
    pub total: usize,
    pub failed: usize,
}

/// One page of a day's history, newest first.
#[derive(Serialize, Clone, Debug)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub total: usize,
    pub failed: usize,
    /// 1-based, clamped to the available pages.
    pub page: usize,
    pub pages: usize,
    pub per_page: usize,
}
