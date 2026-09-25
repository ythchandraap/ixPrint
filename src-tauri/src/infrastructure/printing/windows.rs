//! Windows printer spooler listing and queue adapter.

use self::shared::{on_spooler_thread, PrinterHandle};
use crate::domain::printer::{JobState, Printer, PrinterQueue, QueueJob};
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Graphics::Printing::{
    EnumJobsW, EnumPrintersW, GetDefaultPrinterW, GetPrinterW, SetJobW, JOB_CONTROL_DELETE, JOB_INFO_1W,
    JOB_STATUS_BLOCKED_DEVQ, JOB_STATUS_COMPLETE, JOB_STATUS_DELETED, JOB_STATUS_DELETING, JOB_STATUS_ERROR,
    JOB_STATUS_OFFLINE, JOB_STATUS_PAPEROUT, JOB_STATUS_PAUSED, JOB_STATUS_PRINTED, JOB_STATUS_PRINTING,
    JOB_STATUS_USER_INTERVENTION, PRINTER_ATTRIBUTE_SHARED, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL,
    PRINTER_INFO_2W, PRINTER_STATUS_DOOR_OPEN, PRINTER_STATUS_ERROR, PRINTER_STATUS_NOT_AVAILABLE,
    PRINTER_STATUS_NO_TONER, PRINTER_STATUS_OFFLINE, PRINTER_STATUS_OUT_OF_MEMORY, PRINTER_STATUS_PAPER_JAM,
    PRINTER_STATUS_PAPER_OUT, PRINTER_STATUS_PAPER_PROBLEM, PRINTER_STATUS_PAUSED, PRINTER_STATUS_PRINTING,
    PRINTER_STATUS_SERVER_OFFLINE, PRINTER_STATUS_TONER_LOW, PRINTER_STATUS_USER_INTERVENTION,
};

mod capabilities;
mod printing;
mod shared;

pub use capabilities::capabilities;
pub use printing::print;

// ---------------------------------------------------------------------------
// Listing

pub fn list_printers() -> Vec<Printer> {
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let (mut needed, mut count) = (0u32, 0u32);
    unsafe {
        // First call only reports the buffer size.
        let _ = EnumPrintersW(flags, PCWSTR::null(), 2, None, &mut needed, &mut count);
    }
    if needed == 0 {
        return vec![];
    }
    // u64 storage keeps PRINTER_INFO_2W suitably aligned.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    let bytes = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, needed as usize) };
    if unsafe { EnumPrintersW(flags, PCWSTR::null(), 2, Some(bytes), &mut needed, &mut count) }.is_err() {
        return vec![];
    }
    let infos = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const PRINTER_INFO_2W, count as usize) };
    infos
        .iter()
        .map(|info| {
            let (status, message) = printer_status(info.Status);
            let shared = info.Attributes & PRINTER_ATTRIBUTE_SHARED != 0;
            Printer {
                name: unsafe { info.pPrinterName.to_string() }.unwrap_or_default(),
                status,
                message,
                shared: if shared { "True" } else { "False" }.to_string(),
                os_default: false,
            }
        })
        .collect()
}

/// Summary status and an explanation of the problem, if any.
fn printer_status(bits: u32) -> (String, Option<String>) {
    const PROBLEMS: &[(u32, &str, &str)] = &[
        (PRINTER_STATUS_PAPER_OUT, "Error", "Kertas habis"),
        (PRINTER_STATUS_PAPER_JAM, "Error", "Kertas macet"),
        (PRINTER_STATUS_PAPER_PROBLEM, "Error", "Paper problem"),
        (PRINTER_STATUS_NO_TONER, "Error", "Tinta/toner habis"),
        (PRINTER_STATUS_DOOR_OPEN, "Error", "Printer cover is open"),
        (PRINTER_STATUS_USER_INTERVENTION, "Error", "Printer butuh tindakan"),
        (PRINTER_STATUS_OUT_OF_MEMORY, "Error", "Memori printer penuh"),
        (PRINTER_STATUS_OFFLINE, "Offline", "Printer offline"),
        (PRINTER_STATUS_SERVER_OFFLINE, "Offline", "Server printer offline"),
        (PRINTER_STATUS_NOT_AVAILABLE, "Offline", "Printer unavailable"),
        (PRINTER_STATUS_ERROR, "Error", "Printer error"),
        (PRINTER_STATUS_PAUSED, "Paused", "Printer di-pause"),
    ];
    if let Some(&(_, status, msg)) = PROBLEMS.iter().find(|(bit, _, _)| bits & bit != 0) {
        return (status.into(), Some(msg.into()));
    }
    let status = if bits & PRINTER_STATUS_PRINTING != 0 {
        "Printing"
    } else {
        "Normal"
    };
    let warning = (bits & PRINTER_STATUS_TONER_LOW != 0).then(|| "Tinta/toner hampir habis".to_string());
    (status.into(), warning)
}

