//! Windows backend: PDFium renders every sheet of the print-ready PDF and GDI
//! sends it to the printer, silently. The sheet is mapped onto the physical
//! page (not the printable area), so positions match the other platforms.

use super::{Capabilities, DeviceOptions, JobState, PaperInfo, Printer, PrinterQueue, QueueJob};
use crate::layout::{pdfium, Sheet};
use crate::settings::{pt_to_mm, ColorMode, Duplex, Paper};
use pdfium_render::prelude::*;
use std::path::Path;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Graphics::Gdi::{
    CreateDCW, CreateICW, DeleteDC, GetDeviceCaps, ResetDCW, SetBrushOrgEx, SetStretchBltMode,
    StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DEVMODEW, DIB_RGB_COLORS,
    DMCOLOR_COLOR, DMCOLOR_MONOCHROME, DMCOLLATE_FALSE, DMDUP_HORIZONTAL, DMDUP_SIMPLEX,
    DMDUP_VERTICAL, DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT, DMPAPER_USER, DM_COLLATE, DM_COLOR,
    DM_COPIES, DM_DEFAULTSOURCE, DM_DUPLEX, DM_ORIENTATION, DM_PAPERLENGTH, DM_PAPERSIZE,
    DM_PAPERWIDTH, HALFTONE, HDC, LOGPIXELSX, LOGPIXELSY, PHYSICALHEIGHT, PHYSICALOFFSETX,
    PHYSICALOFFSETY, PHYSICALWIDTH, SRCCOPY,
};
use windows::Win32::Graphics::Printing::{
    ClosePrinter, DocumentPropertiesW, EnumJobsW, EnumPrintersW, GetDefaultPrinterW, GetPrinterW,
    OpenPrinterW, JOB_INFO_1W, JOB_STATUS_BLOCKED_DEVQ, JOB_STATUS_COMPLETE, JOB_STATUS_DELETED,
    JOB_STATUS_DELETING, JOB_STATUS_ERROR, JOB_STATUS_OFFLINE, JOB_STATUS_PAPEROUT,
    JOB_STATUS_PAUSED, JOB_STATUS_PRINTED, JOB_STATUS_PRINTING, JOB_STATUS_USER_INTERVENTION,
    PRINTER_ATTRIBUTE_SHARED, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_HANDLE,
    PRINTER_INFO_2W, PRINTER_STATUS_DOOR_OPEN, PRINTER_STATUS_ERROR, PRINTER_STATUS_NOT_AVAILABLE,
    PRINTER_STATUS_NO_TONER, PRINTER_STATUS_OFFLINE, PRINTER_STATUS_OUT_OF_MEMORY,
    PRINTER_STATUS_PAPER_JAM, PRINTER_STATUS_PAPER_OUT, PRINTER_STATUS_PAPER_PROBLEM,
    PRINTER_STATUS_PAUSED, PRINTER_STATUS_PRINTING, PRINTER_STATUS_SERVER_OFFLINE,
    PRINTER_STATUS_TONER_LOW, PRINTER_STATUS_USER_INTERVENTION,
};
use windows::Win32::Storage::Xps::{
    AbortDoc, DeviceCapabilitiesW, EndDoc, EndPage, StartDocW, StartPage, DC_BINNAMES, DC_BINS,
    DC_COLORDEVICE, DC_DUPLEX, DC_MAXEXTENT, DC_PAPERNAMES, DC_PAPERS, DC_PAPERSIZE, DOCINFOW,
    PRINTER_DEVICE_CAPABILITIES,
};

const DEFAULT_MAX_DPI: u32 = 300;
const DM_IN_BUFFER: u32 = 8;
const DM_OUT_BUFFER: u32 = 2;
const PAPER_NAME_LEN: usize = 64;
const BIN_NAME_LEN: usize = 24;

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
    let bytes =
        unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, needed as usize) };
    if unsafe { EnumPrintersW(flags, PCWSTR::null(), 2, Some(bytes), &mut needed, &mut count) }
        .is_err()
    {
        return vec![];
    }
    let infos =
        unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const PRINTER_INFO_2W, count as usize) };
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
    let status = if bits & PRINTER_STATUS_PRINTING != 0 { "Printing" } else { "Normal" };
    let warning = (bits & PRINTER_STATUS_TONER_LOW != 0).then(|| "Tinta/toner hampir habis".to_string());
    (status.into(), warning)
}

