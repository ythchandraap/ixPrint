//! One print job, from uploaded PDF to spooler: resolve settings, lay out,
//! print, record history. Shared by the HTTP endpoint and the app's test print.

use crate::config::ConfigStore;
use crate::history::{History, HistoryEntry};
use crate::settings::{Align, JobSettings, Margins, Orientation, Paper, PrintSettings, Scale};
use crate::{layout, printer, testpage};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct AppState {
    pub config: ConfigStore,
    /// Random in-process credential used only by the desktop window's local
    /// HTTP client. It is never persisted or exposed to the frontend.
    pub desktop_key: String,
    pub remotes: crate::remote::RemoteStore,
    /// Last /list-printer answer per remote id.
    pub remote_cache: crate::remote::RemoteCache,
    /// Why the HTTP server could not start (e.g. port in use); `None` while it runs.
    pub server_error: std::sync::Mutex<Option<String>>,
    /// Handle used to stop the local HTTP bridge without stopping the app.
    pub server_handle: std::sync::Mutex<Option<actix_web::dev::ServerHandle>>,
    pub token_usage: crate::auth::TokenUsage,
    /// Clients locked out after repeatedly sending wrong tokens.
    pub failed_auth: crate::auth::FailedAttempts,
    pub history: History,
    pub diagnostics: crate::diagnostics::Diagnostics,
    /// Scratch space for uploads and print-ready PDFs; files are deleted after each job.
    pub jobs_dir: PathBuf,
}

impl AppState {
    /// The printer used when neither the request nor the preset names one.
    pub fn default_printer(&self) -> Option<String> {
        self.config.get().default_printer.or_else(printer::os_default_printer)
    }
}

#[derive(Debug)]
pub enum JobError {
    /// Invalid request (unknown preset, bad settings, no printer): HTTP 400.
    BadRequest(String),
    /// The document or printer failed: HTTP 500.
    Failed(String),
}

impl JobError {
    pub fn message(&self) -> &str {
        match self {
            JobError::BadRequest(m) | JobError::Failed(m) => m,
        }
    }
}

pub enum Document {
    /// An uploaded PDF.
    File(PathBuf),
    /// Edge-to-edge calibration pages generated at the sheet size of the job.
    TestPage { pages: u32 },
    /// The preset's design filled with the records; `header` values are shared by all pages.
    Data {
        records: Vec<crate::template::Record>,
        header: crate::template::Record,
    },
}

pub struct JobRequest {
    pub document: Document,
    pub file_name: String,
    /// Overrides sent with the request.
    pub settings: PrintSettings,
    pub preset: Option<String>,
    pub source: &'static str,
    pub client: Option<String>,
    /// Name of the API token used, for the history.
    pub token: Option<String>,
    /// True when this request was already forwarded by another ix.print.
    pub hop: bool,
}

#[derive(Serialize, serde::Deserialize, Debug)]
pub struct JobResult {
    pub job_id: String,
    pub printer: String,
    pub preset: Option<String>,
    pub sheets: u32,
    /// The spooler's reference for the job, when it provides one.
    pub spooler_job: Option<String>,
}

