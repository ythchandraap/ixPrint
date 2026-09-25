use std::collections::BTreeMap;

/// Values for the placeholders of one printed page.
pub type Record = BTreeMap<String, String>;

fn is_field_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// Names inside `{{…}}` (surrounding spaces allowed).
pub(super) fn placeholders(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if !name.is_empty() && name.chars().all(is_field_char) {
            out.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

/// Replaces `{{field}}` with the record's value; unknown fields become empty.
pub fn fill(s: &str, record: &Record) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) if after[..end].trim().chars().all(is_field_char) && !after[..end].trim().is_empty() => {
                out.push_str(record.get(after[..end].trim()).map(String::as_str).unwrap_or(""));
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Converts a JSON object (strings, numbers, booleans, null) into a record.
pub fn record_from_json(value: &serde_json::Value) -> Result<Record, String> {
    let obj = value.as_object().ok_or("Each data item must be a JSON object")?;
    obj.iter()
        .map(|(k, v)| {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => String::new(),
                _ => return Err(format!("Value '{}' must be text/number, not an object/array", k)),
            };
            Ok((k.clone(), s))
        })
        .collect()
}
