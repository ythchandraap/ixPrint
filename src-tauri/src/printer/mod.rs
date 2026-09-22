//! Printer access, per platform. Everything here works on the print-ready PDF
//! produced by [`crate::layout`], so backends only have to print sheets 1:1.
//!
//! - Windows: PDFium renders each sheet, GDI sends it to the printer.
//! - macOS / Linux: the print-ready PDF goes to CUPS (`lp`), vector intact.

use crate::layout::Sheet;
use crate::settings::{ColorMode, Duplex, Paper};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub mod manage;
#[cfg(not(windows))]
mod manage_cups;
#[cfg(windows)]
mod manage_windows;

#[cfg(not(windows))]
mod cups;
#[cfg(not(windows))]
use cups as backend;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as backend;

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

pub fn queue(printer: &str) -> Result<PrinterQueue, String> {
    backend::queue(printer)
}

pub fn job_status(printer: &str, spooler_job: &str) -> Result<JobStatus, String> {
    let q = queue(printer)?;
    let (state, position, message) = match q.jobs.iter().find(|j| j.id == spooler_job) {
        Some(job) => {
            // A printer problem (paper out, offline) blocks every job in its queue.
            // Messages while printing ("Finished page 1.") are progress, not problems.
            let printer_problem = matches!(q.status.as_str(), "Error" | "Offline");
            let state = if job.state != JobState::Paused && printer_problem {
                JobState::Error
            } else {
                job.state
            };
            (state, Some(job.position), job.message.clone().or_else(|| q.message.clone()))
        }
        None if backend::was_completed(printer, spooler_job) => (JobState::Completed, None, None),
        None => (JobState::Gone, None, None),
    };
    Ok(JobStatus {
        state,
        position,
        ahead: position.map(|p| p.saturating_sub(1)),
        message,
        printer_status: q.status,
        printer_message: q.message,
    })
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

pub fn list_printers() -> Vec<Printer> {
    let default = os_default_printer();
    let mut printers = backend::list_printers();
    for p in &mut printers {
        p.os_default = default.as_deref() == Some(p.name.as_str());
    }
    printers
}

pub fn os_default_printer() -> Option<String> {
    backend::os_default_printer()
}

pub fn capabilities(printer: &str) -> Result<Capabilities, String> {
    backend::capabilities(printer)
}

/// Portrait size of the paper currently loaded/configured, falling back to A4.
pub fn default_paper_mm(printer: &str) -> (f32, f32) {
    capabilities(printer)
        .ok()
        .and_then(|c| c.default_paper)
        .map(|p| (p.width_mm.min(p.height_mm), p.width_mm.max(p.height_mm)))
        .unwrap_or((210.0, 297.0))
}

/// Sends the print-ready PDF at `pdf` to `printer`. Returns the spooler's job
/// reference when it gives one.
pub fn print(
    printer: &str,
    pdf: &Path,
    sheets: &[Sheet],
    options: &DeviceOptions,
) -> Result<Option<String>, String> {
    backend::print(printer, pdf, sheets, options)
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
    if let Some(&(_, w, h)) = crate::settings::STANDARD_PAPERS
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
                return Some((crate::settings::pt_to_mm(w), crate::settings::pt_to_mm(h)));
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

#[cfg(test)]
mod live_probe {
    #[test]
    #[ignore]
    fn show_local_printers() {
        for p in super::list_printers() {
            println!("{:?}", p);
            match super::capabilities(&p.name) {
                Ok(c) => println!("  papers={} default={:?} trays={:?} duplex={} color={}", c.papers.len(), c.default_paper, c.trays, c.duplex, c.color),
                Err(e) => println!("  caps error: {}", e),
            }
            println!("  default_paper_mm={:?}", super::default_paper_mm(&p.name));
        }
        println!("os default: {:?}", super::os_default_printer());
        for p in super::list_printers() {
            println!("queue {}: {:?}", p.name, super::queue(&p.name).map(|q| (q.status, q.message, q.jobs.len())));
        }
        if let Ok(job) = std::env::var("ixPrint_PROBE_JOB") {
            let (printer, id) = job.split_once('|').unwrap();
            println!("job {}: {:?}", id, super::job_status(printer, id));
        }
    }
}
