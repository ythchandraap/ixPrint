use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiAccessEntry {
    pub id: String,
    /// RFC 3339 local time.
    pub time: String,
    pub method: String,
    /// URL path only. Query parameters are deliberately excluded.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// recognized-token | no-credential | unrecognized-credential | desktop
    pub auth: String,
    /// API token name, never the token secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_content_type: Option<String>,
    /// Declared Content-Length; absent for chunked requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    pub response_status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_content_type: Option<String>,
    /// Declared Content-Length; absent for streamed responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_bytes: Option<u64>,
    pub duration_ms: u64,
}
