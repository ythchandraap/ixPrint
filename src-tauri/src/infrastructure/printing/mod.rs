//! OS printer adapter dispatch and platform-neutral printer operations.

use crate::application::ports::PrinterGateway;
use crate::domain::layout::Sheet;
use crate::domain::printer::{Capabilities, DeviceOptions, JobState, JobStatus, Printer, PrinterQueue};
use std::path::Path;

pub mod admin;
#[cfg(not(windows))]
#[path = "admin_cups.rs"]
mod admin_cups;
#[cfg(windows)]
#[path = "admin_windows.rs"]
mod admin_windows;
#[cfg(not(windows))]
mod cups;
#[cfg(not(windows))]
use self::cups as backend;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as backend;

pub fn queue(printer: &str) -> Result<PrinterQueue, String> {
    backend::queue(printer)
}

pub fn cancel(printer: &str, spooler_job: &str) -> Result<(), String> {
    backend::cancel(printer, spooler_job)
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
            (
                state,
                Some(job.position),
                job.message.clone().or_else(|| q.message.clone()),
            )
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
pub fn print(printer: &str, pdf: &Path, sheets: &[Sheet], options: &DeviceOptions) -> Result<Option<String>, String> {
    backend::print(printer, pdf, sheets, options)
}

#[derive(Default)]
pub struct LocalPrinterGateway;

impl PrinterGateway for LocalPrinterGateway {
    fn list_printers(&self) -> Vec<Printer> {
        list_printers()
    }

    fn os_default_printer(&self) -> Option<String> {
        os_default_printer()
    }

    fn capabilities(&self, printer: &str) -> Result<Capabilities, String> {
        capabilities(printer)
    }

    fn default_paper_mm(&self, printer: &str) -> (f32, f32) {
        default_paper_mm(printer)
    }

    fn print(
        &self,
        printer: &str,
        pdf: &Path,
        sheets: &[Sheet],
        options: &DeviceOptions,
    ) -> Result<Option<String>, String> {
        print(printer, pdf, sheets, options)
    }

    fn queue(&self, printer: &str) -> Result<PrinterQueue, String> {
        queue(printer)
    }

    fn job_status(&self, printer: &str, spooler_job: &str) -> Result<JobStatus, String> {
        job_status(printer, spooler_job)
    }

    fn cancel(&self, printer: &str, spooler_job: &str) -> Result<(), String> {
        cancel(printer, spooler_job)
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
                Ok(c) => println!(
                    "  papers={} default={:?} trays={:?} duplex={} color={}",
                    c.papers.len(),
                    c.default_paper,
                    c.trays,
                    c.duplex,
                    c.color
                ),
                Err(e) => println!("  caps error: {}", e),
            }
            println!("  default_paper_mm={:?}", super::default_paper_mm(&p.name));
        }
        println!("os default: {:?}", super::os_default_printer());
        for p in super::list_printers() {
            println!(
                "queue {}: {:?}",
                p.name,
                super::queue(&p.name).map(|q| (q.status, q.message, q.jobs.len()))
            );
        }
        if let Ok(job) = std::env::var("ixPrint_PROBE_JOB") {
            let (printer, id) = job.split_once('|').unwrap();
            println!("job {}: {:?}", id, super::job_status(printer, id));
        }
    }
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod job_tests;
