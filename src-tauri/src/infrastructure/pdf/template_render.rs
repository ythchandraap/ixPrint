use crate::domain::settings::{mm_to_pt, pt_to_mm};
use crate::domain::template::*;
use crate::infrastructure::pdf::canvas::Canvas;
use crate::infrastructure::pdf::layout_engine::pdfium;
use crate::infrastructure::pdf::template_draw::{draw_element, Fonts};
use pdfium_render::prelude::*;
use std::path::Path;

/// Values for one element: automatic fields, overridden by the shared header
/// values, overridden by the record. Clients may send their own {{no}}, {{halaman}}...
fn values(header: &Record, record: Option<&Record>, auto: [(&str, usize); 4]) -> Record {
    let mut v: Record = auto.iter().map(|(k, n)| (k.to_string(), n.to_string())).collect();
    // An empty value for an automatic field means "not given", not "print nothing".
    let given = |(k, val): (&String, &String)| {
        (!(val.is_empty() && AUTO_FIELDS.contains(&k.as_str()))).then(|| (k.clone(), val.clone()))
    };
    v.extend(header.iter().filter_map(given));
    if let Some(r) = record {
        v.extend(r.iter().filter_map(given));
    }
    v
}

/// Builds the PDF: one page per record (label) or rows paginated under a header (list).
fn build<'a>(
    pdfium: &'a Pdfium,
    template: &Template,
    records: &[Record],
    header: &Record,
) -> Result<PdfDocument<'a>, String> {
    let mut doc = pdfium.create_new_pdf().map_err(|e| e.to_string())?;
    let fonts = Fonts {
        helvetica_regular: doc.fonts_mut().helvetica(),
        helvetica_bold: doc.fonts_mut().helvetica_bold(),
        times_regular: doc.fonts_mut().times_roman(),
        times_bold: doc.fonts_mut().times_bold(),
        courier_regular: doc.fonts_mut().courier(),
        courier_bold: doc.fonts_mut().courier_bold(),
        instrument_sans_regular: doc
            .fonts_mut()
            .load_true_type_from_bytes(
                include_bytes!("../../../assets/fonts/instrument-sans/InstrumentSans-400.ttf"),
                false,
            )
            .map_err(|e| e.to_string())?,
        instrument_sans_medium: doc
            .fonts_mut()
            .load_true_type_from_bytes(
                include_bytes!("../../../assets/fonts/instrument-sans/InstrumentSans-500.ttf"),
                false,
            )
            .map_err(|e| e.to_string())?,
        instrument_sans_semibold: doc
            .fonts_mut()
            .load_true_type_from_bytes(
                include_bytes!("../../../assets/fonts/instrument-sans/InstrumentSans-600.ttf"),
                false,
            )
            .map_err(|e| e.to_string())?,
        instrument_sans_bold: doc
            .fonts_mut()
            .load_true_type_from_bytes(
                include_bytes!("../../../assets/fonts/instrument-sans/InstrumentSans-700.ttf"),
                false,
            )
            .map_err(|e| e.to_string())?,
        ibm_plex_mono_regular: doc
            .fonts_mut()
            .load_true_type_from_bytes(
                include_bytes!("../../../assets/fonts/ibm-plex-mono/IBMPlexMono-400.ttf"),
                false,
            )
            .map_err(|e| e.to_string())?,
        ibm_plex_mono_medium: doc
            .fonts_mut()
            .load_true_type_from_bytes(
                include_bytes!("../../../assets/fonts/ibm-plex-mono/IBMPlexMono-500.ttf"),
                false,
            )
            .map_err(|e| e.to_string())?,
        ibm_plex_mono_semibold: doc
            .fonts_mut()
            .load_true_type_from_bytes(
                include_bytes!("../../../assets/fonts/ibm-plex-mono/IBMPlexMono-600.ttf"),
                false,
            )
            .map_err(|e| e.to_string())?,
    };
    let w = template.width_mm;
    let n = records.len();
    let err = |i: usize, e: String| format!("elemen #{}: {}", i + 1, e);

    // (page height, list of (element, dy, values)) per page
    type PageElements = Vec<(usize, f32, Record)>;
    type RenderedPage = (f32, PageElements);
    let pages: Vec<RenderedPage> = match template.layout {
        Layout::Label => records
            .iter()
            .enumerate()
            .map(|(p, r)| {
                let v = values(
                    header,
                    Some(r),
                    [
                        ("no", p + 1),
                        ("halaman", p + 1),
                        ("total_halaman", n),
                        ("jumlah_data", n),
                    ],
                );
                (
                    template.height_mm,
                    (0..template.elements.len()).map(|i| (i, 0.0, v.clone())).collect(),
                )
            })
            .collect(),
        Layout::List {
            header_mm,
            row_mm,
            footer_mm,
            footer,
            roll,
        } => {
            let band = |el: &Element| {
                let top = el.top();
                if top < header_mm - 0.001 {
                    Band::Header
                } else if top < header_mm + row_mm - 0.001 {
                    Band::Row
                } else {
                    Band::Footer
                }
            };
            let layout = paginate(n, template.height_mm, header_mm, row_mm, footer_mm, footer, roll);
            let total = layout.len();
            layout
                .iter()
                .enumerate()
                .map(|(p, page)| {
                    let auto = |no: usize| {
                        [
                            ("no", no),
                            ("halaman", p + 1),
                            ("total_halaman", total),
                            ("jumlah_data", n),
                        ]
                    };
                    let shared = values(header, None, auto(0));
                    let mut items = Vec::new();
                    for (i, el) in template.elements.iter().enumerate() {
                        match band(el) {
                            Band::Header => items.push((i, 0.0, shared.clone())),
                            Band::Row => {
                                for (k, r) in page.rows.clone().enumerate() {
                                    let dy = k as f32 * row_mm;
                                    items.push((i, dy, values(header, Some(&records[r]), auto(r + 1))));
                                }
                            }
                            Band::Footer => {
                                if let Some(top) = page.footer_top {
                                    items.push((i, top - (header_mm + row_mm), shared.clone()));
                                }
                            }
                        }
                    }
                    (page.height_mm, items)
                })
                .collect()
        }
    };

    for (height, items) in pages {
        let page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(
                PdfPoints::new(mm_to_pt(w)),
                PdfPoints::new(mm_to_pt(height)),
            ))
            .map_err(|e| e.to_string())?;
        let mut c = Canvas {
            doc: &doc,
            page,
            width_mm: w,
            height_mm: height,
        };
        for (i, dy, vals) in items {
            let el = &template.elements[i];
            let el = if dy == 0.0 { el.clone() } else { el.shifted(dy) };
            draw_element(&mut c, &fonts, &el, &vals).map_err(|e| err(i, e))?;
        }
        c.page.regenerate_content().map_err(|e| e.to_string())?;
    }
    Ok(doc)
}