/// Calls a size-then-fill winspool API into an 8-byte aligned buffer.
fn winspool_buffer(mut call: impl FnMut(Option<&mut [u8]>, &mut u32) -> bool) -> Option<Vec<u64>> {
    let mut needed = 0u32;
    call(None, &mut needed);
    if needed == 0 {
        return None;
    }
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    let bytes = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, needed as usize) };
    call(Some(bytes), &mut needed).then_some(buf)
}

pub fn queue(printer: &str) -> Result<PrinterQueue, String> {
    let name = HSTRING::from(printer);
    let handle = PrinterHandle::open(&name)?;

    let info = winspool_buffer(|buf, needed| unsafe { GetPrinterW(handle.0, 2, buf, needed) }.is_ok())
		.ok_or_else(|| format!("Could not read the status of printer '{}'", printer))?;
    let info = unsafe { &*(info.as_ptr() as *const PRINTER_INFO_2W) };
    let (status, message) = printer_status(info.Status);

    let mut count = 0u32;
    let jobs_buf = winspool_buffer(|buf, needed| {
        unsafe { EnumJobsW(handle.0, 0, u32::MAX, 1, buf, needed, &mut count) }.is_ok()
    });
    let jobs = match &jobs_buf {
        Some(buf) if count > 0 => unsafe {
            std::slice::from_raw_parts(buf.as_ptr() as *const JOB_INFO_1W, count as usize)
        },
        _ => &[],
    };

    let jobs = jobs
        .iter()
        .map(|j| {
            let bits = j.Status;
            let driver_msg = unsafe { j.pStatus.to_string() }.ok().filter(|m| !m.is_empty());
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
                document: unsafe { j.pDocument.to_string() }.unwrap_or_default(),
                position: j.Position,
                state,
                message: driver_msg.or(msg.map(str::to_string)),
                pages: (j.TotalPages > 0).then_some(j.TotalPages),
                pages_printed: (j.TotalPages > 0).then_some(j.PagesPrinted),
            }
        })
        .collect();

    Ok(PrinterQueue { printer: printer.to_string(), status, message, jobs })
}

/// Windows drops finished jobs from the queue (unless "keep printed documents" is on).
pub fn was_completed(_printer: &str, _spooler_job: &str) -> bool {
    true
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

// ---------------------------------------------------------------------------
// DEVMODE and capabilities

/// A printer's DEVMODE as raw bytes (drivers append private data after DEVMODEW).
struct DevMode(Vec<u64>);

impl DevMode {
    fn ptr(&self) -> *const DEVMODEW {
        self.0.as_ptr() as *const DEVMODEW
    }
    fn get(&self) -> &DEVMODEW {
        unsafe { &*self.ptr() }
    }
    fn get_mut(&mut self) -> &mut DEVMODEW {
        unsafe { &mut *(self.0.as_mut_ptr() as *mut DEVMODEW) }
    }
}

struct PrinterHandle(PRINTER_HANDLE);

impl PrinterHandle {
    fn open(name: &HSTRING) -> Result<Self, String> {
        let mut handle = PRINTER_HANDLE::default();
        unsafe { OpenPrinterW(PCWSTR(name.as_ptr()), &mut handle, None) }
			.map_err(|_| format!("Printer '{}' was not found", name))?;
        Ok(PrinterHandle(handle))
    }

    fn default_devmode(&self, name: &HSTRING) -> Result<DevMode, String> {
        let device = PCWSTR(name.as_ptr());
        let size = unsafe { DocumentPropertiesW(None, self.0, device, None, None, 0) };
        if size <= 0 {
			return Err(format!("Could not read the settings for printer '{}'", name));
        }
        let mut dm = DevMode(vec![0u64; (size as usize).div_ceil(8)]);
        let ptr = dm.get_mut() as *mut DEVMODEW;
        if unsafe { DocumentPropertiesW(None, self.0, device, Some(ptr), None, DM_OUT_BUFFER) } < 0 {
			return Err(format!("Could not read the settings for printer '{}'", name));
        }
        Ok(dm)
    }

    /// Lets the driver validate and complete a modified DEVMODE.
    fn merge(&self, name: &HSTRING, dm: &mut DevMode) -> Result<(), String> {
        let ptr = dm.get_mut() as *mut DEVMODEW;
        let ret = unsafe {
            DocumentPropertiesW(
                None,
                self.0,
                PCWSTR(name.as_ptr()),
                Some(ptr),
                Some(ptr as *const DEVMODEW),
                DM_IN_BUFFER | DM_OUT_BUFFER,
            )
        };
        if ret < 0 {
			return Err(format!("Printer '{}' rejected the print settings", name));
        }
        Ok(())
    }
}

impl Drop for PrinterHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = ClosePrinter(self.0);
        }
    }
}

