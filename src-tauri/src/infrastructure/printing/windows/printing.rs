//! Windows GDI printing adapter.

use super::capabilities::{papers, trays, PaperEntry};
use super::shared::{on_spooler_thread, DevMode, PrinterHandle};
use crate::domain::layout::Sheet;
use crate::domain::printer::DeviceOptions;
use crate::domain::settings::{pt_to_mm, ColorMode, Duplex, Paper};
use crate::infrastructure::pdf::layout_engine::pdfium;
use pdfium_render::prelude::*;
use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Graphics::Gdi::{
    CreateDCW, DeleteDC, GetDeviceCaps, ResetDCW, SetBrushOrgEx, SetStretchBltMode, StretchDIBits, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DMCOLLATE_FALSE, DMCOLOR_COLOR, DMCOLOR_MONOCHROME, DMDUP_HORIZONTAL,
    DMDUP_SIMPLEX, DMDUP_VERTICAL, DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT, DMPAPER_USER, DM_COLLATE, DM_COLOR,
    DM_COPIES, DM_DEFAULTSOURCE, DM_DUPLEX, DM_ORIENTATION, DM_PAPERLENGTH, DM_PAPERSIZE, DM_PAPERWIDTH, HALFTONE, HDC,
    LOGPIXELSX, LOGPIXELSY, PHYSICALOFFSETX, PHYSICALOFFSETY, SRCCOPY,
};
use windows::Win32::Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};

const DEFAULT_MAX_DPI: u32 = 300;

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
        s.dmOrientation = if landscape {
            DMORIENT_LANDSCAPE
        } else {
            DMORIENT_PORTRAIT
        } as i16;
        if *paper != Paper::PrinterDefault {
            let (w, h) = if exact {
                (pt_to_mm(sheet.width_pt), pt_to_mm(sheet.height_pt))
            } else {
                (
                    pt_to_mm(sheet.width_pt.min(sheet.height_pt)),
                    pt_to_mm(sheet.width_pt.max(sheet.height_pt)),
                )
            };
            let known = papers
                .iter()
                .find(|p| (p.info.width_mm - w).abs() < 1.0 && (p.info.height_mm - h).abs() < 1.0);
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

pub fn print(printer: &str, pdf: &Path, sheets: &[Sheet], o: &DeviceOptions) -> Result<Option<String>, String> {
    let (printer, pdf, sheets) = (printer.to_string(), pdf.to_path_buf(), sheets.to_vec());
    let (job_name, paper, tray) = (o.job_name.to_string(), o.paper.clone(), o.tray.map(str::to_string));
    let (copies, collate, duplex, color, dpi) = (o.copies, o.collate, o.duplex, o.color, o.dpi);
    on_spooler_thread(move || {
        let options = DeviceOptions {
            job_name: &job_name,
            copies,
            collate,
            paper: &paper,
            duplex,
            color,
            tray: tray.as_deref(),
            dpi,
        };
        print_impl(&printer, &pdf, &sheets, &options)
    })?
}

fn print_impl(printer: &str, pdf: &Path, sheets: &[Sheet], o: &DeviceOptions) -> Result<Option<String>, String> {
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
    let dc = Dc(unsafe { CreateDCW(&HSTRING::from("WINSPOOL"), &name, PCWSTR::null(), Some(first_dm.ptr())) });
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