pub fn run(state: &AppState, req: JobRequest) -> Result<JobResult, JobError> {
    let started = Instant::now();
    let job_id = uuid::Uuid::new_v4().to_string();
    let config = state.config.get();
    let preset_name = req.preset.clone().or(config.default_preset.clone());

    let mut record = HistoryEntry {
        id: job_id.clone(),
        time: chrono::Local::now().to_rfc3339(),
        source: req.source.to_string(),
        client: req.client.clone(),
        token: req.token.clone(),
        file_name: req.file_name.clone(),
        preset: preset_name.clone(),
        printer: String::new(),
        spooler_job: None,
        sheets: 0,
        settings: serde_json::to_value(&req.settings).unwrap_or_default(),
        success: false,
        error: None,
        duration_ms: 0,
        callback: None,
        remote: None,
    };

    let result = (|| -> Result<JobResult, JobError> {
        let preset = match &preset_name {
            Some(name) => config
                .presets
                .get(name)
                .cloned()
                .ok_or_else(|| JobError::BadRequest(format!("Preset '{}' was not found", name)))?,
            None => Default::default(),
        };
        let merged = preset.settings.overlay(&req.settings);
        merged.validate().map_err(JobError::BadRequest)?;

        let printer_name = merged
            .printer
            .clone()
            .or_else(|| state.default_printer())
            .ok_or_else(|| {
                JobError::BadRequest("No printer: choose one in the request, preset or default printer".into())
            })?;
        record.printer = printer_name.clone();
        let local_names: Vec<String> = printer::list_printers().into_iter().map(|p| p.name).collect();
        match crate::remote::resolve(&local_names, &state.remotes.list(), &printer_name) {
            crate::remote::Target::Local(_) => {}
            crate::remote::Target::Unknown => {
                return Err(JobError::BadRequest(format!(
                    "Printer '{}' was not found on this computer",
                    printer_name
                )));
            }
            crate::remote::Target::Remote { remote, printer } => {
                if req.hop {
                    return Err(JobError::BadRequest(
                        "Forwarding between ix.print computers is limited to one hop".into(),
                    ));
                }
                return crate::remote::forward(
                    state,
                    &job_id,
                    preset_name.as_deref(),
                    &preset,
                    &merged,
                    &req,
                    &remote,
                    &printer,
                    &mut record,
                );
            }
        }
        let mut job = JobSettings::resolve(&merged, printer_name.clone());
        let _active_job = state.diagnostics.begin_job(&job_id, req.source, &printer_name).ok();

        let default_paper = if job.paper == Paper::PrinterDefault {
            printer::default_paper_mm(&printer_name)
        } else {
            (210.0, 297.0)
        };

        let generated = state.jobs_dir.join(format!("{}-test.pdf", job_id));
        let source = match &req.document {
            Document::File(path) => path.clone(),
            Document::Data { records, header } => {
                let template = preset.design.as_ref().ok_or_else(|| {
                    JobError::BadRequest(match &preset_name {
                        Some(name) => format!("Preset '{}' has no design; send a PDF instead of JSON data", name),
                        None => "Specify a preset with a design in the \"preset\" field".into(),
                    })
                })?;
                if records.len() > crate::template::MAX_RECORDS {
                    return Err(JobError::BadRequest(format!(
                        "at most {} records per job",
                        crate::template::MAX_RECORDS
                    )));
                }
                crate::template::render(&generated, template, records, header).map_err(JobError::BadRequest)?;
                // The design is the paper: print it exactly as designed.
                job.paper = design_paper(template);
                job.scale = Scale::None;
                job.margins = Margins::default();
                job.align = Align::Center;
                job.orientation = Orientation::Portrait;
                job.pages = None;
                generated.clone()
            }
            // A preset with a design tests with its design and sample data.
            Document::TestPage { pages } if preset.design.is_some() => {
                let design = preset.design.as_ref().unwrap();
                let records = vec![design.sample.clone(); (*pages).clamp(1, testpage::MAX_TEST_PAGES) as usize];
                crate::template::render(&generated, design, &records, &design.sample).map_err(JobError::BadRequest)?;
                job.paper = design_paper(design);
                job.scale = Scale::None;
                job.margins = Margins::default();
                job.align = Align::Center;
                job.orientation = Orientation::Portrait;
                job.pages = None;
                job.copies = 1;
                generated.clone()
            }
            Document::TestPage { pages } => {
                let (w, h) = test_sheet_mm(&job, default_paper);
                let info = test_page_info(&job, preset_name.as_deref(), w, h);
                testpage::generate(&generated, w, h, &info, *pages).map_err(JobError::Failed)?;
                // The page already has the sheet size: print it 1:1, keeping the
                // offsets so calibration shows their effect.
                job.scale = Scale::None;
                job.margins = Margins::default();
                job.align = Align::Center;
                job.orientation = Orientation::Auto;
                job.pages = None;
                job.copies = 1;
                generated.clone()
            }
        };
        check_paper_fits(&job).map_err(JobError::BadRequest)?;
        record.settings = serde_json::to_value(&job).unwrap_or_default();

        let ready = state.jobs_dir.join(format!("{}-ready.pdf", job_id));
        let outcome = print_ready(&source, &ready, &job, default_paper, &req.file_name);
        let _ = std::fs::remove_file(&ready);
        let _ = std::fs::remove_file(&generated);
        let (sheets, spooler_job) = outcome.map_err(JobError::Failed)?;

        record.sheets = sheets;
        record.spooler_job = spooler_job.clone();
        Ok(JobResult {
            job_id: job_id.clone(),
            printer: printer_name,
            preset: preset_name.clone(),
            sheets,
            spooler_job,
        })
    })();

    // Generated pages may hold patient data: remove them even when the job failed early.
    let _ = std::fs::remove_file(state.jobs_dir.join(format!("{}-test.pdf", job_id)));
    record.success = result.is_ok();
    record.error = result.as_ref().err().map(|e| e.message().to_string());
    record.duration_ms = started.elapsed().as_millis() as u64;
    if let Err(e) = state.history.append(&record) {
        eprintln!("Failed to write history: {}", e);
    }
    if let Err(e) = &result {
        let _ = state.diagnostics.record("error", req.source, e.message(), None);
    }
    result
}