/// Calls DeviceCapabilitiesW twice (count, then data) with `item_size` elements of `T`.
fn device_caps<T: Default + Clone>(name: &HSTRING, cap: PRINTER_DEVICE_CAPABILITIES, item_len: usize) -> Vec<T> {
    let device = PCWSTR(name.as_ptr());
    let count = unsafe { DeviceCapabilitiesW(device, PCWSTR::null(), cap, None, None) };
    if count <= 0 {
        return vec![];
    }
    let mut buf = vec![T::default(); count as usize * item_len];
    let got = unsafe {
        DeviceCapabilitiesW(device, PCWSTR::null(), cap, Some(PWSTR(buf.as_mut_ptr() as *mut u16)), None)
    };
    if got <= 0 {
        return vec![];
    }
    buf.truncate(got as usize * item_len);
    buf
}

fn wide_names(buf: &[u16], len: usize) -> Vec<String> {
    buf.chunks(len)
        .map(|c| {
            let end = c.iter().position(|&ch| ch == 0).unwrap_or(c.len());
            String::from_utf16_lossy(&c[..end])
        })
        .collect()
}

struct PaperEntry {
    id: i16,
    info: PaperInfo,
}

fn papers(name: &HSTRING) -> Vec<PaperEntry> {
    let ids: Vec<u16> = device_caps(name, DC_PAPERS, 1);
    let sizes: Vec<i32> = device_caps(name, DC_PAPERSIZE, 2); // POINT, tenths of a mm
    let names = wide_names(&device_caps::<u16>(name, DC_PAPERNAMES, PAPER_NAME_LEN), PAPER_NAME_LEN);
    ids.iter()
        .enumerate()
        .filter_map(|(i, &id)| {
            let (w, h) = (*sizes.get(i * 2)?, *sizes.get(i * 2 + 1)?);
            Some(PaperEntry {
                id: id as i16,
                info: PaperInfo {
                    name: names.get(i).cloned().unwrap_or_else(|| format!("#{}", id)),
                    width_mm: w as f32 / 10.0,
                    height_mm: h as f32 / 10.0,
                },
            })
        })
        .collect()
}

fn trays(name: &HSTRING) -> Vec<(i16, String)> {
    let ids: Vec<u16> = device_caps(name, DC_BINS, 1);
    let names = wide_names(&device_caps::<u16>(name, DC_BINNAMES, BIN_NAME_LEN), BIN_NAME_LEN);
    ids.iter().zip(names).map(|(&id, n)| (id as i16, n)).collect()
}

