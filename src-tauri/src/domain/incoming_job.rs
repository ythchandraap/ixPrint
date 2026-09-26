use serde::{Deserialize, Serialize};

use crate::domain::printer::Capabilities;

const POINTS_PER_MM: f32 = 72.0 / 25.4;
const MEDIA_TOLERANCE_MM: f32 = 1.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncomingSource {
    SystemPrint,
    FilePicker,
    Finder,
    Explorer,
}

impl IncomingSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SystemPrint => "system_print",
            Self::FilePicker => "file_picker",
            Self::Finder => "finder",
            Self::Explorer => "explorer",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncomingState {
    AwaitingDestination,
    Submitting,
    Queued,
    Failed,
    Cancelled,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PageMedia {
    pub width_pt: f32,
    pub height_pt: f32,
}

impl PageMedia {
    pub fn dimensions_mm(self) -> (f32, f32) {
        (self.width_pt / POINTS_PER_MM, self.height_pt / POINTS_PER_MM)
    }

    fn matches(self, width_mm: f32, height_mm: f32) -> bool {
        let (width, height) = self.dimensions_mm();
        close(width, width_mm) && close(height, height_mm) || close(width, height_mm) && close(height, width_mm)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IncomingJob {
    pub id: String,
    pub source: IncomingSource,
    pub document_name: String,
    pub pages: Vec<PageMedia>,
    pub copies: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_job_id: Option<String>,
    pub state: IncomingState,
    pub received_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spooler_job: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl IncomingJob {
    pub fn validate(&self) -> Result<(), String> {
        if self.pages.is_empty() {
            return Err("The document has no pages".into());
        }
        if self.pages.iter().any(|page| {
            !page.width_pt.is_finite() || !page.height_pt.is_finite() || page.width_pt <= 0.0 || page.height_pt <= 0.0
        }) {
            return Err("The document contains an invalid page size".into());
        }
        if !(1..=999).contains(&self.copies) {
            return Err("Copies must be between 1 and 999".into());
        }
        Ok(())
    }
}

/// Checks every page against the document-print media supported by one printer.
/// Label and other non-A4/Letter media are intentionally never treated as compatible.
pub fn check_compatibility(pages: &[PageMedia], capabilities: &Capabilities) -> Result<(), String> {
    let mut supported = capabilities.papers.clone();
    if let Some(default) = &capabilities.default_paper {
        if !supported
            .iter()
            .any(|paper| same_size(paper.width_mm, paper.height_mm, default.width_mm, default.height_mm))
        {
            supported.push(default.clone());
        }
    }

    for (index, page) in pages.iter().enumerate() {
        let (width, height) = page.dimensions_mm();
        let Some((document_size, standard_width, standard_height)) = standard_document_size(page) else {
            return Err(format!(
                "Page {} is {:.1} × {:.1} mm. Incoming documents can print only on A4 or Letter media.",
                index + 1,
                width,
                height
            ));
        };
        if !supported
            .iter()
            .any(|paper| page.matches(paper.width_mm, paper.height_mm))
        {
            let names = supported
                .iter()
                .filter_map(|paper| standard_media_name(paper.width_mm, paper.height_mm))
                .collect::<Vec<_>>();
            let support_text = if names.is_empty() {
                "no A4 or Letter media".to_string()
            } else {
                names.join(" and ")
            };
            return Err(format!(
                "Page {} is {} ({:.1} × {:.1} mm), but this printer supports {}.",
                index + 1,
                document_size,
                standard_width,
                standard_height,
                support_text
            ));
        }
    }
    Ok(())
}

fn standard_document_size(page: &PageMedia) -> Option<(&'static str, f32, f32)> {
    [("A4", 210.0, 297.0), ("Letter", 215.9, 279.4)]
        .into_iter()
        .find(|(_, width, height)| page.matches(*width, *height))
}

fn standard_media_name(width: f32, height: f32) -> Option<&'static str> {
    [("A4", 210.0, 297.0), ("Letter", 215.9, 279.4)]
        .into_iter()
        .find(|(_, w, h)| same_size(width, height, *w, *h))
        .map(|(name, _, _)| name)
}

fn same_size(a_width: f32, a_height: f32, b_width: f32, b_height: f32) -> bool {
    close(a_width, b_width) && close(a_height, b_height) || close(a_width, b_height) && close(a_height, b_width)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= MEDIA_TOLERANCE_MM
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::printer::PaperInfo;

    fn page(width_mm: f32, height_mm: f32) -> PageMedia {
        PageMedia {
            width_pt: width_mm * POINTS_PER_MM,
            height_pt: height_mm * POINTS_PER_MM,
        }
    }

    fn caps(papers: &[(&str, f32, f32)]) -> Capabilities {
        Capabilities {
            papers: papers
                .iter()
                .map(|(name, width_mm, height_mm)| PaperInfo {
                    name: (*name).into(),
                    width_mm: *width_mm,
                    height_mm: *height_mm,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn accepts_a4_and_letter_in_either_orientation() {
        let supported = caps(&[("A4", 210.0, 297.0), ("Letter", 215.9, 279.4)]);
        assert!(check_compatibility(&[page(297.0, 210.0), page(215.9, 279.4)], &supported).is_ok());
    }

    #[test]
    fn rejects_wrong_media_and_names_document_dimensions() {
        let supported = caps(&[("Letter", 215.9, 279.4)]);
        let error = check_compatibility(&[page(210.0, 297.0)], &supported).unwrap_err();
        assert!(error.contains("Page 1 is A4"));
        assert!(error.contains("supports Letter"));
    }

    #[test]
    fn label_and_other_media_are_never_compatible() {
        let zebra = caps(&[("60x75mm", 60.0, 75.0)]);
        assert!(check_compatibility(&[page(60.0, 75.0)], &zebra)
            .unwrap_err()
            .contains("only on A4 or Letter"));
    }

    #[test]
    fn rejects_mixed_document_when_any_page_is_unsupported() {
        let a4_only = caps(&[("A4", 210.0, 297.0)]);
        let error = check_compatibility(&[page(210.0, 297.0), page(215.9, 279.4)], &a4_only).unwrap_err();
        assert!(error.contains("Page 2 is Letter"));
    }
}
