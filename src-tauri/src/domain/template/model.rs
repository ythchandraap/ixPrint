use super::element::{Element, MAX_ELEMENTS, MAX_SIZE_MM, MIN_SIZE_MM};
use super::record::{fill, placeholders, Record};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_RECORDS: usize = 500;

/// Fields ix.print fills in itself.
pub const AUTO_FIELDS: [&str; 4] = ["no", "halaman", "total_halaman", "jumlah_data"];

/// Editing-only group metadata; print rendering continues to use `elements`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateGroup {
    pub id: String,
    pub name: String,
    pub parent_group_id: Option<String>,
    #[serde(default)]
    pub element_ids: Vec<String>,
}

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
    /// Nested editing structure, independent of element z-order.
    #[serde(default)]
    pub groups: Vec<TemplateGroup>,
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
        self.validate_groups()?;
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

    fn validate_groups(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for (index, element) in self.elements.iter().enumerate() {
            let id = element.id();
            if id.trim().is_empty() && !self.groups.is_empty() {
                return Err(format!("Element #{} has a missing ID", index + 1));
            }
            if !id.trim().is_empty() && !ids.insert(id) {
                return Err(format!("Duplicate template ID '{}'", id));
            }
        }

        let element_ids = self.elements.iter().map(Element::id).collect::<BTreeSet<_>>();
        let mut group_by_id = BTreeMap::new();
        for (index, group) in self.groups.iter().enumerate() {
            if group.id.trim().is_empty() {
                return Err(format!("Group #{} has a missing ID", index + 1));
            }
            if !ids.insert(group.id.as_str()) {
                return Err(format!("Duplicate template ID '{}'", group.id));
            }
            if group.name.trim().is_empty() || group.name.trim() != group.name || group.name.chars().count() > 60 {
                return Err(format!(
                    "Group '{}' must have a trimmed name of 1–60 characters",
                    group.id
                ));
            }
            group_by_id.insert(group.id.as_str(), group);
        }

        let mut assigned_elements = BTreeSet::new();
        for group in &self.groups {
            if let Some(parent) = &group.parent_group_id {
                if !group_by_id.contains_key(parent.as_str()) {
                    return Err(format!(
                        "Group '{}' refers to missing parent group '{}'",
                        group.name, parent
                    ));
                }
            }
            for element_id in &group.element_ids {
                if !element_ids.contains(element_id.as_str()) {
                    return Err(format!(
                        "Group '{}' refers to missing element '{}'",
                        group.name, element_id
                    ));
                }
                if !assigned_elements.insert(element_id.as_str()) {
                    return Err(format!(
                        "Element '{}' belongs directly to more than one group",
                        element_id
                    ));
                }
            }
        }

        for group in &self.groups {
            let mut seen = BTreeSet::new();
            let mut current = Some(group.id.as_str());
            while let Some(id) = current {
                if !seen.insert(id) {
                    return Err(format!("Group cycle detected at '{}'", group.name));
                }
                current = group_by_id.get(id).and_then(|node| node.parent_group_id.as_deref());
            }
        }

        let mut bands = BTreeMap::<&str, BTreeSet<&str>>::new();
        if let Layout::List { header_mm, row_mm, .. } = self.layout {
            for group in &self.groups {
                let descendants = self.group_element_ids(&group.id, &group_by_id);
                if descendants.is_empty() {
                    return Err(format!("Group '{}' is empty and must be removed", group.name));
                }
                for element in &self.elements {
                    if descendants.contains(element.id()) {
                        let band = if element.top() < header_mm - 0.001 {
                            "header"
                        } else if element.top() < header_mm + row_mm - 0.001 {
                            "row"
                        } else {
                            "footer"
                        };
                        bands.entry(group.id.as_str()).or_default().insert(band);
                    }
                }
                if bands.get(group.id.as_str()).is_some_and(|set| set.len() > 1) {
                    return Err(format!(
                        "Group '{}' contains elements from different list bands",
                        group.name
                    ));
                }
            }
        } else {
            for group in &self.groups {
                if self.group_element_ids(&group.id, &group_by_id).is_empty() {
                    return Err(format!("Group '{}' is empty and must be removed", group.name));
                }
            }
        }
        Ok(())
    }

    fn group_element_ids<'a>(
        &'a self,
        group_id: &'a str,
        group_by_id: &BTreeMap<&str, &'a TemplateGroup>,
    ) -> BTreeSet<&'a str> {
        let mut ids = BTreeSet::new();
        let mut pending = vec![group_id];
        while let Some(current) = pending.pop() {
            if let Some(group) = group_by_id.get(current) {
                ids.extend(group.element_ids.iter().map(String::as_str));
                pending.extend(
                    self.groups
                        .iter()
                        .filter(|child| child.parent_group_id.as_deref() == Some(current))
                        .map(|child| child.id.as_str()),
                );
            }
        }
        ids
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
            "elements": [{"type": "barcode", "id": "barcode-1", "x": 1, "y": 1, "w": 30, "h": 10, "value": "{{kode}}"}],
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

    fn nested_design() -> Template {
        serde_json::from_value(serde_json::json!({
            "width_mm": 60,
            "height_mm": 40,
            "elements": [
                {"type":"rect", "id":"e1", "x":2, "y":2, "w":10, "h":5},
                {"type":"rect", "id":"e2", "x":20, "y":2, "w":10, "h":5}
            ],
            "groups": [
                {"id":"outer", "name":"Group A", "parentGroupId":null, "elementIds":[]},
                {"id":"inner", "name":"Group B", "parentGroupId":"outer", "elementIds":["e1", "e2"]}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn nested_groups_validate_and_round_trip_as_editing_metadata() {
        let design = nested_design();
        assert!(design.validate().is_ok());
        assert_eq!(design.elements.len(), 2);
        let encoded = serde_json::to_value(&design).unwrap();
        assert_eq!(encoded["groups"][1]["parentGroupId"], "outer");
        assert_eq!(encoded["groups"][1]["elementIds"], serde_json::json!(["e1", "e2"]));
        let reopened: Template = serde_json::from_value(encoded).unwrap();
        assert_eq!(reopened.groups, design.groups);
    }

    #[test]
    fn old_templates_default_to_an_empty_group_list() {
        let design: Template = serde_json::from_value(serde_json::json!({ "width_mm": 60, "height_mm": 40 })).unwrap();
        assert!(design.groups.is_empty());
        assert!(design.validate().is_ok());
    }

    #[test]
    fn group_validation_rejects_cycles_duplicate_members_missing_references_and_empty_groups() {
        let mut design = nested_design();
        design.groups[0].parent_group_id = Some("inner".into());
        assert!(design.validate().unwrap_err().contains("cycle"));

        let mut design = nested_design();
        design.groups[0].element_ids.push("e1".into());
        assert!(design.validate().unwrap_err().contains("more than one group"));

        let mut design = nested_design();
        design.groups[1].element_ids[0] = "missing".into();
        assert!(design.validate().unwrap_err().contains("missing element"));

        let mut design = nested_design();
        design.groups[1].parent_group_id = Some("missing".into());
        assert!(design.validate().unwrap_err().contains("missing parent group"));

        let mut design = nested_design();
        design.groups[0].id = "e1".into();
        assert!(design.validate().unwrap_err().contains("Duplicate template ID"));

        let mut design = nested_design();
        design.groups[1].element_ids.clear();
        assert!(design.validate().unwrap_err().contains("empty"));
    }

    #[test]
    fn list_groups_cannot_span_header_row_and_footer_bands() {
        let mut design = nested_design();
        if let Element::Rect { y, .. } = &mut design.elements[1] {
            *y = 8.0;
        }
        design.layout = Layout::List {
            header_mm: 5.0,
            row_mm: 5.0,
            footer_mm: 10.0,
            footer: FooterMode::LastPage,
            roll: false,
        };
        assert!(design.validate().unwrap_err().contains("different list bands"));
    }
}
