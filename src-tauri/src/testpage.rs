//! Edge-to-edge calibration page, generated at the exact size of the sheet
//! being printed. The outer frame sits on the paper edge and mm rulers run
//! inwards from every edge, so the first visible number on each side tells how
//! many millimetres the printer clips there.

use crate::canvas::{wrap, Canvas};
use crate::layout::pdfium;
use crate::settings::mm_to_pt;
use pdfium_render::prelude::*;
use std::path::Path;

/// Longest ruler, in mm from the edge.
const RULER_MM: f32 = 15.0;

pub const MAX_TEST_PAGES: u32 = 50;

/// Writes a calibration PDF of `pages` pages, each `width_mm` × `height_mm`.
/// `info` lines (paper, printer, preset, ...) are printed under the title; with
/// several pages each is numbered, so feed drift between labels shows up.
pub fn generate(
    path: &Path,
    width_mm: f32,
    height_mm: f32,
    info: &[String],
    pages: u32,
) -> Result<(), String> {
    let pages = pages.clamp(1, MAX_TEST_PAGES);
    let run = || -> Result<(), PdfiumError> {
        let pdfium = pdfium().map_err(|_| {
            PdfiumError::PdfiumLibraryInternalError(
                pdfium_render::prelude::PdfiumInternalError::Unknown,
            )
        })?;
        let mut doc = pdfium.create_new_pdf()?;
        let regular = doc.fonts_mut().helvetica();
        let bold = doc.fonts_mut().helvetica_bold();
        for number in 1..=pages {
            let mut info = info.to_vec();
            if pages > 1 {
				info.insert(0, format!("Page {} of {}", number, pages));
            }
            let page = doc
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::Custom(
                    PdfPoints::new(mm_to_pt(width_mm)),
                    PdfPoints::new(mm_to_pt(height_mm)),
                ))?;
            let mut c = Canvas {
                doc: &doc,
                page,
                width_mm,
                height_mm,
            };
            let (w, h) = (width_mm, height_mm);
            let ruler = RULER_MM.min(w.min(h) / 4.0).floor();
            let label = 4.5; // pt

            // Light center cross: shows whether the page is centered on the paper.
            c.line(w / 2.0, 0.0, w / 2.0, h, 0.25, 200)?;
            c.line(0.0, h / 2.0, w, h / 2.0, 0.25, 200)?;

            // Guides 5 mm and 10 mm from every edge.
            for (inset, gray) in [(5.0, 150u8), (10.0, 190u8)] {
                if w > inset * 3.0 && h > inset * 3.0 {
                    c.rect(
                        inset,
                        inset,
                        w - 2.0 * inset,
                        h - 2.0 * inset,
                        Some((gray, 0.3)),
                        None,
                    )?;
                }
            }

            // Rulers from the middle of each edge inwards: tick every mm, labels spaced
            // so two-digit numbers don't collide.
            for i in 0..=(ruler as i32) {
                let d = i as f32;
                let len = if i % 5 == 0 { 2.2 } else { 1.2 };
                let (lw, g) = (0.3, 0);
                // left / right edges: horizontal rulers, vertical ticks
                c.line(d, h / 2.0 - len, d, h / 2.0, lw, g)?;
                c.line(w - d, h / 2.0 - len, w - d, h / 2.0, lw, g)?;
                // top / bottom edges: vertical rulers, horizontal ticks
                c.line(w / 2.0, d, w / 2.0 + len, d, lw, g)?;
                c.line(w / 2.0, h - d, w / 2.0 + len, h - d, lw, g)?;
                if matches!(i, 2 | 4 | 6 | 8 | 10 | 15) {
                    let t = i.to_string();
                    let tw = c.measure(&t, regular, label);
                    c.text(&t, regular, label, Some(d - tw / 2.0), h / 2.0 + 2.0)?;
                    c.text(&t, regular, label, Some(w - d - tw / 2.0), h / 2.0 + 2.0)?;
                    c.text(&t, regular, label, Some(w / 2.0 - tw - 0.6), d + 0.6)?;
                    c.text(&t, regular, label, Some(w / 2.0 - tw - 0.6), h - d + 0.6)?;
                }
            }
            c.line(0.0, h / 2.0, ruler, h / 2.0, 0.3, 0)?;
            c.line(w - ruler, h / 2.0, w, h / 2.0, 0.3, 0)?;
            c.line(w / 2.0, 0.0, w / 2.0, ruler, 0.3, 0)?;
            c.line(w / 2.0, h - ruler, w / 2.0, h, 0.3, 0)?;

            // Full-width strips: grayscale steps at the top, solid black at the bottom.
            let strip_h = (h * 0.04).clamp(2.0, 5.0);
            let top = ruler + 2.0;
            let bottom = h - ruler - 2.0 - strip_h;
            if bottom > top + strip_h + 4.0 {
                let steps = 10;
                for s in 0..steps {
                    let gray = 255 - ((s + 1) * 255 / steps) as u8;
                    let x = w * s as f32 / steps as f32;
                    c.rect(x, top, w / steps as f32 + 0.05, strip_h, None, Some(gray))?;
                }
                c.rect(0.0, bottom, w, strip_h, None, Some(0))?;
            }

            // Text block between the strips, skipping the band of the side rulers.
            let margin = if w > 30.0 { 6.0 } else { 2.0 };
            let text_w = w - 2.0 * margin;
			let title = "ix.print · Paper Edge Test";
            let fit = |size: f32| c.measure(title, bold, size) <= text_w;
            let mut title_size = 16.0f32;
            while title_size > 5.0 && !fit(title_size) {
                title_size -= 0.5;
            }
            let body = (title_size * 0.62).clamp(4.5, 10.0);
            let line_h = |size: f32| crate::settings::pt_to_mm(size) * 1.35;
            let limit = if bottom > top {
                bottom - 1.0
            } else {
                h - ruler - 1.0
            };
            let mid_band = (h / 2.0 - 3.0, h / 2.0 + 4.0);

            // Paragraphs are kept whole: one that would cross the side rulers moves below them.
            let mut paragraphs: Vec<Vec<(String, PdfFontToken, f32)>> =
                vec![vec![(title.into(), bold, title_size)]];
            let tips = [
			"The outer black line marks the paper edge.",
			"The first visible ruler number on each side shows the millimetres clipped on that side.",
			"The grey lines mark 5 mm and 10 mm from the edge.",
			"Not symmetrical? Adjust the X/Y offsets in the preset.",
        ];
            for text in info.iter().map(String::as_str).chain(tips) {
                paragraphs.push(
                    wrap(&c, text, regular, body, text_w)
                        .into_iter()
                        .map(|l| (l, regular, body))
                        .collect(),
                );
            }

            let mut y = top + strip_h + 2.0;
            'outer: for para in &paragraphs {
                let height: f32 = para.iter().map(|(_, _, size)| line_h(*size)).sum();
                if y < mid_band.1 && y + height > mid_band.0 {
                    y = mid_band.1;
                }
                for (text, font, size) in para {
                    let lh = line_h(*size);
                    if y + lh > limit {
                        break 'outer;
                    }
                    y += lh;
                    c.text(text, *font, *size, None, y - lh * 0.25)?;
                }
            }

            // Outer frame last so nothing covers it; inset by half the stroke to sit on the edge.
            let stroke = 0.8;
            let inset = crate::settings::pt_to_mm(stroke / 2.0);
            c.rect(
                inset,
                inset,
                w - 2.0 * inset,
                h - 2.0 * inset,
                Some((0, stroke)),
                None,
            )?;

            c.page.regenerate_content()?;
            drop(c);
        }
        doc.save_to_file(path)?;
        Ok(())
    };
	run().map_err(|e| format!("Failed to create the test page: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_page_of_requested_size() {
        crate::layout::tests::init_for_tests();
        let dir = std::env::temp_dir().join(format!("ixprint-testpage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for (w, h) in [(210.0, 297.0), (80.0, 100.0), (58.0, 80.0), (50.0, 25.0)] {
            let path = dir.join(format!("{}x{}.pdf", w, h));
            generate(&path, w, h, &["Kertas: test".into()], 1).unwrap();
            let pdfium = pdfium().unwrap();
            let doc = pdfium.load_pdf_from_file(&path, None).unwrap();
            let page = doc.pages().get(0).unwrap();
            assert!((crate::settings::pt_to_mm(page.width().value) - w).abs() < 0.1);
            assert!((crate::settings::pt_to_mm(page.height().value) - h).abs() < 0.1);
        }
        // Several pages: same size, one PDF page each.
        let multi = dir.join("multi.pdf");
        generate(&multi, 60.0, 75.0, &[], 3).unwrap();
        {
            let pdfium = pdfium().unwrap();
            let doc = pdfium.load_pdf_from_file(&multi, None).unwrap();
            assert_eq!(doc.pages().len(), 3);
            let text = doc.pages().get(2).unwrap().text().unwrap().all();
			assert!(text.contains("Page 3 of 3"), "{}", text);
        }
        if let Ok(out) = std::env::var("IXPRINT_VISUAL_OUT") {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let _ = std::fs::copy(entry.path(), Path::new(&out).join(entry.file_name()));
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