/// Calls a size-then-fill winspool API into an 8-byte aligned buffer. The
/// queue can change between the size and the fill call, so it retries when the
/// buffer turns out too small.
fn winspool_buffer(mut call: impl FnMut(Option<&mut [u8]>, &mut u32) -> bool) -> Option<Vec<u64>> {
    for _ in 0..4 {
        let mut needed = 0u32;
        call(None, &mut needed);
        if needed == 0 {
            return None;
        }
        // Headroom for entries added since the size query.
        let size = needed as usize + 512;
        let mut buf = vec![0u64; size.div_ceil(8)];
        let bytes = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, size) };
        let mut got = size as u32;
        if call(Some(bytes), &mut got) {
            return Some(buf);
        }
    }
    None
}

pub fn queue(printer: &str) -> Result<PrinterQueue, String> {
    let printer = printer.to_string();
    on_spooler_thread(move || queue_impl(&printer))?
}

fn queue_impl(printer: &str) -> Result<PrinterQueue, String> {
    let name = HSTRING::from(printer);
    let handle = PrinterHandle::open(&name)?;

    let info = winspool_buffer(|buf, needed| unsafe { GetPrinterW(handle.0, 2, buf, needed) }.is_ok())
        .ok_or_else(|| format!("Could not read the status of printer '{}'", printer))?;
    let info = unsafe { &*(info.as_ptr() as *const PRINTER_INFO_2W) };
    let (status, message) = printer_status(info.Status);

    let mut count = 0u32;
    let jobs_buf = winspool_buffer(|buf, needed| {
        count = 0;
        unsafe { EnumJobsW(handle.0, 0, u32::MAX, 1, buf, needed, &mut count) }.is_ok()
    });
    let jobs = match &jobs_buf {
        Some(buf) if count > 0 && count as usize * std::mem::size_of::<JOB_INFO_1W>() <= buf.len() * 8 => unsafe {
            std::slice::from_raw_parts(buf.as_ptr() as *const JOB_INFO_1W, count as usize)
        },
        _ => &[],
    };

    let jobs = jobs
        .iter()
        .map(|j| {
            let bits = j.Status;
            // JOB_INFO_1W::pStatus is optional; some drivers return null when
            // there is no status message. PWSTR::to_string() dereferences it.
            let driver_msg = if j.pStatus.is_null() {
                None
            } else {
                unsafe { j.pStatus.to_string() }.ok().filter(|m| !m.is_empty())
            };
            let (state, msg) = if bits & JOB_STATUS_PAPEROUT != 0 {
                (JobState::Error, Some("Kertas habis"))
            } else if bits & JOB_STATUS_OFFLINE != 0 {
                (JobState::Error, Some("Printer offline"))
            } else if bits & (JOB_STATUS_ERROR | JOB_STATUS_USER_INTERVENTION | JOB_STATUS_BLOCKED_DEVQ) != 0 {
                (JobState::Error, Some("Printer error"))
            } else if bits & (JOB_STATUS_DELETING | JOB_STATUS_DELETED) != 0 {
                (JobState::Gone, Some("Dibatalkan"))
            } else if bits & JOB_STATUS_PAUSED != 0 {
                (JobState::Paused, None)
            } else if bits & (JOB_STATUS_PRINTED | JOB_STATUS_COMPLETE) != 0 {
                (JobState::Completed, None)
            } else if bits & JOB_STATUS_PRINTING != 0 {
                (JobState::Printing, None)
            } else {
                (JobState::Queued, None)
            };
            QueueJob {
                id: j.JobId.to_string(),
                document: if j.pDocument.is_null() {
                    String::new()
                } else {
                    unsafe { j.pDocument.to_string() }.unwrap_or_default()
                },
                position: j.Position,
                state,
                message: driver_msg.or(msg.map(str::to_string)),
                pages: (j.TotalPages > 0).then_some(j.TotalPages),
                pages_printed: (j.TotalPages > 0).then_some(j.PagesPrinted),
            }
        })
        .collect();

    Ok(PrinterQueue {
        printer: printer.to_string(),
        status,
        message,
        jobs,
    })
}

/// A missing queue entry may have printed, failed, or been cancelled. Windows
/// does not provide enough evidence here to claim completion.
pub fn was_completed(_printer: &str, _spooler_job: &str) -> bool {
    false
}

pub fn cancel(printer: &str, spooler_job: &str) -> Result<(), String> {
    let name = HSTRING::from(printer);
    let id = spooler_job
        .parse::<u32>()
        .map_err(|_| "Invalid Windows spooler job ID")?;
    on_spooler_thread(move || {
        let handle = PrinterHandle::open(&name)?;
        unsafe { SetJobW(handle.0, id, 0, None, JOB_CONTROL_DELETE) }
            .ok()
            .map_err(|error| format!("Could not cancel job: {}", error))
    })?
}

pub fn os_default_printer() -> Option<String> {
    let mut len = 0u32;
    unsafe {
        let _ = GetDefaultPrinterW(None, &mut len);
    }
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    unsafe { GetDefaultPrinterW(Some(PWSTR(buf.as_mut_ptr())), &mut len) }
        .as_bool()
        .then(|| String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)]))
}