/// Paper for a design: its page, or a roll of its width when a list grows with its rows.
pub(crate) fn design_paper(t: &crate::template::Template) -> Paper {
    match t.layout {
        crate::template::Layout::List { roll: true, .. } => Paper::Roll { width_mm: t.width_mm },
        _ => Paper::Custom {
            width_mm: t.width_mm,
            height_mm: t.height_mm,
        },
    }
}

/// Rejects custom/roll sizes the driver can't take, instead of letting it
/// silently clamp or rescale them (e.g. 600x750mm typed for 60x75mm).
fn check_paper_fits(job: &JobSettings) -> Result<(), String> {
    let (w, h) = match job.paper {
        Paper::Custom { width_mm, height_mm } => (width_mm, Some(height_mm)),
        Paper::Roll { width_mm } => (width_mm, None),
        _ => return Ok(()),
    };
    let Ok(caps) = printer::capabilities(&job.printer) else {
        return Ok(());
    };
    let too_wide = caps.max_width_mm.is_some_and(|max| w > max + 0.5);
    let too_long = matches!((h, caps.max_height_mm), (Some(h), Some(max)) if h > max + 0.5);
    if too_wide || too_long {
        let max = |v: Option<f32>| v.map(|m| format!("{:.0}", m)).unwrap_or_else(|| "?".into());
        return Err(format!(
            "Paper size {} is larger than printer '{}' maximum ({} × {} mm). Check the units (mm).",
            job.paper,
            job.printer,
            max(caps.max_width_mm),
            max(caps.max_height_mm)
        ));
    }
    Ok(())
}

/// Sheet size for the test page, in mm, as the job would print it.
fn test_sheet_mm(job: &JobSettings, default_paper: (f32, f32)) -> (f32, f32) {
    if let Paper::Roll { width_mm } = job.paper {
        return (width_mm, width_mm.max(80.0));
    }
    // A custom size is the physical label: exactly as entered.
    if let Paper::Custom { width_mm, height_mm } = job.paper {
        return (width_mm, height_mm);
    }
    let (w, h) = job.paper.size_mm().unwrap_or(default_paper);
    let (short, long) = (w.min(h), w.max(h));
    match job.orientation {
        Orientation::Landscape => (long, short),
        Orientation::Portrait | Orientation::Auto => (short, long),
    }
}

fn test_page_info(job: &JobSettings, preset: Option<&str>, w: f32, h: f32) -> Vec<String> {
    let paper = match &job.paper {
        Paper::Roll { .. } => format!("{:.0} mm roll (example length {:.0} mm)", w, h),
        Paper::PrinterDefault => format!("{:.0} × {:.0} mm (default printer)", w, h),
        p => format!("{} = {:.0} × {:.0} mm", p, w, h),
    };
    vec![
        format!("Paper: {}", paper),
        format!("Printer: {}", job.printer),
        format!("Preset: {}", preset.unwrap_or("—")),
        format!("Offset X: {} mm · Offset Y: {} mm", job.offset_x, job.offset_y),
        chrono::Local::now().format("%d-%m-%Y %H:%M:%S").to_string(),
    ]
}

