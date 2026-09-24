//! Layout engine: turns any PDF into a "print-ready" PDF where every page is
//! exactly one sheet of paper with the content already rotated, scaled and
//! positioned. Printer backends then print it 1:1, which is what makes the
//! result identical on Windows, macOS and Linux.
//!
//! Source pages are embedded as form XObjects, so text and vector graphics stay vector.

use crate::settings::{mm_to_pt, Align, JobSettings, Orientation, Paper, Scale};
use crate::sync::LockExt;
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

/// One printed sheet, in points. `width > height` means the sheet is fed in landscape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheet {
    pub width_pt: f32,
    pub height_pt: f32,
}

impl Sheet {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn is_landscape(&self) -> bool {
        self.width_pt > self.height_pt
    }
}

pub struct LayoutResult {
    pub sheets: Vec<Sheet>,
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
pub(crate) mod tests {
    use super::*;
    use crate::settings::{Margins, PageRange, PrintSettings};

    /// Loads the PDFium build for the machine running the tests.
    pub fn init_for_tests() {
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
        let os = if cfg!(target_os = "macos") {
            "mac"
        } else if cfg!(windows) {
            "win"
        } else {
            "linux"
        };
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("pdfium")
            .join(format!("{}-{}", os, arch));
        init(&[dir]).expect("run `bun run fetch-pdfium` first");
    }

    fn job(overrides: PrintSettings) -> JobSettings {
        JobSettings::resolve(&overrides, "test".into())
    }

    const A4: (f32, f32) = (210.0, 297.0);

    #[test]
    fn shrink_fit_and_orientation() {
        // Landscape A4 content, auto orientation → landscape A4 sheet, 1:1.
        let (w, h) = (mm_to_pt(297.0), mm_to_pt(210.0));
        let p = place(w, h, &job(PrintSettings::default()), A4);
        assert!(p.sheet.is_landscape());
        assert!((p.scale - 1.0).abs() < 1e-4);

        // Forced portrait: shrunk to the sheet width and centered vertically.
        let s = job(PrintSettings {
            orientation: Some(Orientation::Portrait),
            ..Default::default()
        });
        let p = place(w, h, &s, A4);
        assert!(!p.sheet.is_landscape());
        assert!((p.scale - 210.0 / 297.0).abs() < 1e-3);
        assert!((p.x).abs() < 0.01);
        assert!((p.y - (p.sheet.height_pt - h * p.scale) / 2.0).abs() < 0.01);

        // Shrink never enlarges; fit does.
        let small = (100.0, 100.0);
        assert_eq!(place(small.0, small.1, &job(PrintSettings::default()), A4).scale, 1.0);
        let fit = job(PrintSettings {
            scale: Some(Scale::Fit),
            ..Default::default()
        });
        assert!(place(small.0, small.1, &fit, A4).scale > 5.0);
    }

    #[test]
    fn roll_paper_follows_content() {
        let s = job(PrintSettings {
            paper: Some(Paper::Roll { width_mm: 80.0 }),
            scale: Some(Scale::Fit),
            margins: Some(Margins {
                top: 2.0,
                right: 2.0,
                bottom: 2.0,
                left: 2.0,
            }),
            ..Default::default()
        });
        let p = place(mm_to_pt(210.0), mm_to_pt(297.0), &s, A4);
        assert!((p.sheet.width_pt - mm_to_pt(80.0)).abs() < 0.01);
        let expected_h = mm_to_pt(297.0) * (mm_to_pt(76.0) / mm_to_pt(210.0)) + mm_to_pt(4.0);
        assert!((p.sheet.height_pt - expected_h).abs() < 0.1);
    }

    #[test]
    fn custom_size_is_used_as_entered() {
        let label = |orientation| {
            job(PrintSettings {
                paper: Some(Paper::Custom {
                    width_mm: 60.0,
                    height_mm: 75.0,
                }),
                orientation: Some(orientation),
                scale: Some(Scale::Fit),
                ..Default::default()
            })
        };
        let (w, h) = (mm_to_pt(60.0), mm_to_pt(75.0));
        let landscape_doc = (mm_to_pt(297.0), mm_to_pt(210.0));

        for o in [Orientation::Auto, Orientation::Portrait, Orientation::Landscape] {
            let p = place(landscape_doc.0, landscape_doc.1, &label(o), A4);
            // The label never changes size or orientation.
            assert!(
                (p.sheet.width_pt - w).abs() < 0.01 && (p.sheet.height_pt - h).abs() < 0.01,
                "{:?}",
                o
            );
        }
        // Auto turns a landscape page to fit the portrait label; Portrait keeps it upright.
        assert_eq!(
            place(landscape_doc.0, landscape_doc.1, &label(Orientation::Auto), A4).extra_rotation,
            90
        );
        assert_eq!(
            place(landscape_doc.0, landscape_doc.1, &label(Orientation::Portrait), A4).extra_rotation,
            0
        );
        assert_eq!(
            place(landscape_doc.0, landscape_doc.1, &label(Orientation::Landscape), A4).extra_rotation,
            90
        );
        // A portrait page on a portrait label needs no turn in Auto.
        assert_eq!(
            place(landscape_doc.1, landscape_doc.0, &label(Orientation::Auto), A4).extra_rotation,
            0
        );

        // A landscape label entered as 100x50 stays 100 wide.
        let wide = job(PrintSettings {
            paper: Some(Paper::Custom {
                width_mm: 100.0,
                height_mm: 50.0,
            }),
            ..Default::default()
        });
        let p = place(landscape_doc.0, landscape_doc.1, &wide, A4);
        assert!((p.sheet.width_pt - mm_to_pt(100.0)).abs() < 0.01);
        assert_eq!(p.extra_rotation, 0);
    }

