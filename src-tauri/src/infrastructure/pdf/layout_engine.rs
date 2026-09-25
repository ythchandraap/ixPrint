use crate::domain::layout::{LayoutResult, Sheet};
use crate::domain::settings::{mm_to_pt, Align, JobSettings, Orientation, Paper, Scale};
use crate::shared::sync::LockExt;
use pdfium_render::prelude::*;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

/// PDFium itself is not thread-safe: pdfium-render's `thread_safe` feature only
/// serialises individual calls, so documents used from two threads at once
/// (e.g. concurrent `/print` requests) can crash. All PDFium work happens while
/// holding this lock, via [`PdfiumGuard`].
static PDFIUM_LOCK: Mutex<()> = Mutex::new(());

/// Exclusive access to PDFium. Documents borrow from the guard, so the lock is
/// held until every document opened through it has been dropped.
pub struct PdfiumGuard {
    _lock: MutexGuard<'static, ()>,
    pdfium: &'static Pdfium,
}

impl Deref for PdfiumGuard {
    type Target = Pdfium;
    fn deref(&self) -> &Pdfium {
        self.pdfium
    }
}

/// Loads the bundled PDFium library from the first directory that contains it.
pub fn init(candidate_dirs: &[PathBuf]) -> Result<(), String> {
    PDFIUM
        .get_or_init(|| {
            let mut tried = Vec::new();
            for dir in candidate_dirs {
                let path = Pdfium::pdfium_platform_library_name_at_path(dir);
                if path.exists() {
                    return Pdfium::bind_to_library(&path)
                        .map(Pdfium::new)
                        .map_err(|e| format!("Failed to load PDFium {}: {}", path.display(), e));
                }
                tried.push(path.display().to_string());
            }
            Err(format!("PDFium library not found. Searched: {}", tried.join(", ")))
        })
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

/// Locks PDFium for the lifetime of the returned guard.
pub fn pdfium() -> Result<PdfiumGuard, String> {
    let pdfium = match PDFIUM.get() {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Err(e.clone()),
        None => return Err("PDFium has not been initialised".into()),
    };
    // A panic elsewhere doesn't leave PDFium in a bad state; keep going.
    let lock = PDFIUM_LOCK.lock_safe();
    Ok(PdfiumGuard { _lock: lock, pdfium })
}

/// A 2D affine transform in PDF matrix order: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Affine {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Affine {
    fn then(self, n: Affine) -> Affine {
        Affine {
            a: n.a * self.a + n.c * self.b,
            b: n.b * self.a + n.d * self.b,
            c: n.a * self.c + n.c * self.d,
            d: n.b * self.c + n.d * self.d,
            e: n.a * self.e + n.c * self.f + n.e,
            f: n.b * self.e + n.d * self.f + n.f,
        }
    }

    fn translate(x: f32, y: f32) -> Affine {
        Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: x,
            f: y,
        }
    }

    fn scale(s: f32) -> Affine {
        Affine {
            a: s,
            b: 0.0,
            c: 0.0,
            d: s,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Clockwise rotation (as PDF `/Rotate` is displayed) of a `w`×`h` box whose
    /// lower-left corner is at the origin; the result again starts at the origin.
    fn rotate_box_cw(degrees: u32, w: f32, h: f32) -> Affine {
        match degrees % 360 {
            90 => Affine {
                a: 0.0,
                b: -1.0,
                c: 1.0,
                d: 0.0,
                e: 0.0,
                f: w,
            },
            180 => Affine {
                a: -1.0,
                b: 0.0,
                c: 0.0,
                d: -1.0,
                e: w,
                f: h,
            },
            270 => Affine {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                e: h,
                f: 0.0,
            },
            _ => Affine::translate(0.0, 0.0),
        }
    }
}

/// Where one source page goes on its sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    sheet: Sheet,
    /// Extra clockwise rotation of the content (roll/custom paper in landscape).
    extra_rotation: u32,
    scale: f32,
    /// Lower-left corner of the placed content, in sheet points.
    x: f32,
    y: f32,
}

