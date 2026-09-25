use super::element::{Element, MAX_ELEMENTS, MAX_SIZE_MM, MIN_SIZE_MM};
use super::record::{fill, placeholders, Record};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_RECORDS: usize = 500;

/// Fields ix.print fills in itself.
pub const AUTO_FIELDS: [&str; 4] = ["no", "halaman", "total_halaman", "jumlah_data"];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FooterMode {
    /// Once, right after the last row (totals, signatures).
    #[default]
    LastPage,
    /// At the bottom of every page.
    EveryPage,
}

/// How records map to pages.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Layout {
    /// One page per record (labels).
    #[default]
    Label,
    /// Records as rows under a header, continuing on new pages with the header
    /// repeated. In the editor the design is three stacked bands: header
    /// (`header_mm`), one row (`row_mm`) and footer (`footer_mm`); an element
    /// belongs to the band its top edge is in.
    List {
        header_mm: f32,
        row_mm: f32,
        #[serde(default)]
        footer_mm: f32,
        #[serde(default)]
        footer: FooterMode,
        /// Page height follows the content (receipt rolls): one long page.
        #[serde(default)]
        roll: bool,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Template {
    /// Page width.
    pub width_mm: f32,
    /// Page height (for label mode also the design height; unused on a roll).
    pub height_mm: f32,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub elements: Vec<Element>,
    /// Example data for the editor preview and test prints.
    #[serde(default)]
    pub sample: Record,
    /// Fields that must have a value in each record or the shared header.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_fields: Vec<String>,
}

impl Template {
    pub fn validate(&self) -> Result<(), String> {
        for (label, v) in [("width", self.width_mm), ("height", self.height_mm)] {
            if !(MIN_SIZE_MM..=MAX_SIZE_MM).contains(&v) {
                return Err(format!("Template {} must be {}–{} mm", label, MIN_SIZE_MM, MAX_SIZE_MM));
            }
        }
        if self.elements.len() > MAX_ELEMENTS {
            return Err(format!("At most {} elements are allowed", MAX_ELEMENTS));
        }
        for (i, el) in self.elements.iter().enumerate() {
            el.check().map_err(|e| format!("elemen #{}: {}", i + 1, e))?;
        }
        let available = self.fields();
        for field in &self.required_fields {
            if !available.contains(field) {
                return Err(format!("Required field '{}' is not used by this design", field));
            }
        }
        if let Layout::List {
            header_mm,
            row_mm,
            footer_mm,
            footer,
            roll,
        } = self.layout
        {
            let ok = |v: f32| v.is_finite() && (0.0..=MAX_SIZE_MM).contains(&v);
            if !(ok(header_mm) && ok(footer_mm) && ok(row_mm) && row_mm >= 1.0) {
                return Err("Header/row/footer height is invalid (row must be at least 1 mm)".into());
            }
            if !roll {
                let fixed = header_mm
                    + if footer == FooterMode::EveryPage {
                        footer_mm
                    } else {
                        0.0
                    };
                if fixed + row_mm > self.height_mm + 0.01 {
                    return Err(format!(
                        "header + row{} ({} mm) is taller than the page ({} mm)",
                        if footer == FooterMode::EveryPage {
                            " + footer"
                        } else {
                            ""
                        },
                        fixed + row_mm,
                        self.height_mm
                    ));
                }
                if header_mm + row_mm + footer_mm > self.height_mm + 0.01 {
                    return Err("Header + row + footer is taller than the page".into());
                }
            }
        }
        Ok(())
    }

    /// Checks values before any job reaches the printer. Legacy designs have
    /// no required fields, so their optional placeholders still print empty.
    pub fn preflight(&self, records: &[Record], header: &Record) -> Result<(), String> {
        match self.preflight_errors(records, header).into_iter().next() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Returns the first 50 issues so integrations can fix a batch at once.
    pub fn preflight_errors(&self, records: &[Record], header: &Record) -> Vec<String> {
        if let Err(error) = self.validate() {
            return vec![error];
        }
        if records.is_empty() {
            return vec!["data must contain at least one record".into()];
        }
        let mut errors = Vec::new();
        for (index, record) in records.iter().enumerate() {
            for field in &self.required_fields {
                let value = record.get(field).or_else(|| header.get(field));
                if value.is_none_or(|value| value.trim().is_empty()) {
                    errors.push(format!("data[{}].{} is required", index + 1, field));
                }
            }
            let mut values = header.clone();
            values.extend(record.clone());
            for element in &self.elements {
                if let Element::Barcode { value, .. } = element {
                    let filled = fill(value, &values);
                    if !filled.bytes().all(|byte| (32..=126).contains(&byte)) {
                        errors.push(format!("data[{}]: barcode must contain printable ASCII", index + 1));
                    }
                }
            }
            if errors.len() >= 50 {
                errors.truncate(50);
                break;
            }
        }
        errors
    }

    /// Placeholder names clients should send, sorted; excludes the fields ix.print
    /// fills in itself ({{no}}, {{halaman}}, ...), see [`Template::auto_fields`].
    pub fn fields(&self) -> Vec<String> {
        let mut out = self.all_fields();
        out.retain(|f| !AUTO_FIELDS.contains(&f.as_str()));
        out
    }

    /// Automatic fields used by the design; clients may still send their own values.
    pub fn auto_fields(&self) -> Vec<String> {
        let mut out = self.all_fields();
        out.retain(|f| AUTO_FIELDS.contains(&f.as_str()));
        out
    }

    fn all_fields(&self) -> Vec<String> {
        let mut out = BTreeSet::new();
        for el in &self.elements {
            for s in el.templates() {
                out.extend(placeholders(s));
            }
        }
        out.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_requires_configured_fields_and_checks_barcode_values() {
        let mut design: Template = serde_json::from_value(serde_json::json!({
            "width_mm": 60,
            "height_mm": 40,
            "elements": [{"type": "barcode", "x": 1, "y": 1, "w": 30, "h": 10, "value": "{{kode}}"}],
            "required_fields": ["kode"]
        }))
        .unwrap();
        let data = |value: &str| Record::from([("kode".into(), value.into())]);
        assert!(design.preflight(&[data("A-123")], &Record::new()).is_ok());
        assert!(design
            .preflight(&[Record::new()], &Record::new())
            .unwrap_err()
            .contains("data[1].kode"));
        assert!(design
            .preflight(&[data("é")], &Record::new())
            .unwrap_err()
            .contains("ASCII"));
        design.required_fields = vec!["unknown".into()];
        assert!(design.validate().unwrap_err().contains("not used"));
    }
}
