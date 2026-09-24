//! macOS / Linux backend: CUPS command-line tools. Arguments are passed
//! directly (no shell), so printer and file names cannot inject commands.

use crate::domain::layout::Sheet;
use crate::domain::printer::{
    paper_size_from_name, Capabilities, DeviceOptions, JobState, PaperInfo, Printer, PrinterQueue, QueueJob,
};
use crate::domain::settings::{pt_to_mm, ColorMode, Duplex, Paper};
use std::path::Path;
use std::process::Command;

/// Runs a CUPS tool with untranslated output so it can be parsed.
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("Failed to run {}: {}", program, e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Explains CUPS printer/job alerts; `None` for alerts that are not problems.
fn alert_message(alerts: &str) -> Option<(&'static str, bool)> {
    // (message, is_error)
    const KNOWN: &[(&str, &str, bool)] = &[
        ("media-empty", "Out of paper", true),
        ("media-needed", "Out of paper", true),
        ("media-jam", "Paper jam", true),
        ("toner-empty", "Ink/toner empty", true),
        ("marker-supply-empty", "Ink/toner empty", true),
        ("door-open", "Printer door is open", true),
        ("cover-open", "Printer cover is open", true),
        ("offline", "Printer offline / not responding", true),
        ("other-error", "Printer error", true),
        ("paused", "Printer paused", true),
        ("toner-low", "Ink/toner is low", false),
        ("marker-supply-low", "Ink/toner is low", false),
        ("connecting-to-device", "Connecting to printer…", false),
    ];
    KNOWN
        .iter()
        .find(|(key, _, _)| alerts.contains(key))
        .map(|&(_, msg, err)| (msg, err))
}

struct PrinterState {
    name: String,
    status: String,
    message: Option<String>,
    /// Job id after "now printing".
    printing: Option<String>,
}

/// Parses `lpstat -l -p` output (one block per printer).
fn parse_printer_states(text: &str) -> Vec<PrinterState> {
    let mut out: Vec<PrinterState> = Vec::new();
    let mut in_header = false; // lines between the "printer" line and "Form mounted:"
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("printer ") {
            let Some((name, state)) = rest.split_once(' ') else {
                continue;
            };
            let printing = state
                .strip_prefix("now printing ")
                .and_then(|r| r.split([' ', '.']).next())
                .filter(|id| !id.is_empty())
                .map(str::to_string);
            let status = if state.contains("disabled") {
                "Offline"
            } else if printing.is_some() {
                "Printing"
            } else {
                "Normal"
            };
            out.push(PrinterState {
                name: name.to_string(),
                status: status.into(),
                message: None,
                printing,
            });
            in_header = true;
            continue;
        }
        let Some(current) = out.last_mut() else { continue };
        let trimmed = line.trim();
        if trimmed.starts_with("Form mounted:") {
            in_header = false;
        } else if let Some(alerts) = trimmed.strip_prefix("Alerts:") {
            if let Some((msg, is_error)) = alert_message(alerts) {
                current.message.get_or_insert_with(|| msg.to_string());
                if is_error && current.status == "Normal" {
                    current.status = if alerts.contains("offline") { "Offline" } else { "Error" }.into();
                }
            }
        } else if in_header && !trimmed.is_empty() && current.message.is_none() {
            // printer-state-message printed right below the state line
            current.message = Some(trimmed.to_string());
        }
    }
    out
}

pub fn list_printers() -> Vec<Printer> {
    let Ok(stdout) = run("lpstat", &["-l", "-p"]) else {
        return vec![];
    };
    parse_printer_states(&stdout)
        .into_iter()
        .map(|p| {
            let shared = run("lpoptions", &["-p", &p.name])
                .map(|opts| opts.contains("printer-is-shared=true"))
                .unwrap_or(false);
            Printer {
                name: p.name,
                status: p.status,
                message: p.message,
                shared: if shared { "True" } else { "False" }.to_string(),
                os_default: false,
            }
        })
        .collect()
}

/// Parses `lpstat -l -o` output: a header line per job followed by indented details.
fn parse_jobs(text: &str, printing: Option<&str>) -> Vec<QueueJob> {
    let mut jobs: Vec<QueueJob> = Vec::new();
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) {
            let Some(id) = line.split_whitespace().next() else {
                continue;
            };
            jobs.push(QueueJob {
                id: id.to_string(),
                document: String::new(),
                position: jobs.len() as u32 + 1,
                state: if Some(id) == printing {
                    JobState::Printing
                } else {
                    JobState::Queued
                },
                message: None,
                pages: None,
                pages_printed: None,
            });
            continue;
        }
        let Some(job) = jobs.last_mut() else { continue };
        let trimmed = line.trim();
        if let Some(status) = trimmed.strip_prefix("Status:") {
            let status = status.trim();
            if !status.is_empty() {
                job.message = Some(status.to_string());
            }
        } else if let Some(alerts) = trimmed.strip_prefix("Alerts:") {
            if alerts.contains("job-printing") {
                job.state = JobState::Printing;
            } else if alerts.contains("held") || alerts.contains("hold-until") || alerts.contains("suspended") {
                job.state = JobState::Paused;
            } else if alerts.contains("stopped") || alerts.contains("error") {
                job.state = JobState::Error;
            }
        }
    }
    jobs
}