/// Pure layout math for a page whose displayed size (after its own `/Rotate`)
/// is `content_w`×`content_h` points.
fn place(content_w: f32, content_h: f32, s: &JobSettings, default_paper_mm: (f32, f32)) -> Placement {
    let m = &s.margins;
    let (ml, mr, mt, mb) = (mm_to_pt(m.left), mm_to_pt(m.right), mm_to_pt(m.top), mm_to_pt(m.bottom));

    let roll_width = match s.paper {
        Paper::Roll { width_mm } => Some(mm_to_pt(width_mm)),
        _ => None,
    };
    // A custom size is the physical label: the sheet is used exactly as entered
    // (width × length) and orientation only turns the content on it.
    let fixed_sheet = match s.paper {
        Paper::Custom { width_mm, height_mm } => Some((mm_to_pt(width_mm), mm_to_pt(height_mm))),
        _ => None,
    };

    let extra_rotation = match (roll_width, fixed_sheet, s.orientation) {
        (Some(_), _, Orientation::Landscape) | (_, Some(_), Orientation::Landscape) => 90,
        // Auto on a label: turn the content when its shape doesn't match the label's.
        (_, Some((sw, sh)), Orientation::Auto)
            if (content_w - content_h).abs() > 0.5 && (sw - sh).abs() > 0.5 && (content_w > content_h) != (sw > sh) =>
        {
            90
        }
        _ => 0,
    };
    let (cw, ch) = if extra_rotation == 90 {
        (content_h, content_w)
    } else {
        (content_w, content_h)
    };

    let fit_scale = |aw: f32, ah: Option<f32>| {
        let fit = match ah {
            Some(ah) => (aw / cw).min(ah / ch),
            None => aw / cw,
        };
        match s.scale {
            Scale::Shrink => fit.min(1.0),
            Scale::Fit => fit,
            Scale::None => 1.0,
            Scale::Percent(p) => p / 100.0,
        }
        .max(0.01)
    };

    let (sheet, scale) = match (roll_width, fixed_sheet) {
        (Some(width), _) => {
            let aw = (width - ml - mr).max(1.0);
            let scale = fit_scale(aw, None);
            let height = ch * scale + mt + mb;
            (
                Sheet {
                    width_pt: width,
                    height_pt: height,
                },
                scale,
            )
        }
        (None, Some((width, height))) => {
            let sheet = Sheet {
                width_pt: width,
                height_pt: height,
            };
            let aw = (width - ml - mr).max(1.0);
            let ah = (height - mt - mb).max(1.0);
            (sheet, fit_scale(aw, Some(ah)))
        }
        (None, None) => {
            let (pw, ph) = s.paper.size_mm().unwrap_or(default_paper_mm);
            let (short, long) = (mm_to_pt(pw.min(ph)), mm_to_pt(pw.max(ph)));
            let landscape = match s.orientation {
                Orientation::Portrait => false,
                Orientation::Landscape => true,
                Orientation::Auto => cw > ch,
            };
            let sheet = if landscape {
                Sheet {
                    width_pt: long,
                    height_pt: short,
                }
            } else {
                Sheet {
                    width_pt: short,
                    height_pt: long,
                }
            };
            let aw = (sheet.width_pt - ml - mr).max(1.0);
            let ah = (sheet.height_pt - mt - mb).max(1.0);
            (sheet, fit_scale(aw, Some(ah)))
        }
    };

    let (dw, dh) = (cw * scale, ch * scale);
    let aw = sheet.width_pt - ml - mr;
    let ah = sheet.height_pt - mt - mb;
    let x = match s.align {
        Align::TopLeft => ml,
        Align::Center | Align::Top => ml + (aw - dw) / 2.0,
    };
    let top = match s.align {
        Align::Center => mt + (ah - dh) / 2.0,
        Align::Top | Align::TopLeft => mt,
    };
    // PDF y grows upwards; positive offsets move right and down.
    let x = x + mm_to_pt(s.offset_x);
    let y = sheet.height_pt - top - dh - mm_to_pt(s.offset_y);

    Placement {
        sheet,
        extra_rotation,
        scale,
        x,
        y,
    }
}

/// Builds the print-ready PDF for `source` and writes it to `output`.
pub fn build(
    source: &Path,
    output: &Path,
    settings: &JobSettings,
    default_paper_mm: (f32, f32),
) -> Result<LayoutResult, String> {
    let pdfium = pdfium()?;
    let src = pdfium
        .load_pdf_from_file(source, None)
        .map_err(|e| format!("PDF could not be opened: {}", e))?;
    let page_count = src.pages().len() as u32;
    if page_count == 0 {
        return Err("PDF has no pages".into());
    }
    let selected = match &settings.pages {
        Some(range) => range.indexes(page_count),
        None => (0..page_count).collect(),
    };
    if selected.is_empty() {
        return Err(format!(
            "Page range '{}' is outside the document ({} pages)",
            settings.pages.as_ref().map(|p| p.to_string()).unwrap_or_default(),
            page_count
        ));
    }

    // Burn form fields and annotations into the page content so they print too.
    for index in 0..page_count {
        if let Ok(mut page) = src.pages().get(index as PdfPageIndex) {
            let _ = page.flatten();
        }
    }

    // Loaded before `out`: the embedded XObjects borrow from these pages.
    let pages = selected
        .iter()
        .map(|&index| {
            src.pages()
                .get(index as PdfPageIndex)
                .map(|page| (index, page))
                .map_err(|e| format!("page {}: {}", index + 1, e))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut out = pdfium.create_new_pdf().map_err(|e| e.to_string())?;
    let mut sheets = Vec::with_capacity(selected.len());

    for (index, page) in &pages {
        let index = *index;
        // Displayed size: crop box with the page's own /Rotate applied.
        let (content_w, content_h) = (page.width().value, page.height().value);
        let p = place(content_w, content_h, settings, default_paper_mm);

        let form = page
            .objects()
            .copy_into_x_object_form_object(&mut out)
            .map_err(|e| format!("page {}: {}", index + 1, e))?;

        // PDFium's XObject already maps the crop box to the origin and applies the
        // page's /Rotate, so it spans (0,0)-(content_w,content_h). Add the extra
        // rotation (roll/custom landscape), then scale and move into place.
        let matrix = Affine::rotate_box_cw(p.extra_rotation, content_w, content_h)
            .then(Affine::scale(p.scale))
            .then(Affine::translate(p.x, p.y));

        let mut sheet_page = out
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(
                PdfPoints::new(p.sheet.width_pt),
                PdfPoints::new(p.sheet.height_pt),
            ))
            .map_err(|e| e.to_string())?;
        let mut form = sheet_page
            .objects_mut()
            .add_object(form)
            .map_err(|e| format!("page {}: {}", index + 1, e))?;
        form.apply_matrix(PdfMatrix::new(
            matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
        ))
        .map_err(|e| e.to_string())?;
        sheet_page.regenerate_content().map_err(|e| e.to_string())?;

        sheets.push(p.sheet);
    }

    out.save_to_file(output)
        .map_err(|e| format!("Failed to save print-ready PDF: {}", e))?;
    Ok(LayoutResult { sheets })
}

#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod visual;