/// Writes the filled design to `path`. `header` holds values shared by every page.
pub fn render(path: &Path, template: &Template, records: &[Record], header: &Record) -> Result<(), String> {
    template.validate()?;
    if records.is_empty() {
        return Err("Data is empty: send at least one record".into());
    }
    if records.len() > MAX_RECORDS {
        return Err(format!("at most {} records per job", MAX_RECORDS));
    }
    let pdfium = pdfium()?;
    let doc = build(&pdfium, template, records, header)?;
    doc.save_to_file(path)
        .map_err(|e| format!("Failed to save template PDF: {}", e))
}

/// The first `max_pages` printed pages as PNGs, for a print preview.
pub fn pages_png(
    template: &Template,
    records: &[Record],
    header: &Record,
    max_pages: usize,
    px_per_mm: f32,
) -> Result<(Vec<Vec<u8>>, usize), String> {
    template.validate()?;
    let pdfium = pdfium()?;
    let doc = build(&pdfium, template, records, header)?;
    let total = doc.pages().len() as usize;
    let pages = (0..total.min(max_pages))
        .map(|i| render_page_png(&doc, i, px_per_mm))
        .collect::<Result<_, _>>()?;
    Ok((pages, total))
}

/// The editor canvas: a label page, or for lists the bands (header, one row,
/// footer) stacked, so element positions match the editor 1:1.
pub fn preview_png(template: &Template, record: &Record, px_per_mm: f32) -> Result<Vec<u8>, String> {
    template.validate()?;
    let mut t = template.clone();
    if let Layout::List {
        header_mm,
        row_mm,
        footer_mm,
        ..
    } = t.layout
    {
        t.layout = Layout::List {
            header_mm,
            row_mm,
            footer_mm,
            footer: FooterMode::LastPage,
            roll: true,
        };
    }
    let pdfium = pdfium()?;
    let doc = build(&pdfium, &t, std::slice::from_ref(record), record)?;
    render_page_png(&doc, 0, px_per_mm)
}

pub(crate) fn render_page_png(doc: &PdfDocument, index: usize, px_per_mm: f32) -> Result<Vec<u8>, String> {
    let page = doc.pages().get(index as PdfPageIndex).map_err(|e| e.to_string())?;
    let w = (pt_to_mm(page.width().value) * px_per_mm).round().clamp(1.0, 4000.0) as i32;
    let h = (pt_to_mm(page.height().value) * px_per_mm).round().clamp(1.0, 4000.0) as i32;
    let rgba = page
        .render_with_config(&PdfRenderConfig::new().set_fixed_size(w, h))
        .map_err(|e| e.to_string())?
        .as_rgba_bytes();

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