pub fn capabilities(printer: &str) -> Result<Capabilities, String> {
    let name = HSTRING::from(printer);
    let handle = PrinterHandle::open(&name)?;
    let dm = handle.default_devmode(&name)?;
    let papers = papers(&name);
    let trays = trays(&name);

    let fields = dm.get().dmFields;
    let settings = unsafe { dm.get().Anonymous1.Anonymous1 };
    let default_paper = papers
        .iter()
        .find(|p| fields.contains(DM_PAPERSIZE) && p.id == settings.dmPaperSize)
        .map(|p| p.info.clone())
        .or_else(|| physical_paper(&name, &dm));
    let default_tray = trays
        .iter()
        .find(|(id, _)| fields.contains(DM_DEFAULTSOURCE) && *id == settings.dmDefaultSource)
        .map(|(_, n)| n.clone());

    let device = PCWSTR(name.as_ptr());
    let flag = |cap| unsafe { DeviceCapabilitiesW(device, PCWSTR::null(), cap, None, None) } == 1;

    // LOWORD = max dmPaperWidth, HIWORD = max dmPaperLength, tenths of a mm.
    let extent = unsafe { DeviceCapabilitiesW(device, PCWSTR::null(), DC_MAXEXTENT, None, None) };
    let (max_width_mm, max_height_mm) = if extent > 0 {
        let (w, h) = ((extent & 0xFFFF) as f32 / 10.0, ((extent >> 16) & 0xFFFF) as f32 / 10.0);
        ((w > 0.0).then_some(w), (h > 0.0).then_some(h))
    } else {
        (None, None)
    };

    Ok(Capabilities {
        papers: papers.into_iter().map(|p| p.info).collect(),
        default_paper,
        trays: trays.into_iter().map(|(_, n)| n).collect(),
        default_tray,
        duplex: flag(DC_DUPLEX),
        color: flag(DC_COLORDEVICE),
        max_width_mm,
        max_height_mm,
    })
}

/// Physical paper size of the default DEVMODE (portrait), read from an information context.
fn physical_paper(name: &HSTRING, dm: &DevMode) -> Option<PaperInfo> {
    let mut dm = DevMode(dm.0.clone());
    {
        let d = dm.get_mut();
        d.dmFields |= DM_ORIENTATION;
        d.Anonymous1.Anonymous1.dmOrientation = DMORIENT_PORTRAIT as i16;
    }
    let ic = unsafe { CreateICW(&HSTRING::from("WINSPOOL"), name, PCWSTR::null(), Some(dm.ptr())) };
    if ic.is_invalid() {
        return None;
    }
    let caps = |i| unsafe { GetDeviceCaps(Some(ic), i) } as f32;
    let info = PaperInfo {
        name: "default".into(),
        width_mm: caps(PHYSICALWIDTH) / caps(LOGPIXELSX) * 25.4,
        height_mm: caps(PHYSICALHEIGHT) / caps(LOGPIXELSY) * 25.4,
    };
    unsafe {
        let _ = DeleteDC(ic);
    }
    (info.width_mm > 0.0 && info.height_mm > 0.0).then_some(info)
}

// ---------------------------------------------------------------------------
// Printing

struct Dc(HDC);

impl Drop for Dc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

/// DEVMODE for one sheet: job options plus paper size and orientation.
///
/// Standard papers are set in portrait with the orientation flag, so the driver
/// rotates landscape sheets. Custom and roll sizes are sent exactly as entered
/// (width across the feed × length) in portrait, as label printers expect.
fn sheet_devmode(
    base: &DevMode,
    handle: &PrinterHandle,
    name: &HSTRING,
    papers: &[PaperEntry],
    paper: &Paper,
    sheet: &Sheet,
) -> Result<DevMode, String> {
    let mut dm = DevMode(base.0.clone());
    let d = dm.get_mut();
    d.dmFields |= DM_ORIENTATION;
    let exact = matches!(paper, Paper::Custom { .. } | Paper::Roll { .. });
    unsafe {
        let s = &mut d.Anonymous1.Anonymous1;
        let landscape = !exact && sheet.is_landscape();
        s.dmOrientation = if landscape { DMORIENT_LANDSCAPE } else { DMORIENT_PORTRAIT } as i16;
        if *paper != Paper::PrinterDefault {
            let (w, h) = if exact {
                (pt_to_mm(sheet.width_pt), pt_to_mm(sheet.height_pt))
            } else {
                (pt_to_mm(sheet.width_pt.min(sheet.height_pt)), pt_to_mm(sheet.width_pt.max(sheet.height_pt)))
            };
            let known = papers.iter().find(|p| {
                (p.info.width_mm - w).abs() < 1.0 && (p.info.height_mm - h).abs() < 1.0
            });
            match known {
                Some(p) => {
                    d.dmFields |= DM_PAPERSIZE;
                    s.dmPaperSize = p.id;
                }
                None => {
                    d.dmFields |= DM_PAPERSIZE | DM_PAPERWIDTH | DM_PAPERLENGTH;
                    s.dmPaperSize = DMPAPER_USER as i16;
                    s.dmPaperWidth = (w * 10.0).round() as i16;
                    s.dmPaperLength = (h * 10.0).round() as i16;
                }
            }
        }
    }
    handle.merge(name, &mut dm)?;
    Ok(dm)
}

