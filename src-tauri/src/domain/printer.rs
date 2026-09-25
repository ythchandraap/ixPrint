use serde::{Deserialize, Serialize};

use crate::domain::settings::{ColorMode, Duplex, Paper};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Printer {
    pub name: String,
    pub status: String,
    /// Human-readable problem reported by the printer (paper out, offline, ...).
    pub message: Option<String>,
    pub shared: String,
    /// The operating system's default printer.
    pub os_default: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaperInfo {
    /// The printer's own name for the size (use it as-is in `paper` when it is
    /// a standard name, otherwise use the dimensions).
    pub name: String,
    pub width_mm: f32,
    pub height_mm: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Capabilities {
    pub papers: Vec<PaperInfo>,
    pub default_paper: Option<PaperInfo>,
    pub trays: Vec<String>,
    pub default_tray: Option<String>,
    pub duplex: bool,
    pub color: bool,
    /// Largest custom size the driver accepts (width across the feed, length along it).
    pub max_width_mm: Option<f32>,
    pub max_height_mm: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Printing,
    Paused,
    /// The job or the printer reports a problem; the job may resume once fixed.
    Error,
    Completed,
    /// Gone from the queue without a known outcome (e.g. cancelled).
    Gone,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QueueJob {
    /// Spooler job id, as returned when printing.
    pub id: String,
    pub document: String,
    /// 1 = next to print.
    pub position: u32,
    pub state: JobState,
    pub message: Option<String>,
    pub pages: Option<u32>,
    pub pages_printed: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PrinterQueue {
    pub printer: String,
    pub status: String,
    pub message: Option<String>,
    pub jobs: Vec<QueueJob>,
}

/// Where one job is: in the queue (with position), finished, or stuck.
#[derive(Serialize, Clone, Debug)]
pub struct JobStatus {
    pub state: JobState,
    pub position: Option<u32>,
    /// Jobs that will print before this one.
    pub ahead: Option<u32>,
    pub message: Option<String>,
    pub printer_status: String,
    pub printer_message: Option<String>,
}

/// Printer-side options for one job.
pub struct DeviceOptions<'a> {
    pub job_name: &'a str,
    pub copies: u32,
    pub collate: bool,
    /// What the user asked for; the actual sheet sizes are in the print-ready PDF.
    pub paper: &'a Paper,
    pub duplex: Option<Duplex>,
    pub color: Option<ColorMode>,
    pub tray: Option<&'a str>,
    /// Windows raster resolution; `None` = printer resolution capped at 300 dpi.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub dpi: Option<u32>,
}

/// Parses paper names that carry their own size, as used by CUPS/IPP and
/// many drivers: PWG names (`iso_a4_210x297mm`, `na_letter_8.5x11in`),
/// PPD point sizes (`w288h432`) and plain `100x150mm`.
#[allow(dead_code)]
pub(crate) fn paper_size_from_name(name: &str) -> Option<(f32, f32)> {
    let lower = name.to_ascii_lowercase();
    // Drop variant suffixes such as ".Fullbleed", but not decimals like "8.5x11in".
    let base = match lower.rsplit_once('.') {
        Some((head, tail)) if tail.chars().all(|c| c.is_ascii_alphabetic()) => head,
        _ => lower.as_str(),
    };
    if let Some(&(_, w, h)) = crate::domain::settings::STANDARD_PAPERS
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(base))
    {
        return Some((w, h));
    }
    let known: &[(&str, f32, f32)] = &[
        ("executive", 184.15, 266.7),
        ("tabloid", 279.4, 431.8),
        ("ledger", 279.4, 431.8),
        ("statement", 139.7, 215.9),
        ("env10", 104.8, 241.3),
        ("envdl", 110.0, 220.0),
        ("envc5", 162.0, 229.0),
    ];
    if let Some(&(_, w, h)) = known.iter().find(|(n, _, _)| *n == base) {
        return Some((w, h));
    }
    // PPD style: w<points>h<points>
    if let Some(rest) = base.strip_prefix('w') {
        if let Some((w, h)) = rest.split_once('h') {
            if let (Ok(w), Ok(h)) = (w.parse::<f32>(), h.parse::<f32>()) {
                return Some((
                    crate::domain::settings::pt_to_mm(w),
                    crate::domain::settings::pt_to_mm(h),
                ));
            }
        }
    }
    // PWG style: the last `_` segment is `<w>x<h><unit>`.
    let dims = base.rsplit('_').next().unwrap_or(base);
    let (unit, factor) = if dims.ends_with("mm") {
        ("mm", 1.0)
    } else if dims.ends_with("in") {
        ("in", 25.4)
    } else {
        return None;
    };
    let (w, h) = dims.trim_end_matches(unit).split_once('x')?;
    Some((w.parse::<f32>().ok()? * factor, h.parse::<f32>().ok()? * factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_names() {
        assert_eq!(paper_size_from_name("A4"), Some((210.0, 297.0)));
        assert_eq!(paper_size_from_name("iso_a5_148x210mm"), Some((148.0, 210.0)));
        let (w, h) = paper_size_from_name("na_letter_8.5x11in").unwrap();
        assert!((w - 215.9).abs() < 0.01 && (h - 279.4).abs() < 0.01);
        let (w, h) = paper_size_from_name("w288h432").unwrap();
        assert!((w - 101.6).abs() < 0.01 && (h - 152.4).abs() < 0.01);
        assert_eq!(paper_size_from_name("A4.Fullbleed"), Some((210.0, 297.0)));
        assert_eq!(paper_size_from_name("Custom.WIDTHxHEIGHT"), None);
    }
}