fn print_ready(
    source: &Path,
    ready: &Path,
    job: &JobSettings,
    default_paper: (f32, f32),
    file_name: &str,
) -> Result<(u32, Option<String>), String> {
    let layout = layout::build(source, ready, job, default_paper)?;
    let options = printer::DeviceOptions {
        job_name: file_name,
        copies: job.copies,
        collate: job.collate,
        paper: &job.paper,
        duplex: job.duplex,
        color: job.color,
        tray: job.tray.as_deref(),
        dpi: job.dpi,
    };
    let spooler_job = printer::print(&job.printer, ready, &layout.sheets, &options)?;
    Ok((layout.sheets.len() as u32 * job.copies, spooler_job))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::pt_to_mm;
    use pdfium_render::prelude::*;

    /// The calibration page must come out of the layout engine at the sheet size,
    /// with its frame exactly on the paper edge.
    #[test]
    fn test_page_frame_lands_on_sheet_edge() {
        crate::layout::tests::init_for_tests();
        let dir = std::env::temp_dir().join(format!("ixprint-tp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cases = [
            (
                r#"{"paper":"A4","orientation":"landscape","margins":"10","scale":"fit"}"#,
                (297.0, 210.0),
            ),
            (r#"{"paper":"80mm"}"#, (80.0, 80.0)),
            (r#"{"paper":"100x150mm","orientation":"auto"}"#, (100.0, 150.0)),
        ];
        for (i, (json, (ew, eh))) in cases.iter().enumerate() {
            let s: PrintSettings = serde_json::from_str(json).unwrap();
            let mut job = JobSettings::resolve(&s, "test".into());
            let (w, h) = test_sheet_mm(&job, (210.0, 297.0));
            let src = dir.join(format!("t{}.pdf", i));
            testpage::generate(&src, w, h, &test_page_info(&job, None, w, h), 1).unwrap();
            job.scale = Scale::None;
            job.margins = Margins::default();
            job.orientation = Orientation::Auto;
            let out = dir.join(format!("o{}.pdf", i));
            let result = layout::build(&src, &out, &job, (210.0, 297.0)).unwrap();
            let sheet = result.sheets[0];
            assert!((pt_to_mm(sheet.width_pt) - ew).abs() < 0.2, "{} width", json);
            assert!((pt_to_mm(sheet.height_pt) - eh).abs() < 0.2, "{} height", json);

            // Render at 4 px/pt: the outermost pixels on every side must be the black frame.
            let pdfium = layout::pdfium().unwrap();
            let doc = pdfium.load_pdf_from_file(&out, None).unwrap();
            let page = doc.pages().get(0).unwrap();
            let (pw, ph) = ((sheet.width_pt * 4.0) as i32, (sheet.height_pt * 4.0) as i32);
            let bmp = page
                .render_with_config(&PdfRenderConfig::new().set_fixed_size(pw, ph))
                .unwrap();
            let px = bmp.as_rgba_bytes();
            let dark = |x: i32, y: i32| px[((y * pw + x) * 4) as usize] < 80;
            for (x, y) in [(1, ph / 3), (pw - 2, ph / 3), (pw / 3, 1), (pw / 3, ph - 2)] {
                assert!(dark(x, y), "{}: frame missing at ({}, {})", json, x, y);
            }
            // ...and just inside the frame it is white again.
            assert!(!dark(12, ph / 3), "{}: frame too thick", json);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod live_probe {
    use super::*;

    /// `ixPrint_PROBE_PRINTER=<name> cargo test --lib paper_limits -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn paper_limits() {
        let printer = std::env::var("ixPrint_PROBE_PRINTER").unwrap();
        let caps = printer::capabilities(&printer).unwrap();
        println!("max: {:?} x {:?} mm", caps.max_width_mm, caps.max_height_mm);
        for paper in ["600x750mm", "60x75mm", "80mm", "300mm"] {
            let s = PrintSettings {
                paper: Some(paper.parse().unwrap()),
                ..Default::default()
            };
            println!(
                "{}: {:?}",
                paper,
                check_paper_fits(&JobSettings::resolve(&s, printer.clone()))
            );
        }
    }
}