pub fn print(
    printer: &str,
    pdf: &Path,
    sheets: &[Sheet],
    o: &DeviceOptions,
) -> Result<Option<String>, String> {
    let name = HSTRING::from(printer);
    let handle = PrinterHandle::open(&name)?;
    let mut base = handle.default_devmode(&name)?;
    let papers = papers(&name);

    // Job-wide options. Copies are sent one by one (many drivers ignore dmCopies).
    {
        let d = base.get_mut();
        d.dmFields |= DM_COPIES | DM_COLLATE;
        d.dmCollate = DMCOLLATE_FALSE;
        d.Anonymous1.Anonymous1.dmCopies = 1;
        if let Some(duplex) = o.duplex {
            d.dmFields |= DM_DUPLEX;
            d.dmDuplex = match duplex {
                Duplex::Off => DMDUP_SIMPLEX,
                Duplex::LongEdge => DMDUP_VERTICAL,
                Duplex::ShortEdge => DMDUP_HORIZONTAL,
            };
        }
        if let Some(color) = o.color {
            d.dmFields |= DM_COLOR;
            d.dmColor = match color {
                ColorMode::Color => DMCOLOR_COLOR,
                ColorMode::Grayscale => DMCOLOR_MONOCHROME,
            };
        }
    }
    if let Some(tray) = o.tray {
        let trays = trays(&name);
        let (id, _) = trays
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(tray))
            .ok_or_else(|| {
                let names: Vec<_> = trays.iter().map(|(_, n)| n.as_str()).collect();
				format!("Tray '{}' does not exist. Options: {}", tray, names.join(", "))
            })?;
        let d = base.get_mut();
        d.dmFields |= DM_DEFAULTSOURCE;
        d.Anonymous1.Anonymous1.dmDefaultSource = *id;
    }

    // Held for the whole job: rendering happens page by page while spooling.
    let pdfium = pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(pdf, None)
		.map_err(|e| format!("Print-ready PDF could not be opened: {}", e))?;
    let grayscale = o.color == Some(ColorMode::Grayscale);
    let duplex = matches!(o.duplex, Some(Duplex::LongEdge | Duplex::ShortEdge));

    // Order of sheets to emit; `None` is a blank back side so each collated
    // duplex copy starts on a fresh sheet.
    let n = sheets.len();
    let mut order: Vec<Option<usize>> = Vec::with_capacity(n * o.copies as usize);
    if o.collate {
        for copy in 0..o.copies {
            order.extend((0..n).map(Some));
            if duplex && n % 2 == 1 && copy + 1 < o.copies {
                order.push(None);
            }
        }
    } else {
        for i in 0..n {
            order.extend(std::iter::repeat(Some(i)).take(o.copies as usize));
        }
    }

    let first_dm = sheet_devmode(&base, &handle, &name, &papers, o.paper, &sheets[0])?;
    let dc = Dc(unsafe {
        CreateDCW(&HSTRING::from("WINSPOOL"), &name, PCWSTR::null(), Some(first_dm.ptr()))
    });
    if dc.0.is_invalid() {
		return Err(format!("Printer '{}' could not be opened", printer));
    }

    let doc_name = HSTRING::from(o.job_name);
    let info = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: PCWSTR(doc_name.as_ptr()),
        ..Default::default()
    };
    let job_id = unsafe { StartDocW(dc.0, &info) };
    if job_id <= 0 {
		return Err(format!("Printer '{}' rejected the print job", printer));
    }

    let result = (|| -> Result<(), String> {
        let mut current: Option<Sheet> = Some(sheets[0]);
        for entry in &order {
            let sheet = entry.map(|i| sheets[i]).unwrap_or(current.unwrap_or(sheets[0]));
            if current != Some(sheet) {
                let dm = sheet_devmode(&base, &handle, &name, &papers, o.paper, &sheet)?;
                if unsafe { ResetDCW(dc.0, dm.ptr()) }.is_invalid() {
					return Err("Printer rejected the paper size/orientation change".into());
                }
                current = Some(sheet);
            }
            if unsafe { StartPage(dc.0) } <= 0 {
				return Err("StartPage failed".into());
            }
            let drawn = match entry {
                Some(i) => draw_sheet(dc.0, &doc, *i, &sheet, o.dpi, grayscale),
                None => Ok(()),
            };
            if unsafe { EndPage(dc.0) } <= 0 {
				return Err("EndPage failed".into());
            }
            drawn?;
        }
        Ok(())
    })();

    unsafe {
        if result.is_ok() {
            EndDoc(dc.0);
        } else {
            AbortDoc(dc.0);
        }
    }
    result.map(|_| Some(job_id.to_string()))
}