    #[test]
    fn offsets_and_alignment() {
        let s = job(PrintSettings {
            align: Some(Align::TopLeft),
            offset_x: Some(10.0),
            offset_y: Some(5.0),
            ..Default::default()
        });
        let p = place(100.0, 100.0, &s, A4);
        assert!((p.x - mm_to_pt(10.0)).abs() < 0.01);
        assert!((p.y - (p.sheet.height_pt - 100.0 - mm_to_pt(5.0))).abs() < 0.01);
    }

    /// Makes a page with a red square marking its displayed top-left corner, then
    /// applies `rotation` and an offset crop box, which is where layout bugs hide.
    fn make_marked_pdf(path: &Path, rotation: PdfPageRenderRotation) {
        let pdfium = pdfium().unwrap();
        let mut doc = pdfium.create_new_pdf().unwrap();
        // Media 400x300; crop to (50,50)-(350,250) → 300x200 visible.
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(400.0), PdfPoints::new(300.0)))
            .unwrap();
        // Visible area gets a blue background; the marker sits at the displayed
        // top-left corner once the rotation is applied.
        let blue = PdfPagePathObject::new_rect(
            &doc,
            PdfRect::new_from_values(50.0, 50.0, 250.0, 350.0),
            None,
            None,
            Some(PdfColor::new(0, 0, 255, 255)),
        )
        .unwrap();
        page.objects_mut().add_path_object(blue).unwrap();
        // Unrotated corner that ends up displayed top-left for each rotation.
        let (x, y) = match rotation {
            PdfPageRenderRotation::None => (50.0, 210.0),        // top-left
            PdfPageRenderRotation::Degrees90 => (50.0, 50.0),    // bottom-left
            PdfPageRenderRotation::Degrees180 => (310.0, 50.0),  // bottom-right
            PdfPageRenderRotation::Degrees270 => (310.0, 210.0), // top-right
        };
        let red = PdfPagePathObject::new_rect(
            &doc,
            PdfRect::new_from_values(y, x, y + 40.0, x + 40.0),
            None,
            None,
            Some(PdfColor::new(255, 0, 0, 255)),
        )
        .unwrap();
        page.objects_mut().add_path_object(red).unwrap();
        page.boundaries_mut()
            .set_crop(PdfRect::new_from_values(50.0, 50.0, 250.0, 350.0))
            .unwrap();
        page.set_rotation(rotation);
        doc.save_to_file(path).unwrap();
    }

    /// Renders page 0 of `path` at 1px per point and returns (width, height, rgba).
    fn render(path: &Path) -> (usize, usize, Vec<u8>) {
        let pdfium = pdfium().unwrap();
        let doc = pdfium.load_pdf_from_file(path, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        let (w, h) = (page.width().value.round() as i32, page.height().value.round() as i32);
        let bmp = page
            .render_with_config(&PdfRenderConfig::new().set_fixed_size(w, h))
            .unwrap();
        (w as usize, h as usize, bmp.as_rgba_bytes())
    }

    fn pixel(img: &(usize, usize, Vec<u8>), x: usize, y: usize) -> (u8, u8, u8) {
        let i = (y * img.0 + x) * 4;
        (img.2[i], img.2[i + 1], img.2[i + 2])
    }

    #[test]
    fn output_matches_source_for_every_rotation() {
        init_for_tests();
        let dir = std::env::temp_dir().join(format!("ixprint-layout-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        for rotation in [
            PdfPageRenderRotation::None,
            PdfPageRenderRotation::Degrees90,
            PdfPageRenderRotation::Degrees180,
            PdfPageRenderRotation::Degrees270,
        ] {
            let src = dir.join(format!("src-{:?}.pdf", rotation));
            let out = dir.join(format!("out-{:?}.pdf", rotation));
            make_marked_pdf(&src, rotation);

            let expected = render(&src);
            let (ew, eh) = (expected.0 as f32, expected.1 as f32);
            // Sheet exactly the displayed page size, 1:1 → output must equal the source render.
            let s = job(PrintSettings {
                paper: Some(Paper::Custom {
                    width_mm: crate::settings::pt_to_mm(ew),
                    height_mm: crate::settings::pt_to_mm(eh),
                }),
                scale: Some(Scale::None),
                ..Default::default()
            });
            let result = build(&src, &out, &s, A4).unwrap();
            assert_eq!(result.sheets.len(), 1);
            let actual = render(&out);
            assert_eq!((actual.0, actual.1), (expected.0, expected.1), "{:?} size", rotation);

            // Red marker top-left, blue elsewhere, no white gaps from a wrong crop offset.
            assert_eq!(pixel(&actual, 5, 5), (255, 0, 0), "{:?} marker", rotation);
            assert_eq!(
                pixel(&actual, actual.0 - 5, actual.1 - 5),
                (0, 0, 255),
                "{:?} corner",
                rotation
            );
            assert_eq!(
                pixel(&actual, actual.0 / 2, actual.1 / 2),
                (0, 0, 255),
                "{:?} middle",
                rotation
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn roll_landscape_turns_content_clockwise() {
        init_for_tests();
        let dir = std::env::temp_dir().join(format!("ixprint-roll-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, out) = (dir.join("src.pdf"), dir.join("out.pdf"));
        make_marked_pdf(&src, PdfPageRenderRotation::None); // 300x200 displayed

        let s = job(PrintSettings {
            paper: Some(Paper::Roll {
                width_mm: crate::settings::pt_to_mm(200.0),
            }),
            orientation: Some(Orientation::Landscape),
            scale: Some(Scale::None),
            ..Default::default()
        });
        build(&src, &out, &s, A4).unwrap();
        let img = render(&out);
        assert_eq!((img.0, img.1), (200, 300));
        // The top-left marker is now top-right.
        assert_eq!(pixel(&img, img.0 - 5, 5), (255, 0, 0));
        assert_eq!(pixel(&img, 5, 5), (0, 0, 255));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Concurrent jobs (parallel `/print` requests) must not crash PDFium.
    #[test]
    fn concurrent_layouts() {
        init_for_tests();
        let dir = std::env::temp_dir().join(format!("ixprint-conc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.pdf");
        make_marked_pdf(&src, PdfPageRenderRotation::Degrees90);
        let handles: Vec<_> = (0..12)
            .map(|i| {
                let (src, out) = (src.clone(), dir.join(format!("o{}.pdf", i)));
                let page = dir.join(format!("t{}.pdf", i));
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        crate::testpage::generate(&page, 80.0, 100.0, &["x".into()], 2).unwrap();
                        build(&page, &out, &job(PrintSettings::default()), A4).unwrap();
                        build(&src, &out, &job(PrintSettings::default()), A4).unwrap();
                        render(&out);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn page_selection_and_errors() {
        init_for_tests();
        let dir = std::env::temp_dir().join(format!("ixprint-pages-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.pdf");
        make_marked_pdf(&src, PdfPageRenderRotation::None);

        let s = job(PrintSettings {
            pages: Some("5-9".parse::<PageRange>().unwrap()),
            ..Default::default()
        });
        assert!(build(&src, &dir.join("o.pdf"), &s, A4).is_err());

        let bogus = dir.join("bogus.pdf");
        std::fs::write(&bogus, b"not a pdf").unwrap();
        assert!(build(&bogus, &dir.join("o2.pdf"), &job(PrintSettings::default()), A4).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod visual {
    use super::*;
    use crate::settings::PrintSettings;

    /// Writes print-ready PDFs of the sample document to $IXPRINT_VISUAL_OUT for eyeballing.
    #[test]
    #[ignore]
    fn sample_layouts() {
        tests::init_for_tests();
        let out = PathBuf::from(std::env::var("IXPRINT_VISUAL_OUT").unwrap());
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assets/RSUD.pdf");
        let cases: &[(&str, &str)] = &[
            ("a4-auto", r#"{}"#),
            ("a4-portrait", r#"{"paper":"A4","orientation":"portrait"}"#),
            ("roll80-fit", r#"{"paper":"80mm","scale":"fit","margins":"2"}"#),
            (
                "label-100x150",
                r#"{"paper":"100x150mm","scale":"fit","orientation":"portrait","margins":"3"}"#,
            ),
            (
                "a5-topleft-offset",
                r#"{"paper":"A5","align":"top-left","offset_x":5,"offset_y":5,"scale":"80%"}"#,
            ),
        ];
        for (name, json) in cases {
            let s: PrintSettings = serde_json::from_str(json).unwrap();
            let job = JobSettings::resolve(&s, "test".into());
            let r = build(&src, &out.join(format!("{}.pdf", name)), &job, (210.0, 297.0)).unwrap();
            println!(
                "{}: {} sheet(s), first {:.1}x{:.1}mm",
                name,
                r.sheets.len(),
                crate::settings::pt_to_mm(r.sheets[0].width_pt),
                crate::settings::pt_to_mm(r.sheets[0].height_pt)
            );
        }
    }
}