/// Document names from `lpq`: `Rank Owner Job File(s) Total Size`.
fn job_documents(printer: &str) -> Vec<(String, String)> {
    let Ok(text) = run("lpq", &["-P", printer]) else {
        return vec![];
    };
    text.lines()
        .skip_while(|l| !l.starts_with("Rank"))
        .skip(1)
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            (t.len() >= 6).then(|| (t[2].to_string(), t[3..t.len() - 2].join(" ")))
        })
        .collect()
}

pub fn queue(printer: &str) -> Result<PrinterQueue, String> {
    let states =
        run("lpstat", &["-l", "-p", printer]).map_err(|e| format!("Printer '{}' was not found: {}", printer, e))?;
    let state = parse_printer_states(&states)
        .into_iter()
        .next()
        .ok_or_else(|| format!("Printer '{}' was not found", printer))?;
    let mut jobs = parse_jobs(
        &run("lpstat", &["-l", "-o", printer]).unwrap_or_default(),
        state.printing.as_deref(),
    );
    let docs = job_documents(printer);
    for job in &mut jobs {
        let number = job.id.rsplit('-').next().unwrap_or_default();
        if let Some((_, doc)) = docs.iter().find(|(n, _)| n == number) {
            job.document = doc.clone();
        }
    }
    Ok(PrinterQueue {
        printer: printer.to_string(),
        status: state.status,
        message: state.message,
        jobs,
    })
}

pub fn was_completed(printer: &str, spooler_job: &str) -> bool {
    run("lpstat", &["-W", "completed", "-o", printer])
        .map(|text| text.lines().any(|l| l.split_whitespace().next() == Some(spooler_job)))
        .unwrap_or(false)
}

pub fn os_default_printer() -> Option<String> {
    // "system default destination: NAME"
    run("lpstat", &["-d"])
        .ok()?
        .lines()
        .find_map(|l| l.split_once("destination: ").map(|(_, n)| n.trim().to_string()))
        .filter(|n| !n.is_empty())
}

/// One `lpoptions -l` line: `Key/Label: value *default value`.
struct PpdOption {
    key: String,
    values: Vec<String>,
    default: Option<String>,
}

fn ppd_options(printer: &str) -> Result<Vec<PpdOption>, String> {
    let text =
        run("lpoptions", &["-p", printer, "-l"]).map_err(|e| format!("Printer '{}' was not found: {}", printer, e))?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let (head, values) = line.split_once(':')?;
            let key = head.split('/').next()?.trim().to_string();
            let mut default = None;
            let values = values
                .split_whitespace()
                .map(|v| match v.strip_prefix('*') {
                    Some(v) => {
                        default = Some(v.to_string());
                        v.to_string()
                    }
                    None => v.to_string(),
                })
                .collect();
            Some(PpdOption { key, values, default })
        })
        .collect())
}

pub fn capabilities(printer: &str) -> Result<Capabilities, String> {
    let options = ppd_options(printer)?;
    let find = |keys: &[&str]| options.iter().find(|o| keys.contains(&o.key.as_str()));

    let paper_info = |name: &str| {
        paper_size_from_name(name).map(|(w, h)| PaperInfo {
            name: name.to_string(),
            width_mm: w,
            height_mm: h,
        })
    };
    let (papers, default_paper) = match find(&["PageSize", "media"]) {
        Some(o) => (
            o.values.iter().filter_map(|v| paper_info(v)).collect(),
            o.default.as_deref().and_then(paper_info),
        ),
        None => (vec![], None),
    };
    let (trays, default_tray) = match find(&["InputSlot", "MediaSource"]) {
        Some(o) => (o.values.clone(), o.default.clone()),
        None => (vec![], None),
    };
    let duplex = find(&["Duplex", "sides", "EFDuplex", "KMDuplex"]).is_some_and(|o| {
        o.values
            .iter()
            .any(|v| !matches!(v.as_str(), "None" | "False" | "one-sided"))
    });
    let color = find(&["ColorModel", "print-color-mode", "ColorMode"]).is_some_and(|o| {
        o.values.iter().any(|v| {
            let v = v.to_ascii_lowercase();
            v.contains("rgb") || v.contains("cmy") || v == "color"
        })
    });

    let (max_width_mm, max_height_mm) = ppd_max_media(printer);
    Ok(Capabilities {
        papers,
        default_paper,
        trays,
        default_tray,
        duplex,
        color,
        max_width_mm,
        max_height_mm,
    })
}

/// `*MaxMediaWidth` / `*MaxMediaHeight` (points) from the printer's PPD, if readable.
fn ppd_max_media(printer: &str) -> (Option<f32>, Option<f32>) {
    let Ok(ppd) = std::fs::read_to_string(format!("/etc/cups/ppd/{}.ppd", printer)) else {
        return (None, None);
    };
    let value = |key: &str| {
        ppd.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| {
                v.trim()
                    .trim_start_matches(':')
                    .trim()
                    .trim_matches('"')
                    .parse::<f32>()
                    .ok()
            })
            .map(pt_to_mm)
    };
    (value("*MaxMediaWidth"), value("*MaxMediaHeight"))
}