/// Renders sheet `index` and stretches it over the whole physical page.
fn draw_sheet(
    hdc: HDC,
    doc: &PdfDocument,
    index: usize,
    sheet: &Sheet,
    dpi: Option<u32>,
    grayscale: bool,
) -> Result<(), String> {
    let caps = |i| unsafe { GetDeviceCaps(Some(hdc), i) };
    let (dpi_x, dpi_y) = (caps(LOGPIXELSX) as f32, caps(LOGPIXELSY) as f32);
    let (off_x, off_y) = (caps(PHYSICALOFFSETX), caps(PHYSICALOFFSETY));

    let render_dpi = dpi
        .map(|d| d as f32)
        .unwrap_or_else(|| dpi_x.max(dpi_y).min(DEFAULT_MAX_DPI as f32));
    let bmp_w = (sheet.width_pt / 72.0 * render_dpi).round().max(1.0) as i32;
    let bmp_h = (sheet.height_pt / 72.0 * render_dpi).round().max(1.0) as i32;

    let page = doc
        .pages()
        .get(index as PdfPageIndex)
        .map_err(|e| format!("lembar {}: {}", index + 1, e))?;
    let bitmap = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_fixed_size(bmp_w, bmp_h)
                .set_format(PdfBitmapFormat::BGRA)
                .use_print_quality(true)
                .use_grayscale_rendering(grayscale),
        )
		.map_err(|e| format!("sheet {} failed to render: {}", index + 1, e))?;
    let pixels = bitmap.as_raw_bytes();

    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bmp_w,
            biHeight: -bmp_h, // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    // Device coordinates start at the printable area; shift back to the paper edge.
    let dest_w = (sheet.width_pt / 72.0 * dpi_x).round() as i32;
    let dest_h = (sheet.height_pt / 72.0 * dpi_y).round() as i32;
    let lines = unsafe {
        SetStretchBltMode(hdc, HALFTONE);
        let _ = SetBrushOrgEx(hdc, 0, 0, None);
        StretchDIBits(
            hdc,
            -off_x,
            -off_y,
            dest_w,
            dest_h,
            0,
            0,
            bmp_w,
            bmp_h,
            Some(pixels.as_ptr() as *const _),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        )
    };
    if lines == 0 {
		return Err(format!("sheet {} failed to send to the printer", index + 1));
    }
    Ok(())
}