/// CUPS `media` value for the job, preferring the printer's own name for a
/// size it knows so drivers pick the right tray/cutter.
///
/// Standard papers are matched in either orientation (the driver rotates
/// landscape pages). Custom and roll sizes are sent exactly as entered —
/// width across the feed × length — because label printers can't rotate.
fn media_option(printer: &str, paper: &Paper, sheets: &[Sheet]) -> Option<String> {
    let exact = !matches!(paper, Paper::Standard(_));
    let (w, h) = match paper {
        Paper::PrinterDefault => return None,
        Paper::Roll { width_mm } => {
            let longest = sheets.iter().map(|s| s.height_pt).fold(0.0f32, f32::max);
            (*width_mm, pt_to_mm(longest))
        }
        Paper::Custom { width_mm, height_mm } => (*width_mm, *height_mm),
        Paper::Standard(_) => {
            let (w, h) = paper.size_mm()?;
            (w.min(h), w.max(h))
        }
    };
    let close = |a: f32, b: f32| (a - b).abs() < 1.0;
    if let Ok(caps) = capabilities(printer) {
        let found = caps.papers.iter().find(|p| {
            if exact {
                close(p.width_mm, w) && close(p.height_mm, h)
            } else {
                close(p.width_mm.min(p.height_mm), w) && close(p.width_mm.max(p.height_mm), h)
            }
        });
        if let Some(p) = found {
            return Some(p.name.clone());
        }
    }
    Some(format!("Custom.{:.1}x{:.1}mm", w, h))
}

pub fn print(printer: &str, pdf: &Path, sheets: &[Sheet], o: &DeviceOptions) -> Result<Option<String>, String> {
    let mut args: Vec<String> = vec![
        "-d".into(),
        printer.into(),
        "-t".into(),
        o.job_name.into(),
        "-n".into(),
        o.copies.to_string(),
        "-o".into(),
        format!("collate={}", o.collate),
        // The PDF is already laid out per sheet; stop CUPS from rescaling it.
        "-o".into(),
        "print-scaling=none".into(),
    ];
    if let Some(media) = media_option(printer, o.paper, sheets) {
        args.extend(["-o".into(), format!("media={}", media)]);
    }
    if let Some(duplex) = o.duplex {
        let sides = match duplex {
            Duplex::Off => "one-sided",
            Duplex::LongEdge => "two-sided-long-edge",
            Duplex::ShortEdge => "two-sided-short-edge",
        };
        args.extend(["-o".into(), format!("sides={}", sides)]);
    }
    if let Some(color) = o.color {
        let mode = match color {
            ColorMode::Color => "color",
            ColorMode::Grayscale => "monochrome",
        };
        args.extend(["-o".into(), format!("print-color-mode={}", mode)]);
    }
    if let Some(tray) = o.tray {
        args.extend(["-o".into(), format!("InputSlot={}", tray)]);
    }
    args.push("--".into());
    args.push(pdf.to_string_lossy().into_owned());

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let stdout = run("lp", &arg_refs).map_err(|e| format!("Failed to send to printer: {}", e))?;
    // "request id is PRINTER-123 (1 file(s))"
    Ok(stdout
        .split_once("request id is ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_printer_states() {
        let text = "printer EPSON is idle.  enabled since Wed Mar 19 10:52:28 2025\n\tForm mounted:\n\tAlerts: offline-report\n\tDescription: EPSON\nprinter HP now printing HP-12.  enabled since Mon\n\tRendering page 2\n\tForm mounted:\n\tAlerts: none\nprinter Z disabled since Mon -\n\tPaused by admin\n\tForm mounted:\n\tAlerts: media-empty-error\n";
        let s = parse_printer_states(text);
        assert_eq!(s.len(), 3);
        assert_eq!(
            (s[0].status.as_str(), s[0].message.as_deref()),
            ("Offline", Some("Printer offline / not responding"))
        );
        assert_eq!(
            (s[1].status.as_str(), s[1].printing.as_deref()),
            ("Printing", Some("HP-12"))
        );
        assert_eq!(s[1].message.as_deref(), Some("Rendering page 2"));
        assert_eq!(
            (s[2].status.as_str(), s[2].message.as_deref()),
            ("Offline", Some("Paused by admin"))
        );
    }

    #[test]
    fn parses_jobs() {
        let text = "HP-12 datin 1024 Mon Sep 22 10:00:00 2026\n\tStatus: \n\tAlerts: job-printing\n\tqueued for HP\nHP-13 datin 2048 Mon Sep 22 10:01:00 2026\n\tStatus: Waiting for printer\n\tAlerts: none\n\tqueued for HP\n";
        let jobs = parse_jobs(text, None);
        assert_eq!(jobs.len(), 2);
        assert_eq!((jobs[0].state, jobs[0].position), (JobState::Printing, 1));
        assert_eq!((jobs[1].state, jobs[1].position), (JobState::Queued, 2));
        assert_eq!(jobs[1].message.as_deref(), Some("Waiting for printer"));
    }
}
